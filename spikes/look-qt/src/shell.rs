//! The look spike's window: real Qt Widgets, real Breeze. Safe Rust only.
#![forbid(unsafe_code)]

use crate::qt::ffi::*;
use crate::qt::*;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

const SHARED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../look-shared");
const SCALES: [f64; 4] = [1.0, 1.5, 1.7, 2.2];
const PAGE_W: f64 = 780.0;
const PAGE_H: f64 = 2400.0;

#[derive(Clone, Copy, PartialEq)]
enum Qss {
    None,
    Widgets,
    App,
}

struct Opts {
    theme: Option<bool>, // Some(dark)
    measure: bool,
    stay_ms: i32,
    screenshot: Option<String>,
    qss: Qss,
    find_open: bool,
    shadow: bool,
    self_test: bool,
    trace: bool,
    scroll: i32,
    win: (i32, i32),
}

fn parse(args: &[String]) -> Opts {
    let mut o = Opts {
        theme: None,
        measure: false,
        stay_ms: 0,
        screenshot: None,
        qss: Qss::Widgets,
        find_open: false,
        shadow: true,
        self_test: false,
        trace: false,
        scroll: 0,
        win: (1220, 820),
    };
    for a in args {
        let (k, v) = a.split_once('=').unwrap_or((a.as_str(), ""));
        match k {
            "--theme" => o.theme = match v { "dark" => Some(true), "light" => Some(false), _ => None },
            "--measure" => o.measure = true,
            "--stay-ms" => o.stay_ms = v.parse().unwrap_or(0),
            "--screenshot" => o.screenshot = Some(v.to_string()),
            "--qss" => o.qss = match v { "none" => Qss::None, "app" => Qss::App, _ => Qss::Widgets },
            "--find-open" => o.find_open = true,
            "--no-shadow" => o.shadow = false,
            "--self-test" => o.self_test = true,
            "--trace" => o.trace = true,
            "--scroll" => o.scroll = v.parse().unwrap_or(0),
            "--win" => {
                if let Some((w, h)) = v.split_once('x') {
                    o.win = (w.parse().unwrap_or(1220), h.parse().unwrap_or(820));
                }
            }
            _ => eprintln!("look-qt: unknown option {a}"),
        }
    }
    o
}

/// Process start in CLOCK_MONOTONIC: harness timestamp if present, else /proc ticks
/// (10 ms resolution), which count from boot, including suspend.
fn process_start_ns() -> (u64, &'static str) {
    if let Some(v) = std::env::var("MDV_T0_NS").ok().and_then(|v| v.parse().ok()) {
        return (v, "harness-spawn");
    }
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after = stat.rsplit(')').next().unwrap_or("");
    let ticks: u64 = after.split_whitespace().nth(19).and_then(|s| s.parse().ok()).unwrap_or(0);
    let age = boottime_ns().saturating_sub(ticks * 10_000_000);
    (monotonic_ns().saturating_sub(age), "proc-starttime-10ms")
}

fn out(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{line}");
    let _ = o.flush();
}

fn memory_line() -> String {
    let s = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let get = |key: &str| -> f64 {
        s.lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                (k == key).then(|| v.trim().trim_end_matches("kB").trim().parse::<f64>().ok())?
            })
            .unwrap_or(0.0)
            / 1024.0
    };
    let (clean, dirty) = (get("Private_Clean"), get("Private_Dirty"));
    format!(
        "MEMORY private_total_mib={:.2} private_dirty_mib={dirty:.2} private_clean_mib={clean:.2} pss_mib={:.2} rss_mib={:.2}",
        clean + dirty,
        get("Pss"),
        get("Rss"),
    )
}

/// Parses a KDE .colors file into `section -> key -> colour`.
fn read_scheme(path: &str) -> HashMap<String, HashMap<String, Rgb>> {
    let mut all: HashMap<String, HashMap<String, Rgb>> = HashMap::new();
    let mut cur = String::new();
    for line in std::fs::read_to_string(path).unwrap_or_default().lines() {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            cur = name.to_string();
        } else if let Some((k, v)) = line.split_once('=') {
            let p: Vec<u8> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if p.len() >= 3 {
                all.entry(cur.clone()).or_default().insert(k.to_string(), Rgb(p[0], p[1], p[2]));
            }
        }
    }
    all
}

