//! harness --runs 12 --drop 2 [--smaps-after-ms 400] [--exit-only] -- CMD ARGS...
//! Spawns CMD repeatedly. T0 is CLOCK_MONOTONIC taken just before spawn and passed in
//! MDV_T0_NS, so present_ms includes fork, exec and dynamic linking.
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn now_ns() -> u64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut t) };
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

fn field(line: &str, key: &str) -> Option<f64> {
    line.split_whitespace().find_map(|w| w.strip_prefix(key)?.strip_prefix('=')?.parse().ok())
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

fn smaps(pid: u32) -> Vec<(String, f64)> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).unwrap_or_default();
    s.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let kb: f64 = v.trim().strip_suffix("kB")?.trim().parse().ok()?;
            Some((k.to_string(), kb))
        })
        .collect()
}

fn report(name: &str, vals: &[f64], drop: usize) {
    let kept: Vec<f64> = vals.iter().skip(drop).cloned().collect();
    let raw: Vec<String> = vals.iter().map(|v| format!("{v:.2}")).collect();
    println!("{name}: median_after_drop={:.2} n={} raw=[{}]", median(kept.clone()), kept.len(), raw.join(" "));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let sep = args.iter().position(|a| a == "--").expect("-- CMD");
    let (opts, cmd) = (&args[..sep], &args[sep + 1..]);
    let get = |n: &str| opts.iter().position(|a| a == n).map(|i| opts[i + 1].clone());
    let runs: usize = get("--runs").unwrap_or("12".into()).parse().unwrap();
    let drop: usize = get("--drop").unwrap_or("2".into()).parse().unwrap();
    let smaps_after: Option<u64> = get("--smaps-after-ms").map(|v| v.parse().unwrap());
    let exit_only = opts.iter().any(|a| a == "--exit-only");

    let (mut present, mut paint, mut exit) = (vec![], vec![], vec![]);
    let mut mem: Vec<Vec<(String, f64)>> = vec![];
    for _ in 0..runs {
        let t0 = now_ns();
        let start = Instant::now();
        let mut c = Command::new(&cmd[0]);
        c.args(&cmd[1..]).env("MDV_T0_NS", t0.to_string());
        if exit_only {
            let st = c.stdout(Stdio::null()).status().expect("spawn");
            assert!(st.success(), "child failed: {st}");
            exit.push(start.elapsed().as_secs_f64() * 1e3);
            continue;
        }
        let mut child = c.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("spawn");
        let pid = child.id();
        let out = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for l in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send(l);
            }
        });
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(l) if l.starts_with("PRESENTED") => {
                present.push(field(&l, "present_ms").unwrap());
                paint.push(field(&l, "first_paint_ms").unwrap());
                if let Some(ms) = smaps_after {
                    std::thread::sleep(Duration::from_millis(ms));
                    mem.push(smaps(pid));
                }
            }
            other => {
                println!("RUN FAILED: {other:?}");
                let _ = child.kill();
            }
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                println!("KILLED after timeout");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    if exit_only {
        report("exit_ms", &exit, drop);
        return;
    }
    report("present_ms", &present, drop);
    report("first_paint_ms", &paint, drop);
    if !mem.is_empty() {
        for key in ["Rss", "Pss", "Pss_Anon", "Pss_File", "Pss_Shmem", "Private_Clean", "Private_Dirty", "Shared_Clean", "Shared_Dirty", "Anonymous"] {
            let vals: Vec<f64> = mem.iter().map(|m| m.iter().find(|(k, _)| k == key).map_or(0.0, |x| x.1) / 1024.0).collect();
            report(&format!("{key}_MiB"), &vals, 0);
        }
        let private: Vec<f64> = mem
            .iter()
            .map(|m| m.iter().filter(|(k, _)| k == "Private_Clean" || k == "Private_Dirty").map(|x| x.1).sum::<f64>() / 1024.0)
            .collect();
        report("PrivateTotal_MiB", &private, 0);
    }
}
