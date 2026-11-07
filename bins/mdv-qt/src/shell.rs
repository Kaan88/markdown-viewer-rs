//! The Qt shell: a window, a menu bar and a canvas that shows the page `mdview-render` drew, with
//! the wheel scrolling it. Linux is the real target; this also builds against Qt on Windows so the
//! variant can be tested there. Safe Rust only; `qt/mod.rs` holds every `unsafe`.
#![forbid(unsafe_code)]

use crate::qt::ffi::*;
use crate::qt::*;
use mdview_core::parse_bytes;
use mdview_render::{render, Bitmap, Options, Theme, DARK, LIGHT};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

const WELCOME: &str = "\
# mdview

A Markdown viewer with its own renderer.

Open a document by passing it on the command line, or dropping it on the window.

- Headings, lists, tables, alerts and code blocks are drawn by `mdview-render`.
- The wheel scrolls.
";

/// One laid-out page, replaced when the viewport width, display scale or theme changes.
struct Page {
    bitmap: Bitmap,
    width: i32,
    scale: f64,
    theme: &'static str,
}

struct Shell {
    window: Cell<Option<H<QMainWindow>>>,
    canvas: Cell<Option<H<Canvas>>>,
    status: Cell<Option<H<QStatusBar>>>,
    document: RefCell<Option<mdview_core::Document>>,
    page: RefCell<Option<Page>>,
    path: RefCell<Option<PathBuf>>,
    scroll: Cell<f64>,
    viewport: Cell<i32>,
    theme: Cell<Theme>,
}

impl Shell {
    fn new(theme: Theme) -> Shell {
        Shell {
            window: Cell::new(None),
            canvas: Cell::new(None),
            status: Cell::new(None),
            document: RefCell::new(None),
            page: RefCell::new(None),
            path: RefCell::new(None),
            scroll: Cell::new(0.0),
            viewport: Cell::new(0),
            theme: Cell::new(theme),
        }
    }

    fn set_title(&self, name: &str) {
        if let Some(window) = self.window.get() {
            window.widget().set_title(&format!("mdview - {name}"));
        }
    }

    fn message(&self, text: &str) {
        if let Some(status) = self.status.get() {
            status.message(text);
        }
    }