/// Installs a real Breeze colour scheme as the application palette, with the shared
/// page's accent, so that --theme works whatever the desktop is set to.
fn install_scheme(dark: bool) {
    let path = format!(
        "/usr/share/color-schemes/{}.colors",
        if dark { "BreezeDark" } else { "BreezeLight" }
    );
    let s = read_scheme(&path);
    let c = |sec: &str, key: &str| s.get(sec).and_then(|m| m.get(key)).copied().unwrap_or(Rgb(128, 128, 128));
    let accent = if dark { Rgb(0x60, 0xCD, 0xFF) } else { Rgb(0x00, 0x5F, 0xB8) };
    let on_accent = if dark { Rgb(0x10, 0x10, 0x10) } else { Rgb(0xFF, 0xFF, 0xFF) };
    let window = c("Colors:Window", "BackgroundNormal");
    let wtext = c("Colors:Window", "ForegroundNormal");
    let button = c("Colors:Button", "BackgroundNormal");
    let mut p = Palette::from_app();
    p.set(R_WINDOW, window)
        .set(R_WINDOW_TEXT, wtext)
        .set(R_BASE, c("Colors:View", "BackgroundNormal"))
        .set(R_ALT_BASE, c("Colors:View", "BackgroundAlternate"))
        .set(R_TEXT, c("Colors:View", "ForegroundNormal"))
        .set(R_BUTTON, button)
        .set(R_BUTTON_TEXT, c("Colors:Button", "ForegroundNormal"))
        .set(R_BRIGHT_TEXT, c("Colors:Selection", "ForegroundNormal"))
        .set(R_HIGHLIGHT, accent)
        .set(R_HIGHLIGHTED_TEXT, on_accent)
        .set(R_LINK, accent)
        .set(R_LINK_VISITED, c("Colors:View", "ForegroundVisited"))
        .set(R_TOOLTIP_BASE, c("Colors:Tooltip", "BackgroundNormal"))
        .set(R_TOOLTIP_TEXT, c("Colors:Tooltip", "ForegroundNormal"))
        .set(R_PLACEHOLDER, c("Colors:View", "ForegroundInactive"))
        .set(R_LIGHT, button.mix(Rgb(255, 255, 255), 0.25))
        .set(R_MIDLIGHT, button.mix(Rgb(255, 255, 255), 0.12))
        .set(R_MID, button.mix(Rgb(0, 0, 0), 0.25))
        .set(R_DARK, button.mix(Rgb(0, 0, 0), 0.45))
        .set(R_SHADOW, Rgb(0, 0, 0));
    for role in [R_WINDOW_TEXT, R_TEXT, R_BUTTON_TEXT] {
        p.set_disabled(role, wtext.mix(window, 0.6));
    }
    p.install_on_app();
}

/// WCAG contrast ratio.
fn contrast(a: Rgb, b: Rgb) -> f64 {
    fn lin(v: u8) -> f64 {
        let c = v as f64 / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    let l = |c: Rgb| 0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2);
    let (x, y) = (l(a).max(l(b)), l(a).min(l(b)));
    (x + 0.05) / (y + 0.05)
}

/// Moves `fg` toward `text` until it reads on `bg`.
fn readable(fg: Rgb, bg: Rgb, text: Rgb) -> Rgb {
    let mut c = fg;
    for i in 1..=10 {
        if contrast(c, bg) >= 4.5 {
            break;
        }
        c = fg.mix(text, i as f64 / 10.0);
    }
    c
}

/// System icon files. See the note on `set_icon_file` for why this isn't QIcon::fromTheme.
fn icon_file(dark: bool, name: &str) -> String {
    let theme = if dark { "breeze-dark" } else { "breeze" };
    format!("/usr/share/icons/{theme}/actions/16/{name}.svg")
}

struct Ui {
    root: H<QWidget>,
    top: H<QWidget>,
    menu_button: H<QToolButton>,
    menu: H<QMenu>,
    tabs: H<QTabBar>,
    find_button: H<QToolButton>,
    left_button: H<QToolButton>,
    right_button: H<QToolButton>,
    splitter: H<QSplitter>,
    left: H<QWidget>,
    right: H<QWidget>,
    file_label: H<QLabel>,
    outline_label: H<QLabel>,
    files: H<QTreeWidget>,
    outline: H<QTreeWidget>,
    current_heading: H<QTreeWidgetItem>,
    canvas: H<Canvas>,
    scrollbar: H<QScrollBar>,
    find_bar: H<QWidget>,
    find_edit: H<QLineEdit>,
    find_count: H<QLabel>,
    find_prev: H<QToolButton>,
    find_next: H<QToolButton>,
    find_close: H<QToolButton>,
    find_action: H<QAction>,
    hairlines: Vec<H<QWidget>>,
    status_path: H<QLabel>,
    status_zoom: H<QLabel>,
}

struct Page {
    map: Mapped,
    w: i32,
    h: i32,
    dark: bool,
}

struct Shell {
    opts: Opts,
    t0: u64,
    t0_src: &'static str,
    ui: OnceCell<Ui>,
    page: RefCell<Option<Page>>,
    dark: Cell<bool>,
    page_bg: Cell<Rgb>,
    paints: Cell<u32>,
    first_paint_ms: Cell<f64>,
    present_ms: Cell<f64>,
    synced_dpr: Cell<f64>,
    applied: Cell<Option<(Rgb, Rgb, Rgb)>>,
    presented: Cell<bool>,
    post_paint_timer: Cell<i32>,
    done_timer: Cell<i32>,
    palette_timer: Cell<i32>,
}

