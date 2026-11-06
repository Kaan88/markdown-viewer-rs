//! `parse_bench <file> [runs]`: parse time and heap use. Run with
//! `cargo run --release -p mdview-core --example parse_bench -- spikes/parser/data/generated_1mib.md`.
//!
//! The file is measured as it is, then again with one bare URL holding an `_` added at the end. That
//! URL makes the parser copy the source to protect it, so real documents can land on either figure.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);

fn grew(n: usize) {
    let live = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(live, Relaxed);
}

// Same accounting as the G4 spike's conservative column: a realloc counts old and new together.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        COUNT.fetch_add(1, Relaxed);
        grew(l.size());
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        COUNT.fetch_add(1, Relaxed);
        grew(new);
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    (v[v.len() / 2] + v[(v.len() - 1) / 2]) / 2.0
}

fn retained_bytes(d: &mdview_core::Document) -> [(&'static str, usize); 9] {
    [
        ("text", d.text.capacity()),
        ("strings", d.strings.capacity()),
        (
            "blocks",
            d.blocks.capacity() * std::mem::size_of::<mdview_core::model::Block>(),
        ),
        (
            "spans",
            d.spans.capacity() * std::mem::size_of::<mdview_core::model::Span>(),
        ),
        (
            "links",
            d.links.capacity() * std::mem::size_of::<mdview_core::model::LinkInfo>(),
        ),
        (
            "images",
            d.images.capacity() * std::mem::size_of::<mdview_core::model::ImageInfo>(),
        ),
        (
            "headings",
            d.headings.capacity() * std::mem::size_of::<mdview_core::model::Heading>(),
        ),
        (
            "anchors",
            d.anchors.capacity() * std::mem::size_of::<mdview_core::model::Anchor>(),
        ),
        (
            "footnotes",
            d.footnotes.capacity() * std::mem::size_of::<mdview_core::model::Footnote>(),
        ),
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: parse_bench <file> [runs]");
    let runs: usize = args
        .get(2)
        .map_or(20, |s| s.parse().expect("runs must be a number"));
    let src = std::fs::read_to_string(path).expect("readable UTF-8 file");
    println!(
        "{path}: {} bytes, {runs} runs (source text not counted in heap figures)",
        src.len()
    );
    report(&src, runs);

    println!("\nwith a bare URL added: http://a.b/c_d");
    report(&format!("{src}\n\nhttp://a.b/c_d\n"), runs);
}

fn report(src: &str, runs: usize) {
    let mut times = Vec::with_capacity(runs);
    let (mut peak, mut retained, mut allocs) = (0, 0, 0);
    for _ in 0..runs {
        let base = LIVE.load(Relaxed);
        PEAK.store(base, Relaxed);
        let c0 = COUNT.load(Relaxed);
        let t = Instant::now();
        let doc = mdview_core::parse(src);
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        peak = PEAK.load(Relaxed) - base;
        retained = LIVE.load(Relaxed) - base;
        allocs = COUNT.load(Relaxed) - c0;
        std::hint::black_box(&doc);
    }
    let (min, max) = times
        .iter()
        .fold((f64::MAX, 0f64), |(a, b), &t| (a.min(t), b.max(t)));
    println!(
        "parse median {:.3} ms (min {min:.3}, max {max:.3})",
        median(&mut times)
    );
    println!(
        "peak heap {:.2} MiB, retained {:.2} MiB, {allocs} allocations",
        peak as f64 / 1048576.0,
        retained as f64 / 1048576.0
    );

    let doc = mdview_core::parse(src);
    println!(
        "{} blocks, {} spans, {} links, {} images, {} headings",
        doc.blocks.len(),
        doc.spans.len(),
        doc.links.len(),
        doc.images.len(),
        doc.headings.len()
    );
    for (name, bytes) in retained_bytes(&doc) {
        println!("  {name:<10} {:>10} bytes", bytes);
    }
}
