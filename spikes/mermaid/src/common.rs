//! Shared harness: fixtures, resvg rasterisation, measurement, hostile cases.
#![allow(dead_code)]
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use std::sync::Arc;
use std::time::Instant;

pub use mermaid_spike::corpus::{showcase, CORE, FAMILIES};

/// Painted fonts for "embedded" mode: Liberation Sans, or Segoe UI with MERMAID_FONT=segoe (the
/// patched harness defaults to that).
fn embedded_font_files() -> (String, String, &'static str) {
    let segoe = std::env::var("MERMAID_FONT").map_or(false, |v| v == "segoe");
    if segoe {
        let d = if std::path::Path::new("C:/Windows/Fonts/segoeui.ttf").exists() {
            "C:/Windows/Fonts"
        } else {
            "/usr/local/share/fonts/WindowsFonts"
        };
        (format!("{d}/segoeui.ttf"), format!("{d}/segoeuib.ttf"), "Segoe UI")
    } else {
        let d = "/usr/share/fonts/liberation";
        (format!("{d}/LiberationSans-Regular.ttf"), format!("{d}/LiberationSans-Bold.ttf"), "Liberation Sans")
    }
}

pub fn fonts(mode: &str) -> usvg::Options<'static> {
    let mut o = usvg::Options::default();
    let db = o.fontdb_mut();
    match mode {
        // "embedded" stand-in: one known font file, as an app would ship via load_font_data.
        "embedded" => {
            let (reg, bold, family) = embedded_font_files();
            db.load_font_data(std::fs::read(reg).unwrap());
            db.load_font_data(std::fs::read(bold).unwrap());
            db.set_sans_serif_family(family);
            o.font_family = family.to_string();
            return o;
        }
        _ => db.load_system_fonts(),
    }
    db.set_sans_serif_family("Liberation Sans");
    o.font_family = "Liberation Sans".to_string();
    o
}

pub fn raster(svg: &str, opt: &usvg::Options, scale: f32) -> Result<Vec<u8>, String> {
    let tree = usvg::Tree::from_str(svg, opt).map_err(|e| e.to_string())?;
    let sz = tree.size();
    let (w, h) = ((sz.width() * scale).ceil() as u32, (sz.height() * scale).ceil() as u32);
    if (w as u64) * (h as u64) > 16_000_000 {
        return Err(format!("raster over pixel cap: {w}x{h}"));
    }
    let mut pm = Pixmap::new(w, h).ok_or(format!("pixmap alloc failed: {w}x{h}"))?;
    pm.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(&tree, Transform::from_scale(scale, scale), &mut pm.as_mut());
    pm.encode_png().map_err(|e| e.to_string())
}

pub fn vm_hwm_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find(|l| l.starts_with("VmHWM")).and_then(|l| {
                l.split_whitespace().nth(1).and_then(|v| v.parse().ok())
            })
        })
        .unwrap_or(0)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

pub type RenderFn = fn(&str) -> Result<String, String>;

