//! mdview-engine: lazy render process. Pipes only. Exits after 10 s idle or on stdin EOF.
mod proto;
use proto::*;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(10);
const MAX_DIM: u32 = 8192;
const MAX_PIXELS: u64 = 16_000_000; // 64 MB of BGRA
const PAD_PX: f64 = 2.0;

type Highlighter = (syntect::parsing::SyntaxSet, syntect::highlighting::Theme);

struct State {
    start: Instant,
    highlight: Option<Highlighter>,
}

/// Reply kind, then body. Pixels stay in their own buffer and are never copied in this process.
struct Out(u8, Vec<u8>, Vec<u8>);

fn us(d: Duration) -> u32 {
    d.as_micros().min(u32::MAX as u128) as u32
}

fn math(p: &[u8], m: &mut Metrics) -> Result<Out, String> {
    if p.len() < 5 {
        return Err("malformed math job".into());
    }
    let display = p[0] != 0;
    let em = f32::from_le_bytes([p[1], p[2], p[3], p[4]]) as f64;
    if !(1.0..=256.0).contains(&em) {
        return Err(format!("font size {em} outside 1..=256"));
    }
    let tex_bytes = &p[5..];
    if tex_bytes.len() > MAX_TEX_BYTES {
        return Err(format!("TeX input {} bytes exceeds {MAX_TEX_BYTES}", tex_bytes.len()));
    }
    let tex = std::str::from_utf8(tex_bytes).map_err(|_| "TeX is not UTF-8".to_string())?;

    let t = Instant::now();
    let ast = ratex_parser::parser::parse(tex).map_err(|e| format!("parse error: {e}"))?;
    m.parse_us = us(t.elapsed());

    let t = Instant::now();
    let style = if display {
        ratex_types::math_style::MathStyle::Display
    } else {
        ratex_types::math_style::MathStyle::Text
    };
    let opts = ratex_layout::LayoutOptions::default().with_style(style);
    let lbox = ratex_layout::layout(&ast, &opts);
    ratex_layout::validate_layout(&lbox).map_err(|e| format!("layout rejected: {e}"))?;
    let list = ratex_layout::to_display_list(&lbox);
    m.layout_us = us(t.elapsed());

    // Reject before any SVG or raster work, so a huge \rule costs nothing.
    let wf = list.width * em + 2.0 * PAD_PX;
    let hf = (list.height + list.depth) * em + 2.0 * PAD_PX;
    if !wf.is_finite() || !hf.is_finite() || wf < 0.0 || hf < 0.0 {
        return Err(format!("non-finite formula size {wf} x {hf}"));
    }
    if wf > MAX_DIM as f64 || hf > MAX_DIM as f64 || wf.ceil() * hf.ceil() > MAX_PIXELS as f64 {
        return Err(format!("formula size {wf:.0} x {hf:.0} px exceeds raster cap"));
    }

    let t = Instant::now();
    let svg = ratex_svg::render_to_svg(
        &list,
        &ratex_svg::SvgOptions {
            font_size: em,
            padding: PAD_PX,
            stroke_width: 1.5,
            embed_glyphs: true,
            font_dir: String::new(),
        },
    );
    m.svg_us = us(t.elapsed());
    // A <text> element means a glyph did not embed. resvg has no font database here.
    if svg.contains("<text") {
        return Err("glyph could not be embedded".into());
    }

    let t = Instant::now();
    // ratex-svg writes width/height in pt. At 72 dpi one pt is one pixel, matching em_px.
    let usvg_opts = resvg::usvg::Options { dpi: 72.0, ..Default::default() };
    let tree = resvg::usvg::Tree::from_str(&svg, &usvg_opts)
        .map_err(|e| format!("usvg: {e}"))?;
    let size = tree.size().to_int_size();
    let (w, h) = (size.width(), size.height());
    if w > MAX_DIM || h > MAX_DIM || (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(format!("raster {w} x {h} exceeds cap"));
    }
    let mut pm = resvg::tiny_skia::Pixmap::new(w, h).ok_or("pixmap allocation failed")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pm.as_mut());
    let mut px = pm.take();
    for c in px.chunks_exact_mut(4) {
        c.swap(0, 2); // RGBA premultiplied to BGRA premultiplied
    }
    m.raster_us = us(t.elapsed());

    let baseline = (list.height * em + PAD_PX).round() as u32;
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&baseline.to_le_bytes());
    Ok(Out(REP_PIXELS, out, px))
}


fn highlight(p: &[u8], st: &mut State, m: &mut Metrics) -> Result<Out, String> {
    use syntect::easy::HighlightLines;
    use syntect::util::LinesWithEndings;
    if p.len() < 2 {
        return Err("malformed highlight job".into());
    }
    let n = u16::from_le_bytes([p[0], p[1]]) as usize;
    if p.len() < 2 + n {
        return Err("malformed highlight job".into());
    }
    let lang = std::str::from_utf8(&p[2..2 + n]).map_err(|_| "lang not UTF-8")?;
    let code = std::str::from_utf8(&p[2 + n..]).map_err(|_| "code not UTF-8")?;

    let t = Instant::now();
    if st.highlight.is_none() {
        let ss = two_face::syntax::extra_newlines();
        let theme = two_face::theme::extra()
            .get(two_face::theme::EmbeddedThemeName::Nord)
            .clone();
        st.highlight = Some((ss, theme));
    }
    m.load_us = us(t.elapsed());
    let (ss, theme) = st.highlight.as_ref().unwrap();
    let syntax = ss
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let t = Instant::now();
    let mut hl = HighlightLines::new(syntax, theme);
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for line in LinesWithEndings::from(code) {
        for (style, text) in hl.highlight_line(line, ss).map_err(|e| format!("syntect: {e}"))? {
            let c = style.foreground;
            let rgba = u32::from_le_bytes([c.r, c.g, c.b, c.a]);
            match runs.last_mut() {
                Some(l) if l.1 == rgba => l.0 += text.len() as u32,
                _ => runs.push((text.len() as u32, rgba)),
            }
        }
    }
    m.parse_us = us(t.elapsed());
    let mut out = Vec::with_capacity(4 + runs.len() * 8);
    out.extend_from_slice(&(runs.len() as u32).to_le_bytes());
    for (l, c) in runs {
        out.extend_from_slice(&l.to_le_bytes());
        out.extend_from_slice(&c.to_le_bytes());
    }
    Ok(Out(REP_SPANS, out, Vec::new()))
}