impl Shell {
    fn ui(&self) -> &Ui {
        self.ui.get().expect("ui built")
    }

    fn trace(&self, m: &str) {
        if self.opts.trace {
            out(&format!("TRACE {m}"));
        }
    }

    fn ms(&self) -> f64 {
        monotonic_ns().saturating_sub(self.t0) as f64 / 1e6
    }

    /// Everything derived from the application palette. Runs at startup and again when
    /// the system's colour scheme changes.
    fn apply_theme(&self) {
        let ui = self.ui();
        let pal = Palette::from_app();
        let (win, text, accent) = (pal.get(R_WINDOW), pal.get(R_WINDOW_TEXT), pal.get(R_HIGHLIGHT));
        self.applied.set(Some((win, text, accent)));
        let dark = win.luma() < 128.0;
        self.dark.set(dark);
        self.trace(&format!("theme window={win:?} text={text:?} accent={accent:?} dark={dark}"));
        let page_bg = if dark { Rgb(0x20, 0x20, 0x20) } else { Rgb(0xFB, 0xFB, 0xFB) };
        self.page_bg.set(page_bg);
        let hair = if dark { win.mix(text, 0.16) } else { win.mix(text, 0.14) };
        let muted = win.mix(text, if dark { 0.62 } else { 0.66 });
        let quiet_fill = win.mix(text, if dark { 0.09 } else { 0.07 });
        // The system accent can be anything (this user's is a dark green), so the two
        // places it colours chrome are kept readable against the window colour.
        let accent = readable(accent, win, text);
        let selected = win.mix(accent, if dark { 0.22 } else { 0.16 });

        let mut root = Palette::from_app();
        root.set(R_WINDOW, win);
        ui.root.set_palette(&root);
        ui.root.set_autofill(true);

        for h in &ui.hairlines {
            let mut p = Palette::from_app();
            p.set(R_WINDOW, hair);
            h.set_palette(&p);
            h.set_autofill(true);
        }
        for l in [ui.file_label, ui.outline_label, ui.status_path, ui.status_zoom, ui.find_count] {
            let mut p = Palette::from_app();
            p.set(R_WINDOW_TEXT, muted);
            l.widget().set_palette(&p);
        }
        for (t, selected_text) in [(ui.files, text), (ui.outline, accent)] {
            // A widget with a style sheet ignores a new palette (seen on a live scheme
            // change), so the sheet comes off first. It goes back on below.
            if self.opts.qss == Qss::Widgets && self.synced_dpr.get() != 0.0 {
                t.widget().set_style_sheet("");
            }
            let mut p = Palette::from_app();
            p.set(R_BASE, win)
                .set(R_ALT_BASE, win)
                .set(R_TEXT, text)
                .set(R_HIGHLIGHT, selected)
                .set(R_HIGHLIGHTED_TEXT, selected_text);
            t.widget().set_palette(&p);
        }

        let mut split = Palette::from_app();
        split.set(R_WINDOW, hair).set(R_WINDOW_TEXT, hair).set(R_BUTTON, hair).set(R_LIGHT, hair).set(R_DARK, hair).set(R_MID, hair);
        ui.splitter.widget().set_palette(&split);

        for (b, name) in [
            (ui.menu_button, "application-menu-symbolic"),
            (ui.find_button, "edit-find-symbolic"),
            (ui.left_button, "sidebar-show-symbolic"),
            (ui.right_button, "sidebar-expand-right-symbolic"),
            (ui.find_prev, "go-up-symbolic"),
            (ui.find_next, "go-down-symbolic"),
            (ui.find_close, "window-close-symbolic"),
        ] {
            b.set_icon_file(&icon_file(dark, name), 16);
        }

        ui.root.set_window_icon_file(&format!(
            "/usr/share/icons/{}/mimetypes/16/text-markdown.svg",
            if dark { "breeze-dark" } else { "breeze" }
        ));

        let card = if dark { Rgb(0x2B, 0x2B, 0x2B) } else { Rgb(0xFF, 0xFF, 0xFF) };
        let thumb = text;
        let tab_qss = format!(
            "QTabBar {{ background: transparent; }}
             QTabBar::tab {{ height: 28px; padding: 0 8px 0 12px; margin: 2px 2px; border: none; border-radius: 6px;
                             background: transparent; color: {muted}; }}
             QTabBar::tab:selected {{ background: {sel}; color: {text}; }}
             QTabBar::tab:hover:!selected {{ background: {hover}; color: {text}; }}
             QTabBar::close-button {{ subcontrol-position: right; margin: 0 4px 0 4px; }}",
            muted = muted.css(),
            text = text.css(),
            sel = if dark { win.mix(text, 0.14) } else { page_bg }.css(),
            hover = quiet_fill.css(),
        );
        let sb_qss = format!(
            "QScrollBar:vertical {{ background: transparent; width: 12px; margin: 2px 0; border: none; }}
             QScrollBar::handle:vertical {{ background: {t}; min-height: 40px; margin: 0 3px; border-radius: 3px; }}
             QScrollBar::handle:vertical:hover {{ background: {th}; margin: 0 2px; border-radius: 4px; }}
             QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{ height: 0; }}
             QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical {{ background: transparent; }}",
            t = thumb.css_a(0.32),
            th = thumb.css_a(0.55),
        );
        let find_qss = format!(
            "#mdfind {{ background: {}; border: 1px solid {}; border-radius: 8px; }}",
            card.css(),
            hair.css(),
        );
        let next = icon_file(dark, "arrow-right");
        let down = icon_file(dark, "arrow-down");
        let files_qss = format!(
            "QTreeView::branch {{ border-image: none; image: none; }}
             QTreeView::branch:has-children:closed {{ image: url({next}); }}
             QTreeView::branch:has-children:open {{ image: url({down}); }}"
        );
        let outline_qss =
            "QTreeView::branch { border-image: none; image: none; }".to_string();
        match self.opts.qss {
            Qss::Widgets => {
                ui.files.widget().set_style_sheet(&files_qss);
                ui.outline.widget().set_style_sheet(&outline_qss);
                ui.tabs.widget().set_style_sheet(&tab_qss);
                ui.scrollbar.widget().set_style_sheet(&sb_qss);
                ui.find_bar.set_style_sheet(&find_qss);
            }
            Qss::App => set_app_style_sheet(&format!("{tab_qss}\n{sb_qss}\n{find_qss}\n{files_qss}")),
            Qss::None => {
                let mut p = Palette::from_app();
                p.set(R_WINDOW, card);
                ui.find_bar.set_palette(&p);
                ui.find_bar.set_autofill(true);
            }
        }
        if self.synced_dpr.get() != 0.0 {
            self.sync_scroll(); // the page image depends on the theme
        }
        ui.canvas.widget().update();
    }

    fn view_device(&self) -> (i32, i32) {
        let w = self.ui().canvas.widget();
        let dpr = w.dpr();
        ((w.width() as f64 * dpr).round() as i32, (w.height() as f64 * dpr).round() as i32)
    }

    /// Maps the shared image for this scale and theme. Mapping costs only the page
    /// table entries for what is actually shown.
    fn ensure_page(&self, dpr: f64) {
        if dpr <= 0.0 {
            return;
        }
        let dark = self.dark.get();
        let scale = SCALES
            .iter()
            .copied()
            .min_by(|a, b| (a - dpr).abs().total_cmp(&(b - dpr).abs()))
            .unwrap_or(1.0);
        let (w, h) = ((PAGE_W * scale).round() as i32, (PAGE_H * scale).round() as i32);
        let mut slot = self.page.borrow_mut();
        if let Some(p) = slot.as_ref() {
            if p.w == w && p.dark == dark {
                return;
            }
        }
        if (scale - dpr).abs() > 0.001 {
            eprintln!("look-qt: no shared page for scale {dpr}; showing {scale} unscaled");
        }
        let theme = if dark { "dark" } else { "light" };
        let path = format!("{SHARED}/content-{theme}@{scale:.1}-{w}x{h}.bgra");
        *slot = Mapped::open(&path).map(|map| Page { map, w, h, dark });
        if slot.is_none() {
            eprintln!("look-qt: can't open {path}");
        }
    }

    fn sync_scroll(&self) {
        let ui = self.ui();
        let dpr = ui.canvas.widget().dpr();
        self.ensure_page(dpr);
        let page_h = self.page.borrow().as_ref().map_or(0, |p| p.h);
        let (_, vh) = self.view_device();
        ui.scrollbar.set_range((page_h - vh).max(0), vh, (60.0 * dpr).round() as i32);
    }

    fn place_overlays(&self) {
        let ui = self.ui();
        let cw = ui.canvas.widget();
        let (w, h) = (cw.width(), cw.height());
        ui.scrollbar.widget().set_geometry(w - 12, 0, 12, h);
        let (fw, fh) = (348, 44);
        // The shadow needs no extra room: the effect draws outside the widget's rect.
        ui.find_bar.set_geometry((w - fw - 20).max(4), 12, fw, fh);
    }

    fn toggle_find(&self, on: bool) {
        let ui = self.ui();
        ui.find_bar.set_visible(on);
        self.trace(&format!("find visible={on}"));
        if on {
            ui.find_bar.raise();
        }
    }

    fn select_tab(&self, i: i32) {
        let ui = self.ui();
        if i < 0 {
            return;
        }
        let name = ui.tabs.text(i);
        self.trace(&format!("tab index={i} name={name}"));
        ui.root.set_title(&format!("{name} \u{2014} mdview"));
        ui.status_path.set_text(&format!("~/markdown_viewer/fixtures/{name}"));
        ui.scrollbar.set_value(0);
    }

    fn on_presented(self: &Rc<Self>) {
        self.presented.set(true);
        let ui = self.ui();
        let cw = ui.canvas.widget();
        let (x, y) = cw.origin_in_window();
        let (page_w, page_h) = self.page.borrow().as_ref().map_or((0, 0), |p| (p.w, p.h));
        out(&format!(
            "PRESENTED present_ms={:.2} first_paint_ms={:.2} t0={} style={} dpr={} dark={} canvas_in_window={x},{y} canvas_logical={}x{} page={page_w}x{page_h} window_logical={}x{}",
            self.ms(),
            self.first_paint_ms.get(),
            self.t0_src,
            style_name(),
            cw.dpr(),
            self.dark.get(),
            cw.width(),
            cw.height(),
            ui.root.width(),
            ui.root.height(),
        ));
        let c = ui.canvas;
        let stay = if self.opts.stay_ms > 0 {
            self.opts.stay_ms
        } else if self.opts.self_test || self.opts.screenshot.is_some() {
            400
        } else if self.opts.measure {
            800
        } else {
            0
        };
        if stay > 0 {
            self.done_timer.set(c.start_timer(stay));
        }
    }

    fn self_test(&self) {
        let ui = self.ui();
        let mut ok = true;
        let mut check = |name: &str, pass: bool, detail: String| {
            ok &= pass;
            out(&format!("SELFTEST {} {name} {detail}", if pass { "PASS" } else { "FAIL" }));
        };

        let before = ui.find_bar.is_visible();
        ui.find_action.trigger();
        check("find action toggles the bar", ui.find_bar.is_visible() != before, String::new());
        ui.find_action.trigger();
        check("find action toggles it back", ui.find_bar.is_visible() == before, String::new());

        let w0 = ui.canvas.widget().width();
        ui.left_button.click();
        process_events();
        let w1 = ui.canvas.widget().width();
        check("left sidebar toggle hides it", !ui.left.is_visible() && w1 > w0, format!("canvas {w0} -> {w1}"));
        ui.left_button.click();
        process_events();
        ui.right_button.click();
        process_events();
        let w2 = ui.canvas.widget().width();
        check("right sidebar toggle hides it", !ui.right.is_visible() && w2 > w0, format!("canvas {w0} -> {w2}"));
        ui.right_button.click();
        process_events();
        check("sidebars restored", ui.canvas.widget().width() == w0, format!("canvas {}", ui.canvas.widget().width()));

        let v0 = ui.scrollbar.value();
        if let Some(qw) = ui.root.window_handle() {
            let (x, y) = ui.canvas.widget().origin_in_window();
            qw.send_wheel(x as f64 + 200.0, y as f64 + 200.0, -240);
        }
        let v1 = ui.scrollbar.value();
        check("wheel over the page scrolls", v1 > v0, format!("value {v0} -> {v1}"));

        ui.tabs.set_current(1);
        check("tab switch", ui.tabs.current() == 1 && ui.scrollbar.value() == 0, ui.tabs.text(1));
        let n = ui.tabs.count();
        ui.tabs.remove_tab(2);
        check("tab removal", ui.tabs.count() == n - 1, format!("{n} -> {}", ui.tabs.count()));
        out(&format!("SELFTEST {}", if ok { "ALL-PASS" } else { "SOME-FAILED" }));
    }

    fn finish(&self) -> ! {
        if self.opts.self_test {
            self.self_test();
        }
        if let Some(path) = &self.opts.screenshot {
            let saved = self.ui().root.save_png(path);
            out(&format!("SCREENSHOT {path} saved={saved}"));
        }
        if self.opts.measure {
            out(&format!("PRESENT_MS {:.2}", self.present_ms.get()));
            out(&memory_line());
        }
        fast_exit(0)
    }
}

