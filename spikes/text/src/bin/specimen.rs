//! G3 specimen: `specimen render RAW` writes RAW/rust_*.png,
//! `specimen compose RAW OUT` joins them with RAW/qt_*.png into OUT/*.png.
use parley::{Alignment, AlignmentOptions, FontContext, FontFamily, Layout, LayoutContext, StyleProperty};
use text_spike::spec::*;
use text_spike::*;

fn lay(fc: &mut FontContext, lc: &mut LayoutContext<Brush>, t: &str, fam: &str, px: f32, sc: f32, c: [u8; 3]) -> Layout<Brush> {
    let list = if std::env::var("CURATED").is_ok() {
        format!("{fam}, Noto Color Emoji, sans-serif")
    } else {
        format!("{fam}, sans-serif")
    };
    let mut b = lc.ranged_builder(fc, t, sc, true);
    b.push_default(StyleProperty::FontFamily(FontFamily::from(list.as_str())));
    b.push_default(StyleProperty::FontSize(px));
    b.push_default(StyleProperty::Brush([c[0], c[1], c[2], 255]));
    let mut l = b.build(t);
    l.break_all_lines(None);
    l.align(Alignment::Start, AlignmentOptions::default());
    l
}

fn variant() -> &'static str {
    if std::env::var("CURATED").is_ok() { "curated" } else { "default" }
}

fn render(dir: &str) {
    let mut fc = FontContext::new();
    let mut lc = LayoutContext::<Brush>::new();
    for &(pct, s) in SCALES {
        for &(name, bg, fg) in THEMES {
            let mut r = Renderer::new(64 << 20);
            let mut fb = Fb::new((W * s).ceil() as usize, (H * s).ceil() as usize);
            fb.clear(bg);
            for &(mono, px, y, t) in LINES {
                let l = lay(&mut fc, &mut lc, t, if mono { MONO } else { BODY }, px, s, fg);
                let base = l.get(0).map(|ln| ln.metrics().baseline).unwrap_or(0.0);
                r.draw_layout(&mut fb, &l, (MARGIN * s).round(), (y * s).round() - base);
                if pct == 100 && name == "light" {
                    // Report which fonts fallback actually chose.
                    let mut fonts = Vec::new();
                    for ln in l.lines() {
                        for run in ln.runs() {
                            let f = run.font();
                            let fr = swash::FontRef::from_index(f.data.as_ref(), f.index as usize).unwrap();
                            let n = fr
                                .localized_strings()
                                .find_by_id(swash::StringId::Full, None)
                                .map(|s| s.to_string())
                                .unwrap_or_default();
                            if !fonts.contains(&n) {
                                fonts.push(n);
                            }
                        }
                    }
                    println!("{:?} -> {:?}", t.chars().take(14).collect::<String>(), fonts);
                }
            }
            let v = variant();
            fb.write_png(&format!("{dir}/rust_{v}_{name}_{pct}.png"));
        }
    }
}

fn read_png(p: &str) -> (usize, usize, Vec<u8>) {
    let d = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(p).unwrap()));
    let mut rd = d.read_info().unwrap();
    let mut buf = vec![0; rd.output_buffer_size().unwrap()];
    let i = rd.next_frame(&mut buf).unwrap();
    let n = i.color_type.samples();
    assert_eq!(i.bit_depth, png::BitDepth::Eight);
    let mut bgra = Vec::with_capacity(i.width as usize * i.height as usize * 4);
    for row in buf.chunks(i.line_size).take(i.height as usize) {
        for c in row.chunks_exact(n) {
            bgra.extend_from_slice(&[c[2], c[1], c[0], 255]);
        }
    }
    (i.width as usize, i.height as usize, bgra)
}

fn compose(raw: &str, out: &str) {
    let mut fc = FontContext::new();
    let mut lc = LayoutContext::<Brush>::new();
    let mut r = Renderer::new(64 << 20);
    for &(pct, _) in SCALES {
        for &(name, _, _) in THEMES {
            let v = variant();
            let a = read_png(&format!("{raw}/rust_{v}_{name}_{pct}.png"));
            let b = read_png(&format!("{raw}/qt_{name}_{pct}.png"));
            let (gap, bar) = (12, 34);
            let (w, h) = (a.0 + gap + b.0, a.1.max(b.1) + bar);
            let mut fb = Fb::new(w, h);
            fb.clear([70, 70, 70]);
            for (img, xo) in [(&a, 0usize), (&b, a.0 + gap)] {
                for y in 0..img.1 {
                    let so = y * img.0 * 4;
                    let d = ((y + bar) * w + xo) * 4;
                    fb.px[d..d + img.0 * 4].copy_from_slice(&img.2[so..so + img.0 * 4]);
                }
            }
            for (t, x) in [
                (format!("Rust: parley + swash, {v} fallback ({name}, {pct}%)"), 8.0),
                (format!("Qt: QPainter in QWidget paintEvent ({name}, {pct}%)"), (a.0 + gap + 8) as f32),
            ] {
                let l = lay(&mut fc, &mut lc, &t, "Noto Sans", 15.0, 1.0, [255, 255, 255]);
                r.draw_layout(&mut fb, &l, x, 6.0);
            }
            fb.write_png(&format!("{out}/{v}_{name}_{pct}.png"));
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("render") => render(&a[2]),
        Some("compose") => compose(&a[2], &a[3]),
        _ => eprintln!("specimen render RAWDIR | compose RAWDIR OUTDIR"),
    }
}
