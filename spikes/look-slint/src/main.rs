//! mdview look spike: Slint 1.18, winit backend, software renderer. Safe Rust only.
#![deny(unsafe_code)]

slint::include_modules!();

mod kde;

use slint::winit_030::{winit, WinitWindowAccessor, EventResult};
use slint::{Color, ComponentHandle, Image, Model, ModelRc, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::fs::File;
use std::io::Write;
use std::rc::Rc;
use std::time::{Duration, Instant};

const SHARED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../look-shared");
const SCALES: [(f32, &str, usize, usize); 4] = [
    (1.0, "1.0", 780, 2400),
    (1.5, "1.5", 1170, 3600),
    (1.7, "1.7", 1326, 4080),
    (2.2, "2.2", 1716, 5280),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum DocMode { Viewport, Full }
#[derive(Clone, Copy, PartialEq, Debug)]
enum PixelFmt { Rgb, Rgba }

#[derive(Clone)]
struct Opts {
    theme: Option<bool>,
    measure: bool,
    screenshot: Option<String>,
    stay_ms: u64,
    scroll_px: i32,
    win: Option<(f32, f32)>,
    left_width: f32,
    right_width: f32,
    mode: DocMode,
    fmt: PixelFmt,
    checker: bool,
    naive: bool,
    stretch: bool,
    smooth: bool,
    no_snap: bool,
    hide_overlays: bool,
    open_find: bool,
    open_menu: Option<i32>,
    open_context: bool,
    open_about: bool,
    no_kde: bool,
    bench: usize,
    self_test: bool,
    bench_step: i32,
    log_state: bool,
}

fn parse_opts() -> Opts {
    let mut o = Opts {
        theme: None, measure: false, screenshot: None, stay_ms: 0, scroll_px: 0, win: None,
        left_width: 230.0, right_width: 210.0, mode: DocMode::Viewport, fmt: PixelFmt::Rgb,
        checker: false, naive: false, stretch: false, smooth: false, no_snap: false, hide_overlays: false,
        open_find: false, open_menu: None, open_context: false, open_about: false, no_kde: false,
        bench: 0, self_test: false, bench_step: 37, log_state: false,
    };
    for a in std::env::args().skip(1) {
        let (k, v) = a.split_once('=').unwrap_or((a.as_str(), ""));
        match k {
            "--theme" => o.theme = Some(v == "dark"),
            "--measure" => o.measure = true,
            "--screenshot" => o.screenshot = Some(v.to_string()),
            "--stay-ms" => o.stay_ms = v.parse().unwrap(),
            "--scroll-px" => o.scroll_px = v.parse().unwrap(),
            "--win" => {
                let (w, h) = v.split_once('x').unwrap();
                o.win = Some((w.parse().unwrap(), h.parse().unwrap()));
            }
            "--left-width" => o.left_width = v.parse().unwrap(),
            "--right-width" => o.right_width = v.parse().unwrap(),
            "--doc-mode" => o.mode = if v == "full" { DocMode::Full } else { DocMode::Viewport },
            "--pixel" => o.fmt = if v == "rgba" { PixelFmt::Rgba } else { PixelFmt::Rgb },
            "--checker" => { o.checker = true; o.hide_overlays = true; }
            "--naive" => o.naive = true,
            "--stretch1" => o.stretch = true,
            "--smooth" => o.smooth = true,
            "--no-snap" => o.no_snap = true,
            "--hide-overlays" => o.hide_overlays = true,
            "--find" => o.open_find = true,
            "--menu" => o.open_menu = Some(v.parse().unwrap_or(0)),
            "--context" => o.open_context = true,
            "--about" => o.open_about = true,
            "--no-kde" => o.no_kde = true,
            "--bench-scroll" => o.bench = v.parse().unwrap(),
            "--bench-step" => o.bench_step = v.parse().unwrap(),
            "--self-test" => o.self_test = true,
            "--log-state" => o.log_state = true,
            _ => eprintln!("unknown option {a}"),
        }
    }
    o
}

/// Milliseconds since process start. The harness passes CLOCK_MONOTONIC from just before
/// spawn in MDV_T0_NS. Without it, T0 comes from /proc starttime (10 ms resolution).
fn process_start() -> (Instant, f64, &'static str) {
    let now = Instant::now();
    let mono = monotonic_now_ns();
    if let Some(t0) = std::env::var("MDV_T0_NS").ok().and_then(|v| v.parse::<u64>().ok()) {
        return (now, mono.saturating_sub(t0) as f64 / 1e6, "harness-spawn");
    }
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after = stat.rsplit(')').next().unwrap_or("");
    let ticks: f64 = after.split_whitespace().nth(19).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    (now, (mono as f64 / 1e6 - ticks * 10.0).max(0.0), "proc-starttime-10ms")
}

#[cfg(unix)]
fn monotonic_now_ns() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}
#[cfg(not(unix))]
fn monotonic_now_ns() -> u64 { 0 }

#[cfg(unix)]
fn read_at(f: &File, buf: &mut [u8], off: u64) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(f, buf, off)
}
#[cfg(windows)]
fn read_at(f: &File, buf: &mut [u8], off: u64) -> std::io::Result<()> {
    let n = std::os::windows::fs::FileExt::seek_read(f, buf, off)?;
    if n == buf.len() { Ok(()) } else { Err(std::io::ErrorKind::UnexpectedEof.into()) }
}