struct Handler(Rc<Shell>);

impl CanvasHandler for Handler {
    fn paint(&self, t: &mut PaintTarget<'_>) {
        let s = &self.0;
        if s.paints.get() == 0 {
            s.first_paint_ms.set(s.ms());
        }
        s.paints.set(s.paints.get() + 1);
        let dpr = t.dpr();
        // Before the first paint Qt can report a rounded-up scale on Wayland (2 instead
        // of 1.5), so the page and scroll range are only settled here.
        if s.synced_dpr.get() != dpr {
            s.synced_dpr.set(dpr);
            s.sync_scroll();
            if s.paints.get() == 1 {
                s.ui().scrollbar.set_value(s.opts.scroll);
            }
        }
        let (lw, lh) = t.logical_size();
        let (vw, vh) = ((lw as f64 * dpr).round() as i32, (lh as f64 * dpr).round() as i32);
        let scroll = s.ui().scrollbar.value();
        if let Some(p) = s.page.borrow().as_ref() {
            let x = (vw - p.w).div_euclid(2);
            // Blit from the scroll row to y = 0 rather than at y = -scroll. On a fractional
            // origin Qt rounds a negative device position the other way from a positive
            // one, so the page would jump a pixel as its top leaves the window (measured).
            let top = scroll.clamp(0, p.h - 1);
            let rest = &p.map.bytes()[top as usize * p.w as usize * 4..];
            let gaps = x > 0 || p.h - top < vh;
            t.blit_page(rest, p.w, p.h - top, x, 0, gaps.then_some(s.page_bg.get()));
        }
        // Ask for the frame callback once this paint is committed: a zero timer runs
        // after the backing store flush, in the same event loop turn.
        if s.paints.get() == 1 {
            s.post_paint_timer.set(s.ui().canvas.start_timer(0));
        }
    }
    fn resized(&self) {
        if self.0.ui.get().is_some() {
            self.0.place_overlays();
            if self.0.synced_dpr.get() != 0.0 {
                self.0.sync_scroll();
            }
        }
    }
    fn wheel(&self, dy: i32) {
        let s = &self.0;
        let sb = s.ui().scrollbar;
        let dpr = s.ui().canvas.widget().dpr();
        sb.set_value(sb.value() - (dy as f64 / 120.0 * 60.0 * dpr).round() as i32);
    }
    fn timer(&self, id: i32) {
        let s = &self.0;
        let c = s.ui().canvas;
        c.kill_timer(id);
        if id == s.post_paint_timer.get() {
            if let Some(qw) = s.ui().root.window_handle() {
                qw.request_update();
            }
        } else if id == s.done_timer.get() {
            s.finish();
        } else if id == s.palette_timer.get() {
            s.palette_timer.set(-1);
            s.apply_theme();
        }
    }
    fn update_request(&self) {
        let s = &self.0;
        if s.paints.get() > 0 && !s.presented.get() {
            s.present_ms.set(s.ms());
            s.on_presented();
        }
    }
    fn app_palette_changed(&self) {
        // Follow the system scheme live. Debounced, since Qt sends this to every widget
        // and restyling sends more. --theme pins the look.
        let s = &self.0;
        if s.opts.theme.is_none() && s.ui.get().is_some() && s.palette_timer.get() < 0 {
            let p = Palette::from_app();
            let now = (p.get(R_WINDOW), p.get(R_WINDOW_TEXT), p.get(R_HIGHLIGHT));
            if s.applied.get() != Some(now) {
                s.palette_timer.set(s.ui().canvas.start_timer(0));
            }
        }
    }
}

