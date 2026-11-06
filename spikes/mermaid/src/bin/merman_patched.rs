//! Harness for the shipped route. Modes:
//!   bench [N]             every diagram, whole route, median and worst of N
//!   one <name>            one diagram in a fresh process, with peak memory
//!   file <path>           any source file through the whole route
//!   hostile [case]        as the other harnesses; hostile-deadline adds merman's own 5 s limit
//! MERMAID_FONT=segoe|liberation picks the faces (default segoe).
//!   render <dir> [light|dark] [real|det] [full|stroke|none]   all sets to <dir>, one line per diagram
#[path = "../common.rs"]
mod common;
use mermaid_spike::patch::{patch_svg_with, PatchOptions};
use mermaid_spike::pipeline::{text_outside_canvas, Metrics, Route, Theme};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn font_set() -> String {
    std::env::var("MERMAID_FONT").unwrap_or_else(|_| "segoe".into())
}

fn route() -> &'static Route {
    static R: OnceLock<Route> = OnceLock::new();
    R.get_or_init(|| Route::new(&font_set()).expect("fonts"))
}

fn render(src: &str) -> Result<String, String> {
    route().svg(src, Theme::Light, None)
}

/// Same as `render`, with the engine's 5 s budget enforced by merman itself.
fn render_deadline(src: &str) -> Result<String, String> {
    route().svg(src, Theme::Light, Some(Duration::from_secs(5)))
}

fn render_sets(args: &[String]) {
    let dir = args.first().expect("dir");
    let theme = if args.get(1).map(String::as_str) == Some("dark") { Theme::Dark } else { Theme::Light };
    let m = if args.get(2).map(String::as_str) == Some("det") { Metrics::Deterministic } else { Metrics::Real };
    let patch = match args.get(3).map(String::as_str) {
        Some("stroke") => Some(PatchOptions::STROKE_ONLY),
        Some("none") => None,
        _ => Some(PatchOptions::FULL),
    };
    std::fs::create_dir_all(dir).unwrap();
    let r = route();
    println!("name\tsvg_ms\tpatch_ms\traster_ms\ttotal_ms\tsvg_bytes\tpng_bytes\tsize\toutside_canvas");
    for (name, src) in mermaid_spike::corpus::all() {
        let t = Instant::now();
        let raw = match r.raw_svg(&src, m, theme, None) {
            Ok(s) => s,
            Err(e) => {
                println!("{name}\tERROR\t{}", e.replace('\n', " "));
                continue;
            }
        };
        let a = t.elapsed();
        let t = Instant::now();
        let svg = match patch {
            Some(p) => patch_svg_with(&raw, p).into_owned(),
            None => raw,
        };
        let b = t.elapsed();
        let t = Instant::now();
        let pm = match r.raster(&svg, 2.0, theme) {
            Ok(p) => p,
            Err(e) => {
                println!("{name}\tRASTER-ERROR\t{e}");
                continue;
            }
        };
        let png = pm.encode_png().unwrap();
        let c = t.elapsed();
        let tree = r.tree(&svg).unwrap();
        let out = text_outside_canvas(&tree);
        std::fs::write(format!("{dir}/{name}.png"), &png).unwrap();
        std::fs::write(format!("{dir}/{name}.svg"), &svg).unwrap();
        println!(
            "{name}\t{:.2}\t{:.3}\t{:.2}\t{:.2}\t{}\t{}\t{}x{}\t{out:?}",
            a.as_secs_f64() * 1e3,
            b.as_secs_f64() * 1e3,
            c.as_secs_f64() * 1e3,
            (a + b + c).as_secs_f64() * 1e3,
            svg.len(),
            png.len(),
            pm.width(),
            pm.height(),
        );
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// Source to PNG bytes, the whole route, for every diagram: median and worst of N.
fn bench(args: &[String]) {
    let iters: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(30);
    let t = Instant::now();
    let r = route();
    println!("# font={} route_init_ms={:.2} iters={iters}", font_set(), t.elapsed().as_secs_f64() * 1e3);
    println!("name\tmed_svg_ms\tmed_png_ms\tmed_total_ms\tmax_total_ms");
    for (name, src) in mermaid_spike::corpus::all() {
        let (mut sv, mut pv, mut tv) = (vec![], vec![], vec![]);
        for _ in 0..iters {
            let t = Instant::now();
            let svg = r.svg(&src, Theme::Light, None).unwrap();
            let a = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let _ = r.raster(&svg, 2.0, Theme::Light).unwrap().encode_png().unwrap();
            let b = t.elapsed().as_secs_f64() * 1e3;
            sv.push(a);
            pv.push(b);
            tv.push(a + b);
        }
        let max = tv.iter().cloned().fold(0., f64::max);
        println!("{name}\t{:.2}\t{:.2}\t{:.2}\t{max:.2}", median(&mut sv), median(&mut pv), median(&mut tv));
    }
    println!("# VmHWM_kb={}", common::vm_hwm_kb());
}

/// One diagram in a fresh process: fonts, render, raster, peak memory.
fn one(args: &[String]) {
    let wanted = args.first().expect("name");
    let (_, src) = mermaid_spike::corpus::all().into_iter().find(|(n, _)| n == wanted).expect("name");
    let before = common::vm_hwm_kb();
    let t0 = Instant::now();
    let r = route();
    let f = t0.elapsed();
    let after_fonts = common::vm_hwm_kb();
    let t = Instant::now();
    let svg = r.svg(&src, Theme::Light, None).unwrap();
    let s = t.elapsed();
    let t = Instant::now();
    let png = r.raster(&svg, 2.0, Theme::Light).unwrap().encode_png().unwrap();
    let p = t.elapsed();
    println!(
        "{wanted}\tfont_ms={:.2}\tsvg_ms={:.2}\tpng_ms={:.2}\ttotal_ms={:.2}\tpng_bytes={}\tHWM_start_kb={before}\tHWM_fonts_kb={after_fonts}\tVmHWM_kb={}",
        f.as_secs_f64() * 1e3,
        s.as_secs_f64() * 1e3,
        p.as_secs_f64() * 1e3,
        t0.elapsed().as_secs_f64() * 1e3,
        png.len(),
        common::vm_hwm_kb()
    );
}

/// Any source file through the shipped route, for checking one reported case.
fn file(args: &[String]) {
    let src = std::fs::read_to_string(&args[0]).expect("source");
    let t = Instant::now();
    let r = route().svg(&src, Theme::Light, None).and_then(|svg| route().raster(&svg, 2.0, Theme::Light));
    let ms = t.elapsed().as_secs_f64() * 1e3;
    match r {
        Ok(p) => println!("ok {}x{} {ms:.1}ms VmHWM_kb={}", p.width(), p.height(), common::vm_hwm_kb()),
        Err(e) => println!("err {e} {ms:.1}ms VmHWM_kb={}", common::vm_hwm_kb()),
    }
}

fn main() {
    // common.rs paints with whatever this says; keep painter and measurer on the same font.
    std::env::set_var("MERMAID_FONT", font_set());
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("render") => render_sets(&args[1..]),
        Some("bench") => bench(&args[1..]),
        Some("one") => one(&args[1..]),
        Some("file") => file(&args[1..]),
        Some("hostile-deadline") => {
            let rest: Vec<String> = std::iter::once("hostile".to_string()).chain(args[1..].iter().cloned()).collect();
            common::run_with("merman-patched-deadline", render_deadline, &rest);
        }
        _ => common::run_with(&format!("merman-patched-{}", font_set()), render, &args),
    }
}
