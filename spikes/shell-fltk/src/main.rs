//! Layout A in fltk-rs, styled as close to a modern Windows 11 app as FLTK allows.
//! Throwaway mockup: the page is the stand-in image from `look-shared`, nothing is wired.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use fltk::{prelude::*, *};
use std::cell::Cell;
use std::rc::Rc;

// Dark palette, as in the Qt shell: the title row 10% and the bar 6% towards the text colour.
const PAGE: (u8, u8, u8) = (0x20, 0x20, 0x20);
const TITLE: (u8, u8, u8) = (0x2e, 0x2e, 0x2e);
const BAR: (u8, u8, u8) = (0x29, 0x29, 0x29);
const PANE: (u8, u8, u8) = (0x26, 0x26, 0x26);
const TEXT: (u8, u8, u8) = (0xe6, 0xe6, 0xe6);
const MUTED: (u8, u8, u8) = (0x9e, 0x9e, 0x9e);
const HOVER: (u8, u8, u8) = (0x3a, 0x3a, 0x3a);
const LINE: (u8, u8, u8) = (0x38, 0x38, 0x38);
const ACCENT: (u8, u8, u8) = (0x4c, 0x8d, 0xff);

fn c(rgb: (u8, u8, u8)) -> enums::Color {
    enums::Color::from_rgb(rgb.0, rgb.1, rgb.2)
}

// Segoe Fluent Icons code points.
const I_OPEN: &str = "\u{E8E5}";
const I_RELOAD: &str = "\u{E72C}";
const I_RECENT: &str = "\u{E81C}";
const I_FILES: &str = "\u{E8B7}";
const I_OUTLINE: &str = "\u{E8FD}";
const I_MINUS: &str = "\u{E738}";
const I_PLUS: &str = "\u{E710}";
const I_UP: &str = "\u{E70E}";
const I_DOWN: &str = "\u{E70D}";
const I_THEME: &str = "\u{E708}";
const I_MORE: &str = "\u{E712}";
const I_SEARCH: &str = "\u{E721}";
const I_CLOSE: &str = "\u{E711}";
const I_DOC: &str = "\u{E8A5}";
const I_FOLDER: &str = "\u{E8B7}";

const UI: enums::Font = enums::Font::Helvetica; // remapped to Segoe UI Variable
const UI_BOLD: enums::Font = enums::Font::HelveticaBold; // Segoe UI Semibold
const ICONS: enums::Font = enums::Font::Zapfdingbats; // remapped to Segoe Fluent Icons

/// A flat toolbar button: icon, optional label, rounded hover and press fills, like WinUI's.
fn tool_button(icon: &'static str, label: Option<&'static str>, on: Rc<Cell<bool>>) -> frame::Frame {
    let width = 36 + label.map_or(0, |l| draw_width(l) + 8);
    let mut b = frame::Frame::default().with_size(width, 32);
    let hover = Rc::new(Cell::new(0u8)); // 0 rest, 1 hover, 2 pressed
    let h = hover.clone();
    b.draw(move |w| {
        let fill = match h.get() {
            1 => Some(HOVER),
            2 => Some((0x44, 0x44, 0x44)),
            _ if on.get() => Some((0x33, 0x3a, 0x48)),
            _ => None,
        };
        if let Some(fill) = fill {
            draw::draw_rbox(w.x(), w.y(), w.w(), w.h(), 6, true, c(fill));
        }
        draw::set_font(ICONS, 16);
        draw::set_draw_color(c(if on.get() { ACCENT } else { TEXT }));
        draw::draw_text2(icon, w.x() + 10, w.y(), 16, w.h(), enums::Align::Center);
        if let Some(label) = label {
            draw::set_font(UI, 14);
            draw::set_draw_color(c(TEXT));
            draw::draw_text2(label, w.x() + 34, w.y(), w.w() - 36, w.h(), enums::Align::Left);
        }
    });
    b.handle(move |w, ev| match ev {
        enums::Event::Enter => {
            hover.set(1);
            w.redraw();
            true
        }
        enums::Event::Leave => {
            hover.set(0);
            w.redraw();
            true
        }
        enums::Event::Push => {
            hover.set(2);
            w.redraw();
            true
        }
        enums::Event::Released => {
            hover.set(1);
            w.do_callback();
            w.redraw();
            true
        }
        _ => false,
    });
    b
}