fn hairline() -> H<QWidget> {
    let h = H::<QWidget>::new();
    h.set_fixed_height(1);
    h
}

fn tool_button(tip: &str, size: i32) -> H<QToolButton> {
    let b = H::<QToolButton>::new();
    b.set_auto_raise(true);
    b.widget().set_fixed_size(size, size);
    b.widget().set_no_focus();
    b.widget().set_tool_tip(tip);
    b
}

fn section_label(text: &str) -> H<QLabel> {
    let l = H::<QLabel>::new(text);
    l.widget().tune_font(0.82, true, 0.6);
    l.widget().set_margins(8, 0, 8, 6);
    l
}

fn build_files() -> H<QTreeWidget> {
    let t = H::<QTreeWidget>::new_flat(16, true);
    let rh = 28;
    let root = t.add_top("fixtures", rh);
    let assets = root.add_child("assets", rh);
    assets.add_child("badge.svg", rh);
    root.add_child("linked.md", rh);
    let current = root.add_child("showcase.md", rh);
    root.add_child("T\u{fc}rk\u{e7}e test.md", rh);
    t.add_top("notes", rh).add_child("ideas.md", rh);
    t.expand_all();
    t.set_current(current);
    t.widget().set_no_focus();
    t
}

fn build_outline() -> (H<QTreeWidget>, H<QTreeWidgetItem>) {
    let t = H::<QTreeWidget>::new_flat(12, false);
    let rh = 26;
    let h1 = t.add_top("Native Markdown Reader", rh);
    let current = h1.add_child("GitHub Markdown", rh);
    h1.add_child("HTML and images", rh);
    h1.add_child("Mathematics", rh);
    let d = h1.add_child("Diagrams", rh);
    d.add_child("Flowchart", rh);
    d.add_child("Sequence", rh);
    d.add_child("State", rh);
    h1.add_child("Repeated heading", rh);
    h1.add_child("Repeated heading", rh);
    t.expand_all();
    t.set_current(current);
    t.widget().set_no_focus();
    current.set_font_from(t.widget(), true);
    (t, current)
}