#[cfg(feature = "test-jobs")]
fn test_job(kind: u8, p: &[u8]) -> Result<Out, String> {
    match kind {
        REQ_SLEEP => {
            std::thread::sleep(Duration::from_millis(u32_at(p, 0) as u64));
            Err("slept".into())
        }
        REQ_BOMB_TOUCH => {
            // Touches every page, so RSS really grows until allocation fails and Rust aborts.
            let mut held: Vec<Vec<u8>> = Vec::new();
            loop {
                held.push(vec![0xA5u8; 1 << 20]);
                std::hint::black_box(&held);
            }
        }
        REQ_BOMB_HUGE => {
            let v: Vec<u8> = Vec::with_capacity(1 << 40);
            std::hint::black_box(&v);
            Err("1 TiB allocation unexpectedly succeeded".into())
        }
        REQ_PANIC => panic!("test panic job"),
        REQ_STACK => {
            #[allow(unconditional_recursion)]
            fn rec(n: u64) -> u64 {
                let a = [n; 64];
                std::hint::black_box(&a);
                rec(n + 1) + a[(n % 64) as usize]
            }
            Err(format!("{}", rec(0)))
        }
        _ => Err(format!("unknown job kind {kind}")),
    }
}

#[cfg(not(feature = "test-jobs"))]
fn test_job(kind: u8, _p: &[u8]) -> Result<Out, String> {
    Err(format!("unknown job kind {kind}"))
}

fn vm_status(m: &mut Metrics) {
    #[cfg(target_os = "linux")]
    if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
        for l in s.lines() {
            let v = || l.split_whitespace().nth(1).and_then(|x| x.parse().ok()).unwrap_or(0);
            if l.starts_with("VmHWM:") {
                m.vm_hwm_kb = v();
            } else if l.starts_with("VmRSS:") {
                m.vm_rss_kb = v();
            } else if l.starts_with("VmPeak:") {
                m.vm_peak_kb = v();
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = m;
}

fn run_job(f: Frame, st: &mut State, stdout: &mut impl Write) {
    let t0 = Instant::now();
    let mut m = Metrics::default();
    let res = if let Some(n) = f.oversized {
        Err(format!("request frame {n} bytes exceeds {MAX_REQUEST_FRAME}"))
    } else {
        catch_unwind(AssertUnwindSafe(|| match f.kind {
            REQ_MATH => math(&f.payload, &mut m),
            REQ_HIGHLIGHT => highlight(&f.payload, st, &mut m),
            k => test_job(k, &f.payload),
        }))
        .unwrap_or_else(|e| {
            let msg = e
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown".into());
            Err(format!("engine panic: {msg}"))
        })
    };
    m.total_us = us(t0.elapsed());
    m.uptime_us = us(st.start.elapsed());
    vm_status(&mut m);
    let Out(kind, head, px) = res.unwrap_or_else(|e| Out(REP_ERROR, e.into_bytes(), Vec::new()));
    let mut hdr = Vec::with_capacity(64 + head.len());
    hdr.extend_from_slice(&((5 + N_METRICS * 4 + head.len() + px.len()) as u32).to_le_bytes());
    hdr.push(kind);
    hdr.extend_from_slice(&f.id.to_le_bytes());
    hdr.extend_from_slice(&m.encode());
    hdr.extend_from_slice(&head);
    if write_all_flush(stdout, &hdr).is_err() || write_all_flush(stdout, &px).is_err() {
        std::process::exit(3);
    }
    drop(px);
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
}

fn main() {
    let start = Instant::now();
    std::panic::set_hook(Box::new(|_| {})); // Message travels in the error reply.
    let (tx, rx) = mpsc::channel::<Option<Frame>>();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            match read_frame(&mut stdin, MAX_REQUEST_FRAME) {
                Ok(f) => {
                    if tx.send(Some(f)).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx.send(None);
                    return;
                }
            }
        }
    });
    // Explicit stack: Windows main threads get 1 MiB, Linux 8 MiB. Keep behaviour the same.
    let worker = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let mut st = State { start, highlight: None };
            let mut stdout = std::io::stdout().lock();
            loop {
                match rx.recv_timeout(IDLE) {
                    Ok(Some(f)) => run_job(f, &mut st, &mut stdout),
                    Ok(None) | Err(RecvTimeoutError::Disconnected) => std::process::exit(0),
                    Err(RecvTimeoutError::Timeout) => std::process::exit(0),
                }
            }
        })
        .expect("spawn worker");
    let _ = worker.join();
}
