//! `bench_heap <doc> [runs]`: peak heap above baseline via a counting global allocator.
use parser_spike::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);
/// Lower bound: treats every realloc as in place, so old and new are never both counted.
static LIVE_LOW: AtomicUsize = AtomicUsize::new(0);
static PEAK_LOW: AtomicUsize = AtomicUsize::new(0);

fn grew(n: usize) {
    let live = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(live, Relaxed);
}
fn low(add: usize, sub: usize) {
    let v = LIVE_LOW.fetch_add(add, Relaxed) + add - sub;
    LIVE_LOW.fetch_sub(sub, Relaxed);
    PEAK_LOW.fetch_max(v, Relaxed);
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        COUNT.fetch_add(1, Relaxed);
        grew(l.size());
        low(l.size(), 0);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        low(0, l.size());
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        COUNT.fetch_add(1, Relaxed);
        // Conservative: a moving realloc briefly holds both blocks, so count both.
        grew(new);
        LIVE.fetch_sub(l.size(), Relaxed);
        low(new, l.size());
        unsafe { System.realloc(p, l, new) }
    }
}
#[global_allocator]
static A: Counting = Counting;

const CASES: &[(&str, fn(&str) -> usize)] = &[
    ("pulldown_walk", walk_pulldown),
    ("pulldown_autolink_walk", walk_pulldown_autolink),
    ("pulldown_autolink_retained", walk_pulldown_retained),
    ("comrak_parse_walk", walk_comrak),
];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let doc = std::fs::read_to_string(&a[1]).unwrap();
    let runs: usize = a.get(2).map_or(20, |s| s.parse().unwrap());
    println!("doc {} bytes, {runs} runs; bytes above the live baseline (document text excluded)", doc.len());
    for (name, f) in CASES {
        let (mut peaks, mut lows, mut counts) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..runs {
            let base = LIVE.load(Relaxed);
            PEAK.store(base, Relaxed);
            let base_low = LIVE_LOW.load(Relaxed);
            PEAK_LOW.store(base_low, Relaxed);
            let c0 = COUNT.load(Relaxed);
            std::hint::black_box(f(&doc));
            peaks.push(PEAK.load(Relaxed) - base);
            lows.push(PEAK_LOW.load(Relaxed) - base_low);
            counts.push(COUNT.load(Relaxed) - c0);
        }
        peaks.sort();
        lows.sort();
        counts.sort();
        println!(
            "{name:<28} peak median {:>10}  min {:>10}  max {:>10}  in-place-realloc peak median {:>10}  allocs median {:>8}",
            (peaks[runs / 2] + peaks[(runs - 1) / 2]) / 2,
            peaks[0],
            peaks[runs - 1],
            (lows[runs / 2] + lows[(runs - 1) / 2]) / 2,
            counts[runs / 2]
        );
    }
}
