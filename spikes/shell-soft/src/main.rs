//! Layout A with no toolkit: winit for the window and input, softbuffer for the pixels, fontdue
//! for text, every control drawn by hand. Throwaway mockup; the page is the `look-shared` image.
//! Assumes 100% display scaling.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const PAGE: u32 = 0x202020;
const TITLE: u32 = 0x2e2e2e;
const BAR: u32 = 0x292929;
const PANE: u32 = 0x262626;
const TEXT: u32 = 0xe6e6e6;
const MUTED: u32 = 0x9e9e9e;
const HOVER: u32 = 0x3a3a3a;
const LINE: u32 = 0x383838;
const ACCENT: u32 = 0x4c8dff;
const ON: u32 = 0x333a48;
const FIELD: u32 = 0x1c1c1c;
const FOLDER: u32 = 0xe8b339;

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Tab(usize),
    Button(usize),
    Files,
    Outline,
    Row(bool, usize),
    Find,
    Scrollbar,
}

/// UI text through the project's own text path: parley lays out, `mdview_render::TextEngine`
/// rasterizes with RGB subpixel masks and gamma-correct blending, as on the page.
struct Fonts {
    fonts: parley::FontContext,
    layouts: parley::LayoutContext<[u8; 4]>,
    engine: mdview_render::TextEngine,
}

impl Fonts {
    fn layout(&mut self, which: u8, text: &str, size: f32, color: u32) -> parley::Layout<[u8; 4]> {
        use parley::{FontFamily, FontWeight, StyleProperty};
        let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, true);
        let family = if which == 2 { "Segoe Fluent Icons" } else { "Segoe UI Variable Text" };
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(family)));
        builder.push_default(StyleProperty::FontSize(size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(if which == 1 { 600.0 } else { 400.0 })));
        builder.push_default(StyleProperty::Brush([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]));
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout
    }
    fn width(&mut self, which: u8, text: &str, size: f32) -> i32 {
        self.layout(which, text, size, 0).width().ceil() as i32
    }
}

struct Canvas<'a> {
    px: &'a mut [u32],
    w: i32,
    h: i32,
    clip: (i32, i32, i32, i32),
}

