//! The Qt shell: a window, a menu bar and a canvas that shows the page `mdview-render` drew, with
//! the wheel scrolling it. Linux is the real target; this also builds against Qt on Windows so the
//! variant can be tested there. Safe Rust only; `qt/mod.rs` holds every `unsafe`.
#![forbid(unsafe_code)]

use crate::qt::ffi::*;
use crate::qt::*;
use mdview_core::parse_bytes;
use mdview_render::{DocumentRenderer, Hit, Options, PageLayout, Rect, Theme, DARK, LIGHT};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

const WELCOME: &str = "\
# mdview

A Markdown viewer with its own renderer.

Open a document by passing it on the command line, or dropping it on the window.

- Headings, lists, tables, alerts and code blocks are drawn by `mdview-render`.
- The wheel, the scrollbar, and Home/End, the arrows, Page Up/Down and Space scroll.
- File > Open picks a file.
";

/// Qt key codes, for the keys the canvas handles.
const KEY_HOME: i32 = 0x0100_0010;
const KEY_END: i32 = 0x0100_0011;
const KEY_LEFT: i32 = 0x0100_0012;
const KEY_UP: i32 = 0x0100_0013;
const KEY_RIGHT: i32 = 0x0100_0014;
const KEY_DOWN: i32 = 0x0100_0015;
const KEY_PAGE_UP: i32 = 0x0100_0016;
const KEY_PAGE_DOWN: i32 = 0x0100_0017;
const KEY_SPACE: i32 = 0x20;
const KEY_ESCAPE: i32 = 0x0100_0000;
/// One arrow-key step, in device px.
const STEP: f64 = 48.0;

/// The current find, as document offsets. `current` is an index into `hits`, not a text offset.
#[derive(Default)]
struct Find {
    query: String,
    hits: Vec<(u32, u32)>,
    current: usize,
    capped: bool,
}

struct Shell {
    window: Cell<Option<H<QMainWindow>>>,
    canvas: Cell<Option<H<Canvas>>>,
    status: Cell<Option<H<QStatusBar>>>,
    document: RefCell<Option<mdview_core::Document>>,
    layout: RefCell<Option<PageLayout>>,
    renderer: RefCell<DocumentRenderer>,
    path: RefCell<Option<PathBuf>>,
    scroll: Cell<f64>,
    scrollbar: Cell<Option<H<QScrollBar>>>,
    /// Set while the shell moves the scrollbar itself, so its signal does not echo back.
    syncing: Cell<bool>,
    viewport: Cell<i32>,
    theme: Cell<Theme>,
    zoom: Cell<f64>,
    /// Device px per logical px, as the last paint saw it, for mouse coordinates.
    scale: Cell<f64>,
    /// The options of the last paint, so a hit test lays out for the same width/scale/theme.
    options: RefCell<Option<Options>>,
    /// The selection, as two offsets into `Document::text`.
    selection: Cell<Option<(u32, u32)>>,
    /// Where the press that started a selection landed.
    anchor: Cell<Option<u32>>,
    /// The link or footnote under the press, followed on release if the pointer did not move.
    pressed_link: Cell<Option<u32>>,
    pressed_footnote: Cell<Option<u32>>,
    pressed_details: Cell<Option<u32>>,
    selecting: Cell<bool>,
    moved: Cell<bool>,
    /// A fragment to scroll to once the next layout exists, for links into another document.
    pending_anchor: RefCell<Option<String>>,
    /// The find bar, its input and the current matches.
    find_bar: Cell<Option<H<QToolBar>>>,
    find_edit: Cell<Option<H<QLineEdit>>>,
    find: RefCell<Find>,
    /// The outline dock and its heading items, each paired with its block.
    outline_dock: Cell<Option<H<QDockWidget>>>,
    outline_tree: Cell<Option<H<QTreeWidget>>>,
    outline_items: RefCell<Vec<(H<QTreeWidgetItem>, u32)>>,
}