fn draw_width(text: &str) -> i32 {
    draw::set_font(UI, 14);
    draw::width(text) as i32
}

fn separator() -> frame::Frame {
    let mut s = frame::Frame::default().with_size(13, 32);
    s.draw(|w| {
        draw::set_draw_color(c(LINE));
        draw::draw_line(w.x() + 6, w.y() + 8, w.x() + 6, w.y() + w.h() - 8);
    });
    s
}

fn spacer() -> frame::Frame {
    frame::Frame::default()
}

/// One tab in the title row: rounded top, page-coloured when active, so it joins the bar below.
fn tab(name: &'static str, active: Rc<Cell<usize>>, index: usize, tabs: Rc<Cell<Vec<frame::Frame>>>) -> frame::Frame {
    let width = draw_width(name) + 70;
    let mut t = frame::Frame::default().with_size(width, 34);
    let hover = Rc::new(Cell::new(false));
    let (h, a) = (hover.clone(), active.clone());
    t.draw(move |w| {
        let is_active = a.get() == index;
        if is_active {
            draw::draw_rbox(w.x(), w.y() + 2, w.w(), w.h() + 8, 8, true, c(BAR));
        } else if h.get() {
            draw::draw_rbox(w.x() + 2, w.y() + 4, w.w() - 4, w.h() - 8, 6, true, c(HOVER));
        }
        draw::set_font(ICONS, 13);
        draw::set_draw_color(c(if is_active { ACCENT } else { MUTED }));
        draw::draw_text2(I_DOC, w.x() + 12, w.y() + 2, 16, w.h(), enums::Align::Center);
        draw::set_font(if is_active { UI_BOLD } else { UI }, 13);
        draw::set_draw_color(c(if is_active { TEXT } else { MUTED }));
        draw::draw_text2(name, w.x() + 34, w.y() + 2, w.w() - 60, w.h(), enums::Align::Left);
        if is_active || h.get() {
            draw::set_font(ICONS, 9);
            draw::set_draw_color(c(MUTED));
            draw::draw_text2(I_CLOSE, w.x() + w.w() - 24, w.y() + 2, 14, w.h(), enums::Align::Center);
        }
    });
    t.handle(move |w, ev| match ev {
        enums::Event::Enter => {
            hover.set(true);
            w.redraw();
            true
        }
        enums::Event::Leave => {
            hover.set(false);
            w.redraw();
            true
        }
        enums::Event::Push => {
            active.set(index);
            let all = tabs.take();
            for mut other in all.iter().cloned() {
                other.redraw();
            }
            tabs.set(all);
            true
        }
        _ => false,
    });
    t
}

/// The page: the stand-in document image, centred, scrolled by the wheel, with a slim overlay
/// scrollbar that widens on hover.
fn page() -> frame::Frame {
    let mut p = frame::Frame::default();
    let image = image::PngImage::load("../look-shared/content-dark@1.0.png").ok();
    let scroll = Rc::new(Cell::new(0i32));
    let hover = Rc::new(Cell::new(false));
    let (s, h) = (scroll.clone(), hover.clone());
    p.draw(move |w| {
        draw::draw_rect_fill(w.x(), w.y(), w.w(), w.h(), c(PAGE));
        draw::push_clip(w.x(), w.y(), w.w(), w.h());
        if let Some(mut img) = image.clone() {
            let x = w.x() + (w.w() - img.w()).max(0) / 2;
            img.draw(x, w.y() - s.get(), img.w(), img.h());
            // Thumb
            let total = img.h() as f64;
            let view = w.h() as f64;
            let len = (view / total * view).max(32.0) as i32;
            let top = w.y() + (s.get() as f64 / (total - view).max(1.0) * (view - len as f64)) as i32;
            let width = if h.get() { 8 } else { 4 };
            let shade = if h.get() { (0x77, 0x77, 0x77) } else { (0x55, 0x55, 0x55) };
            draw::draw_rbox(w.x() + w.w() - width - 4, top + 2, width, len - 4, width / 2, true, c(shade));
        }
        draw::pop_clip();
    });
    p.handle(move |w, ev| match ev {
        enums::Event::MouseWheel => {
            let step = match app::event_dy() {
                app::MouseWheel::Down => 60,
                app::MouseWheel::Up => -60,
                _ => 0,
            };
            scroll.set((scroll.get() + step).clamp(0, (2400 - w.h()).max(0)));
            w.redraw();
            true
        }
        enums::Event::Move => {
            let near = app::event_x() > w.x() + w.w() - 16;
            if near != hover.get() {
                hover.set(near);
                w.redraw();
            }
            true
        }
        enums::Event::Enter => true,
        enums::Event::Leave => {
            hover.set(false);
            w.redraw();
            true
        }
        _ => false,
    });
    p
}

