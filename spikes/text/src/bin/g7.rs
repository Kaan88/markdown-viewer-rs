//! G7: seeded 1 MB Markdown-ish doc, per-block parley layout off-thread.
//! Usage: g7 fontctx | g7 all [--ascii] [--width W] [--scale S] [--passes N] [--threads N]
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Layout, LayoutContext,
    StyleProperty, TextWrapMode,
};
use std::alloc::{GlobalAlloc, Layout as AL, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;
use text_spike::*;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: AL) -> *mut u8 {
        let n = LIVE.fetch_add(l.size(), Relaxed) + l.size();
        PEAK.fetch_max(n, Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: AL) {
        LIVE.fetch_sub(l.size(), Relaxed);
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: AL, new: usize) -> *mut u8 {
        if new > l.size() {
            let n = LIVE.fetch_add(new - l.size(), Relaxed) + new - l.size();
            PEAK.fetch_max(n, Relaxed);
        } else {
            LIVE.fetch_sub(l.size() - new, Relaxed);
        }
        System.realloc(p, l, new)
    }
}
#[global_allocator]
static A: Counting = Counting;

fn live() -> usize {
    LIVE.load(Relaxed)
}
fn mb(b: usize) -> f64 {
    b as f64 / 1048576.0
}
fn status(k: &str) -> f64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap();
    s.lines()
        .find(|l| l.starts_with(k))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
        / 1024.0
}

// ---- seeded document ----
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
const SYL: &[&str] = &[
    "ta", "re", "lo", "mi", "ka", "ven", "dor", "sil", "an", "pre", "con", "ter", "ing", "ment",
    "ous", "al", "ly", "for", "the", "ber", "nu", "ges", "tion", "pa", "ro", "fi", "st", "qua",
];
const TR: &[&str] = &["Işık", "ılık", "İstanbul", "ağaç", "şöyle", "güneş", "çığlık", "öğrenci"];
const OTHER: &[&str] = &[
    "吾輩は猫である。",
    "名前はまだ無い。",
    "مرحبا بالعالم",
    "😀",
    "🚀",
    "Привет",
    "한국어",
];
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    H,
    P,
    L,
    Code,
    Tab,
    Q,
}
struct Block {
    kind: Kind,
    text: String,
}

static HEAVY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn word(r: &mut Rng, ascii: bool, out: &mut String) {
    let mut roll = r.below(1000);
    if HEAVY.load(Relaxed) && roll < 500 {
        roll = 30 + roll % 10;
    }
    if !ascii && roll < 30 {
        out.push_str(TR[r.below(TR.len() as u64) as usize]);
    } else if !ascii && roll < 40 {
        out.push_str(OTHER[r.below(OTHER.len() as u64) as usize]);
    } else {
        for _ in 0..1 + r.below(3) {
            out.push_str(SYL[r.below(SYL.len() as u64) as usize]);
        }
    }
}
fn sentence(r: &mut Rng, ascii: bool, n: u64, out: &mut String) {
    for i in 0..n {
        if i > 0 {
            out.push(' ');
        }
        let style = r.below(100);
        if style < 3 {
            out.push_str("**");
            word(r, ascii, out);
            out.push_str("**");
        } else if style < 6 {
            out.push('`');
            word(r, true, out);
            out.push('`');
        } else {
            word(r, ascii, out);
        }
        if r.below(12) == 0 {
            out.push_str(". ");
        }
    }
}
fn gen_doc(seed: u64, target: usize, ascii: bool) -> Vec<Block> {
    let mut r = Rng(seed);
    let (mut v, mut total) = (Vec::new(), 0);
    while total < target {
        let roll = r.below(100);
        let mut t = String::new();
        let kind = match roll {
            0..=5 => {
                let n = 3 + r.next() % 5;
                sentence(&mut r, ascii, n, &mut t);
                Kind::H
            }
            6..=60 => {
                let n = 30 + r.next() % 150;
                sentence(&mut r, ascii, n, &mut t);
                Kind::P
            }
            61..=75 => {
                let n = 8 + r.next() % 30;
                sentence(&mut r, ascii, n, &mut t);
                Kind::L
            }
            76..=90 => {
                for _ in 0..8 + r.below(18) {
                    t.push_str("    let ");
                    word(&mut r, true, &mut t);
                    t.push_str(" = call(");
                    word(&mut r, true, &mut t);
                    t.push_str(", 0x5f) + 1; // ");
                    sentence(&mut r, true, 3, &mut t);
                    t.push('\n');
                }
                t.pop();
                Kind::Code
            }
            91..=95 => {
                for _ in 0..4 + r.below(10) {
                    for _ in 0..4 {
                        t.push_str("| ");
                        sentence(&mut r, ascii, 2, &mut t);
                        t.push(' ');
                    }
                    t.push_str("|\n");
                }
                t.pop();
                Kind::Tab
            }
            _ => {
                let n = 15 + r.next() % 60;
                sentence(&mut r, ascii, n, &mut t);
                Kind::Q
            }
        };
        total += t.len() + 2;
        v.push(Block { kind, text: t });
    }
    v
}