fn private_kib() -> (u64, u64, u64) {
    let s = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let get = |k: &str| -> u64 {
        s.lines().find_map(|l| l.strip_prefix(k)?.trim().strip_suffix("kB")?.trim().parse().ok()).unwrap_or(0)
    };
    (get("Private_Clean:") + get("Private_Dirty:"), get("Pss:"), get("Rss:"))
}

fn out(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{line}");
    let _ = o.flush();
}

/// The pixels on screen. Rust owns the document surface; Slint gets one image per scroll.
struct Doc {
    opts: Opts,
    dark: bool,
    file: Option<File>,
    page_w: usize,
    page_h: usize,
    scale_loaded: f32,
    theme_loaded: bool,
    scroll: i32,
    target: i32,
    view: (i32, i32),
    drawn: (i32, i32, i32),
    /// Two viewport buffers, swapped so a scroll does not allocate.
    spare_rgb: Option<SharedPixelBuffer<Rgb8Pixel>>,
    cur_rgb: Option<SharedPixelBuffer<Rgb8Pixel>>,
    spare_rgba: Option<SharedPixelBuffer<Rgba8Pixel>>,
    cur_rgba: Option<SharedPixelBuffer<Rgba8Pixel>>,
    chunk: Vec<u8>,
    fill_us: u128,
}

impl Doc {
    fn load_page(&mut self, scale: f32) {
        let (s, name, w, h) = SCALES
            .iter()
            .copied()
            .min_by(|a, b| (a.0 - scale).abs().partial_cmp(&(b.0 - scale).abs()).unwrap())
            .unwrap();
        let theme = if self.dark { "dark" } else { "light" };
        let path = format!("{SHARED}/content-{theme}@{name}-{w}x{h}.bgra");
        self.file = File::open(&path).ok();
        if self.file.is_none() {
            eprintln!("cannot open {path}");
        }
        if (s - scale).abs() > 0.001 {
            eprintln!("no document file for scale {scale}; showing @{name} 1:1");
        }
        self.page_w = w;
        self.page_h = h;
        self.scale_loaded = scale;
        self.theme_loaded = self.dark;
        self.drawn = (-1, -1, -1);
        self.cur_rgb = None;
        self.spare_rgb = None;
        self.cur_rgba = None;
        self.spare_rgba = None;
    }

    fn max_scroll(&self) -> i32 {
        (self.page_h as i32 - self.view.1).max(0)
    }