/// A side pane: a small caps heading and a list of rows drawn by hand, with hover and selection.
fn pane(title: &'static str, rows: &'static [(u8, &'static str, bool)], selected: usize) -> frame::Frame {
    let mut f = frame::Frame::default();
    let hover = Rc::new(Cell::new(usize::MAX));
    let sel = Rc::new(Cell::new(selected));
    let (h, s) = (hover.clone(), sel.clone());
    f.draw(move |w| {
        draw::draw_rect_fill(w.x(), w.y(), w.w(), w.h(), c(PANE));
        draw::set_font(UI_BOLD, 11);
        draw::set_draw_color(c(MUTED));
        draw::draw_text2(title, w.x() + 16, w.y() + 10, w.w(), 20, enums::Align::Left);
        for (i, (level, name, folder)) in rows.iter().enumerate() {
            let y = w.y() + 38 + i as i32 * 30;
            if i == s.get() {
                draw::draw_rbox(w.x() + 6, y, w.w() - 12, 28, 5, true, c((0x33, 0x3a, 0x48)));
                draw::draw_rbox(w.x() + 6, y + 7, 3, 14, 1, true, c(ACCENT));
            } else if i == h.get() {
                draw::draw_rbox(w.x() + 6, y, w.w() - 12, 28, 5, true, c(HOVER));
            }
            let indent = w.x() + 16 + *level as i32 * 16;
            let mut text_x = indent;
            if !title.starts_with("OUT") {
                draw::set_font(ICONS, 13);
                draw::set_draw_color(c(if *folder { (0xe8, 0xb3, 0x39) } else { MUTED }));
                draw::draw_text2(if *folder { I_FOLDER } else { I_DOC }, indent, y, 16, 28, enums::Align::Center);
                text_x += 24;
            }
            draw::set_font(UI, 13);
            draw::set_draw_color(c(if i == s.get() { TEXT } else { (0xcc, 0xcc, 0xcc) }));
            draw::draw_text2(name, text_x, y, w.w() - (text_x - w.x()) - 8, 28, enums::Align::Left | enums::Align::Clip);
        }
        draw::set_draw_color(c(LINE));
    });
    f.handle(move |w, ev| {
        let row = || {
            let y = app::event_y() - w.y() - 38;
            (y >= 0 && (y / 30) < rows.len() as i32).then(|| (y / 30) as usize)
        };
        match ev {
            enums::Event::Enter => true,
            enums::Event::Move => {
                let r = row().unwrap_or(usize::MAX);
                if r != hover.get() {
                    hover.set(r);
                    w.redraw();
                }
                true
            }
            enums::Event::Leave => {
                hover.set(usize::MAX);
                w.redraw();
                true
            }
            enums::Event::Push => {
                if let Some(r) = row() {
                    sel.set(r);
                    w.redraw();
                }
                true
            }
            _ => false,
        }
    });
    f
}

/// The find box: a real FLTK input on a rounded field with a search glyph and a focus underline.
fn find_box() -> group::Group {
    let mut g = group::Group::default().with_size(260, 32);
    let mut bg = frame::Frame::new(g.x(), g.y(), 260, 32, None);
    let mut input = input::Input::new(g.x() + 32, g.y() + 6, 220, 20, None);
    input.set_frame(enums::FrameType::FlatBox);
    input.set_color(c((0x1c, 0x1c, 0x1c)));
    input.set_text_color(c(TEXT));
    input.set_text_font(UI);
    input.set_text_size(13);
    input.set_cursor_color(c(TEXT));
    input.set_selection_color(c(ACCENT));
    let focused = Rc::new(Cell::new(false));
    let f = focused.clone();
    bg.draw(move |w| {
        draw::draw_rbox(w.x(), w.y(), w.w(), w.h(), 6, true, c(LINE));
        draw::draw_rbox(w.x() + 1, w.y() + 1, w.w() - 2, w.h() - 2, 5, true, c((0x1c, 0x1c, 0x1c)));
        if f.get() {
            draw::draw_rect_fill(w.x() + 6, w.y() + w.h() - 2, w.w() - 12, 2, c(ACCENT));
        }
        draw::set_font(ICONS, 13);
        draw::set_draw_color(c(MUTED));
        draw::draw_text2(I_SEARCH, w.x() + 10, w.y(), 16, w.h(), enums::Align::Center);
    });
    let mut bg2 = bg.clone();
    input.handle(move |i, ev| {
        match ev {
            enums::Event::Focus => focused.set(true),
            enums::Event::Unfocus => focused.set(false),
            _ => return false,
        }
        bg2.redraw();
        i.redraw();
        false
    });
    g.end();
    g
}

fn label(text: &'static str, color: (u8, u8, u8), width: i32) -> frame::Frame {
    let mut f = frame::Frame::default().with_size(width, 32).with_label(text);
    f.set_label_font(UI);
    f.set_label_size(13);
    f.set_label_color(c(color));
    f
}

/// Dark caption and caption colour through DWM, so the native frame (snap, shadow, rounded
/// corners) matches the title row.
#[cfg(windows)]
fn dark_caption(window: &window::Window) {
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(hwnd: *mut std::ffi::c_void, attr: u32, value: *const std::ffi::c_void, size: u32) -> i32;
    }
    let hwnd = window.raw_handle() as *mut std::ffi::c_void;
    let dark: u32 = 1;
    let caption: u32 = (TITLE.2 as u32) << 16 | (TITLE.1 as u32) << 8 | TITLE.0 as u32;
    let text: u32 = (TEXT.2 as u32) << 16 | (TEXT.1 as u32) << 8 | TEXT.0 as u32;
    unsafe {
        DwmSetWindowAttribute(hwnd, 20, &dark as *const u32 as _, 4); // immersive dark mode
        DwmSetWindowAttribute(hwnd, 35, &caption as *const u32 as _, 4); // caption colour
        DwmSetWindowAttribute(hwnd, 36, &text as *const u32 as _, 4); // caption text colour
    }
}

fn main() {
    let started = std::time::Instant::now();
    let app = app::App::default();
    app::set_visible_focus(false);
    app::set_background_color(BAR.0, BAR.1, BAR.2);
    app::set_foreground_color(TEXT.0, TEXT.1, TEXT.2);
    if let Ok(name) = enums::Font::load_font("C:/Windows/Fonts/SegUIVar.ttf") {
        enums::Font::set_font(UI, &name);
    }
    enums::Font::set_font(UI_BOLD, "Segoe UI Semibold");
    if let Ok(name) = enums::Font::load_font("C:/Windows/Fonts/SegoeIcons.ttf") {
        enums::Font::set_font(ICONS, &name);
    }

    let mut win = window::Window::default().with_size(1280, 800).with_label("mdview — showcase.md");
    win.set_color(c(TITLE));
    let mut root = group::Flex::default_fill().column();

    // Title row: logo and tabs.
    let mut title = group::Flex::default().row();
    title.set_margins(10, 6, 10, 0);
    title.set_pad(2);
    let mut logo = frame::Frame::default();
    logo.draw(|w| {
        draw::draw_rbox(w.x() + 4, w.y() + 8, 22, 22, 6, true, c(ACCENT));
        draw::set_font(UI_BOLD, 13);
        draw::set_draw_color(enums::Color::White);
        draw::draw_text2("M", w.x() + 4, w.y() + 8, 22, 22, enums::Align::Center);
    });
    title.fixed(&logo, 36);
    let active = Rc::new(Cell::new(0usize));
    let tabs: Rc<Cell<Vec<frame::Frame>>> = Rc::new(Cell::new(Vec::new()));
    let mut list = Vec::new();
    for (i, name) in ["showcase.md", "linked.md", "HANDOFF.md"].into_iter().enumerate() {
        let t = tab(name, active.clone(), i, tabs.clone());
        title.fixed(&t, t.w());
        list.push(t);
    }
    tabs.set(list);
    let mut add = tool_button(I_PLUS, None, Rc::new(Cell::new(false)));
    add.set_size(32, 32);
    title.fixed(&add, 32);
    spacer();
    title.end();
    root.fixed(&title, 40);

    // Command bar.
    let files_on = Rc::new(Cell::new(true));
    let outline_on = Rc::new(Cell::new(true));
    let mut bar = group::Flex::default().row();
    bar.set_margins(8, 4, 8, 4);
    bar.set_pad(2);
    bar.set_frame(enums::FrameType::FlatBox);
    bar.set_color(c(BAR));
    let off = || Rc::new(Cell::new(false));
    for b in [
        tool_button(I_OPEN, Some("Open"), off()),
        tool_button(I_RELOAD, None, off()),
        tool_button(I_RECENT, Some("Recent"), off()),
    ] {
        bar.fixed(&b, b.w());
    }
    let s = separator();
    bar.fixed(&s, 13);
    let mut files_button = tool_button(I_FILES, Some("Files"), files_on.clone());
    bar.fixed(&files_button, files_button.w());
    let mut outline_button = tool_button(I_OUTLINE, Some("Outline"), outline_on.clone());
    bar.fixed(&outline_button, outline_button.w());
    let s = separator();
    bar.fixed(&s, 13);
    let b = tool_button(I_MINUS, None, off());
    bar.fixed(&b, 36);
    let z = label("100%", TEXT, 44);
    bar.fixed(&z, 44);
    let b = tool_button(I_PLUS, None, off());
    bar.fixed(&b, 36);
    spacer();
    let count = label("3 of 12", MUTED, 64);
    bar.fixed(&count, 64);
    let find = find_box();
    bar.fixed(&find, 260);
    for icon in [I_UP, I_DOWN] {
        let b = tool_button(icon, None, off());
        bar.fixed(&b, 36);
    }
    let s = separator();
    bar.fixed(&s, 13);
    let b = tool_button(I_THEME, Some("Dark"), off());
    bar.fixed(&b, b.w());
    let b = tool_button(I_MORE, None, off());
    bar.fixed(&b, 36);
    bar.end();
    root.fixed(&bar, 40);

    // Body: Files, page, Outline.
    let mut body = group::Flex::default().row();
    body.set_pad(0);
    let files = pane(
        "FILES",
        &[
            (0, "docs", true),
            (1, "HANDOFF.md", false),
            (1, "PLAN.md", false),
            (1, "PORT_CHECKLIST.md", false),
            (0, "fixtures", true),
            (1, "linked.md", false),
            (1, "showcase.md", false),
            (0, "spikes", true),
            (0, "CLAUDE.md", false),
            (0, "README.md", false),
        ],
        6,
    );
    body.fixed(&files, 250);
    let _page = page();
    let outline = pane(
        "OUTLINE",
        &[
            (0, "Showcase", false),
            (1, "Text", false),
            (1, "Lists", false),
            (2, "Task lists", false),
            (1, "Code", false),
            (1, "Tables", false),
            (1, "Math", false),
            (1, "Diagrams", false),
            (2, "Flowchart", false),
            (2, "Sequence", false),
            (1, "Images", false),
            (1, "Details", false),
        ],
        1,
    );
    body.fixed(&outline, 230);
    body.end();
    root.end();
    win.end();
    win.make_resizable(true);
    win.show();
    #[cfg(windows)]
    dark_caption(&win);

    // The pane buttons really toggle their panes.
    let (mut body2, files2, on) = (body.clone(), files.clone(), files_on.clone());
    files_button.set_callback(move |b| {
        on.set(!on.get());
        body2.fixed(&files2, if on.get() { 250 } else { 0 });
        body2.layout();
        b.redraw();
        body2.redraw();
    });
    let (mut body3, outline2, on) = (body.clone(), outline.clone(), outline_on.clone());
    outline_button.set_callback(move |b| {
        on.set(!on.get());
        body3.fixed(&outline2, if on.get() { 230 } else { 0 });
        body3.layout();
        b.redraw();
        body3.redraw();
    });

    if std::env::var_os("MOCKUP_TRACE").is_some() {
        app::add_timeout3(0.0, move |_| {
            eprintln!("shell-fltk: first frame after {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
        });
    }
    app.run().unwrap();
}