// ---- layout ----
fn inline_spans(t: &str) -> Vec<(usize, usize, bool)> {
    // Naive `**bold**` and `code` spans; markers stay in the text (spike only).
    let (b, mut out, mut i) = (t.as_bytes(), Vec::new(), 0);
    while i + 1 < b.len() {
        if b[i] == b'*' && b[i + 1] == b'*' {
            if let Some(e) = t[i + 2..].find("**") {
                out.push((i, i + 2 + e + 2, true));
                i += e + 4;
                continue;
            }
        } else if b[i] == b'`' {
            if let Some(e) = t[i + 1..].find('`') {
                out.push((i, i + 1 + e + 1, false));
                i += e + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}
fn build_into(
    fc: &mut FontContext,
    lc: &mut LayoutContext<Brush>,
    b: &Block,
    scale: f32,
    w: f32,
    l: &mut Layout<Brush>,
) {
    let t = b.text.as_str();
    let sans = FontFamily::from("Noto Sans, sans-serif");
    let mono = FontFamily::from("Noto Sans Mono, monospace");
    let mut bl = lc.ranged_builder(fc, t, scale, true);
    bl.push_default(StyleProperty::Brush([0x1f, 0x23, 0x28, 255]));
    match b.kind {
        Kind::Code | Kind::Tab => {
            bl.push_default(StyleProperty::FontFamily(mono.clone()));
            bl.push_default(StyleProperty::FontSize(14.0));
            bl.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
        }
        Kind::H => {
            bl.push_default(StyleProperty::FontFamily(sans.clone()));
            bl.push_default(StyleProperty::FontSize(24.0));
            bl.push_default(StyleProperty::FontWeight(FontWeight::new(600.0)));
        }
        _ => {
            bl.push_default(StyleProperty::FontFamily(sans.clone()));
            bl.push_default(StyleProperty::FontSize(16.0));
            for (s, e, bold) in inline_spans(t) {
                if bold {
                    bl.push(StyleProperty::FontWeight(FontWeight::new(700.0)), s..e);
                } else {
                    bl.push(StyleProperty::FontFamily(mono.clone()), s..e);
                }
            }
        }
    }
    bl.build_into(l, t);
    l.break_all_lines(Some(w));
}
const GAP: f32 = 14.0;

/// One full height pass with reused scratch. Returns block Y offsets.
fn height_pass(
    fc: &mut FontContext,
    lc: &mut LayoutContext<Brush>,
    d: &[Block],
    s: f32,
    w: f32,
) -> Vec<f32> {
    let mut l = Layout::new();
    let mut ys = Vec::with_capacity(d.len() + 1);
    let mut y = 0.0;
    for b in d {
        ys.push(y);
        build_into(fc, lc, b, s, w, &mut l);
        y += l.height() + (GAP * s).round();
    }
    ys.push(y);
    ys
}

fn pctl(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn visible_range(ys: &[f32], top: f32, bot: f32) -> std::ops::Range<usize> {
    let a = ys.partition_point(|y| *y <= top).saturating_sub(1);
    let b = ys.partition_point(|y| *y < bot).min(ys.len() - 1);
    a..b.max(a)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let flag = |n: &str, d: f32| -> f32 {
        a.iter().position(|x| x == n).map(|i| a[i + 1].parse().unwrap()).unwrap_or(d)
    };
    if a.get(1).map(|s| s.as_str()) == Some("fontctx") {
        // Fresh-process measurement of FontContext creation (off main thread, as planned).
        let h = std::thread::spawn(|| {
            let (b0, t) = (live(), Instant::now());
            let mut fc = FontContext::new();
            let new_ms = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let fams = fc.collection.family_names().count();
            let list_ms = t.elapsed().as_secs_f64() * 1e3;
            let mut lc = LayoutContext::<Brush>::new();
            let t = Instant::now();
            let blk = Block { kind: Kind::P, text: "Hello world İı 日本語 😀".into() };
            let mut l = Layout::new();
            build_into(&mut fc, &mut lc, &blk, 1.0, 900.0, &mut l);
            let first_ms = t.elapsed().as_secs_f64() * 1e3;
            println!(
                "fontctx new_ms={new_ms:.2} family_names_ms={list_ms:.2} first_layout_with_fallback_ms={first_ms:.2} families={fams} live_mb={:.2} rss_mb={:.1} rss_anon_mb={:.1}",
                mb(live() - b0), status("VmRSS"), status("RssAnon")
            );
        });
        h.join().unwrap();
        return;
    }
    let ascii = a.iter().any(|x| x == "--ascii");
    HEAVY.store(a.iter().any(|x| x == "--heavy"), Relaxed);
    let scale = flag("--scale", 1.0);
    let logical_w = flag("--width", 900.0);
    let passes = flag("--passes", 5.0) as usize;
    let threads = flag("--threads", 1.0) as usize;
    let cache_bytes = flag("--cache-kb", 4096.0) as usize * 1024;
    let (fw, fh) = (1220usize, 820usize);
    let content_w = (logical_w * scale).min(fw as f32 - 2.0 * (24.0 * scale));

    let doc = gen_doc(0x5EED_1234_ABCD_0001, 1 << 20, ascii);
    let bytes: usize = doc.iter().map(|b| b.text.len()).sum();
    println!(
        "doc bytes={bytes} blocks={} ascii={ascii} scale={scale} content_w={content_w} threads={threads}",
        doc.len()
    );

    // Everything below runs off the main thread.
    let doc = std::sync::Arc::new(doc);
    let d2 = doc.clone();
    std::thread::spawn(move || {
        let doc = d2;
        let t = Instant::now();
        let mut fc = FontContext::new();
        println!("font_context_new_ms={:.2}", t.elapsed().as_secs_f64() * 1e3);
        let mut lc = LayoutContext::<Brush>::new();
        let mut ys = Vec::new();
        for p in 0..passes {
            let (t, b0) = (Instant::now(), live());
            PEAK.store(b0, Relaxed);
            if threads <= 1 {
                ys = height_pass(&mut fc, &mut lc, &doc, scale, content_w);
            } else {
                // Partition blocks; per-thread LayoutContext, cloned FontContext.
                let chunk = doc.len().div_ceil(threads);
                let hs: Vec<f32> = std::thread::scope(|sc| {
                    let hs: Vec<_> = doc
                        .chunks(chunk)
                        .map(|ch| {
                            let mut f = fc.clone();
                            sc.spawn(move || {
                                let mut lc = LayoutContext::<Brush>::new();
                                height_pass(&mut f, &mut lc, ch, scale, content_w)
                            })
                        })
                        .collect();
                    hs.into_iter()
                        .flat_map(|h| {
                            let y = h.join().unwrap();
                            y.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>()
                        })
                        .collect()
                });
                ys.clear();
                let mut y = 0.0;
                for h in hs {
                    ys.push(y);
                    y += h;
                }
                ys.push(y);
            }
            println!(
                "height_pass[{p}] ms={:.1} total_height_px={:.0} peak_extra_mb={:.2}",
                t.elapsed().as_secs_f64() * 1e3,
                ys.last().unwrap(),
                mb(PEAK.load(Relaxed) - b0)
            );
        }
        println!(
            "height table ys bytes={} live_after_pass_mb={:.2} rss_mb={:.1} rss_anon_mb={:.1}",
            ys.len() * 4,
            mb(live()),
            status("VmRSS"),
            status("RssAnon")
        );

        // Visible-only retained layouts at 10 scroll positions.
        let total = *ys.last().unwrap();
        let mut rng = Rng(77);
        for (label, over) in [("viewport_only", 0.0f32), ("plus_1_viewport_each_side", fh as f32)] {
            let (mut sizes, mut counts) = (Vec::new(), Vec::new());
            for _ in 0..10 {
                let top = rng.below((total - fh as f32) as u64) as f32;
                let r = visible_range(&ys, top - over, top + fh as f32 + over);
                let b0 = live();
                let mut kept: Vec<Layout<Brush>> = Vec::new();
                for i in r.clone() {
                    let mut l = Layout::new();
                    build_into(&mut fc, &mut lc, &doc[i], scale, content_w, &mut l);
                    l.align(Alignment::Start, AlignmentOptions::default());
                    kept.push(l);
                }
                sizes.push(mb(live().saturating_sub(b0)));
                counts.push(kept.len());
            }
            let n = sizes.clone();
            println!(
                "visible_layouts[{label}] n=10 blocks={counts:?} mb_median={:.3} mb_max={:.3} raw={n:.3?}",
                pctl(&mut sizes.clone(), 0.5),
                pctl(&mut sizes, 1.0)
            );
        }

        // Frame raster with a 4 MB LRU glyph cache; visible layouts kept on demand.
        let mut r = Renderer::new(cache_bytes);
        let mut fb = Fb::new(fw, fh);
        let margin = ((fw as f32 - content_w) / 2.0).floor();
        let mut cached: std::collections::HashMap<usize, Layout<Brush>> = Default::default();
        let mut frame = |top: f32,
                         r: &mut Renderer,
                         fb: &mut Fb,
                         fc: &mut FontContext,
                         lc: &mut LayoutContext<Brush>|
         -> (f64, f64) {
            let tl = Instant::now();
            let rg = visible_range(&ys, top, top + fh as f32);
            cached.retain(|k, _| rg.contains(k));
            for i in rg.clone() {
                cached.entry(i).or_insert_with(|| {
                    let mut l = Layout::new();
                    build_into(fc, lc, &doc[i], scale, content_w, &mut l);
                    l.align(Alignment::Start, AlignmentOptions::default());
                    l
                });
            }
            let lay_ms = tl.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            fb.clear([255, 255, 255]);
            for i in rg {
                r.draw_layout(fb, &cached[&i], margin, (ys[i] - top).round());
            }
            (t.elapsed().as_secs_f64() * 1e3, lay_ms)
        };
        let (first, first_lay) = frame(0.0, &mut r, &mut fb, &mut fc, &mut lc);
        let s0 = r.stats;
        println!("frame_first_cold raster_ms={first:.2} layout_ms={first_lay:.2} stats={s0:?}");
        // Smooth scroll: 40 px per frame, repainting the full frame each time (worst case).
        for (label, step, frames) in [("scroll_40px", Some(40.0f32), 600usize), ("random_jump", None, 200)] {
            let (mut ras, mut lay) = (Vec::new(), Vec::new());
            let s_before = r.stats;
            let mut top = 0.0f32;
            for _ in 0..frames {
                top = match step {
                    Some(s) => top + s,
                    None => rng.below((total - fh as f32) as u64) as f32,
                };
                let (a, b) = frame(top, &mut r, &mut fb, &mut fc, &mut lc);
                ras.push(a);
                lay.push(b);
            }
            let st = r.stats;
            println!(
                "{label} frames={frames} raster_ms p50={:.2} p95={:.2} p99={:.2} max={:.2} | layout_ms p50={:.2} p99={:.2} max={:.2} | misses={} hits={} evictions={} cache_bytes={} peak={} entries={}",
                pctl(&mut ras, 0.5), pctl(&mut ras, 0.95), pctl(&mut ras, 0.99), pctl(&mut ras, 1.0),
                pctl(&mut lay, 0.5), pctl(&mut lay, 0.99), pctl(&mut lay, 1.0),
                st.misses - s_before.misses, st.hits - s_before.hits, st.evictions - s_before.evictions,
                st.bytes, st.peak_bytes, st.entries
            );
        }
        if std::env::var("G7_DUMP").is_ok() {
            fb.write_png(&std::env::var("G7_DUMP").unwrap());
        }
        println!(
            "end rss_mb={:.1} rss_anon_mb={:.1} live_mb={:.2}",
            status("VmRSS"),
            status("RssAnon"),
            mb(live())
        );
    })
    .join()
    .unwrap();
}