fn mix(dst: u32, src: u32, a: f32) -> u32 {
    let ch = |s: u32| -> u32 {
        let d = (dst >> s) & 0xff;
        let c = (src >> s) & 0xff;
        ((d as f32 + (c as f32 - d as f32) * a).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

impl Canvas<'_> {
    fn put(&mut self, x: i32, y: i32, color: u32, a: f32) {
        let (cx, cy, cw, ch) = self.clip;
        if x < cx.max(0) || y < cy.max(0) || x >= (cx + cw).min(self.w) || y >= (cy + ch).min(self.h) || a <= 0.0 {
            return;
        }
        let i = (y * self.w + x) as usize;
        self.px[i] = if a >= 1.0 { color } else { mix(self.px[i], color, a) };
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.put(xx, yy, color, 1.0);
            }
        }
    }
    /// Rounded rectangle with anti-aliased corners.
    fn rbox(&mut self, x: i32, y: i32, w: i32, h: i32, r: f32, color: u32) {
        let (fx, fy, fw, fh) = (x as f32, y as f32, w as f32, h as f32);
        for yy in y..y + h {
            for xx in x..x + w {
                let px = xx as f32 + 0.5;
                let py = yy as f32 + 0.5;
                let qx = (px - fx).min(fx + fw - px);
                let qy = (py - fy).min(fy + fh - py);
                let a = if qx < r && qy < r {
                    let d = ((r - qx).powi(2) + (r - qy).powi(2)).sqrt();
                    (r - d + 0.5).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                self.put(xx, yy, color, a);
            }
        }
    }
    /// Text with its vertical centre at `cy`; returns the advance. The region under the text is
    /// cut into a small BGRA bitmap, the text engine blends onto it, and it is copied back.
    fn text(&mut self, f: &mut Fonts, which: u8, size: f32, x: i32, cy: i32, text: &str, color: u32, max: i32) -> i32 {
        if text.is_empty() {
            return 0;
        }
        let layout = f.layout(which, text, size, color);
        let w = (layout.width().ceil() as i32 + 4).min(max.max(0));
        let h = layout.height().ceil() as i32 + 4;
        let y = cy - h / 2;
        if w <= 0 {
            return 0;
        }
        let mut bmp = mdview_render::Bitmap::new(w as u32, h as u32);
        let at = |xx: i32, yy: i32| (yy * self.w + xx) as usize;
        let (cx, cy0, cw, ch) = self.clip;
        let inside = |xx: i32, yy: i32| xx >= cx.max(0) && yy >= cy0.max(0) && xx < (cx + cw).min(self.w) && yy < (cy0 + ch).min(self.h);
        for row in 0..h {
            for col in 0..w {
                let (xx, yy) = (x + col, y + row);
                let p = if xx >= 0 && yy >= 0 && xx < self.w && yy < self.h { self.px[at(xx, yy)] } else { 0 };
                let i = ((row * w + col) * 4) as usize;
                bmp.pixels[i..i + 4].copy_from_slice(&[p as u8, (p >> 8) as u8, (p >> 16) as u8, 255]);
            }
        }
        f.engine.draw_layout(&mut bmp, &layout, 0.0, (h as f32 - layout.height()) / 2.0);
        for row in 0..h {
            for col in 0..w {
                let (xx, yy) = (x + col, y + row);
                if inside(xx, yy) {
                    let i = ((row * w + col) * 4) as usize;
                    let b = &bmp.pixels[i..i + 4];
                    self.px[at(xx, yy)] = (b[2] as u32) << 16 | (b[1] as u32) << 8 | b[0] as u32;
                }
            }
        }
        layout.width().round() as i32
    }
}

struct Page {
    w: i32,
    h: i32,
    px: Vec<u32>,
}

fn load_page() -> Option<Page> {
    let file = std::fs::File::open("../look-shared/content-dark@1.0.png").ok()?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file)).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let step = info.color_type.samples();
    let px = buf[..info.buffer_size()]
        .chunks(step)
        .map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32)
        .collect();
    Some(Page { w: info.width as i32, h: info.height as i32, px })
}

const BUTTONS: [(&str, &str, bool); 13] = [
    ("\u{E8E5}", "Open", false),
    ("\u{E72C}", "", false),
    ("\u{E81C}", "Recent", false),
    ("|", "", false),
    ("\u{E8B7}", "Files", true),
    ("\u{E8FD}", "Outline", true),
    ("|", "", false),
    ("\u{E738}", "", false),
    ("%", "100%", false),
    ("\u{E710}", "", false),
    ("\u{E70E}", "", true), // right group starts
    ("\u{E70D}", "", true),
    ("\u{E708}", "Dark", true),
];

const FILES: [(i32, &str, bool); 10] = [
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
];
const OUTLINE: [(i32, &str); 12] = [
    (0, "Showcase"),
    (1, "Text"),
    (1, "Lists"),
    (2, "Task lists"),
    (1, "Code"),
    (1, "Tables"),
    (1, "Math"),
    (1, "Diagrams"),
    (2, "Flowchart"),
    (2, "Sequence"),
    (1, "Images"),
    (1, "Details"),
];
const TABS: [&str; 3] = ["showcase.md", "linked.md", "HANDOFF.md"];

struct App {
    started: Instant,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    fonts: Fonts,
    page: Option<Page>,
    hits: Vec<(Hit, (i32, i32, i32, i32))>,
    mouse: (i32, i32),
    hover: Option<Hit>,
    pressed: Option<Hit>,
    tab: usize,
    files_on: bool,
    outline_on: bool,
    file_sel: usize,
    outline_sel: usize,
    find: String,
    find_focus: bool,
    scroll: i32,
    first: bool,
}

