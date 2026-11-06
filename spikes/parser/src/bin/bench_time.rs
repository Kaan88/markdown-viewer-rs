//! `bench_time <doc> [runs]` round-robin timing with the system allocator.
//! `bench_time <doc> --cold <name>` one parse in a fresh process, for first-parse cost.
use parser_spike::*;
use std::time::Instant;

const CASES: &[(&str, fn(&str) -> usize)] = &[
    ("pulldown_walk", walk_pulldown),
    ("pulldown_autolink_walk", walk_pulldown_autolink),
    ("pulldown_autolink_retained", walk_pulldown_retained),
    ("comrak_parse_walk", walk_comrak),
];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let doc = std::fs::read_to_string(&a[1]).unwrap();
    if a.get(2).map(String::as_str) == Some("--cold") {
        let f = CASES.iter().find(|c| c.0 == a[3]).unwrap().1;
        let t = Instant::now();
        let n = f(&doc);
        println!("{} {} {:.3}", a[3], n, t.elapsed().as_secs_f64() * 1e3);
        return;
    }
    let runs: usize = a.get(2).map_or(20, |s| s.parse().unwrap());
    println!("doc {} bytes, {runs} runs, round-robin, ms", doc.len());
    let mut res = vec![Vec::with_capacity(runs); CASES.len()];
    for _ in 0..runs {
        for (i, (_, f)) in CASES.iter().enumerate() {
            let t = Instant::now();
            std::hint::black_box(f(&doc));
            res[i].push(t.elapsed().as_secs_f64() * 1e3);
        }
    }
    for (i, (name, _)) in CASES.iter().enumerate() {
        let first = res[i][0];
        let mut v = res[i].clone();
        v.sort_by(f64::total_cmp);
        let med = (v[runs / 2] + v[(runs - 1) / 2]) / 2.0;
        println!(
            "{name:<28} median {med:8.3}  min {:8.3}  max {:8.3}  first {first:8.3}  MB/s {:7.1}",
            v[0],
            v[runs - 1],
            doc.len() as f64 / 1e6 / (med / 1e3)
        );
        let raw: Vec<String> = res[i].iter().map(|x| format!("{x:.3}")).collect();
        println!("  raw {}", raw.join(" "));
    }
}