    /// Copies source rows into `dst`, reading the page in small chunks so the transient
    /// buffer stays small. BGRA in the file, RGB or RGBA for Slint.
    fn read_rows(&mut self, x0: usize, y0: usize, w: usize, h: usize, dst: &mut [u8], bpp: usize) {
        const CHUNK_ROWS: usize = 64;
        let stride = self.page_w * 4;
        let Some(file) = self.file.as_ref() else { return };
        self.chunk.resize(stride * CHUNK_ROWS, 0);
        let mut y = 0;
        while y < h {
            let rows = (h - y).min(CHUNK_ROWS);
            let bytes = rows * stride;
            if read_at(file, &mut self.chunk[..bytes], ((y0 + y) * stride) as u64).is_err() {
                eprintln!("short read");
                return;
            }
            for r in 0..rows {
                let src = &self.chunk[r * stride + x0 * 4..][..w * 4];
                let d = &mut dst[(y + r) * w * bpp..][..w * bpp];
                if bpp == 3 {
                    for (o, i) in d.chunks_exact_mut(3).zip(src.chunks_exact(4)) {
                        o[0] = i[2];
                        o[1] = i[1];
                        o[2] = i[0];
                    }
                } else {
                    for (o, i) in d.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                        o[0] = i[2];
                        o[1] = i[1];
                        o[2] = i[0];
                        o[3] = i[3];
                    }
                }
            }
            y += rows;
        }
    }

    fn make_image(&mut self, w: u32, h: u32, top: usize, checker: bool) -> Image {
        let fill = |doc: &mut Doc, bytes: &mut [u8], bpp: usize| {
            if checker {
                for (i, px) in bytes.chunks_exact_mut(bpp).enumerate() {
                    let (x, y) = (i % w as usize, i / w as usize);
                    let c = if (x + y) % 2 == 0 { 0xff } else { 0x00 };
                    px[..3].fill(c);
                    if bpp == 4 { px[3] = 0xff; }
                }
            } else {
                doc.read_rows(0, top, w as usize, h as usize, bytes, bpp);
            }
        };
        match self.opts.fmt {
            PixelFmt::Rgb => {
                let mut b = self.spare_rgb.take().filter(|b| b.width() == w && b.height() == h)
                    .unwrap_or_else(|| SharedPixelBuffer::new(w, h));
                fill(self, b.make_mut_bytes(), 3);
                let img = Image::from_rgb8(b.clone());
                self.spare_rgb = self.cur_rgb.replace(b);
                img
            }
            PixelFmt::Rgba => {
                let mut b = self.spare_rgba.take().filter(|b| b.width() == w && b.height() == h)
                    .unwrap_or_else(|| SharedPixelBuffer::new(w, h));
                fill(self, b.make_mut_bytes(), 4);
                let img = Image::from_rgba8(b.clone());
                self.spare_rgba = self.cur_rgba.replace(b);
                img
            }
        }
    }

    /// Brings Slint's image in line with scale, theme, size and scroll. Runs just before each
    /// draw. Returns true if pixels were copied.
    fn sync(&mut self, app: &App) -> bool {
        let scale = app.window().scale_factor();
        if (scale - self.scale_loaded).abs() > 0.0001 || self.theme_loaded != self.dark {
            self.load_page(scale);
        }
        self.view = (app.get_dev_w(), app.get_dev_h());
        let (vw, vh) = self.view;
        if vw <= 0 || vh <= 0 {
            return false;
        }
        self.scroll = self.scroll.clamp(0, self.max_scroll());
        self.target = self.target.clamp(0, self.max_scroll());
        app.set_max_scroll(self.max_scroll());
        app.set_scroll(self.scroll);
        let t = Instant::now();
        let (key, w, h, top, img_top);
        if self.opts.naive {
            // Negative control: a buffer in logical px, stretched by the toolkit.
            let (lw, lh) = (app.get_logical_doc_w().round() as u32, app.get_logical_doc_h().round() as u32);
            (w, h, top, img_top) = (lw, lh, 0, 0);
            key = (lw as i32, lh as i32, 0);
        } else if self.opts.checker {
            (w, h, top, img_top) = (vw as u32, vh as u32, 0, 0);
            key = (vw, vh, 0);
        } else if self.opts.mode == DocMode::Viewport {
            let iw = (self.page_w as i32).min(vw);
            let ih = (self.page_h as i32 - self.scroll).min(vh).max(1);
            (w, h, top, img_top) = (iw as u32, ih as u32, self.scroll as usize, 0);
            key = (iw, ih, self.scroll);
        } else {
            (w, h, top, img_top) = (self.page_w as u32, self.page_h as u32, 0, -self.scroll);
            key = (self.page_w as i32, self.page_h as i32, 0);
        }
        app.set_img_top(img_top);
        app.set_img_w(w as i32);
        app.set_img_h(h as i32);
        if key == self.drawn {
            return false;
        }
        self.drawn = key;
        let img = self.make_image(w, h, top, self.opts.checker || self.opts.naive);
        app.set_doc_image(img);
        self.fill_us = t.elapsed().as_micros();
        true
    }
}

// Heading tops in logical px, read by eye from the 1.0 page. A real viewer gets these from layout.
const OUTLINE: [(&str, i32, f32); 10] = [
    ("Native Markdown Reader", 1, 24.0),
    ("GitHub Markdown", 2, 320.0),
    ("HTML and images", 2, 1118.0),
    ("Mathematics", 2, 1265.0),
    ("Diagrams", 2, 1475.0),
    ("Flowchart", 3, 1528.0),
    ("Sequence", 3, 1675.0),
    ("State", 3, 1866.0),
    ("Repeated heading", 2, 2047.0),
    ("Repeated heading", 2, 2153.0),
];