fn bench(name: &str, render: RenderFn, args: &[String]) {
    let iters: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(30);
    let mode = args.get(1).map(String::as_str).unwrap_or("embedded");
    let t = Instant::now();
    let opt = Arc::new(fonts(mode));
    println!("# renderer={name} fonts={mode} font_load_ms={:.2} iters={iters}", ms(t));
    println!("set\tfamily\tcold_svg_ms\tcold_png_ms\tmed_svg_ms\tmax_svg_ms\tmed_png_ms\tmax_png_ms\tmed_total_ms\tsvg_bytes\tpng_bytes");
    let show = showcase();
    let core: Vec<String> = CORE.iter().map(|s| s.to_string()).collect();
    for (set, srcs, suffix) in [("showcase", &show, ""), ("core", &core, "-core")] {
        for (i, fam) in FAMILIES.iter().enumerate() {
            let src = &srcs[i];
            let t = Instant::now();
            let svg = match render(src) {
                Ok(s) => s,
                Err(e) => {
                    println!("{set}\t{fam}\tERROR\t{e}");
                    continue;
                }
            };
            let cold_svg = ms(t);
            let t = Instant::now();
            let png = match raster(&svg, &opt, 2.0) {
                Ok(p) => p,
                Err(e) => {
                    println!("{set}\t{fam}\tRASTER-ERROR\t{e}");
                    continue;
                }
            };
            let cold_png = ms(t);
            let (mut sv, mut pv, mut tv) = (vec![], vec![], vec![]);
            for _ in 0..iters {
                let t = Instant::now();
                let s = render(src).unwrap();
                let a = ms(t);
                let t = Instant::now();
                let _ = raster(&s, &opt, 2.0).unwrap();
                let b = ms(t);
                sv.push(a);
                pv.push(b);
                tv.push(a + b);
            }
            let (maxs, maxp) = (
                sv.iter().cloned().fold(0., f64::max),
                pv.iter().cloned().fold(0., f64::max),
            );
            std::fs::write(format!("out/{name}-{fam}{suffix}.png"), &png).unwrap();
            std::fs::write(format!("out/{name}-{fam}{suffix}.svg"), &svg).unwrap();
            println!(
                "{set}\t{fam}\t{cold_svg:.2}\t{cold_png:.2}\t{:.2}\t{maxs:.2}\t{:.2}\t{maxp:.2}\t{:.2}\t{}\t{}",
                median(&mut sv), median(&mut pv), median(&mut tv), svg.len(), png.len()
            );
        }
    }
    println!("# VmHWM_kb={}", vm_hwm_kb());
}

/// One showcase diagram end to end in a fresh process, for cold time and peak RSS.
fn one(render: RenderFn, args: &[String]) {
    let fam = args.first().map(String::as_str).unwrap_or("flowchart");
    let mode = args.get(1).map(String::as_str).unwrap_or("embedded");
    let i = FAMILIES.iter().position(|f| *f == fam).expect("family");
    let src = &showcase()[i];
    let t0 = Instant::now();
    let opt = fonts(mode);
    let f = ms(t0);
    let t = Instant::now();
    let svg = render(src).unwrap();
    let s = ms(t);
    let t = Instant::now();
    let png = raster(&svg, &opt, 2.0).unwrap();
    let p = ms(t);
    println!("{fam}\tfont_ms={f:.2}\tsvg_ms={s:.2}\tpng_ms={p:.2}\ttotal_ms={:.2}\tpng_bytes={}\tVmHWM_kb={}", ms(t0), png.len(), vm_hwm_kb());
}