    fn load(&self, path: &Path) {
        match std::fs::read(path) {
            Ok(bytes) => {
                let started = std::time::Instant::now();
                let document = parse_bytes(&bytes);
                eprintln!(
                    "mdview-qt: parsed {}: {} blocks, {} spans, {} warnings in {:.1} ms",
                    path.display(),
                    document.blocks.len(),
                    document.spans.len(),
                    document.warnings.len(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
                *self.document.borrow_mut() = Some(document);
                *self.page.borrow_mut() = None;
                self.scroll.set(0.0);
                *self.path.borrow_mut() = Some(path.to_path_buf());
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                self.set_title(&name);
                self.message(&format!("{name}: {} blocks", self.blocks()));
                self.redraw();
            }
            Err(e) => {
                self.message(&format!("{}: {e}", path.display()));
                eprintln!("mdview-qt: {}: {e}", path.display());
            }
        }
    }

    fn blocks(&self) -> usize {
        self.document
            .borrow()
            .as_ref()
            .map_or(0, |doc| doc.blocks.len())
    }

    fn redraw(&self) {
        if let Some(canvas) = self.canvas.get() {
            canvas.widget().update();
        }
    }

    fn layout(&self, width: i32, scale: f64) {
        let Some(document) = &*self.document.borrow() else {
            return;
        };
        let theme = self.theme.get();
        let started = std::time::Instant::now();
        let rendered = render(
            document,
            &Options {
                width: width as f32,
                scale: scale as f32,
                theme,
                // A guard until virtualization exists; a very long file is cut off here.
                max_height: Some(30_000.0),
            },
        );
        eprintln!(
            "mdview-qt: layout {width} dip at {scale:.2} scale in {:.1} ms, content {} px, page {}x{}",
            started.elapsed().as_secs_f64() * 1000.0,
            rendered.content_height,
            rendered.bitmap.width,
            rendered.bitmap.height
        );
        *self.page.borrow_mut() = Some(Page {
            bitmap: rendered.bitmap,
            width,
            scale,
            theme: theme.name,
        });
    }

    fn page_is_stale(&self, width: i32, scale: f64) -> bool {
        match &*self.page.borrow() {
            None => true,
            Some(page) => {
                page.width != width
                    || (page.scale - scale).abs() > 0.001
                    || page.theme != self.theme.get().name
            }
        }
    }
}

impl CanvasHandler for Shell {
    fn paint(&self, target: &mut PaintTarget<'_>) {
        let (width, height) = target.logical_size();
        if width <= 0 || height <= 0 {
            return;
        }
        let scale = target.dpr();
        let device_width = (width as f64 * scale).ceil() as i32;
        let device_height = (height as f64 * scale).ceil() as i32;
        self.viewport.set(device_height);
        if self.page_is_stale(width, scale) {
            self.layout(width, scale);
        }
        let page = self.page.borrow();
        let Some(page) = page.as_ref() else {
            return;
        };
        let theme = self.theme.get();
        let [r, g, b] = theme.bg;
        let background = [b, g, r, 255];
        let mut buffer = vec![0u8; (device_width * device_height * 4) as usize];
        for pixel in buffer.chunks_exact_mut(4) {
            pixel.copy_from_slice(&background);
        }
        let page_width = page.bitmap.width as i32;
        let page_height = page.bitmap.height as i32;
        let maximum = (page_height - device_height).max(0) as f64;
        let scroll = self.scroll.get().clamp(0.0, maximum).round() as i64;
        self.scroll.set(scroll as f64);
        let copy_width = page_width.min(device_width);
        let left = (device_width - copy_width) / 2;
        for y in 0..device_height {
            let source_y = scroll + y as i64;
            if source_y < 0 || source_y >= page_height as i64 {
                continue;
            }
            let source = ((source_y as i32 * page_width) * 4) as usize;
            let destination = ((y * device_width + left) * 4) as usize;
            let bytes = (copy_width * 4) as usize;
            buffer[destination..destination + bytes]
                .copy_from_slice(&page.bitmap.pixels[source..source + bytes]);
        }
        target.blit_painter(&buffer, device_width, device_height, true);
    }

    fn mouse_press(&self, _x: f64, _y: f64) {}

    fn mouse_move(&self, _x: f64, _y: f64) {}

    fn wheel(&self, _dx: i32, dy: i32) {
        let page_height = self
            .page
            .borrow()
            .as_ref()
            .map_or(0, |page| page.bitmap.height as i32);
        let maximum = (page_height - self.viewport.get()).max(0) as f64;
        let next = (self.scroll.get() - dy as f64 * 0.75).clamp(0.0, maximum);
        if next != self.scroll.get() {
            self.scroll.set(next);
            self.redraw();
        }
    }

    fn drag_enter(&self, _text: String) -> bool {
        true
    }

    fn dropped(&self, text: String) {
        let first = text.lines().next().unwrap_or("").trim();
        if first.is_empty() {
            return;
        }
        let path = first.strip_prefix("file://").unwrap_or(first);
        let path = PathBuf::from(percent_decode(path));
        if path.is_file() {
            self.load(&path);
        } else {
            self.message(&format!("not a file: {}", path.display()));
        }
    }

    fn timer(&self, _id: i32) {}

    fn update_request(&self) {}
}

/// `%20` and friends in a file URI. Enough for paths with spaces, which is what drag and drop sends.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn build(shell: &Rc<Shell>, file: Option<PathBuf>) {
    let window = H::<QMainWindow>::new();
    window.widget().set_title("mdview");
    window.widget().resize(1100, 900);

    let menus = window.menu_bar();
    let file_menu = menus.add_menu("&File");
    let quit = file_menu.add_action("&Quit");
    quit.on_triggered(|_| std::process::exit(0));
    let view_menu = menus.add_menu("&View");
    let light = view_menu.add_action("&Light theme");
    let dark = view_menu.add_action("&Dark theme");
    let help = menus.add_menu("&Help");
    let about = help.add_action("&About");
    let s = shell.clone();
    about.on_triggered(move |_| s.message("mdview 0.1 - Qt shell"));
    for (action, theme) in [(light, LIGHT), (dark, DARK)] {
        let s = shell.clone();
        action.on_triggered(move |_| {
            s.theme.set(theme);
            *s.page.borrow_mut() = None;
            s.redraw();
        });
    }

    let canvas = H::<Canvas>::new(shell.clone() as Rc<dyn CanvasHandler>);
    canvas.widget().accept_drops();
    let page = new_plain_widget();
    layout_with_margins(page, canvas.widget(), 0, 0);
    window.set_central(page);
    shell.canvas.set(Some(canvas));
    shell.window.set(Some(window));
    shell.status.set(Some(window.status_bar()));
    window.widget().show();

    shell.message("Drag a Markdown file here, or pass one on the command line");
    if let Some(path) = file {
        shell.load(&path);
    } else if shell.document.borrow().is_none() {
        *shell.document.borrow_mut() = Some(parse_bytes(WELCOME.as_bytes()));
        shell.redraw();
    }
}

/// `mdview-qt [FILE] [--light|--dark]`.
pub fn run() -> i32 {
    let mut file = None;
    let mut theme = LIGHT;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--light" => theme = LIGHT,
            "--dark" => theme = DARK,
            other if !other.starts_with("--") => file = Some(PathBuf::from(other)),
            other => eprintln!("mdview-qt: ignoring {other}"),
        }
    }
    let shell = Rc::new(Shell::new(theme));
    run_app(move || build(&shell, file))
}