struct Node { id: i32, label: &'static str, depth: i32, dir: bool }
const TREE: [Node; 13] = [
    Node { id: 0, label: "markdown_viewer", depth: 0, dir: true },
    Node { id: 1, label: "docs", depth: 1, dir: true },
    Node { id: 2, label: "plan.md", depth: 2, dir: false },
    Node { id: 3, label: "fixtures", depth: 1, dir: true },
    Node { id: 4, label: "assets", depth: 2, dir: true },
    Node { id: 5, label: "logo.png", depth: 3, dir: false },
    Node { id: 6, label: "showcase.md", depth: 2, dir: false },
    Node { id: 7, label: "linked.md", depth: 2, dir: false },
    Node { id: 8, label: "Türkçe test.md", depth: 2, dir: false },
    Node { id: 9, label: "spikes", depth: 1, dir: true },
    Node { id: 10, label: "RESULT.md", depth: 2, dir: false },
    Node { id: 11, label: "CHANGELOG.md", depth: 1, dir: false },
    Node { id: 12, label: "README.md", depth: 1, dir: false },
];

fn visible_rows(expanded: &[bool]) -> Vec<TreeRow> {
    let mut rows = vec![];
    let mut hide_below = i32::MAX;
    for n in &TREE {
        if n.depth > hide_below { continue; }
        hide_below = i32::MAX;
        let open = n.dir && expanded[n.id as usize];
        if n.dir && !open { hide_below = n.depth; }
        rows.push(TreeRow { id: n.id, label: n.label.into(), depth: n.depth, expandable: n.dir, expanded: open });
    }
    rows
}

fn c(rgb: (u8, u8, u8)) -> Color { Color::from_rgb_u8(rgb.0, rgb.1, rgb.2) }
fn hex(v: u32) -> Color { Color::from_argb_encoded(0xff000000 | v) }

fn apply_theme(app: &App, dark: bool, kde: &Option<kde::KdeColors>) {
    let t = app.global::<Theme>();
    let matched = kde.as_ref().filter(|k| k.dark == dark);
    t.set_dark(dark);
    // The page colours come from the shared document, so the shell blends with it.
    t.set_page(if dark { hex(0x202020) } else { hex(0xFBFBFB) });
    let accent = kde.as_ref().map(|k| k.accent).unwrap_or(if dark { (0x60, 0xCD, 0xFF) } else { (0x00, 0x5F, 0xB8) });
    t.set_accent(c(accent));
    t.set_accent_text(c(kde::readable_accent(accent, dark)));
    t.set_on_accent(if kde::luminance(accent) > 0.5 { hex(0) } else { hex(0xffffff) });
    if dark {
        t.set_window(hex(0x2B2B2B));
        t.set_text(hex(0xE4E4E4));
        t.set_text_dim(hex(0xA8A8A8));
        t.set_hover(Color::from_argb_encoded(0x14ffffff));
        t.set_pressed(Color::from_argb_encoded(0x0fffffff));
        t.set_hairline(hex(0x3D3D3D));
        t.set_popup(hex(0x2F2F2F));
        t.set_shadow(Color::from_argb_encoded(0x80000000));
    } else {
        t.set_window(hex(0xF3F3F3));
        t.set_text(hex(0x242424));
        t.set_text_dim(hex(0x616161));
        t.set_hover(Color::from_argb_encoded(0x0f000000));
        t.set_pressed(Color::from_argb_encoded(0x08000000));
        t.set_hairline(hex(0xE0E0E0));
        t.set_popup(hex(0xFFFFFF));
        t.set_shadow(Color::from_argb_encoded(0x38000000));
    }
    if let Some(k) = matched {
        t.set_window(c(k.window));
        t.set_text(c(k.text));
        t.set_text_dim(c(k.text_dim));
        t.set_hairline(c(k.hairline));
        t.set_popup(c(k.popup));
    }
    if let Some(f) = kde.as_ref().and_then(|k| k.font.clone()) {
        t.set_font(f.into());
    }
}

fn save_png(path: &str, b: &SharedPixelBuffer<Rgba8Pixel>) {
    let f = std::io::BufWriter::new(File::create(path).expect("create screenshot"));
    let mut e = png::Encoder::new(f, b.width(), b.height());
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    let mut w = e.write_header().unwrap();
    w.write_image_data(b.as_bytes()).unwrap();
}

#[derive(Default)]
struct Bench {
    frames: usize,
    start: Option<Instant>,
    last_redraw: Option<Instant>,
    fill: Vec<f64>,
    render: Vec<f64>,
    interval: Vec<f64>,
    dir: i32,
}

fn summary(name: &str, v: &mut Vec<f64>) {
    if v.is_empty() { return; }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() - 1) as f64 * p) as usize];
    out(&format!("BENCH {name}: n={} median={:.2} p95={:.2} max={:.2} min={:.2}", v.len(), q(0.5), q(0.95), q(1.0), q(0.0)));
}

