//! Spike host. The main thread stands in for the UI thread: it ticks every 16 ms and only ever
//! waits on a channel with that timeout. A supervisor thread owns the engine child.
mod proto;
use proto::*;
use std::collections::VecDeque;
use std::io::BufReader;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(5);
const RLIMIT_AS_BYTES: u64 = 512 << 20;
const TICK: Duration = Duration::from_millis(16);

// ---------- supervisor ----------

struct Job {
    id: u32,
    kind: u8,
    payload: Vec<u8>,
}

#[derive(Debug)]
enum JobOutcome {
    Pixels { w: u32, h: u32, baseline: u32, bgra: Vec<u8> },
    Spans { runs: Vec<(u32, u32)> },
    EngineError(String),
    DeadlineKilled,
    Crashed(String),
    SpawnFailed(String),
}

struct JobDone {
    id: u32,
    outcome: JobOutcome,
    metrics: Metrics,
    /// Time the supervisor started the job, including any spawn. Measures the full round trip.
    start: Instant,
    done: Instant,
    spawned_for_this_job: bool,
    pid: u32,
}

enum ToUi {
    Done(JobDone),
    EngineExited { pid: u32, status: String, idle: bool, at: Instant },
}

enum Ev {
    Submit(Job),
    KillChild,
    Frame(u64, Frame),
    Eof(u64),
}

struct Live {
    child: Child,
    gen: u64,
    writer: Sender<Vec<u8>>,
}

struct Current {
    job: Job,
    start: Instant,
    deadline: Instant,
    spawned: bool,
    retried: bool,
    pid: u32,
}

fn status_text(s: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = s.signal() {
            return format!("signal {sig}");
        }
    }
    format!("exit code {}", s.code().unwrap_or(-1))
}

fn engine_path() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.with_file_name(format!("mdview-engine{}", std::env::consts::EXE_SUFFIX))
}

/// Linux limits. The Windows equivalent is a Job Object (JobMemoryLimit plus
/// KILL_ON_JOB_CLOSE); it is NOT implemented in this spike.
#[cfg(unix)]
fn apply_limits(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    let parent = unsafe { libc::getpid() };
    unsafe {
        cmd.pre_exec(move || {
            let lim = libc::rlimit { rlim_cur: RLIMIT_AS_BYTES, rlim_max: RLIMIT_AS_BYTES };
            if libc::setrlimit(libc::RLIMIT_AS, &lim) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let nocore = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            libc::setrlimit(libc::RLIMIT_CORE, &nocore);
            // Fires when the spawning THREAD dies, so only the long-lived supervisor spawns.
            #[cfg(target_os = "linux")]
            {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    libc::_exit(1);
                }
            }
            Ok(())
        });
    }
}
#[cfg(not(unix))]
fn apply_limits(_cmd: &mut Command) {}

fn spawn_engine(gen: u64, ev: Sender<Ev>) -> std::io::Result<Live> {
    let mut cmd = Command::new(engine_path());
    cmd.env("M_ARENA_MAX", "2")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    apply_limits(&mut cmd);
    let mut child = cmd.spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (wtx, wrx) = mpsc::channel::<Vec<u8>>();
    // Separate writer: a stuck engine can never block the supervisor's deadline check.
    std::thread::spawn(move || {
        for buf in wrx {
            if write_all_flush(&mut stdin, &buf).is_err() {
                return;
            }
        }
    });
    std::thread::spawn(move || {
        let mut r = BufReader::with_capacity(1 << 16, stdout);
        while let Ok(f) = read_frame(&mut r, MAX_REPLY_FRAME) {
            if ev.send(Ev::Frame(gen, f)).is_err() {
                return;
            }
        }
        let _ = ev.send(Ev::Eof(gen));
    });
    Ok(Live { child, gen, writer: wtx })
}

fn decode(f: &Frame) -> (JobOutcome, Metrics) {
    let n = N_METRICS * 4;
    if f.oversized.is_some() || f.payload.len() < n {
        return (JobOutcome::EngineError("malformed reply".into()), Metrics::default());
    }
    let m = Metrics::decode(&f.payload);
    let b = &f.payload[n..];
    let out = match f.kind {
        REP_PIXELS if b.len() >= 12 => {
            let (w, h, baseline) = (u32_at(b, 0), u32_at(b, 4), u32_at(b, 8));
            if b.len() - 12 != (w as usize) * (h as usize) * 4 {
                JobOutcome::EngineError("pixel size mismatch".into())
            } else {
                JobOutcome::Pixels { w, h, baseline, bgra: b[12..].to_vec() }
            }
        }
        REP_SPANS if b.len() >= 4 => JobOutcome::Spans {
            runs: b[4..].chunks_exact(8).map(|c| (u32_at(c, 0), u32_at(c, 4))).collect(),
        },
        REP_ERROR => JobOutcome::EngineError(String::from_utf8_lossy(b).into_owned()),
        k => JobOutcome::EngineError(format!("unknown reply kind {k}")),
    };
    (out, m)
}