impl Shell {
    fn new(theme: Theme, zoom: f64) -> Shell {
        Shell {
            window: Cell::new(None),
            canvas: Cell::new(None),
            status: Cell::new(None),
            document: RefCell::new(None),
            layout: RefCell::new(None),
            renderer: RefCell::new(DocumentRenderer::new()),
            path: RefCell::new(None),
            scroll: Cell::new(0.0),
            scrollbar: Cell::new(None),
            syncing: Cell::new(false),
            viewport: Cell::new(0),
            theme: Cell::new(theme),
            zoom: Cell::new(zoom),
            scale: Cell::new(1.0),
            options: RefCell::new(None),
            selection: Cell::new(None),
            anchor: Cell::new(None),
            pressed_link: Cell::new(None),
            pressed_footnote: Cell::new(None),
            pressed_details: Cell::new(None),
            selecting: Cell::new(false),
            moved: Cell::new(false),
            pending_anchor: RefCell::new(None),
            find_bar: Cell::new(None),
            find_edit: Cell::new(None),
            find: RefCell::new(Find::default()),
            outline_dock: Cell::new(None),
            outline_tree: Cell::new(None),
            outline_items: RefCell::new(Vec::new()),
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

    /// Opens the find bar and puts the caret in its input.
    fn show_find(&self) {
        if let Some(bar) = self.find_bar.get() {
            bar.widget().set_visible(true);
        }
        if let Some(edit) = self.find_edit.get() {
            edit.widget().set_focus();
            edit.select_all();
        }
    }

    fn hide_find(&self) {
        if let Some(bar) = self.find_bar.get() {
            bar.widget().set_visible(false);
        }
        self.find.borrow_mut().hits.clear();
        self.find.borrow_mut().query.clear();
        if let Some(canvas) = self.canvas.get() {
            canvas.widget().set_focus();
        }
        self.message("");
        self.redraw();
    }

    /// Runs find for `query` and jumps to the first hit. Empty input clears the marks.
    fn find_update(&self, query: &str) {
        let mut find = self.find.borrow_mut();
        find.query = query.to_string();
        find.hits.clear();
        find.current = 0;
        find.capped = false;
        if query.is_empty() {
            drop(find);
            self.message("");
            self.redraw();
            return;
        }
        let document = self.document.borrow();
        let Some(document) = document.as_ref() else {
            drop(find);
            return;
        };
        let result = mdview_core::find::find_visible(document, query, |block| {
            self.renderer.borrow().details_open(document, block)
        });
        find.hits = result.hits.iter().map(|hit| (hit.start, hit.end)).collect();
        find.capped = result.capped;
        let first = find.hits.first().copied();
        let total = find.hits.len();
        drop(document);
        drop(find);
        if let Some((from, to)) = first {
            self.scroll_to_find_range(from, to);
        }
        self.find_status(total);
        self.redraw();
    }

    fn find_step(&self, delta: i32) {
        let current = {
            let mut find = self.find.borrow_mut();
            let len = find.hits.len();
            if len == 0 {
                return;
            }
            let step = (find.current as i64 + delta as i64).rem_euclid(len as i64);
            find.current = step as usize;
            find.current
        };
        let range = self.find.borrow().hits.get(current).copied();
        if let Some((from, to)) = range {
            self.scroll_to_find_range(from, to);
        }
        self.find_status(self.find.borrow().hits.len());
        self.redraw();
    }

    fn find_status(&self, total: usize) {
        let find = self.find.borrow();
        if find.query.is_empty() {
            self.message("");
            return;
        }
        let text = if total == 0 {
            "No matches".to_string()
        } else {
            format!(
                "{} of {}{}",
                find.current + 1,
                total,
                if find.capped { "+" } else { "" }
            )
        };
        drop(find);
        self.message(&format!("Find: {text}"));
    }

    /// The page rectangles of the matches. `current_only` selects the active one.
    fn find_rects(&self, current_only: bool) -> Vec<Rect> {
        let (hits, current) = {
            let find = self.find.borrow();
            (find.hits.clone(), find.current)
        };
        if hits.is_empty() {
            return Vec::new();
        }
        if current_only {
            return hits
                .get(current)
                .map(|&(from, to)| self.rects_for(from, to))
                .unwrap_or_default();
        }
        // A pathological query can match thousands of times; drawing every match would make the
        // overlay cost more than the page. The window is enough to see where the hits are.
        hits.iter()
            .take(400)
            .flat_map(|&(from, to)| self.rects_for(from, to))
            .collect()
    }

    fn scroll_to_find_range(&self, from: u32, to: u32) {
        if let Some(top) = self
            .rects_for(from, to)
            .iter()
            .map(|rect| rect.y)
            .reduce(f32::min)
        {
            self.scroll_to_row(top);
        }
    }

    /// Rebuilds the heading tree from the current document. Levels nest, so an `<h3>` under an
    /// `<h2>` becomes its child; a skipped level starts a new top-level branch.
    fn build_outline(&self) {
        let Some(tree) = self.outline_tree.get() else {
            return;
        };
        let headings = {
            let document = self.document.borrow();
            let Some(document) = document.as_ref() else {
                return;
            };
            document
                .headings
                .iter()
                .map(|heading| {
                    (
                        heading.level,
                        document.str(heading.title).to_string(),
                        heading.block,
                    )
                })
                .collect::<Vec<_>>()
        };
        tree.clear();
        let mut items = self.outline_items.borrow_mut();
        items.clear();
        let mut stack: Vec<(u8, H<QTreeWidgetItem>)> = Vec::new();
        for (level, title, block) in headings {
            while stack.last().is_some_and(|(parent, _)| *parent >= level) {
                stack.pop();
            }
            let item = match stack.last() {
                Some((_, parent)) => parent.add_child(&title),
                None => tree.add_top(&title),
            };
            stack.push((level, item));
            items.push((item, block));
        }
        tree.expand_all();
    }

    fn show_outline(&self, visible: bool) {
        if let Some(dock) = self.outline_dock.get() {
            if visible {
                self.build_outline();
            }
            dock.widget().set_visible(visible);
        }
    }

    fn outline_click(&self, item: *mut QTreeWidgetItem) {
        let block = self
            .outline_items
            .borrow()
            .iter()
            .find(|(handle, _)| handle.raw() == item)
            .map(|(_, block)| *block);
        if let Some(block) = block {
            self.scroll_to_outline(block);
        }
    }

    fn scroll_to_outline(&self, block: u32) {
        let row = {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            match (document.as_ref(), layout.as_ref()) {
                (Some(document), Some(layout)) => layout.y_of(document, block),
                _ => None,
            }
        };
        if let Some(row) = row {
            self.scroll_to_row(row);
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
                self.renderer.borrow_mut().clear_details();
                *self.layout.borrow_mut() = None;
                self.scroll.set(0.0);
                self.selection.set(None);
                self.anchor.set(None);
                self.find.borrow_mut().query.clear();
                self.find.borrow_mut().hits.clear();
                if let Some(bar) = self.find_bar.get() {
                    bar.widget().set_visible(false);
                }
                self.pending_anchor.borrow_mut().take();
                self.build_outline();
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

    fn options(&self, width: i32, scale: f64) -> Options {
        let zoom = self.zoom.get();
        Options {
            width: (width as f64 / zoom) as f32,
            scale: (scale * zoom) as f32,
            theme: self.theme.get(),
            max_height: None,
            base_dir: self
                .path
                .borrow()
                .as_ref()
                .and_then(|path| path.parent().map(|parent| parent.to_path_buf())),
        }
    }

    /// The largest useful scroll offset, from the measured page and the visible height.
    fn scroll_max(&self, viewport: i32) -> f64 {
        let total = self
            .layout
            .borrow()
            .as_ref()
            .map_or(0.0, |layout| layout.total_height());
        (total - viewport as f32).max(0.0) as f64
    }

    /// Moves the scroll offset, keeping the scrollbar and the canvas in step.
    fn scroll_to(&self, value: f64) {
        let next = value.clamp(0.0, self.scroll_max(self.viewport.get()));
        if next != self.scroll.get() {
            self.scroll.set(next);
            self.sync_scrollbar_value();
            self.redraw();
        }
    }

    /// Keeps the scrollbar's position in step without the change echoing back.
    fn sync_scrollbar_value(&self) {
        let Some(bar) = self.scrollbar.get() else {
            return;
        };
        let value = self.scroll.get().round() as i32;
        if bar.value() != value {
            self.syncing.set(true);
            bar.set_value(value);
            self.syncing.set(false);
        }
    }

    /// Sizes the scrollbar to the page and the viewport, and places it at the current offset.
    fn sync_scrollbar_range(&self, viewport: i32) {
        let Some(bar) = self.scrollbar.get() else {
            return;
        };
        self.syncing.set(true);
        bar.set_range(0, self.scroll_max(viewport).ceil() as i32);
        bar.set_page_step(viewport);
        bar.set_single_step(STEP as i32);
        self.syncing.set(false);
        self.sync_scrollbar_value();
    }

    /// Home, End, the arrows and Page Up/Down, in device px.
    fn key(&self, code: i32) {
        if code == KEY_ESCAPE
            && self
                .find_bar
                .get()
                .is_some_and(|bar| bar.widget().is_visible())
        {
            self.hide_find();
            return;
        }
        let viewport = self.viewport.get() as f64;
        match code {
            KEY_DOWN | KEY_RIGHT => self.scroll_to(self.scroll.get() + STEP),
            KEY_UP | KEY_LEFT => self.scroll_to(self.scroll.get() - STEP),
            KEY_PAGE_DOWN | KEY_SPACE => self.scroll_to(self.scroll.get() + viewport * 0.9),
            KEY_PAGE_UP => self.scroll_to(self.scroll.get() - viewport * 0.9),
            KEY_HOME => self.scroll_to(0.0),
            KEY_END => self.scroll_to(f64::MAX),
            _ => {}
        }
    }

    /// Lays the page out without waiting for a paint event, for the scripted checks: a window that
    /// has been shown but not mapped yet has no paint yet on some platforms.
    fn ensure_layout(&self) {
        let Some(canvas) = self.canvas.get() else {
            return;
        };
        let width = canvas.widget().width();
        let scale = canvas.widget().dpr();
        if width <= 0 {
            return;
        }
        self.scale.set(scale);
        let options = self.options(width, scale);
        *self.options.borrow_mut() = Some(options.clone());
        let stale = match &*self.layout.borrow() {
            None => true,
            Some(page) => !page.matches(&options),
        };
        if stale {
            if let Some(document) = self.document.borrow().as_ref() {
                let page = self.renderer.borrow_mut().layout(document, &options);
                *self.layout.borrow_mut() = Some(page);
            }
        }
    }

    /// What is under a logical widget position, in page coordinates.
    fn hit(&self, x: f64, y: f64) -> Option<Hit> {
        let options = self.options.borrow().clone()?;
        let scale = self.scale.get();
        let document = self.document.borrow();
        let document = document.as_ref()?;
        let layout = self.layout.borrow();
        let layout = layout.as_ref()?;
        self.renderer.borrow_mut().hit_test(
            document,
            &options,
            layout,
            (x * scale) as f32,
            (y * scale + self.scroll.get()) as f32,
        )
    }

    /// The selected text's page rectangles, in device px.
    fn selection_rects(&self) -> Vec<Rect> {
        let Some((from, to)) = self.selection.get() else {
            return Vec::new();
        };
        self.rects_for(from, to)
    }

    /// Copies the selected document text to the system clipboard.
    fn copy_selection(&self) {
        let Some((from, to)) = self.selection.get() else {
            self.message("Copy: no selection");
            return;
        };
        let (from, to) = (from.min(to) as usize, from.max(to) as usize);
        if from == to {
            self.message("Copy: no selection");
            return;
        }
        let text = {
            let document = self.document.borrow();
            let Some(document) = document.as_ref() else {
                return;
            };
            document.text[from..to].to_string()
        };
        set_clipboard(&text);
        self.message(&format!("Copied {} characters", text.chars().count()));
    }

    /// The page rectangles a range would paint, whether or not it is the current selection.
    fn rects_for(&self, from: u32, to: u32) -> Vec<Rect> {
        if from == to {
            return Vec::new();
        }
        let Some(options) = self.options.borrow().clone() else {
            return Vec::new();
        };
        let document = self.document.borrow();
        let Some(document) = document.as_ref() else {
            return Vec::new();
        };
        let layout = self.layout.borrow();
        let Some(layout) = layout.as_ref() else {
            return Vec::new();
        };
        self.renderer
            .borrow_mut()
            .selection_rects(document, &options, layout, from, to)
    }

    fn press(&self, x: f64, y: f64) {
        let Some(hit) = self.hit(x, y) else {
            return;
        };
        self.anchor.set(Some(hit.offset));
        self.selection.set(Some((hit.offset, hit.offset)));
        self.pressed_link.set(hit.link);
        self.pressed_footnote.set(hit.footnote);
        self.pressed_details.set(hit.details);
        self.moved.set(false);
        self.selecting.set(true);
    }

    fn drag(&self, x: f64, y: f64) {
        if !self.selecting.get() {
            // Hovering: a link or a footnote mark gets the pointing hand.
            let pointing = self
                .hit(x, y)
                .is_some_and(|hit| hit.link.is_some() || hit.footnote.is_some());
            if let Some(canvas) = self.canvas.get() {
                canvas.set_cursor_shape(if pointing { 13 } else { 0 });
            }
            return;
        }
        let (Some(anchor), Some(hit)) = (self.anchor.get(), self.hit(x, y)) else {
            return;
        };
        if hit.offset != anchor {
            self.moved.set(true);
        }
        self.selection.set(Some((anchor, hit.offset)));
        self.redraw();
    }

    fn release(&self, _x: f64, _y: f64) {
        if !self.selecting.get() {
            return;
        }
        self.selecting.set(false);
        if self.moved.get() {
            self.pressed_link.set(None);
            self.pressed_footnote.set(None);
            self.pressed_details.set(None);
            return;
        }
        if let Some(link) = self.pressed_link.take() {
            self.follow_link(link);
        } else if let Some(note) = self.pressed_footnote.take() {
            self.follow_footnote(note);
        } else if let Some(details) = self.pressed_details.take() {
            self.toggle_details(details);
        } else {
            self.selection.set(None);
            self.redraw();
        }
    }

    /// Flips a `<details>`; the next paint measures the page again, because its body changes height.
    fn toggle_details(&self, block: u32) {
        let open = {
            let document = self.document.borrow();
            let Some(document) = document.as_ref() else {
                return;
            };
            self.renderer.borrow().details_open(document, block)
        };
        self.renderer.borrow_mut().set_details(block, !open);
        *self.layout.borrow_mut() = None;
        self.redraw();
    }

    /// A margin above a jumped-to anchor, so the heading is not glued to the top edge.
    fn jump_margin(&self) -> f64 {
        self.scale.get() * 24.0
    }

    fn scroll_to_row(&self, y: f32) {
        self.scroll_to((y as f64 - self.jump_margin()).max(0.0));
    }

    fn follow_link(&self, index: u32) {
        let destination = {
            let document = self.document.borrow();
            let Some(document) = document.as_ref() else {
                return;
            };
            document
                .links
                .get(index as usize)
                .map(|link| document.str(link.dest).to_string())
        };
        if let Some(destination) = destination {
            self.open_destination(&destination);
        }
    }

    fn follow_footnote(&self, index: u32) {
        let number = {
            let document = self.document.borrow();
            document
                .as_ref()
                .and_then(|document| document.footnotes.get(index as usize))
                .map(|note| note.number)
        };
        let Some(number) = number else {
            return;
        };
        let layout = self.layout.borrow();
        if let Some(layout) = layout.as_ref() {
            if let Some(y) = layout.footnote_y(number) {
                self.scroll_to_row(y);
            }
        }
    }

    /// A link destination: a fragment in this document, another local Markdown file (with an
    /// optional fragment) or a web/mail address. The shell loads files itself rather than handing
    /// markdown paths to the browser.
    fn open_destination(&self, destination: &str) {
        let (path, fragment) = match destination.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment)),
            None => (destination, None),
        };
        if path.is_empty() {
            if let Some(fragment) = fragment {
                self.jump_fragment(fragment);
            }
            return;
        }
        let lower = path.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
        {
            if !open_url(destination) {
                self.message(&format!("could not open {destination}"));
            }
            return;
        }
        let target = PathBuf::from(percent_decode(path));
        let resolved = if target.is_absolute() {
            target
        } else {
            match self.path.borrow().as_ref().and_then(|path| path.parent()) {
                Some(dir) => dir.join(target),
                None => target,
            }
        };
        if !resolved.is_file() {
            self.message(&format!("not a file: {}", resolved.display()));
            return;
        }
        self.load(&resolved);
        if let Some(fragment) = fragment {
            self.pending_anchor
                .borrow_mut()
                .replace(fragment.to_string());
        }
        self.redraw();
    }

    fn jump_fragment(&self, fragment: &str) {
        let block = {
            let document = self.document.borrow();
            document
                .as_ref()
                .and_then(|document| document.resolve_fragment(fragment))
        };
        let Some(block) = block else {
            self.message(&format!("no anchor named {fragment}"));
            return;
        };
        let layout = self.layout.borrow();
        let document = self.document.borrow();
        let mut jumped = false;
        if let (Some(layout), Some(document)) = (layout.as_ref(), document.as_ref()) {
            if let Some(y) = layout.y_of(document, block) {
                self.scroll_to_row(y);
                jumped = true;
            }
        }
        if !jumped {
            self.pending_anchor
                .borrow_mut()
                .replace(fragment.to_string());
        }
    }

    /// Applies a fragment saved while a linked document was still being laid out.
    fn apply_pending_anchor(&self, page: &PageLayout) {
        let Some(fragment) = self.pending_anchor.borrow_mut().take() else {
            return;
        };
        let document = self.document.borrow();
        let Some(document) = document.as_ref() else {
            return;
        };
        if let Some(block) = document.resolve_fragment(&fragment) {
            if let Some(y) = page.y_of(document, block) {
                self.scroll_to_row(y);
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
        let device_height = (height as f64 * scale).ceil() as i32;
        self.viewport.set(device_height);
        self.scale.set(scale);
        let options = self.options(width, scale);
        *self.options.borrow_mut() = Some(options.clone());
        let stale = match &*self.layout.borrow() {
            None => true,
            Some(page) => !page.matches(&options),
        };
        if stale {
            let Some(document) = &*self.document.borrow() else {
                return;
            };
            let started = std::time::Instant::now();
            let page = self.renderer.borrow_mut().layout(document, &options);
            eprintln!(
                "mdview-qt: height pass {width} dip at {scale:.2} scale in {:.1} ms, page {:.0} px",
                started.elapsed().as_secs_f64() * 1000.0,
                page.total_height()
            );
            *self.layout.borrow_mut() = Some(page);
        }
        if let Some(page) = &*self.layout.borrow() {
            self.apply_pending_anchor(page);
        }
        let scroll = self.scroll.get().clamp(0.0, self.scroll_max(device_height));
        self.scroll.set(scroll);
        self.sync_scrollbar_range(device_height);
        let Some(layout) = &*self.layout.borrow() else {
            return;
        };
        let Some(document) = &*self.document.borrow() else {
            return;
        };
        let started = std::time::Instant::now();
        self.renderer.borrow_mut().view_frame(
            document,
            &options,
            layout,
            scroll as f32,
            device_height as f32,
        );
        {
            let renderer = self.renderer.borrow();
            let frame = renderer.frame().expect("frame");
            target.blit_painter(
                &frame.bitmap.pixels,
                frame.bitmap.width as i32,
                frame.bitmap.height as i32,
                true,
            );
            eprintln!(
                "mdview-qt: painted {width}x{height} at scroll {scroll:.0} in {:.1} ms{}",
                started.elapsed().as_secs_f64() * 1000.0,
                match frame.band {
                    None => ", whole frame".to_string(),
                    Some((top, rows)) => format!(", band {top}+{rows}, shifted {}", frame.shift),
                }
            );
        }
        // Find marks and the selection are drawn over the frame, which stays the document's pixels
        // only.
        let marks: Vec<(f64, f64, f64, f64)> = self
            .find_rects(false)
            .iter()
            .map(|rect| (rect.x as f64, rect.y as f64, rect.w as f64, rect.h as f64))
            .collect();
        target.fill_rects(&marks, scroll, [255, 214, 0, 64]);
        let current: Vec<(f64, f64, f64, f64)> = self
            .find_rects(true)
            .iter()
            .map(|rect| (rect.x as f64, rect.y as f64, rect.w as f64, rect.h as f64))
            .collect();
        target.fill_rects(&current, scroll, [255, 145, 0, 112]);
        let rects: Vec<(f64, f64, f64, f64)> = self
            .selection_rects()
            .iter()
            .map(|rect| (rect.x as f64, rect.y as f64, rect.w as f64, rect.h as f64))
            .collect();
        let [r, g, b] = self.theme.get().accent;
        target.fill_rects(&rects, scroll, [r, g, b, 72]);
    }

    fn mouse_press(&self, x: f64, y: f64) {
        self.press(x, y);
    }

    fn mouse_move(&self, x: f64, y: f64) {
        self.drag(x, y);
    }

    fn mouse_release(&self, x: f64, y: f64) {
        self.release(x, y);
    }

    fn wheel(&self, _dx: i32, dy: i32) {
        self.scroll_to(self.scroll.get() - dy as f64 * 0.75);
    }

    fn key(&self, code: i32) {
        Shell::key(self, code);
    }

    fn find(&self) {
        self.show_find();
    }

    fn find_step(&self, delta: i32) {
        Shell::find_step(self, delta);
    }

    fn copy(&self) {
        self.copy_selection();
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Check {
    /// Drag over a word and report the selection, then exit.
    Select,
    /// The same, but leave the window open so a screenshot can show the selection.
    Hold,
    /// Click a link and report where it scrolled, then exit.
    Link,
    /// Click a `<details>` summary and report the height change, then exit.
    Details,
    /// Type in the find bar, step to the next match and report the marks, then exit.
    Find,
    /// The same, but leave the window open so a screenshot can show the find bar and marks.
    FindHold,
    /// Show the outline and jump to a heading, then exit.
    Outline,
    /// The same, but leave the window open so a screenshot can show the dock.
    OutlineHold,
    /// Copy the selected word and read it back from the clipboard, then exit.
    Copy,
}

/// Scripted checks through Qt's real mouse path, for `--select-test`, `--select-hold` and
/// `--link-test`; the same route a person's click takes, not a shortcut to the hit test.
fn run_check(shell: &Rc<Shell>, check: Check) {
    let Some(canvas) = shell.canvas.get() else {
        println!("check failed: no canvas");
        std::process::exit(1);
    };
    shell.ensure_layout();
    canvas.widget().repaint();
    if matches!(check, Check::Find | Check::FindHold) {
        shell.show_find();
        let bar_visible = shell
            .find_bar
            .get()
            .is_some_and(|bar| bar.widget().is_visible());
        // Go through the line edit's textChanged signal, which is what typing uses.
        if let Some(edit) = shell.find_edit.get() {
            edit.set_text("Markdown");
        }
        let hits = shell.find.borrow().hits.len();
        let marks = shell.find_rects(false).len();
        let current = shell.find_rects(true).len();
        shell.find_step(1);
        let index = shell.find.borrow().current;
        let ok = bar_visible && hits >= 2 && marks > 0 && current > 0 && index == 1;
        println!(
            "find test {}: {hits} matches, {marks} mark rects, {current} current rects, \
             bar {bar_visible}, index {index}",
            if ok { "ok" } else { "failed" }
        );
        if !ok {
            std::process::exit(1);
        }
        if check == Check::FindHold {
            return;
        }
        std::process::exit(0);
    }
    if check == Check::Copy {
        let range = {
            let document = shell.document.borrow();
            document.as_ref().and_then(|document| {
                document
                    .text
                    .find("Markdown")
                    .map(|at| (at as u32, (at + "Markdown".len()) as u32))
            })
        };
        let Some((from, to)) = range else {
            println!("copy test failed: no word");
            std::process::exit(1);
        };
        shell.selection.set(Some((from, to)));
        shell.copy_selection();
        let copied = read_clipboard();
        let ok = copied == "Markdown";
        println!(
            "copy test {}: {:?} at {from}..{to}, clipboard {} chars",
            if ok { "ok" } else { "failed" },
            copied,
            copied.chars().count()
        );
        std::process::exit(if ok { 0 } else { 1 });
    }
    if matches!(check, Check::Outline | Check::OutlineHold) {
        shell.show_outline(true);
        let dock_visible = shell
            .outline_dock
            .get()
            .is_some_and(|dock| dock.widget().is_visible());
        let entries = shell.outline_items.borrow().len();
        let target = {
            let document = shell.document.borrow();
            document.as_ref().and_then(|document| {
                document
                    .headings
                    .iter()
                    .find(|heading| document.str(heading.title) == "Diagrams")
                    .map(|heading| heading.block)
            })
        };
        let expected = target.and_then(|block| {
            let document = shell.document.borrow();
            let layout = shell.layout.borrow();
            match (document.as_ref(), layout.as_ref()) {
                (Some(document), Some(layout)) => layout
                    .y_of(document, block)
                    .map(|y| (y as f64 - shell.jump_margin()).max(0.0)),
                _ => None,
            }
        });
        if let Some(block) = target {
            shell.scroll_to_outline(block);
        }
        let moved = shell.scroll.get();
        let ok = dock_visible
            && entries >= 5
            && expected.is_some_and(|expected| moved > 0.0 && (moved - expected).abs() < 1.0);
        println!(
            "outline test {}: {entries} headings, dock {dock_visible}, scrolled to {moved:.0}, \
             expected {expected:?}",
            if ok { "ok" } else { "failed" }
        );
        if !ok {
            std::process::exit(1);
        }
        if check == Check::OutlineHold {
            return;
        }
        std::process::exit(0);
    }
    let scale = shell.scale.get();
    let scroll = shell.scroll.get();
    let word = match check {
        Check::Link => "Jump to diagrams",
        Check::Details => "Read more about this viewer",
        _ => "Markdown",
    };
    let start = {
        let document = shell.document.borrow();
        let Some(document) = document.as_ref() else {
            println!("check failed: no document");
            std::process::exit(1);
        };
        document.text.find(word).map(|at| at as u32)
    };
    let Some(start) = start else {
        println!("check failed: {word:?} is not in the document");
        std::process::exit(1);
    };
    let rects = shell.rects_for(start, start + word.len() as u32);
    let Some(rect) = rects.first().copied() else {
        println!("check failed: no rectangle for {word:?}");
        std::process::exit(1);
    };
    let to_local = |x: f32, y: f32| (x as f64 / scale, (y as f64 - scroll) / scale);
    let (x0, y0) = to_local(rect.x + 2.0, rect.y + rect.h / 2.0);
    let (x1, _) = to_local(rect.x + rect.w - 2.0, rect.y + rect.h / 2.0);
    match check {
        Check::Link => {
            canvas.send_mouse(EV_MOUSE_PRESS, x0, y0);
            canvas.send_mouse(EV_MOUSE_RELEASE, x0, y0);
            let moved = shell.scroll.get();
            let expected = {
                let document = shell.document.borrow();
                let layout = shell.layout.borrow();
                match (document.as_ref(), layout.as_ref()) {
                    (Some(document), Some(layout)) => document
                        .resolve_fragment("diagrams")
                        .and_then(|block| layout.y_of(document, block))
                        .map(|y| (y as f64 - shell.jump_margin()).max(0.0)),
                    _ => None,
                }
            };
            let ok = moved > 0.0 && expected.is_some_and(|expected| (moved - expected).abs() < 1.0);
            println!(
                "link test {}: scrolled to {moved:.0}, expected {expected:?}",
                if ok { "ok" } else { "failed" }
            );
            std::process::exit(if ok { 0 } else { 1 });
        }
        Check::Details => {
            let before = shell
                .layout
                .borrow()
                .as_ref()
                .map_or(0.0, |page| page.total_height());
            canvas.send_mouse(EV_MOUSE_PRESS, x0, y0);
            canvas.send_mouse(EV_MOUSE_RELEASE, x0, y0);
            shell.ensure_layout();
            let after = shell
                .layout
                .borrow()
                .as_ref()
                .map_or(0.0, |page| page.total_height());
            let ok = after > before + 20.0;
            println!(
                "details test {}: {before:.0} -> {after:.0} px",
                if ok { "ok" } else { "failed" }
            );
            std::process::exit(if ok { 0 } else { 1 });
        }
        Check::Select
        | Check::Hold
        | Check::Find
        | Check::FindHold
        | Check::Outline
        | Check::OutlineHold
        | Check::Copy => {
            canvas.send_mouse(EV_MOUSE_PRESS, x0, y0);
            canvas.send_mouse(EV_MOUSE_MOVE, x1, y0);
            canvas.send_mouse(EV_MOUSE_RELEASE, x1, y0);
            let selection = shell.selection.get();
            let selected = {
                let document = shell.document.borrow();
                match (document.as_ref(), selection) {
                    (Some(document), Some((from, to))) => {
                        document.text[from.min(to) as usize..from.max(to) as usize].to_string()
                    }
                    _ => String::new(),
                }
            };
            let painted = shell.selection_rects().len();
            let ok = painted > 0 && selected.contains(word);
            println!(
                "select test {}: selection {:?} of {:?}, {painted} rects",
                if ok { "ok" } else { "failed" },
                selection,
                selected
            );
            if !ok {
                std::process::exit(1);
            }
            if check == Check::Hold {
                return;
            }
            std::process::exit(0);
        }
    }
}

fn build(
    shell: &Rc<Shell>,
    file: Option<PathBuf>,
    drop_test: Option<PathBuf>,
    check: Option<Check>,
    scroll: f64,
) {
    let window = H::<QMainWindow>::new();
    window.widget().set_title("mdview");
    window.widget().resize(1100, 900);

    let menus = window.menu_bar();
    let file_menu = menus.add_menu("&File");
    let open = file_menu.add_action("&Open...");
    let s = shell.clone();
    open.on_triggered(move |_| {
        let dir = s
            .path
            .borrow()
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        if let Some(window) = s.window.get() {
            if let Some(path) = choose_file(window.widget(), &dir) {
                s.load(&path);
            }
        }
    });
    let quit = file_menu.add_action("&Quit");
    quit.on_triggered(|_| std::process::exit(0));
    let edit_menu = menus.add_menu("&Edit");
    let find = edit_menu.add_action("&Find...\tCtrl+F");
    let find_next = edit_menu.add_action("Find &Next\tF3");
    let find_previous = edit_menu.add_action("Find &Previous\tShift+F3");
    let s = shell.clone();
    find.on_triggered(move |_| s.show_find());
    let s = shell.clone();
    find_next.on_triggered(move |_| s.find_step(1));
    let s = shell.clone();
    find_previous.on_triggered(move |_| s.find_step(-1));
    let copy = edit_menu.add_action("&Copy\tCtrl+C");
    let s = shell.clone();
    copy.on_triggered(move |_| s.copy_selection());

    let find_bar = window.add_tool_bar("Find");
    find_bar.widget().set_visible(false);
    let find_edit = H::<QLineEdit>::new();
    find_edit.set_placeholder("Find in document");
    find_bar.add_widget(find_edit.widget());
    let previous = find_bar.add_action("Previous");
    let next = find_bar.add_action("Next");
    let close = find_bar.add_action("Close");
    let s = shell.clone();
    find_edit.on_text_changed(move |text| s.find_update(&text));
    let s = shell.clone();
    find_edit.on_return_pressed(move || s.find_step(1));
    let s = shell.clone();
    previous.on_triggered(move |_| s.find_step(-1));
    let s = shell.clone();
    next.on_triggered(move |_| s.find_step(1));
    let s = shell.clone();
    close.on_triggered(move |_| s.hide_find());
    shell.find_bar.set(Some(find_bar));
    shell.find_edit.set(Some(find_edit));

    let outline_dock = H::<QDockWidget>::new("Outline", window.clone());
    let outline_tree = H::<QTreeWidget>::new();
    outline_dock.set_widget(outline_tree.widget());
    window.add_dock(true, outline_dock);
    outline_dock.widget().set_visible(false);
    outline_dock.widget().resize(240, 700);
    let s = shell.clone();
    outline_tree.on_item_clicked(move |item| s.outline_click(item));
    shell.outline_dock.set(Some(outline_dock));
    shell.outline_tree.set(Some(outline_tree));

    let view_menu = menus.add_menu("&View");
    let outline = view_menu.add_action("&Outline");
    let s = shell.clone();
    outline.on_triggered(move |_| {
        let visible = s
            .outline_dock
            .get()
            .is_some_and(|dock| dock.widget().is_visible());
        s.show_outline(!visible);
    });
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
            *s.layout.borrow_mut() = None;
            s.redraw();
        });
    }

    let canvas = H::<Canvas>::new(shell.clone() as Rc<dyn CanvasHandler>);
    canvas.widget().accept_drops();
    canvas.widget().track_mouse();
    canvas.widget().set_focus_policy(11); // Qt::StrongFocus, so the keys arrive
    let page = new_plain_widget();
    let bar = H::<QScrollBar>::vertical(page);
    bar.widget().fix_width(14);
    let s = shell.clone();
    bar.on_value_changed(move |value| {
        if s.syncing.get() {
            return;
        }
        s.scroll.set(value as f64);
        s.redraw();
    });
    layout_row(page, canvas.widget(), bar.widget());
    window.set_central(page);
    shell.canvas.set(Some(canvas));
    shell.scrollbar.set(Some(bar));
    shell.window.set(Some(window));
    shell.status.set(Some(window.status_bar()));
    window.widget().show();
    if let Some(canvas) = shell.canvas.get() {
        canvas.widget().set_focus();
    }

    shell.message("Drag a Markdown file here, or pass one on the command line");
    if let Some(path) = file {
        shell.load(&path);
    } else if shell.document.borrow().is_none() {
        *shell.document.borrow_mut() = Some(parse_bytes(WELCOME.as_bytes()));
        shell.redraw();
    }

    // `--drop-test` sends one drop through the window's event path (Qt uses OLE, so a WM_DROPFILES
    // from another process never arrives) and exits with the result, for scripted checks.
    if let Some(check) = check {
        if scroll > 0.0 {
            shell.scroll.set(scroll);
        }
        run_check(shell, check);
    } else if scroll > 0.0 {
        shell.scroll.set(scroll);
        shell.redraw();
    }
    if let Some(target) = drop_test {
        let (x, y) = canvas.widget().center_in_window();
        let text = target.display().to_string();
        let sent = window
            .widget()
            .window_handle()
            .map(|window| window.send_drag_and_drop(x, y, &text));
        let loaded = shell.path.borrow().as_deref() == Some(target.as_path());
        match sent {
            Some((entered, dropped)) if loaded => {
                println!(
                    "drop test ok: {} entered={entered} dropped={dropped}",
                    target.display()
                );
                std::process::exit(0);
            }
            Some((entered, dropped)) => {
                println!(
                    "drop test failed: {} entered={entered} dropped={dropped}",
                    target.display()
                );
                std::process::exit(1);
            }
            None => {
                println!("drop test failed: no window handle");
                std::process::exit(1);
            }
        }
    }
}

/// `mdview-qt [FILE] [--light|--dark] [--select-test|--select-hold|--link-test]`.
pub fn run() -> i32 {
    let mut file = None;
    let mut drop_test = None;
    let mut check = None;
    let mut theme = LIGHT;
    let mut zoom = 1.0f64;
    let mut scroll = 0.0f64;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--light" => theme = LIGHT,
            "--dark" => theme = DARK,
            "--scroll" => {
                scroll = args
                    .next()
                    .expect("--scroll needs a number")
                    .parse()
                    .unwrap();
            }
            "--select-test" => check = Some(Check::Select),
            "--select-hold" => check = Some(Check::Hold),
            "--link-test" => check = Some(Check::Link),
            "--details-test" => check = Some(Check::Details),
            "--find-test" => check = Some(Check::Find),
            "--find-hold" => check = Some(Check::FindHold),
            "--outline-test" => check = Some(Check::Outline),
            "--outline-hold" => check = Some(Check::OutlineHold),
            "--copy-test" => check = Some(Check::Copy),
            "--zoom" => {
                zoom = args.next().expect("--zoom needs a number").parse().unwrap();
            }
            "--drop-test" => {
                drop_test = Some(PathBuf::from(
                    args.next().expect("--drop-test needs a path"),
                ));
            }
            other if !other.starts_with("--") => file = Some(PathBuf::from(other)),
            other => eprintln!("mdview-qt: ignoring {other}"),
        }
    }
    let shell = Rc::new(Shell::new(theme, zoom.clamp(0.5, 3.0)));
    run_app(move || build(&shell, file, drop_test, check, scroll))
}