fn main() {
    let (t_start, ms_at_entry, t0_src) = process_start();
    let opts = parse_opts();
    let k = if opts.no_kde { None } else { kde::load() };
    let app = App::new().unwrap();
    let app_new_ms = ms_at_entry + t_start.elapsed().as_secs_f64() * 1e3;

    let dark = Rc::new(Cell::new(opts.theme.unwrap_or(k.as_ref().map(|k| k.dark).unwrap_or(false))));
    let theme_explicit = opts.theme.is_some() || k.is_some();
    apply_theme(&app, dark.get(), &k);

    if let Some((w, h)) = opts.win {
        app.window().set_size(slint::LogicalSize::new(w, h));
    }
    app.set_left_width(opts.left_width);
    app.set_right_width(opts.right_width);
    app.set_snap(!opts.no_snap);
    app.set_smooth(opts.smooth || opts.naive);
    app.set_naive(opts.naive);
    if opts.stretch { app.set_extra_w(1); }
    app.set_hide_overlays(opts.hide_overlays);

    let tabs = Rc::new(VecModel::from(vec![
        TabData { title: "showcase.md".into(), path: "fixtures/showcase.md".into() },
        TabData { title: "linked.md".into(), path: "fixtures/linked.md".into() },
        TabData { title: "Türkçe test.md".into(), path: "fixtures/Türkçe test.md".into() },
    ]));
    app.set_tabs(ModelRc::from(tabs.clone()));

    let expanded = Rc::new(RefCell::new(vec![true, false, false, true, false, false, false, false, false, false, false, false, false]));
    let rows = Rc::new(VecModel::from(visible_rows(&expanded.borrow())));
    app.set_tree_rows(ModelRc::from(rows.clone()));
    app.set_tree_selected(6);

    let outline: Vec<OutlineRow> = OUTLINE.iter().map(|o| OutlineRow { label: o.0.into(), level: o.1 }).collect();
    app.set_outline_rows(ModelRc::new(VecModel::from(outline)));

    let doc = Rc::new(RefCell::new(Doc {
        opts: opts.clone(), dark: dark.get(), file: None, page_w: 0, page_h: 0, scale_loaded: 0.0, theme_loaded: dark.get(),
        scroll: opts.scroll_px, target: opts.scroll_px, view: (0, 0), drawn: (-1, -1, -1),
        spare_rgb: None, cur_rgb: None, spare_rgba: None, cur_rgba: None, chunk: Vec::new(), fill_us: 0,
    }));

    // Callbacks.
    let weak = app.as_weak();
    {
        let (r, e, w) = (rows.clone(), expanded.clone(), weak.clone());
        app.on_tree_toggle(move |id| {
            let mut ex = e.borrow_mut();
            ex[id as usize] = !ex[id as usize];
            r.set_vec(visible_rows(&ex));
            let _ = &w;
        });
        let w = weak.clone();
        let t = tabs.clone();
        app.on_tree_activate(move |id| {
            let Some(a) = w.upgrade() else { return };
            let n = &TREE[id as usize];
            if n.dir { return; }
            a.set_tree_selected(id);
            if let Some(i) = t.iter().position(|d| d.title == n.label) {
                a.set_current_tab(i as i32);
            }
        });
    }
    {
        let w = weak.clone();
        app.on_tab_select(move |i| {
            let Some(a) = w.upgrade() else { return };
            a.set_current_tab(i);
            sync_tree_to_tab(&a);
        });
        let w = weak.clone();
        let t = tabs.clone();
        app.on_tab_close(move |i| {
            let Some(a) = w.upgrade() else { return };
            close_tab(&a, &t, i);
        });
    }
    let anim = Rc::new(slint::Timer::default());
    let start_anim = {
        let (d, w, anim) = (doc.clone(), weak.clone(), anim.clone());
        Rc::new(move || {
            if anim.running() { return; }
            let (d, w) = (d.clone(), w.clone());
            let a2 = anim.clone();
            anim.start(slint::TimerMode::Repeated, Duration::from_millis(8), move || {
                let Some(a) = w.upgrade() else { return };
                let mut x = d.borrow_mut();
                let delta = x.target - x.scroll;
                if delta == 0 { a2.stop(); return; }
                let step = (delta as f32 * 0.35).round() as i32;
                x.scroll += if step == 0 { delta.signum() } else { step };
                app_scrolled(&a, &mut x);
            });
        })
    };
    {
        let (d, sa) = (doc.clone(), start_anim.clone());
        app.on_scroll_delta(move |delta| {
            {
                let mut x = d.borrow_mut();
                let m = x.max_scroll();
                x.target = (x.target - delta.round() as i32).clamp(0, m);
            }
            sa();
        });
        let (d, w) = (doc.clone(), weak.clone());
        app.on_scroll_to(move |v| {
            let Some(a) = w.upgrade() else { return };
            let mut x = d.borrow_mut();
            let m = x.max_scroll();
            x.scroll = v.clamp(0, m);
            x.target = x.scroll;
            app_scrolled(&a, &mut x);
        });
        let (d, sa) = (doc.clone(), start_anim.clone());
        app.on_key_scroll(move |what| {
            {
                let mut x = d.borrow_mut();
                let m = x.max_scroll();
                let page = (x.view.1 as f32 * 0.9) as i32;
                let line = 64;
                x.target = match what.as_str() {
                    "page-down" => x.target + page,
                    "page-up" => x.target - page,
                    "down" => x.target + line,
                    "up" => x.target - line,
                    "home" => 0,
                    "end" => m,
                    _ => x.target,
                }.clamp(0, m);
            }
            sa();
        });
        let (d, sa, w) = (doc.clone(), start_anim.clone(), weak.clone());
        app.on_outline_activate(move |i| {
            let Some(a) = w.upgrade() else { return };
            {
                let mut x = d.borrow_mut();
                let m = x.max_scroll();
                let scale = a.window().scale_factor();
                x.target = ((OUTLINE[i as usize].2 * scale) as i32 - 16).clamp(0, m);
            }
            sa();
        });
    }
    {
        let (w, t, d, dk, kk) = (weak.clone(), tabs.clone(), doc.clone(), dark.clone(), k.clone());
        app.on_command(move |cmd| {
            let Some(a) = w.upgrade() else { return };
            match cmd.as_str() {
                "quit" => { let _ = slint::quit_event_loop(); }
                "close-tab" => { let i = a.get_current_tab(); close_tab(&a, &t, i); }
                "zoom-in" => a.set_zoom((a.get_zoom() + 10).min(300)),
                "zoom-out" => a.set_zoom((a.get_zoom() - 10).max(50)),
                "zoom-reset" => a.set_zoom(100),
                "theme" => {
                    dk.set(!dk.get());
                    apply_theme(&a, dk.get(), &kk);
                    d.borrow_mut().dark = dk.get();
                    a.window().request_redraw();
                }
                _ => {}
            }
        });
    }

    // Frame hook. winit's Wayland backend holds a requested redraw until the frame callback of
    // the previous commit, so the RedrawRequested after the first present is the compositor's
    // answer. The trace in results/ confirms this ordering.
    let out_phases = Rc::new(Cell::new((0.0f64, 0.0f64, 0.0f64)));
    let frame_no = Rc::new(Cell::new(0u32));
    let bench = Rc::new(RefCell::new(Bench { dir: 1, ..Default::default() }));
    {
        let (d, w, n, op, b, dk, ph) = (doc.clone(), weak.clone(), frame_no.clone(), opts.clone(), bench.clone(), dark.clone(), out_phases.clone());
        let explicit = theme_explicit;
        let first_paint_ms = Rc::new(Cell::new(0.0f64));
        let t_draw = Rc::new(Cell::new(None::<Instant>));
        app.window().on_winit_window_event(move |_win, ev| {
            if let winit::event::WindowEvent::ThemeChanged(th) = ev {
                if !explicit {
                    if let Some(a) = w.upgrade() {
                        dk.set(*th == winit::window::Theme::Dark);
                        apply_theme(&a, dk.get(), &None);
                        d.borrow_mut().dark = dk.get();
                    }
                }
            }
            if !matches!(ev, winit::event::WindowEvent::RedrawRequested) {
                return EventResult::Propagate;
            }
            let Some(a) = w.upgrade() else { return EventResult::Propagate };
            let num = n.get();
            n.set(num + 1);
            let now = Instant::now();
            if num >= 1 {
                let bench_on = op.bench > 0;
                if num == 1 {
                    let (e, an, sh) = ph.get();
                    on_presented(&a, &d, &op, ms_at_entry + t_start.elapsed().as_secs_f64() * 1e3, first_paint_ms.get(), t0_src);
                    if op.measure { out(&format!("PHASES entry_ms={e:.2} app_new_ms={an:.2} show_ms={sh:.2}")); }
                }
                if bench_on {
                    let mut x = d.borrow_mut();
                    let mut be = b.borrow_mut();
                    if be.start.is_none() { be.start = Some(now); }
                    if let Some(l) = be.last_redraw { let v = (now - l).as_secs_f64() * 1e3; be.interval.push(v); }
                    be.last_redraw = Some(now);
                    be.frames += 1;
                    if be.frames > op.bench {
                        drop(x);
                        let mut be2 = std::mem::take(&mut *be);
                        drop(be);
                        out(&format!("BENCH mode={:?} fmt={:?} scale={} view={}x{} step={}", op.mode, op.fmt, a.window().scale_factor(), a.get_dev_w(), a.get_dev_h(), op.bench_step));
                        summary("fill_ms", &mut be2.fill);
                        summary("render_present_ms", &mut be2.render);
                        summary("interval_ms", &mut be2.interval);
                        out(&format!("BENCH wall_s={:.2}", be2.start.unwrap().elapsed().as_secs_f64()));
                        memcpy_baseline(a.get_dev_w() as usize, a.get_dev_h() as usize);
                        print_mem("BENCH-END");
                        std::process::exit(0);
                    }
                    let m = x.max_scroll();
                    let mut nx = x.scroll + be.dir * op.bench_step;
                    if nx >= m { nx = m; be.dir = -1; }
                    if nx <= 0 { nx = 0; be.dir = 1; }
                    x.scroll = nx;
                    x.target = nx;
                    let copied = x.sync(&a);
                    if copied { be.fill.push(x.fill_us as f64 / 1e3); }
                    drop(be);
                    drop(x);
                    let (b2, w2, t) = (b.clone(), w.clone(), now);
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        b2.borrow_mut().render.push(t.elapsed().as_secs_f64() * 1e3);
                        if let Some(a) = w2.upgrade() { a.window().request_redraw(); }
                    });
                    return EventResult::Propagate;
                }
            }
            d.borrow_mut().sync(&a);
            if num == 0 {
                update_outline(&a, &d.borrow());
                first_paint_ms.set(ms_at_entry + t_start.elapsed().as_secs_f64() * 1e3);
                let w2 = w.clone();
                let tt = t_draw.clone();
                tt.set(Some(now));
                slint::Timer::single_shot(Duration::ZERO, move || {
                    if let Some(a) = w2.upgrade() { a.window().request_redraw(); }
                });
            }
            EventResult::Propagate
        });
    }

    if opts.open_find { app.invoke_toggle_find(); }

    app.show().unwrap();
    let show_ms = ms_at_entry + t_start.elapsed().as_secs_f64() * 1e3;
    out_phases.set((ms_at_entry, app_new_ms, show_ms));
    if !theme_explicit {
        // No kdeglobals (Windows, for example): ask the system through winit.
        let th = app.window().with_winit_window(|w| w.theme()).flatten();
        if let Some(th) = th {
            dark.set(th == winit::window::Theme::Dark);
            apply_theme(&app, dark.get(), &None);
            doc.borrow_mut().dark = dark.get();
        }
    }
    let self_test = opts.self_test;
    if self_test { selftest::start(&app, doc.clone()); }
    if opts.log_state { start_state_log(&app); }
    app.run().unwrap();
    let _ = (self_test, SharedString::new());
}