fn build_menu(root: H<QWidget>) -> (H<QMenu>, Vec<(&'static str, H<QAction>)>) {
    let menu = H::<QMenu>::new(root);
    // Qt sizes a menu of only submenus so tightly that the arrows touch the labels.
    menu.widget().set_min_width(168);
    let mut acts = vec![];
    let file = menu.add_menu("&File");
    file.add_action("&Open\u{2026}").set_shortcut("Ctrl+O");
    file.add_action("&Reload").set_shortcut("F5");
    file.add_separator();
    file.add_action("&Close Tab").set_shortcut("Ctrl+W");
    file.add_action("&Quit").set_shortcut("Ctrl+Q");
    let edit = menu.add_menu("&Edit");
    edit.add_action("&Copy").set_shortcut("Ctrl+C");
    edit.add_action("Select &All").set_shortcut("Ctrl+A");
    edit.add_separator();
    let find = edit.add_action("&Find\u{2026}");
    find.set_shortcut("Ctrl+F");
    acts.push(("find", find));
    let esc = edit.add_action("Close Find &Bar");
    esc.set_shortcut("Esc");
    acts.push(("find-close", esc));
    let view = menu.add_menu("&View");
    view.add_action("Zoom &In").set_shortcut("Ctrl++");
    view.add_action("Zoom &Out").set_shortcut("Ctrl+-");
    view.add_action("&Reset Zoom").set_shortcut("Ctrl+0");
    let help = menu.add_menu("&Help");
    help.add_action("&About mdview");
    (menu, acts)
}

fn build(shell: &Rc<Shell>) {
    if let Some(dark) = shell.opts.theme {
        install_scheme(dark);
    }
    let o = &shell.opts;
    let root = H::<QWidget>::new();
    root.resize(o.win.0, o.win.1);
    let layout = VBox::on(root, (0, 0, 0, 0), 0);

    // Top row: app menu, tabs, right-aligned tools.
    let top = H::<QWidget>::new();
    top.set_fixed_height(40);
    let row = HBox::on(top, (4, 4, 8, 4), 4);
    let menu_button = tool_button("Menu", 32);
    row.add(menu_button.widget(), 0);
    let tabs = H::<QTabBar>::new();
    tabs.add_tab("showcase.md");
    tabs.add_tab("linked.md");
    tabs.add_tab("T\u{fc}rk\u{e7}e test.md");
    tabs.widget().set_no_focus();
    row.add(tabs.widget(), 0);
    row.stretch();
    let find_button = tool_button("Find (Ctrl+F)", 32);
    let left_button = tool_button("Files sidebar", 32);
    let right_button = tool_button("Outline sidebar", 32);
    row.add(find_button.widget(), 0);
    row.add(left_button.widget(), 0);
    row.add(right_button.widget(), 0);
    layout.add(top, 0);
    let top_rule = hairline();
    layout.add(top_rule, 0);

    // Sidebars and the document.
    let splitter = H::<QSplitter>::new_horizontal();
    splitter.set_handle_width(1);

    let left = H::<QWidget>::new();
    left.set_min_width(160);
    let ll = VBox::on(left, (8, 14, 8, 8), 0);
    let file_label = section_label("FILES");
    ll.add(file_label.widget(), 0);
    let files = build_files();
    ll.add(files.widget(), 1);

    let right = H::<QWidget>::new();
    right.set_min_width(160);
    let rl = VBox::on(right, (8, 14, 8, 8), 0);
    let outline_label = section_label("ON THIS PAGE");
    rl.add(outline_label.widget(), 0);
    let (outline, current_heading) = build_outline();
    rl.add(outline.widget(), 1);

    let canvas = H::<Canvas>::new(Rc::new(Handler(shell.clone())) as Rc<dyn CanvasHandler>);
    canvas.widget().set_opaque_paint();
    canvas.widget().set_min_width(320);

    splitter.add(left);
    splitter.add(canvas.widget());
    splitter.add(right);
    splitter.set_stretch(0, 0);
    splitter.set_stretch(1, 1);
    splitter.set_stretch(2, 0);
    splitter.set_sizes(&[230, o.win.0 - 230 - 210 - 2, 210]);
    layout.add(splitter.widget(), 1);

    // Overlays are children of the canvas, so Qt composites them over the blit.
    let scrollbar = H::<QScrollBar>::new_vertical();
    scrollbar.widget().set_parent(canvas.widget());

    let find_bar = H::<QWidget>::new();
    find_bar.set_object_name("mdfind");
    find_bar.set_styled_background();
    find_bar.set_parent(canvas.widget());
    let fr = HBox::on(find_bar, (8, 6, 6, 6), 2);
    let find_edit = H::<QLineEdit>::new();
    find_edit.set_placeholder("Find");
    find_edit.set_text("heading");
    fr.add(find_edit.widget(), 1);
    let find_count = H::<QLabel>::new("2 of 14");
    find_count.widget().set_margins(8, 0, 6, 0);
    fr.add(find_count.widget(), 0);
    let find_prev = tool_button("Previous match", 28);
    let find_next = tool_button("Next match", 28);
    let find_close = tool_button("Close", 28);
    fr.add(find_prev.widget(), 0);
    fr.add(find_next.widget(), 0);
    fr.add(find_close.widget(), 0);
    find_bar.set_visible(false);

    // Status bar.
    let status_rule = hairline();
    layout.add(status_rule, 0);
    let status = H::<QWidget>::new();
    status.set_fixed_height(24);
    let sr = HBox::on(status, (12, 0, 12, 0), 8);
    let status_path = H::<QLabel>::new("");
    status_path.widget().tune_font(0.9, false, 0.0);
    let status_zoom = H::<QLabel>::new("100%");
    status_zoom.widget().tune_font(0.9, false, 0.0);
    sr.add(status_path.widget(), 0);
    sr.stretch();
    sr.add(status_zoom.widget(), 0);
    layout.add(status, 0);

    let (menu, acts) = build_menu(root);
    let find_action = acts.iter().find(|a| a.0 == "find").unwrap().1;
    let esc_action = acts.iter().find(|a| a.0 == "find-close").unwrap().1;
    root.add_action(find_action);
    root.add_action(esc_action);

    let _ = shell.ui.set(Ui {
        root,
        top,
        menu_button,
        menu,
        tabs,
        find_button,
        left_button,
        right_button,
        splitter,
        left,
        right,
        file_label,
        outline_label,
        files,
        outline,
        current_heading,
        canvas,
        scrollbar,
        find_bar,
        find_edit,
        find_count,
        find_prev,
        find_next,
        find_close,
        find_action,
        hairlines: vec![top_rule, status_rule],
        status_path,
        status_zoom,
    });

    // Wiring.
    let s = shell.clone();
    menu_button.on_clicked(move |_| {
        let ui = s.ui();
        let (x, y) = ui.menu_button.widget().map_to_global(0, ui.menu_button.widget().height() + 2);
        ui.menu.popup_at(x, y);
    });
    let s = shell.clone();
    find_action.on_triggered(move || s.toggle_find(!s.ui().find_bar.is_visible()));
    let s = shell.clone();
    esc_action.on_triggered(move || s.toggle_find(false));
    let s = shell.clone();
    find_button.on_clicked(move |_| s.toggle_find(!s.ui().find_bar.is_visible()));
    let s = shell.clone();
    find_close.on_clicked(move |_| s.toggle_find(false));
    let s = shell.clone();
    left_button.on_clicked(move |_| s.ui().left.set_visible(!s.ui().left.is_visible()));
    let s = shell.clone();
    right_button.on_clicked(move |_| s.ui().right.set_visible(!s.ui().right.is_visible()));
    let s = shell.clone();
    tabs.on_current_changed(move |i| s.select_tab(i));
    let s = shell.clone();
    tabs.on_close_requested(move |i| {
        let t = s.ui().tabs;
        if t.count() > 1 {
            t.remove_tab(i);
        }
    });
    let s = shell.clone();
    scrollbar.on_value_changed(move |v| {
        s.trace(&format!("scroll value={v}"));
        s.ui().canvas.widget().update();
    });

    shell.apply_theme();
    shell.select_tab(0);
    shell.place_overlays();
    if o.find_open {
        shell.toggle_find(true);
    }
    if o.shadow {
        let alpha = if shell.dark.get() { 120 } else { 56 };
        find_bar.set_shadow(16.0, 3.0, Rgb(0, 0, 0), alpha);
    }
    root.show();
    let qw = root.window_handle().expect("window handle after show");
    canvas.watch(qw);
}

pub fn main_with_args(args: Vec<String>) -> i32 {
    let opts = parse(&args);
    let (t0, t0_src) = process_start_ns();
    let shell = Rc::new(Shell {
        opts,
        t0,
        t0_src,
        ui: OnceCell::new(),
        page: RefCell::new(None),
        dark: Cell::new(false),
        page_bg: Cell::new(Rgb(0xFB, 0xFB, 0xFB)),
        paints: Cell::new(0),
        first_paint_ms: Cell::new(0.0),
        present_ms: Cell::new(0.0),
        synced_dpr: Cell::new(0.0),
        applied: Cell::new(None),
        presented: Cell::new(false),
        post_paint_timer: Cell::new(-1),
        done_timer: Cell::new(-1),
        palette_timer: Cell::new(-1),
    });
    run_app(|| build(&shell))
}