impl App {
    fn draw(&mut self) {
        let (Some(window), Some(surface)) = (self.window.clone(), self.surface.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        let (w, h) = (size.width.max(1) as i32, size.height.max(1) as i32);
        surface.resize(NonZeroU32::new(w as u32).unwrap(), NonZeroU32::new(h as u32).unwrap()).unwrap();
        let mut buffer = surface.buffer_mut().unwrap();
        let mut c = Canvas { px: &mut buffer, w, h, clip: (0, 0, w, h) };
        let f = &mut self.fonts;
        let mut hits = Vec::new();
        let hover = self.hover;
        let pressed = self.pressed;
        let fill_for = |hit: Hit, on: bool| -> Option<u32> {
            if pressed == Some(hit) && hover == Some(hit) {
                Some(0x444444)
            } else if hover == Some(hit) {
                Some(HOVER)
            } else if on {
                Some(ON)
            } else {
                None
            }
        };

        // Title row and tabs.
        c.rect(0, 0, w, 40, TITLE);
        c.rbox(14, 9, 22, 22, 6.0, ACCENT);
        c.text(f, 1, 13.0, 21, 20, "M", 0xffffff, 40);
        let mut x = 56;
        for (i, name) in TABS.iter().enumerate() {
            let tw = f.width(if i == self.tab { 1 } else { 0 }, name, 13.0) + 70;
            let active = i == self.tab;
            if active {
                c.rbox(x, 8, tw, 40, 8.0, BAR);
            } else if hover == Some(Hit::Tab(i)) {
                c.rbox(x + 2, 10, tw - 4, 26, 6.0, HOVER);
            }
            c.text(f, 2, 13.0, x + 12, 24, "\u{E8A5}", if active { ACCENT } else { MUTED }, 20);
            c.text(f, if active { 1 } else { 0 }, 13.0, x + 34, 24, name, if active { TEXT } else { MUTED }, tw);
            if active || hover == Some(Hit::Tab(i)) {
                c.text(f, 2, 9.0, x + tw - 22, 24, "\u{E711}", MUTED, 20);
            }
            hits.push((Hit::Tab(i), (x, 6, tw, 34)));
            x += tw + 2;
        }
        c.text(f, 2, 14.0, x + 10, 22, "\u{E710}", TEXT, 20);

        // Command bar.
        c.rect(0, 40, w, 40, BAR);
        let mut x = 8;
        let right_start = 10;
        let mut layout: Vec<(usize, i32, i32)> = Vec::new();
        for (i, (icon, label, _)) in BUTTONS.iter().enumerate().take(right_start) {
            let bw = match *icon {
                "|" => 13,
                "%" => 44,
                _ if label.is_empty() => 36,
                _ => 44 + f.width(0, label, 14.0),
            };
            layout.push((i, x, bw));
            x += bw + 2;
        }
        // Right group: ↑ ↓ | Theme ⋯, laid out from the right edge, then the find box and count.
        let mut rx = w - 8;
        let more_x = rx - 36;
        rx = more_x - 2;
        let theme_w = 44 + f.width(0, "Dark", 14.0);
        let theme_x = rx - theme_w;
        layout.push((12, theme_x, theme_w));
        rx = theme_x - 2 - 13;
        let sep2 = rx;
        rx -= 2;
        layout.push((11, rx - 36, 36));
        rx -= 38;
        layout.push((10, rx - 36, 36));
        rx -= 38;
        let find_x = rx - 260;
        for (i, bx, bw) in layout {
            let (icon, label, _) = BUTTONS[i];
            if icon == "|" {
                c.rect(bx + 6, 48, 1, 24, LINE);
                continue;
            }
            if icon == "%" {
                c.text(f, 0, 13.0, bx + 6, 60, label, TEXT, bw);
                continue;
            }
            let hit = match i {
                4 => Hit::Files,
                5 => Hit::Outline,
                _ => Hit::Button(i),
            };
            let on = (i == 4 && self.files_on) || (i == 5 && self.outline_on);
            if let Some(fill) = fill_for(hit, on) {
                c.rbox(bx, 44, bw, 32, 6.0, fill);
            }
            c.text(f, 2, 16.0, bx + 10, 60, icon, if on { ACCENT } else { TEXT }, 24);
            if !label.is_empty() {
                c.text(f, 0, 14.0, bx + 34, 60, label, TEXT, bw);
            }
            hits.push((hit, (bx, 44, bw, 32)));
        }
        c.rect(sep2 + 6, 48, 1, 24, LINE);
        if let Some(fill) = fill_for(Hit::Button(99), false) {
            c.rbox(more_x, 44, 36, 32, 6.0, fill);
        }
        c.text(f, 2, 16.0, more_x + 10, 60, "\u{E712}", TEXT, 24);
        hits.push((Hit::Button(99), (more_x, 44, 36, 32)));
        // Find box.
        c.rbox(find_x, 44, 260, 32, 6.0, LINE);
        c.rbox(find_x + 1, 45, 258, 30, 5.0, FIELD);
        c.text(f, 2, 13.0, find_x + 10, 60, "\u{E721}", MUTED, 20);
        if self.find.is_empty() && !self.find_focus {
            c.text(f, 0, 13.0, find_x + 32, 60, "Find in document", 0x777777, 220);
        } else {
            let tw = c.text(f, 0, 13.0, find_x + 32, 60, &self.find, TEXT, 220);
            if self.find_focus {
                c.rect(find_x + 33 + tw, 52, 1, 16, TEXT);
            }
        }
        if self.find_focus {
            c.rect(find_x + 6, 74, 248, 2, ACCENT);
        }
        hits.push((Hit::Find, (find_x, 44, 260, 32)));
        let count = if self.find.is_empty() { "" } else { "3 of 12" };
        let cw = f.width(0, count, 13.0);
        c.text(f, 0, 13.0, find_x - cw - 12, 60, count, MUTED, 80);

        // Body.
        let top = 80;
        let body_h = h - top;
        let files_w = if self.files_on { 250 } else { 0 };
        let outline_w = if self.outline_on { 230 } else { 0 };
        if self.files_on {
            c.rect(0, top, files_w, body_h, PANE);
            c.text(f, 1, 11.0, 16, top + 20, "FILES", MUTED, 200);
            for (i, (level, name, folder)) in FILES.iter().enumerate() {
                let y = top + 38 + i as i32 * 30;
                let hit = Hit::Row(false, i);
                if i == self.file_sel {
                    c.rbox(6, y, files_w - 12, 28, 5.0, ON);
                    c.rbox(6, y + 7, 3, 14, 1.5, ACCENT);
                } else if hover == Some(hit) {
                    c.rbox(6, y, files_w - 12, 28, 5.0, HOVER);
                }
                let ix = 16 + level * 16;
                c.text(f, 2, 13.0, ix, y + 14, if *folder { "\u{E8B7}" } else { "\u{E8A5}" }, if *folder { FOLDER } else { MUTED }, 20);
                c.text(f, 0, 13.0, ix + 24, y + 14, name, if i == self.file_sel { TEXT } else { 0xcccccc }, files_w - ix - 32);
                hits.push((hit, (6, y, files_w - 12, 28)));
            }
        }
        let page_x = files_w;
        let page_w = w - files_w - outline_w;
        c.rect(page_x, top, page_w, body_h, PAGE);
        if let Some(page) = &self.page {
            self.scroll = self.scroll.clamp(0, (page.h - body_h).max(0));
            let ox = page_x + (page_w - page.w).max(0) / 2;
            c.clip = (page_x, top, page_w, body_h);
            for row in 0..body_h.min(page.h - self.scroll) {
                let src = ((row + self.scroll) * page.w) as usize;
                let y = top + row;
                for col in 0..page.w {
                    c.put(ox + col, y, page.px[src + col as usize], 1.0);
                }
            }
            // Overlay scrollbar, wider on hover.
            let wide = hover == Some(Hit::Scrollbar);
            let len = ((body_h as f32 / page.h as f32) * body_h as f32).max(32.0) as i32;
            let thumb_y = top + (self.scroll as f32 / (page.h - body_h).max(1) as f32 * (body_h - len) as f32) as i32;
            let tw = if wide { 8 } else { 4 };
            c.rbox(page_x + page_w - tw - 4, thumb_y + 2, tw, len - 4, tw as f32 / 2.0, if wide { 0x777777 } else { 0x555555 });
            c.clip = (0, 0, w, h);
            hits.push((Hit::Scrollbar, (page_x + page_w - 16, top, 16, body_h)));
        }
        if self.outline_on {
            let ox = w - outline_w;
            c.rect(ox, top, outline_w, body_h, PANE);
            c.text(f, 1, 11.0, ox + 16, top + 20, "OUTLINE", MUTED, 200);
            for (i, (level, name)) in OUTLINE.iter().enumerate() {
                let y = top + 38 + i as i32 * 30;
                let hit = Hit::Row(true, i);
                if i == self.outline_sel {
                    c.rbox(ox + 6, y, outline_w - 12, 28, 5.0, ON);
                    c.rbox(ox + 6, y + 7, 3, 14, 1.5, ACCENT);
                } else if hover == Some(hit) {
                    c.rbox(ox + 6, y, outline_w - 12, 28, 5.0, HOVER);
                }
                c.text(f, 0, 13.0, ox + 16 + level * 16, y + 14, name, if i == self.outline_sel { TEXT } else { 0xcccccc }, outline_w - 40);
                hits.push((hit, (ox + 6, y, outline_w - 12, 28)));
            }
        }
        self.hits = hits;
        buffer.present().unwrap();
        if self.first {
            self.first = false;
            if std::env::var_os("MOCKUP_TRACE").is_some() {
                eprintln!("shell-soft: first frame after {:.1} ms", self.started.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }

    fn hit_at(&self, (x, y): (i32, i32)) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(_, (hx, hy, hw, hh))| x >= *hx && y >= *hy && x < hx + hw && y < hy + hh)
            .map(|(hit, _)| *hit)
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attributes = Window::default_attributes()
            .with_title("mdview — showcase.md")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        #[cfg(windows)]
        {
            use winit::platform::windows::{Color, WindowAttributesExtWindows};
            attributes = attributes
                .with_title_background_color(Some(Color::from_rgb(0x2e, 0x2e, 0x2e)))
                .with_title_text_color(Color::from_rgb(0xe6, 0xe6, 0xe6));
        }
        let window = Rc::new(event_loop.create_window(attributes).unwrap());
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let redraw = |app: &App| {
            if let Some(w) = &app.window {
                w.request_redraw();
            }
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) => redraw(self),
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = (position.x as i32, position.y as i32);
                let hit = self.hit_at(self.mouse);
                if hit != self.hover {
                    self.hover = hit;
                    redraw(self);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.hover = None;
                redraw(self);
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let hit = self.hit_at(self.mouse);
                match state {
                    ElementState::Pressed => {
                        self.pressed = hit;
                        self.find_focus = hit == Some(Hit::Find);
                    }
                    ElementState::Released => {
                        if hit.is_some() && hit == self.pressed {
                            match hit.unwrap() {
                                Hit::Tab(i) => self.tab = i,
                                Hit::Files => self.files_on = !self.files_on,
                                Hit::Outline => self.outline_on = !self.outline_on,
                                Hit::Row(false, i) => self.file_sel = i,
                                Hit::Row(true, i) => self.outline_sel = i,
                                _ => {}
                            }
                        }
                        self.pressed = None;
                    }
                }
                redraw(self);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y * 60.0,
                    MouseScrollDelta::PixelDelta(p) => -p.y as f32,
                };
                self.scroll += dy as i32;
                redraw(self);
            }
            WindowEvent::KeyboardInput { event, .. } if self.find_focus && event.state == ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::Backspace) => {
                        self.find.pop();
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.find.clear();
                        self.find_focus = false;
                    }
                    _ => {
                        if let Some(text) = &event.text {
                            self.find.extend(text.chars().filter(|c| !c.is_control()));
                        }
                    }
                }
                redraw(self);
            }
            _ => {}
        }
    }
}

fn main() {
    let started = Instant::now();
    let fonts = Fonts {
        fonts: parley::FontContext::new(),
        layouts: parley::LayoutContext::new(),
        engine: mdview_render::TextEngine::new(4 << 20),
    };
    let mut app = App {
        started,
        window: None,
        surface: None,
        fonts,
        page: load_page(),
        hits: Vec::new(),
        mouse: (0, 0),
        hover: None,
        pressed: None,
        tab: 0,
        files_on: true,
        outline_on: true,
        file_sel: 6,
        outline_sel: 1,
        find: String::new(),
        find_focus: false,
        scroll: 0,
        first: true,
    };
    EventLoop::new().unwrap().run_app(&mut app).unwrap();
}