pub fn hostile_cases() -> Vec<(&'static str, String)> {
    let mut v: Vec<(&'static str, String)> = vec![
        ("empty", String::new()),
        ("prose", "hello world, not a diagram".into()),
        ("garbage", "@@@ %%{ ]]] <<< >>> \u{0}\u{1} ((((".into()),
        ("bad_header", "flowchartt LR\n A-->B".into()),
        ("flow_unterminated", "flowchart LR\n A[Start --> B[".into()),
        ("flow_bad_arrow", "flowchart LR\n A -->> --> ==> \n ]]]".into()),
        ("seq_bad", "sequenceDiagram\n Alice->>\n ->>Bob\n end end end".into()),
        ("class_bad", "classDiagram\n <|-- <|-- \n class {{{".into()),
        ("state_bad", "stateDiagram-v2\n --> --> \n state {".into()),
        ("er_bad", "erDiagram\n A ||--o{ : \n {{{".into()),
        ("gantt_bad_date", "gantt\n dateFormat YYYY-MM-DD\n section S\n T :a, 9999-99-99, -3d".into()),
        ("gantt_cycle", "gantt\n dateFormat YYYY-MM-DD\n section S\n T :a, after b, 1d\n U :b, after a, 1d".into()),
        ("pie_nonnum", "pie\n \"A\" : abc\n \"B\" : -5".into()),
        ("pie_zero", "pie\n \"A\" : 0".into()),
        ("mindmap_bad", "mindmap\n\n ((((\n   ))))".into()),
        ("init_bad_json", "%%{init: {bad json}}%%\nflowchart LR\n A-->B".into()),
    ];
    // Over merman's interactive limits (2 MiB source, 32k items, 256 depth).
    let mut s = String::from("flowchart LR\n");
    let mut i = 0;
    while s.len() < 3 * 1024 * 1024 {
        s.push_str(&format!(" n{i}[L] --> n{}[L]\n", i + 1));
        i += 1;
    }
    v.push(("over_source_3MiB", s));
    let mut s = String::from("flowchart LR\n");
    for i in 0..40_000 {
        s.push_str(&format!(" n{i} --> n{}\n", i + 1));
    }
    v.push(("over_items_40k_edges", s));
    let mut s = String::from("flowchart LR\n");
    for i in 0..2000 {
        s.push_str(&format!("subgraph s{i}\n"));
    }
    s.push_str("A-->B\n");
    for _ in 0..2000 {
        s.push_str("end\n");
    }
    v.push(("over_depth_2000_subgraphs", s));
    let mut s = String::from("mindmap\n root\n");
    for i in 1..2000 {
        s.push_str(&" ".repeat(i + 1));
        s.push_str("x\n");
    }
    v.push(("over_depth_2000_mindmap", s));
    v.push(("over_label_1p5MiB", format!("flowchart LR\n A[{}] --> B", "x".repeat(3 * 512 * 1024))));
    // Where does the long-label cost start? Same shape at growing sizes, with and without spaces.
    for (name, n) in [("label_16k", 16usize), ("label_64k", 64), ("label_256k", 256)] {
        v.push((name, format!("flowchart LR\n A[{}] --> B", "x".repeat(n * 1024))));
    }
    for (name, n) in [("words_16k", 16usize), ("words_64k", 64), ("words_256k", 256)] {
        v.push((name, format!("flowchart LR\n A[{}] --> B", "word ".repeat(n * 1024 / 5))));
    }
    v.push(("label_multiline_64k", format!("flowchart LR\n A[\"{}\"] --> B", "forty characters of text on a line here\n".repeat(1600))));
    let mut s = String::from("pie\n");
    for i in 0..40_000 {
        s.push_str(&format!(" \"s{i}\" : 1\n"));
    }
    v.push(("over_pie_40k", s));
    let mut s = String::from("sequenceDiagram\n");
    for i in 0..40_000 {
        s.push_str(&format!(" A{}->>A{}: m\n", i % 50, (i + 1) % 50));
    }
    v.push(("over_seq_40k", s));
    // Not over any limit, but big enough to expose layout cost.
    let mut s = String::from("flowchart TD\n");
    for i in 0..2000 {
        s.push_str(&format!(" n{i} --> n{}\n", i + 1));
    }
    v.push(("big_flow_2000", s));
    v
}

fn hostile(render: RenderFn, args: &[String]) {
    let cases = hostile_cases();
    let Some(name) = args.first() else {
        for (n, s) in &cases {
            println!("{n}\t{}", s.len());
        }
        return;
    };
    let (_, src) = cases.iter().find(|(n, _)| n == name).expect("case");
    std::panic::set_hook(Box::new(|_| {}));
    let t = Instant::now();
    let r = std::panic::catch_unwind(|| render(src));
    let el = ms(t);
    let res = match r {
        Ok(Ok(svg)) => {
            let opt = fonts("embedded");
            let t2 = Instant::now();
            let pr = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| raster(&svg, &opt, 1.0)));
            let pms = ms(t2);
            let ps = match pr {
                Ok(Ok(p)) => format!("png_ok {} bytes", p.len()),
                Ok(Err(e)) => format!("png_err {}", trunc(&e)),
                Err(_) => "PNG_PANIC".to_string(),
            };
            format!("OK svg_bytes={} | {ps} {pms:.1}ms", svg.len())
        }
        Ok(Err(e)) => format!("ERR {}", trunc(&e)),
        Err(p) => {
            let m = p.downcast_ref::<String>().cloned().or(p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            format!("PANIC {}", trunc(&m))
        }
    };
    println!("{name}\t{}\t{el:.1}ms\tVmHWM_kb={}\t{res}", src.len(), vm_hwm_kb());
}

fn trunc(s: &str) -> String {
    let s = s.replace(['\n', '\t'], " ");
    s.chars().take(110).collect()
}

pub fn run(name: &str, render: RenderFn) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run_with(name, render, &args);
}

pub fn run_with(name: &str, render: RenderFn, args: &[String]) {
    let rest = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        Some("one") => one(render, rest),
        Some("hostile") => hostile(render, rest),
        Some("bench") | None => bench(name, render, rest),
        Some(o) => panic!("unknown mode {o}"),
    }
}