mod selftest;

/// What a plain row-by-row copy of the same area costs, as a floor for the blit.
fn memcpy_baseline(w: usize, h: usize) {
    let (stride, src) = (w * 4, vec![0x55u8; w * 4 * h]);
    let mut dst = vec![0u8; w * 4 * h];
    let mut v = vec![];
    for _ in 0..50 {
        let t = Instant::now();
        for (d, s) in dst.chunks_exact_mut(stride).zip(src.chunks_exact(stride)) {
            d.copy_from_slice(s);
        }
        std::hint::black_box(&mut dst);
        v.push(t.elapsed().as_secs_f64() * 1e3);
    }
    summary("memcpy_baseline_ms", &mut v);
}

/// Prints the UI state whenever it changes, so that real-seat input can be checked from outside.
fn start_state_log(app: &App) {
    let w = app.as_weak();
    let last = RefCell::new(String::new());
    let t = Box::leak(Box::new(slint::Timer::default()));
    t.start(slint::TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(a) = w.upgrade() else { return };
        let s = format!(
            "STATE find={} text={:?} menu={} cmd={} left={} right={} left_w={} scroll={} tab={} tabs={} rows={}",
            a.get_find_open(), a.get_find_text().as_str(), a.get_menu_open(), a.get_last_command(), a.get_left_open(),
            a.get_right_open(), a.get_left_width(), a.get_scroll(), a.get_current_tab(), a.get_tabs().row_count(), a.get_tree_rows().row_count()
        );
        if *last.borrow() != s {
            out(&s);
            *last.borrow_mut() = s;
        }
    });
}