fn supervisor(rx: Receiver<Ev>, ev_tx: Sender<Ev>, ui: Sender<ToUi>) {
    let mut live: Option<Live> = None;
    let mut gen = 0u64;
    let mut queue: VecDeque<Job> = VecDeque::new();
    let mut cur: Option<Current> = None;
    let finish = |c: Current, outcome: JobOutcome, metrics: Metrics, ui: &Sender<ToUi>| {
        let _ = ui.send(ToUi::Done(JobDone {
            id: c.job.id,
            outcome,
            metrics,
            start: c.start,
            done: Instant::now(),
            spawned_for_this_job: c.spawned,
            pid: c.pid,
        }));
    };
    loop {
        // Start the next job if idle.
        while cur.is_none() {
            let Some(job) = queue.pop_front() else { break };
            let start = Instant::now();
            let mut spawned = false;
            if live.is_none() {
                gen += 1;
                match spawn_engine(gen, ev_tx.clone()) {
                    Ok(l) => {
                        live = Some(l);
                        spawned = true;
                    }
                    Err(e) => {
                        let c = Current { job, start, deadline: start, spawned, retried: false, pid: 0 };
                        finish(c, JobOutcome::SpawnFailed(e.to_string()), Metrics::default(), &ui);
                        continue;
                    }
                }
            }
            let l = live.as_ref().unwrap();
            let _ = l.writer.send(encode_frame(job.kind, job.id, &job.payload));
            let pid = l.child.id();
            cur = Some(Current { job, start, deadline: Instant::now() + DEADLINE, spawned, retried: false, pid });
        }

        let ev = match &cur {
            Some(c) => rx.recv_timeout(c.deadline.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match ev {
            Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {
                if let Some(mut l) = live.take() {
                    let _ = l.child.kill();
                    let _ = l.child.wait();
                }
                finish(cur.take().unwrap(), JobOutcome::DeadlineKilled, Metrics::default(), &ui);
            }
            Ok(Ev::Submit(j)) => queue.push_back(j),
            Ok(Ev::KillChild) => {
                if let Some(mut l) = live.take() {
                    let _ = l.child.kill();
                    let _ = l.child.wait();
                }
            }
            Ok(Ev::Frame(g, f)) => {
                if live.as_ref().map(|l| l.gen) == Some(g)
                    && cur.as_ref().map(|c| c.job.id) == Some(f.id)
                {
                    let (o, m) = decode(&f);
                    finish(cur.take().unwrap(), o, m, &ui);
                }
            }
            Ok(Ev::Eof(g)) => {
                if live.as_ref().map(|l| l.gen) != Some(g) {
                    continue;
                }
                let mut l = live.take().unwrap();
                let pid = l.child.id();
                let status = l.child.wait().map(status_text).unwrap_or_else(|e| e.to_string());
                let idle = status == "exit code 0";
                let _ = ui.send(ToUi::EngineExited { pid, status: status.clone(), idle, at: Instant::now() });
                if let Some(mut c) = cur.take() {
                    if idle && !c.retried {
                        // Lost the idle-exit race: the engine never took the job. Resend once.
                        c.retried = true;
                        let job = c.job;
                        queue.push_front(job);
                        let _ = ui.send(ToUi::EngineExited { pid, status: "RETRIED after idle race".into(), idle: false, at: Instant::now() });
                    } else {
                        finish(c, JobOutcome::Crashed(status), Metrics::default(), &ui);
                    }
                }
            }
        }
    }
}

// ---------- "UI" thread ----------

#[derive(Default, Clone, Copy)]
struct Ticks {
    count: u64,
    max_gap: Duration,
}

struct Ui {
    tx: Sender<Ev>,
    rx: Receiver<ToUi>,
    next_id: u32,
    exits: Vec<(u32, String, bool, Instant)>,
}

impl Ui {
    fn new() -> Ui {
        let (tx, ev_rx) = mpsc::channel();
        let (ui_tx, rx) = mpsc::channel();
        let tx2 = tx.clone();
        std::thread::Builder::new()
            .name("supervisor".into())
            .spawn(move || supervisor(ev_rx, tx2, ui_tx))
            .unwrap();
        Ui { tx, rx, next_id: 0, exits: Vec::new() }
    }
    fn submit(&mut self, kind: u8, payload: Vec<u8>) -> u32 {
        self.next_id += 1;
        self.tx.send(Ev::Submit(Job { id: self.next_id, kind, payload })).unwrap();
        self.next_id
    }
    /// The only wait the UI thread does: tick, drain, repeat.
    fn pump(&mut self, until: impl Fn(&[JobDone], &[(u32, String, bool, Instant)]) -> bool) -> (Vec<JobDone>, Ticks) {
        let mut done = Vec::new();
        let mut t = Ticks::default();
        let mut last = Instant::now();
        let mut next_tick = last + TICK;
        loop {
            let wait = next_tick.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(wait) {
                Ok(ToUi::Done(d)) => done.push(d),
                Ok(ToUi::EngineExited { pid, status, idle, at }) => self.exits.push((pid, status, idle, at)),
                Err(_) => {}
            }
            let now = Instant::now();
            if now >= next_tick {
                t.count += 1;
                t.max_gap = t.max_gap.max(now - last);
                last = now;
                next_tick = now + TICK;
            }
            if until(&done, &self.exits) {
                return (done, t);
            }
        }
    }
    fn run(&mut self, kind: u8, payload: Vec<u8>) -> (JobDone, Ticks) {
        let id = self.submit(kind, payload);
        let (mut d, t) = self.pump(move |d, _| d.iter().any(|x| x.id == id));
        (d.pop().unwrap(), t)
    }
    fn wait_for(&mut self, dur: Duration) -> Ticks {
        let end = Instant::now() + dur;
        self.pump(move |_, _| Instant::now() >= end).1
    }
    fn kill_child(&mut self) {
        self.tx.send(Ev::KillChild).unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn math_payload(tex: &[u8], display: bool, em: f32) -> Vec<u8> {
    let mut p = vec![display as u8];
    p.extend_from_slice(&em.to_le_bytes());
    p.extend_from_slice(tex);
    p
}

fn hl_payload(lang: &str, code: &str) -> Vec<u8> {
    let mut p = (lang.len() as u16).to_le_bytes().to_vec();
    p.extend_from_slice(lang.as_bytes());
    p.extend_from_slice(code.as_bytes());
    p
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn stats(v: &mut [f64]) -> String {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    format!("n={} min={:.1} median={:.1} p95={:.1} max={:.1} ms", v.len(), v[0], p(0.5), p(0.95), v[v.len() - 1])
}

fn smaps(pid: u32) -> (u64, u64) {
    let s = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).unwrap_or_default();
    let get = |k: &str| {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    (get("Pss:"), get("Private_Clean:") + get("Private_Dirty:"))
}

/// All math in the fixture: $..$, $$..$$ and ```math fences. Returns (tex, display).
fn fixture_math(md: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut lines = md.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "```math" || line.trim() == "$$" {
            let end = if line.trim() == "$$" { "$$" } else { "```" };
            let body: Vec<&str> = lines.by_ref().take_while(|l| l.trim() != end).collect();
            out.push((body.join("\n"), true));
        } else if line.starts_with("```") {
            lines.by_ref().take_while(|l| !l.starts_with("```")).for_each(drop);
        } else {
            let parts: Vec<&str> = line.split('$').collect();
            for t in parts.iter().skip(1).step_by(2).take(parts.len().saturating_sub(1) / 2) {
                out.push((t.to_string(), false));
            }
        }
    }
    out
}

fn fixture_rust(md: &str) -> String {
    let start = md.find("```rust\n").unwrap() + 8;
    let end = start + md[start..].find("```").unwrap();
    md[start..end].to_string()
}

fn read_fixture(root: &str) -> String {
    std::fs::read_to_string(format!("{root}/showcase.md")).expect("fixture")
}

/// BGRA premultiplied over white to opaque RGB, for inspection only.
fn save_png(path: &str, w: u32, h: u32, bgra: &[u8]) {
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for c in bgra.chunks_exact(4) {
        let inv = 255 - c[3] as u32;
        rgb.push((c[2] as u32 + inv).min(255) as u8);
        rgb.push((c[1] as u32 + inv).min(255) as u8);
        rgb.push((c[0] as u32 + inv).min(255) as u8);
    }
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&rgb).unwrap();
}

fn cmd_render(fixtures: &str, out: &str) {
    std::fs::create_dir_all(out).unwrap();
    let md = read_fixture(fixtures);
    let mut items = fixture_math(&md);
    println!("fixture math examples found: {}", items.len());
    let n = items.len();
    items.push((r"\left(\begin{matrix} a & b \\ c & d \end{matrix}\right) \sqrt{\frac{x^2+1}{y_i}} \quad \text{Türkçe}".into(), true));
    let mut ui = Ui::new();
    for (i, (tex, display)) in items.iter().enumerate() {
        let (d, _) = ui.run(REQ_MATH, math_payload(tex.as_bytes(), *display, 48.0));
        let tag = if i < n { "fixture" } else { "extra" };
        match d.outcome {
            JobOutcome::Pixels { w, h, baseline, bgra } => {
                let p = format!("{out}/{tag}-{i}.png");
                save_png(&p, w, h, &bgra);
                let inked = bgra.chunks_exact(4).filter(|c| c[3] > 0).count();
                println!("OK {p} {w}x{h} baseline={baseline} inked_px={inked} total_us={} tex={tex:?} display={display}", d.metrics.total_us);
            }
            o => println!("FAIL {tex:?}: {o:?}"),
        }
    }
    ui.kill_child();
}

fn cmd_measure(fixtures: &str, runs: usize) {
    let md = read_fixture(fixtures);
    let rust = fixture_rust(&md);
    let tex: &[u8] = br"\int_0^1 x^2 dx = \frac{1}{3}";
    let mut ui = Ui::new();
    let (mut first_math, mut second_math, mut first_hl_live, mut second_hl) = (vec![], vec![], vec![], vec![]);
    let (mut hl_fresh, mut hl_load, mut cjk) = (vec![], vec![], vec![]);
    let (mut hwm_math, mut hwm_all, mut pss_math, mut priv_math, mut pss_all, mut priv_all, mut peak_vm) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let warm = 2;
    let mut last_runs = 0;
    for i in 0..runs + warm {
        // Sequence A: spawn, math, math, highlight, highlight.
        let t0 = Instant::now();
        let (a, _) = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
        let a_ui = ms(t0.elapsed());
        assert!(a.spawned_for_this_job, "expected a fresh spawn");
        assert!(matches!(a.outcome, JobOutcome::Pixels { .. }), "{:?}", a.outcome);
        let (b, _) = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
        std::thread::sleep(Duration::from_millis(5));
        let (p_m, q_m) = smaps(a.pid);
        let m_hwm = b.metrics.vm_hwm_kb;
        let (c, _) = ui.run(REQ_HIGHLIGHT, hl_payload("rust", &rust));
        let (d, _) = ui.run(REQ_HIGHLIGHT, hl_payload("rust", &rust));
        std::thread::sleep(Duration::from_millis(5));
        let (p_a, q_a) = smaps(a.pid);
        if let JobOutcome::Spans { runs, .. } = &c.outcome {
            let total: u32 = runs.iter().map(|r| r.0).sum();
            assert_eq!(total as usize, rust.len());
            last_runs = runs.len();
        } else {
            panic!("{:?}", c.outcome)
        }
        ui.kill_child();
        // Sequence B: spawn, highlight only.
        let (e, _) = ui.run(REQ_HIGHLIGHT, hl_payload("rust", &rust));
        assert!(e.spawned_for_this_job);
        ui.kill_child();
        // Sequence C: spawn, CJK math, which hits the system font fallback.
        let (f, _) = ui.run(REQ_MATH, math_payload("\\text{日本語} x^2".as_bytes(), true, 32.0));
        let cjk_ok = matches!(f.outcome, JobOutcome::Pixels { .. });
        if !cjk_ok && i == 0 {
            println!("cjk outcome: {:?}", f.outcome);
        }
        ui.kill_child();
        let t = |j: &JobDone| ms(j.done - j.start);
        println!(
            "run {i}{}: spawn+math={:.1} math2={:.2} hl_live_first={:.1} hl2={:.2} spawn+hl={:.1} hl_load={:.1} spawn+cjk={:.1} cjk_ok={cjk_ok} hwm_math={}KB hwm_all={}KB pss_math={}KB private_math={}KB pss_all={}KB private_all={}KB vmpeak={}KB engine_math_total={}us parse={}us layout={}us svg={}us raster={}us",
            if i < warm { " (warm-up, discarded)" } else { "" },
            a_ui, t(&b), t(&c), t(&d), t(&e), e.metrics.load_us as f64 / 1000.0, t(&f),
            m_hwm, d.metrics.vm_hwm_kb, p_m, q_m, p_a, q_a, d.metrics.vm_peak_kb,
            a.metrics.total_us, a.metrics.parse_us, a.metrics.layout_us, a.metrics.svg_us, a.metrics.raster_us
        );
        if i >= warm {
            first_math.push(a_ui);
            second_math.push(t(&b));
            first_hl_live.push(t(&c));
            second_hl.push(t(&d));
            hl_fresh.push(t(&e));
            hl_load.push(e.metrics.load_us as f64 / 1000.0);
            if cjk_ok {
                cjk.push(t(&f));
            }
            hwm_math.push(m_hwm as f64 / 1024.0);
            hwm_all.push(d.metrics.vm_hwm_kb as f64 / 1024.0);
            pss_math.push(p_m as f64 / 1024.0);
            priv_math.push(q_m as f64 / 1024.0);
            pss_all.push(p_a as f64 / 1024.0);
            priv_all.push(q_a as f64 / 1024.0);
            peak_vm.push(d.metrics.vm_peak_kb as f64 / 1024.0);
        }
    }
    println!("--- summary, measured, noisy (parallel builds running) ---");
    println!("rust fixture highlight: {} bytes, {last_runs} colour runs", rust.len());
    println!("spawn + first math      {}", stats(&mut first_math));
    println!("second math, live       {}", stats(&mut second_math));
    println!("first highlight, live   {}", stats(&mut first_hl_live));
    println!("second highlight, live  {}", stats(&mut second_hl));
    println!("spawn + first highlight {}", stats(&mut hl_fresh));
    println!("  of which syntax load  {}", stats(&mut hl_load));
    if !cjk.is_empty() {
        println!("spawn + CJK math        {}", stats(&mut cjk));
    }
    let mb = |s: String| s.replace(" ms", " MiB");
    println!("VmHWM after math        {}", mb(stats(&mut hwm_math)));
    println!("VmHWM after math+hl     {}", mb(stats(&mut hwm_all)));
    println!("Pss after math, trimmed {}", mb(stats(&mut pss_math)));
    println!("Private after math      {}", mb(stats(&mut priv_math)));
    println!("Pss after math+hl       {}", mb(stats(&mut pss_all)));
    println!("Private after math+hl   {}", mb(stats(&mut priv_all)));
    println!("VmPeak (address space)  {}", mb(stats(&mut peak_vm)));
}

/// Page-cache cold approximation: evict the engine binary with fadvise, then spawn. No root needed.
/// Evicts only this spike's file. Shared libraries stay cached, so it is not a true cold boot.
#[cfg(target_os = "linux")]
fn cmd_cold(runs: usize) {
    use std::os::fd::AsRawFd;
    let tex: &[u8] = br"\int_0^1 x^2 dx = \frac{1}{3}";
    let path = engine_path();
    let mut ui = Ui::new();
    let (mut cold, mut warm) = (vec![], vec![]);
    for i in 0..runs {
        let f = std::fs::File::open(&path).unwrap();
        f.sync_all().ok();
        let rc = unsafe { libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
        let t0 = Instant::now();
        let (a, _) = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
        let c = ms(t0.elapsed());
        ui.kill_child();
        let t0 = Instant::now();
        let (b, _) = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
        let w = ms(t0.elapsed());
        ui.kill_child();
        println!("run {i}: fadvise_rc={rc} evicted+spawn+math={c:.2} ms ok={} then warm spawn+math={w:.2} ms ok={}", matches!(a.outcome, JobOutcome::Pixels { .. }), matches!(b.outcome, JobOutcome::Pixels { .. }));
        cold.push(c);
        warm.push(w);
    }
    println!("evicted binary  {}", stats(&mut cold));
    println!("warm            {}", stats(&mut warm));
}

fn cmd_hostile() {
    let rep = |s: &str, n: usize| s.repeat(n);
    let cap = |s: String| {
        assert!(s.len() <= MAX_TEX_BYTES, "{}", s.len());
        s
    };
    let deep_frac = cap(format!("{}x{}", rep(r"\frac{", 400), rep("}{y}", 400)));
    let deep_brace = cap(format!("{}x{}", rep("{", 2000), rep("}", 2000)));
    let deep_sup = cap(format!("{}x{}", rep("x^{", 1000), rep("}", 1000)));
    let deep_sqrt = cap(format!("{}x{}", rep(r"\sqrt{", 500), rep("}", 500)));
    let deep_left = cap(format!("{}x{}", rep(r"\left(", 300), rep(r"\right)", 300)));
    let macro_bomb = cap({
        let mut s = String::from(r"\def\a{xxxxxxxxxx}");
        let mut prev = 'a';
        for c in 'b'..='k' {
            s.push_str(&format!("\\def\\{c}{{{}}}", format!("\\{prev}").repeat(10)));
            prev = c;
        }
        s.push_str(&format!("\\{prev}"));
        s
    });
    let big_array = cap(format!(r"\begin{{matrix}}{}\end{{matrix}}", rep(r"a&b\\", 800)));
    let many_glyphs = cap(rep("x", 4096));
    let mut cases: Vec<(&str, u8, Vec<u8>)> = vec![
        ("control: valid math", REQ_MATH, math_payload(br"\frac{a}{b}", true, 32.0)),
        ("TeX syntax error", REQ_MATH, math_payload(br"\frac{", true, 32.0)),
        ("TeX not UTF-8", REQ_MATH, math_payload(&[0xff, 0xfe, 0x41], true, 32.0)),
        ("deep \\frac x400", REQ_MATH, math_payload(deep_frac.as_bytes(), true, 32.0)),
        ("deep braces x2000", REQ_MATH, math_payload(deep_brace.as_bytes(), true, 32.0)),
        ("deep x^{ x1000", REQ_MATH, math_payload(deep_sup.as_bytes(), true, 32.0)),
        ("deep \\sqrt x500", REQ_MATH, math_payload(deep_sqrt.as_bytes(), true, 32.0)),
        ("deep \\left( x300", REQ_MATH, math_payload(deep_left.as_bytes(), true, 32.0)),
        ("\\def macro bomb 10^11", REQ_MATH, math_payload(macro_bomb.as_bytes(), true, 32.0)),
        ("matrix 800 rows", REQ_MATH, math_payload(big_array.as_bytes(), true, 32.0)),
        ("4096 glyphs", REQ_MATH, math_payload(many_glyphs.as_bytes(), true, 32.0)),
        ("\\rule 100000em", REQ_MATH, math_payload(br"\rule{100000em}{100000em}", true, 32.0)),
        ("\\rule 1e300em", REQ_MATH, math_payload(format!(r"\rule{{1{}em}}{{1em}}", "0".repeat(300)).as_bytes(), true, 32.0)),
        ("\\rule 1e400em (inf)", REQ_MATH, math_payload(format!(r"\rule{{1{}em}}{{1em}}", "0".repeat(400)).as_bytes(), true, 32.0)),
        ("\\rule 124em, 15.8 MP, fits", REQ_MATH, math_payload(br"\rule{124em}{124em}", true, 32.0)),
        ("\\rule 126em, over 16 MP", REQ_MATH, math_payload(br"\rule{126em}{126em}", true, 32.0)),
        ("\\left( around 1e5em rule", REQ_MATH, math_payload(br"\left(\rule{0pt}{100000em}\right)", true, 32.0)),
        ("\\left( around 200em rule", REQ_MATH, math_payload(br"\left(\rule{0pt}{200em}\right)", true, 32.0)),
        ("\\kern 1e6em", REQ_MATH, math_payload(br"x\kern1000000em y", true, 32.0)),
        ("\\raisebox 1e6em", REQ_MATH, math_payload(br"\raisebox{1000000em}{x}", true, 32.0)),
        ("array row gap 1e6em", REQ_MATH, math_payload(br"\begin{matrix}a\\[1000000em]b\end{matrix}", true, 32.0)),
        ("empty TeX", REQ_MATH, math_payload(b"", true, 32.0)),
        ("font size 1e9", REQ_MATH, math_payload(b"x", true, 1e9)),
        ("oversized TeX 4097 B", REQ_MATH, math_payload(rep("x", 4097).as_bytes(), true, 32.0)),
        ("oversized frame 10 MiB", REQ_MATH, math_payload(rep("x", 10 << 20).as_bytes(), true, 32.0)),
        ("unknown job kind", 77, vec![]),
        ("highlight 900 KB rust", REQ_HIGHLIGHT, hl_payload("rust", &"fn main() { let x = \"hello\"; }\n".repeat(29_000))),
        ("highlight 900 KB one line", REQ_HIGHLIGHT, hl_payload("js", &format!("var a=[{}];", "{\"k\":\"v\",\"n\":1},".repeat(50_000)))),
        ("panic job", REQ_PANIC, vec![]),
        ("hang job (60 s sleep)", REQ_SLEEP, 60_000u32.to_le_bytes().to_vec()),
        ("memory bomb, touched", REQ_BOMB_TOUCH, vec![]),
        ("memory bomb, 1 TiB alloc", REQ_BOMB_HUGE, vec![]),
        ("stack overflow", REQ_STACK, vec![]),
    ];
    let recovery = br"e^{i\pi}+1=0";
    let mut ui = Ui::new();
    let mut bad = 0;
    println!("{:<27} {:>9} {:>6} {:>8}  {:<8} result", "case", "wall ms", "ticks", "maxgap", "recover");
    for (name, kind, payload) in cases.drain(..) {
        let (d, t) = ui.run(kind, payload);
        let wall = ms(d.done - d.start);
        let result = match &d.outcome {
            JobOutcome::Pixels { w, h, .. } => format!("rendered {w}x{h}, engine HWM {} KB", d.metrics.vm_hwm_kb),
            JobOutcome::Spans { runs } => format!("{} colour runs, engine HWM {} KB", runs.len(), d.metrics.vm_hwm_kb),
            JobOutcome::EngineError(e) => format!("error: {}", e.chars().take(90).collect::<String>()),
            JobOutcome::DeadlineKilled => "error: deadline, engine killed".to_string(),
            JobOutcome::Crashed(s) => format!("error: engine died, {s}"),
            JobOutcome::SpawnFailed(s) => format!("error: spawn failed {s}"),
        };
        // Next job must work, restarting the engine if it died.
        let (r, _) = ui.run(REQ_MATH, math_payload(recovery, false, 32.0));
        let rec = match r.outcome {
            JobOutcome::Pixels { .. } if r.spawned_for_this_job => format!("{:.0}ms new", ms(r.done - r.start)),
            JobOutcome::Pixels { .. } => format!("{:.1}ms", ms(r.done - r.start)),
            _ => {
                bad += 1;
                "FAILED".to_string()
            }
        };
        println!("{name:<27} {wall:>9.1} {:>6} {:>6.1}ms  {rec:<8} {result}", t.count, ms(t.max_gap));
    }
    #[cfg(unix)]
    {
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut ru) };
        println!("largest RSS of any reaped engine (RUSAGE_CHILDREN): {} KB, RLIMIT_AS {} KB", ru.ru_maxrss, RLIMIT_AS_BYTES / 1024);
    }
    // Deepest nesting the parser accepts: the real stack and layout test, since 4 KiB of nesting
    // is refused outright.
    let shapes: [(&str, &str, &str, &str); 6] = [
        ("frac", r"\frac{", "}{y}", "x"),
        ("braces", "{", "}", "x"),
        ("sup", "x^{", "}", "x"),
        ("sqrt", r"\sqrt{", "}", "x"),
        ("left", r"\left(", r"\right)", "x"),
        ("array", r"\begin{matrix}", r"\end{matrix}", "x"),
    ];
    for (name, open, close, mid) in shapes {
        let mut best = None;
        for depth in 1..=64usize {
            let tex = format!("{}{mid}{}", open.repeat(depth), close.repeat(depth));
            let (d, _) = ui.run(REQ_MATH, math_payload(tex.as_bytes(), true, 32.0));
            let wall = ms(d.done - d.start);
            match d.outcome {
                JobOutcome::Pixels { w, h, .. } => best = Some((depth, w, h, wall, d.metrics.vm_hwm_kb)),
                JobOutcome::EngineError(e) if e.contains("Recursion") => {
                    println!("depth sweep {name}: deepest ok {best:?} (depth,w,h,wall ms,HWM KB); refused at {depth}");
                    break;
                }
                o => {
                    let text = format!("{o:?}");
                    println!("depth sweep {name}: depth {depth} wall {wall:.1} ms: {}", text.chars().take(90).collect::<String>());
                }
            }
        }
    }
    // Queue during a hang: the job behind it survives the kill.
    let a = ui.submit(REQ_SLEEP, 60_000u32.to_le_bytes().to_vec());
    let b = ui.submit(REQ_MATH, math_payload(br"\frac{a}{b}", true, 32.0));
    let (d, t) = ui.pump(move |d, _| d.iter().any(|x| x.id == b));
    let ha = d.iter().find(|x| x.id == a).unwrap();
    let hb = d.iter().find(|x| x.id == b).unwrap();
    println!(
        "queued behind hang: hang={:?} after {:.1} ms; queued job ok={} {:.1} ms; ticks={} maxgap={:.1}ms",
        matches!(ha.outcome, JobOutcome::DeadlineKilled), ms(ha.done - ha.start),
        matches!(hb.outcome, JobOutcome::Pixels { .. }), ms(hb.done - hb.start), t.count, ms(t.max_gap)
    );
    println!("engine exits seen by host: {}", ui.exits.len());
    for (pid, s, _, _) in &ui.exits {
        println!("  pid {pid}: {s}");
    }
    println!("recovery failures: {bad}");
    ui.kill_child();
}

fn cmd_idle() {
    let tex = br"\frac{a}{b}";
    let mut ui = Ui::new();
    // 1. Plain idle exit.
    for run in 0..2 {
        let n = ui.exits.len();
        let (d, _) = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
        let (done, pid) = (d.done, d.pid);
        println!("run {run}: pid {pid} alive after job: {}", std::path::Path::new(&format!("/proc/{pid}")).exists());
        let (_, t) = ui.pump(move |_, e| e.len() > n);
        let (_, st, _, at) = ui.exits.last().unwrap().clone();
        println!(
            "  exit after {:.0} ms idle ({st}); /proc/{pid} exists: {}; ticks={} maxgap={:.1}ms",
            ms(at - done), std::path::Path::new(&format!("/proc/{pid}")).exists(), t.count, ms(t.max_gap)
        );
    }
    // 2. A job at 6 s resets the timer.
    let n = ui.exits.len();
    let _ = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
    ui.wait_for(Duration::from_secs(6));
    let (d, _) = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
    let done = d.done;
    println!("job at 6 s: same engine, spawned={}", d.spawned_for_this_job);
    ui.pump(move |_, e| e.len() > n);
    println!("  exit {:.0} ms after the second job", ms(ui.exits.last().unwrap().3 - done));
    // 3. Submit right on the exit boundary. Must succeed whichever side it lands on.
    for offset in [-3i64, -1, 0, 1] {
        let (d, _) = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
        let target = d.done + Duration::from_millis((10_000 + offset) as u64);
        ui.pump(move |_, _| Instant::now() >= target);
        let (r, _) = ui.run(REQ_MATH, math_payload(tex, true, 32.0));
        println!(
            "boundary offset {offset} ms: ok={} spawned={} wall={:.1} ms",
            matches!(r.outcome, JobOutcome::Pixels { .. }), r.spawned_for_this_job, ms(r.done - r.start)
        );
        ui.kill_child();
        ui.wait_for(Duration::from_millis(20));
    }
    let retried = ui.exits.iter().filter(|e| e.1.starts_with("RETRIED")).count();
    println!("idle-race retries used: {retried}");
}

#[cfg(unix)]
fn limits_report(pid: u32) {
    let s = std::fs::read_to_string(format!("/proc/{pid}/limits")).unwrap_or_default();
    for l in s.lines().filter(|l| l.starts_with("Max address space") || l.starts_with("Max core")) {
        println!("  {l}");
    }
}

/// Child role: start an engine, print its pid, then wait to be SIGKILLed by the parent role.
fn cmd_pdeath_child() {
    let mut ui = Ui::new();
    let (d, _) = ui.run(REQ_SLEEP, 1u32.to_le_bytes().to_vec());
    println!("{}", d.pid);
    #[cfg(unix)]
    limits_report(d.pid);
    println!("ready");
    ui.wait_for(Duration::from_secs(30));
}

#[cfg(unix)]
fn cmd_pdeath() {
    use std::io::BufRead;
    let mut c = Command::new(std::env::current_exe().unwrap())
        .arg("pdeath-child")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let out = c.stdout.take().unwrap();
    let mut lines = BufReader::new(out).lines();
    let pid: u32 = lines.next().unwrap().unwrap().trim().parse().unwrap();
    for l in lines.by_ref() {
        let l = l.unwrap();
        if l == "ready" {
            break;
        }
        println!("{l}");
    }
    let alive = |p: u32| std::fs::read_to_string(format!("/proc/{p}/cmdline")).map(|s| s.contains("mdview-engine")).unwrap_or(false);
    println!("engine pid {pid}, alive before host SIGKILL: {}", alive(pid));
    let t = Instant::now();
    c.kill().unwrap();
    c.wait().unwrap();
    while alive(pid) && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_micros(200));
    }
    println!("engine alive after: {} ({:.2} ms after host SIGKILL)", alive(pid), ms(t.elapsed()));
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let fixtures = std::env::var("FIXTURES").unwrap_or_else(|_| "../../fixtures".into());
    match a.first().map(|s| s.as_str()) {
        Some("render") => cmd_render(&fixtures, a.get(1).map(|s| s.as_str()).unwrap_or("out")),
        Some("measure") => cmd_measure(&fixtures, a.get(1).and_then(|s| s.parse().ok()).unwrap_or(20)),
        #[cfg(target_os = "linux")]
        Some("cold") => cmd_cold(a.get(1).and_then(|s| s.parse().ok()).unwrap_or(10)),
        Some("hostile") => cmd_hostile(),
        Some("idle") => cmd_idle(),
        Some("pdeath-child") => cmd_pdeath_child(),
        #[cfg(unix)]
        Some("pdeath") => cmd_pdeath(),
        _ => eprintln!("usage: host render|measure [n]|cold [n]|hostile|idle|pdeath"),
    }
}