fn print_mem(tag: &str) {
    let (pr, pss, rss) = private_kib();
    out(&format!("{tag} private_kib={pr} pss_kib={pss} rss_kib={rss} private_mib={:.2}", pr as f64 / 1024.0));
}

fn app_scrolled(a: &App, x: &mut Doc) {
    x.sync(a);
    update_outline(a, x);
}

fn update_outline(a: &App, x: &Doc) {
    let scale = a.window().scale_factor();
    let y = (x.scroll as f32 + 80.0) / scale;
    let mut cur = 0;
    for (i, o) in OUTLINE.iter().enumerate() {
        if o.2 <= y { cur = i as i32; }
    }
    a.set_outline_current(cur);
}

fn on_presented(a: &App, d: &Rc<RefCell<Doc>>, op: &Opts, present_ms: f64, first_paint_ms: f64, t0: &str) {
    let x = d.borrow();
    let line = format!(
        "PRESENTED present_ms={present_ms:.2} first_paint_ms={first_paint_ms:.2} t0={t0} dpr={} canvas_device_size={}x{} canvas_device_origin={},{} page_x={} window_phys={}x{} window_logical={:.1}x{:.1} page={}x{}",
        a.window().scale_factor(), a.get_dev_w(), a.get_dev_h(), a.get_dev_left(), a.get_dev_top(), a.get_page_x(),
        a.window().size().width, a.window().size().height,
        a.window().size().to_logical(a.window().scale_factor()).width, a.window().size().to_logical(a.window().scale_factor()).height,
        x.page_w, x.page_h,
    );
    out(&line);
    drop(x);
    // Popups take their position when shown, so they wait until the window has its real size.
    if let Some(m) = op.open_menu { a.invoke_open_menu(m); }
    if op.open_about { a.invoke_open_about(); }
    if op.open_context { a.invoke_open_context(500.0, 300.0); }
    if op.measure {
        let t = slint::Timer::default();
        Box::leak(Box::new(t)).start(slint::TimerMode::SingleShot, Duration::from_millis(800), move || {
            print_mem("MEM");
            // Stay a little longer: the harness also reads smaps from outside at 800 ms.
            slint::Timer::single_shot(Duration::from_millis(400), || std::process::exit(0));
        });
    }
    if let Some(p) = op.screenshot.clone() {
        let w = a.as_weak();
        let t = slint::Timer::default();
        Box::leak(Box::new(t)).start(slint::TimerMode::SingleShot, Duration::from_millis(200), move || {
            let Some(a) = w.upgrade() else { return };
            match a.window().take_snapshot() {
                Ok(b) => { save_png(&p, &b); out(&format!("SCREENSHOT {p} {}x{}", b.width(), b.height())); }
                Err(e) => eprintln!("snapshot failed: {e}"),
            }
            std::process::exit(0);
        });
    }
    if op.stay_ms > 0 {
        let t = slint::Timer::default();
        Box::leak(Box::new(t)).start(slint::TimerMode::SingleShot, Duration::from_millis(op.stay_ms), || std::process::exit(0));
    }
}

fn sync_tree_to_tab(a: &App) {
    let tabs = a.get_tabs();
    if let Some(t) = tabs.row_data(a.get_current_tab() as usize) {
        if let Some(n) = TREE.iter().find(|n| n.label == t.title.as_str()) {
            a.set_tree_selected(n.id);
        }
    }
}

fn close_tab(a: &App, t: &Rc<VecModel<TabData>>, i: i32) {
    if i < 0 || i as usize >= t.row_count() { return; }
    t.remove(i as usize);
    let n = t.row_count() as i32;
    let cur = a.get_current_tab();
    a.set_current_tab(if i < cur { cur - 1 } else { cur.min(n - 1).max(0) });
    sync_tree_to_tab(a);
}
