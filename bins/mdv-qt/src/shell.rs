//! The Qt shell: a window, a menu bar and a canvas that shows the page `mdview-render` drew, with
//! the wheel scrolling it. Linux is the real target; this also builds against Qt on Windows so the
//! variant can be tested there. Safe Rust only; `qt/mod.rs` holds every `unsafe`.
#![forbid(unsafe_code)]

use crate::qt::ffi::*;
use crate::qt::*;
use mdview_core::commands::strings;
use mdview_core::files::{canonical_or_absolute, read_stable, stamp, tab_labels, FileStamp};
use mdview_core::links::{classify_link, LinkAction};
use mdview_core::session::{Recents, Session, Theme as ThemeChoice};
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
const KEY_TAB: i32 = 0x0100_0001;
/// Shift+Tab arrives as Backtab.
const KEY_BACKTAB: i32 = 0x0100_0002;
const KEY_F4: i32 = 0x0100_0033;
const KEY_F5: i32 = 0x0100_0034;
const KEY_PLUS: i32 = 0x2b;
const KEY_EQUAL: i32 = 0x3d;
const KEY_MINUS: i32 = 0x2d;
const KEY_0: i32 = 0x30;
const KEY_F: i32 = 0x46;
/// Zoom limits and step, from the original (60-250%, step 10).
const ZOOM_MIN: f64 = 0.6;
const ZOOM_MAX: f64 = 2.5;
const ZOOM_STEP: f64 = 0.1;
const KEY_O: i32 = 0x4f;
const KEY_W: i32 = 0x57;
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

/// A tab's state while another tab is showing. The showing tab's state lives in `Shell`'s own
/// fields, so everything that reads the page keeps working; switching swaps the two.
struct Page {
    path: PathBuf,
    document: Option<mdview_core::Document>,
    renderer: DocumentRenderer,
    scroll: f64,
    selection: Option<(u32, u32)>,
    /// What the file looked like when last read; the poll compares against it.
    stamp: FileStamp,
}

struct Shell {
    /// Open documents in tab order, and which one is showing. `None` shows the empty page.
    pages: RefCell<Vec<Page>>,
    current: Cell<Option<usize>>,
    tab_bar: Cell<Option<H<QTabBar>>>,
    /// Set while the shell changes the tab bar itself, so its signals do not re-enter.
    tabs_busy: Cell<bool>,
    /// The tab context menu, created on first use and rebuilt for each right click.
    tab_popup: Cell<Option<H<QMenu>>>,
    recent: RefCell<Recents>,
    /// The saved session, kept so keys this shell does not use round-trip. `None` with the
    /// session off (`--no-session` and the scripted checks).
    session: RefCell<Option<(PathBuf, Session)>>,
    window: Cell<Option<H<QMainWindow>>>,
    canvas: Cell<Option<H<Canvas>>>,
    status: Cell<Option<H<QStatusBar>>>,
    menu_bar: Cell<Option<H<QMenuBar>>>,
    /// Focus reading mode, and what was visible before it: (menu bar, status bar, outline, find).
    focus: Cell<Option<[bool; 4]>>,
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
    /// What the View menu picked; `theme` is what it resolves to (System follows the desktop).
    choice: Cell<ThemeChoice>,
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
    outline_tree: RefCell<Option<Tree>>,
    /// The Files dock: the open folder, its tree, and the file behind each tree row (None = folder).
    files_dock: Cell<Option<H<QDockWidget>>>,
    files_tree: RefCell<Option<Tree>>,
    folder: RefCell<Option<PathBuf>>,
    folder_rows: RefCell<Vec<Option<PathBuf>>>,
    /// Paths sent by later launches, drained on the poll tick.
    incoming: RefCell<Option<std::sync::mpsc::Receiver<PathBuf>>>,
    /// The AppImage update check or apply in flight, if any.
    updating: RefCell<Option<std::sync::mpsc::Receiver<crate::appimage_update::Status>>>,
    /// Manage recent files: a dock with the list, a toolbar with its buttons, the chosen row.
    manage_dock: Cell<Option<H<QDockWidget>>>,
    manage_bar: Cell<Option<H<QToolBar>>>,
    manage_tree: RefCell<Option<Tree>>,
    manage_rows: RefCell<Vec<PathBuf>>,
    manage_sel: Cell<Option<usize>>,
    /// The block of each outline item, by the item's index in the tree.
    outline_blocks: RefCell<Vec<u32>>,
}

impl Shell {
    fn new(theme: Theme, zoom: f64) -> Shell {
        Shell {
            pages: RefCell::new(Vec::new()),
            current: Cell::new(None),
            tab_bar: Cell::new(None),
            tabs_busy: Cell::new(false),
            tab_popup: Cell::new(None),
            recent: RefCell::new(Recents::default()),
            session: RefCell::new(None),
            window: Cell::new(None),
            canvas: Cell::new(None),
            status: Cell::new(None),
            menu_bar: Cell::new(None),
            focus: Cell::new(None),
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
            choice: Cell::new(if theme.name == DARK.name { ThemeChoice::Dark } else { ThemeChoice::Light }),
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
            outline_tree: RefCell::new(None),
            files_dock: Cell::new(None),
            files_tree: RefCell::new(None),
            folder: RefCell::new(None),
            folder_rows: RefCell::new(Vec::new()),
            incoming: RefCell::new(None),
            updating: RefCell::new(None),
            manage_dock: Cell::new(None),
            manage_bar: Cell::new(None),
            manage_tree: RefCell::new(None),
            manage_rows: RefCell::new(Vec::new()),
            manage_sel: Cell::new(None),
            outline_blocks: RefCell::new(Vec::new()),
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
        let Some(tree) = self.outline_tree.borrow().clone() else {
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
        let entries: Vec<(u8, &str)> = headings
            .iter()
            .map(|(level, title, _)| (*level, title.as_str()))
            .collect();
        tree.set_items(&entries);
        *self.outline_blocks.borrow_mut() = headings.iter().map(|(_, _, block)| *block).collect();
    }

    fn show_outline(&self, visible: bool) {
        if let Some(dock) = self.outline_dock.get() {
            if visible {
                self.build_outline();
            }
            dock.widget().set_visible(visible);
        }
    }

    /// Shows `dir` in the Files dock: folders and Markdown files, hidden names skipped.
    fn open_folder(&self, dir: &Path) {
        let mut rows: Vec<(u8, String, Option<PathBuf>)> = Vec::new();
        scan_folder(dir, 1, &mut rows);
        let titles: Vec<(u8, &str)> = rows.iter().map(|(l, t, _)| (*l, t.as_str())).collect();
        if let Some(tree) = self.files_tree.borrow().as_ref() {
            tree.set_items(&titles);
        }
        *self.folder_rows.borrow_mut() = rows.into_iter().map(|(_, _, p)| p).collect();
        *self.folder.borrow_mut() = Some(dir.to_path_buf());
        if let Some(dock) = self.files_dock.get() {
            dock.widget().set_visible(true);
        }
        self.message(&format!("Folder {}", dir.display()));
        if let Some((_, session)) = self.session.borrow_mut().as_mut() {
            session.folder = Some(dir.to_path_buf());
        }
        self.save_session();
    }

    /// Fills the Manage recent list from Recent, missing files marked.
    fn manage_fill(&self) {
        let entries = self.recent.borrow().entries();
        let titles: Vec<(u8, &str)> = entries.iter().map(|e| (1u8, e.label.as_str())).collect();
        if let Some(tree) = self.manage_tree.borrow().as_ref() {
            tree.set_items(&titles);
        }
        *self.manage_rows.borrow_mut() = entries.into_iter().map(|e| e.path).collect();
        self.manage_sel.set(None);
    }

    fn manage_show(&self, visible: bool) {
        if visible {
            self.manage_fill();
        }
        if let Some(dock) = self.manage_dock.get() {
            dock.widget().set_visible(visible);
        }
        if let Some(bar) = self.manage_bar.get() {
            bar.widget().set_visible(visible);
        }
    }

    fn manage_remove(&self) {
        let path = self
            .manage_sel
            .get()
            .and_then(|i| self.manage_rows.borrow().get(i).cloned());
        if let Some(path) = path {
            self.recent.borrow_mut().remove(&path);
            self.save_session();
            self.manage_fill();
        }
    }

    fn manage_clear(&self) {
        self.recent.borrow_mut().clear();
        self.save_session();
        self.manage_fill();
        self.message(strings::RECENT_CLEARED);
    }

    fn manage_open(&self) {
        let path = self
            .manage_sel
            .get()
            .and_then(|i| self.manage_rows.borrow().get(i).cloned());
        if let Some(path) = path.filter(|p| p.exists()) {
            self.manage_show(false);
            self.open(&path);
        }
    }

    fn choose_folder(&self) {
        let start = self
            .folder
            .borrow()
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        if let Some(window) = self.window.get() {
            if let Some(dir) = choose_folder(window.widget(), &start) {
                self.open_folder(&dir);
            }
        }
    }

    fn files_click(&self, index: usize) {
        let path = self.folder_rows.borrow().get(index).cloned().flatten();
        if let Some(path) = path {
            self.open(&path);
        }
    }

    fn outline_click(&self, index: usize) {
        let block = self.outline_blocks.borrow().get(index).copied();
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

    /// Opens `path` in a new tab, or switches to the tab that already shows it. Adds it to Recent
    /// either way. False when the file could not be read.
    fn open(&self, path: &Path) -> bool {
        let path = canonical_or_absolute(path);
        let existing = self
            .pages
            .borrow()
            .iter()
            .position(|page| same_file(&page.path, &path));
        if let Some(index) = existing {
            self.recent.borrow_mut().add(path);
            self.switch_to(index);
            self.save_session();
            return true;
        }
        match std::fs::read(&path) {
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
                let trace = std::env::var_os("MDVIEW_STARTUP_TRACE").is_some();
                let t = std::time::Instant::now();
                self.recent.borrow_mut().add(path.clone());
                let index = self.add_page(path, document);
                if trace {
                    eprintln!("mdview-qt: open: add_page {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
                }
                self.switch_to(index);
                if trace {
                    eprintln!("mdview-qt: open: switch_to {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
                }
                self.save_session();
                true
            }
            Err(e) => {
                self.message(&format!("{}: {e}", path.display()));
                eprintln!("mdview-qt: {}: {e}", path.display());
                false
            }
        }
    }

    /// Appends a tab without showing it, and returns its index.
    fn add_page(&self, path: PathBuf, document: mdview_core::Document) -> usize {
        let index = {
            let mut pages = self.pages.borrow_mut();
            pages.push(Page {
                stamp: stamp(&path),
                path,
                document: Some(document),
                renderer: DocumentRenderer::new(),
                scroll: 0.0,
                selection: None,
            });
            pages.len() - 1
        };
        let trace = std::env::var_os("MDVIEW_STARTUP_TRACE").is_some();
        let t = std::time::Instant::now();
        if let Some(bar) = self.tab_bar.get() {
            self.tabs_busy.set(true);
            bar.add("");
            self.tabs_busy.set(false);
        }
        if trace {
            eprintln!("mdview-qt: add_page: bar.add {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
        self.relabel();
        if trace {
            eprintln!("mdview-qt: add_page: relabel {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
        index
    }

    /// Restores a saved tab. A file that is gone still gets its tab, saying so, as the original did.
    fn restore_page(&self, path: &Path) {
        let path = canonical_or_absolute(path);
        if self
            .pages
            .borrow()
            .iter()
            .any(|page| same_file(&page.path, &path))
        {
            return;
        }
        let document = match std::fs::read(&path) {
            Ok(bytes) => parse_bytes(&bytes),
            Err(_) => {
                let name = file_name(&path);
                parse_bytes(format!("# {name}\n\n{}\n", strings::FILE_NOT_FOUND).as_bytes())
            }
        };
        self.add_page(path, document);
    }

    /// Selects tab `index` in the bar and shows it.
    fn switch_to(&self, index: usize) {
        if let Some(bar) = self.tab_bar.get() {
            self.tabs_busy.set(true);
            bar.set_current(index);
            self.tabs_busy.set(false);
        }
        self.activate(index);
    }

    /// Shows tab `index`, keeping the state of the tab it replaces.
    fn activate(&self, index: usize) {
        if self.current.get() == Some(index) || index >= self.pages.borrow().len() {
            return;
        }
        self.stash();
        self.current.set(Some(index));
        let (path, document, renderer, scroll, selection) = {
            let mut pages = self.pages.borrow_mut();
            let page = &mut pages[index];
            (
                page.path.clone(),
                page.document.take(),
                std::mem::take(&mut page.renderer),
                page.scroll,
                page.selection,
            )
        };
        *self.document.borrow_mut() = document;
        *self.renderer.borrow_mut() = renderer;
        let name = file_name(&path);
        *self.path.borrow_mut() = Some(path);
        self.show_page(scroll, selection);
        self.set_title(&name);
        self.message(&format!("{name}: {} blocks", self.blocks()));
    }

    /// Moves the showing tab's state back into its `Page`.
    fn stash(&self) {
        let Some(index) = self.current.get() else {
            return;
        };
        let mut pages = self.pages.borrow_mut();
        let Some(page) = pages.get_mut(index) else {
            return;
        };
        page.document = self.document.borrow_mut().take();
        page.renderer = std::mem::take(&mut *self.renderer.borrow_mut());
        page.scroll = self.scroll.get();
        page.selection = self.selection.get();
    }

    /// Resets what belongs to the page on screen: layout, find, outline, pending jumps.
    fn show_page(&self, scroll: f64, selection: Option<(u32, u32)>) {
        *self.layout.borrow_mut() = None;
        self.scroll.set(scroll);
        self.selection.set(selection);
        self.anchor.set(None);
        self.find.borrow_mut().query.clear();
        self.find.borrow_mut().hits.clear();
        if let Some(bar) = self.find_bar.get() {
            bar.widget().set_visible(false);
        }
        self.pending_anchor.borrow_mut().take();
        // The outline tree is only filled while it is showing (View > Outline fills it on open).
        if self
            .outline_dock
            .get()
            .is_some_and(|dock| dock.widget().is_visible())
        {
            self.build_outline();
        }
        self.redraw();
    }

    /// Closes tab `index`. The last one closing shows the empty page; the app stays open.
    fn close_tab(&self, index: usize) {
        if index >= self.pages.borrow().len() {
            return;
        }
        match self.current.get() {
            // The showing tab's state is in `Shell` and is simply replaced, so nothing to stash.
            Some(current) if current == index => self.current.set(None),
            Some(current) if current > index => self.current.set(Some(current - 1)),
            _ => {}
        }
        self.pages.borrow_mut().remove(index);
        let next = self.tab_bar.get().and_then(|bar| {
            self.tabs_busy.set(true);
            bar.remove(index);
            self.tabs_busy.set(false);
            bar.current()
        });
        self.relabel();
        if self.pages.borrow().is_empty() {
            self.show_empty(strings::STATUS_ALL_CLOSED);
        } else if self.current.get().is_none() {
            let last = self.pages.borrow().len() - 1;
            self.activate(next.unwrap_or(index).min(last));
        }
        self.save_session();
    }

    /// Closes every tab but `keep`, last first so the indices ahead stay valid.
    fn close_others(&self, keep: usize) {
        let count = self.pages.borrow().len();
        for index in (0..count).rev() {
            if index != keep {
                self.close_tab(index);
            }
        }
    }

    /// The tab context menu: shown over a tab only, as in the original.
    fn tab_menu(self: &Rc<Self>, tab: Option<usize>, at: (i32, i32)) {
        let Some(index) = tab.filter(|&i| i < self.pages.borrow().len()) else {
            return;
        };
        let menu = match self.tab_popup.get() {
            Some(menu) => menu,
            None => {
                let menu = H::<QMenu>::new_popup();
                self.tab_popup.set(Some(menu));
                menu
            }
        };
        menu.clear();
        let path = self.pages.borrow()[index].path.clone();
        let s = self.clone();
        menu.add_action("&Close tab").on_triggered(move |_| s.close_tab(index));
        let others = menu.add_action("Close &other tabs");
        others.set_enabled(self.pages.borrow().len() > 1);
        let s = self.clone();
        others.on_triggered(move |_| s.close_others(index));
        menu.add_separator();
        let copy_path = path.clone();
        menu.add_action("Copy file &path")
            .on_triggered(move |_| set_clipboard(&copy_path.display().to_string()));
        menu.add_action("&Show in file manager")
            .on_triggered(move |_| reveal_in_file_manager(&path));
        menu.popup(at);
    }

    fn close_all(&self) {
        while !self.pages.borrow().is_empty() {
            let last = self.pages.borrow().len() - 1;
            self.close_tab(last);
        }
    }

    /// Ctrl+Tab and Ctrl+Shift+Tab: the next or previous tab, wrapping. Nothing with one tab.
    fn cycle(&self, delta: isize) {
        let count = self.pages.borrow().len();
        let Some(current) = self.current.get() else {
            return;
        };
        if count > 1 {
            let next = (current as isize + delta).rem_euclid(count as isize) as usize;
            self.switch_to(next);
        }
    }

    /// A tab was dragged from `from` to `to`; the pages follow, and so does the current index.
    fn tab_moved(&self, from: usize, to: usize) {
        {
            let mut pages = self.pages.borrow_mut();
            if from >= pages.len() || to >= pages.len() {
                return;
            }
            let page = pages.remove(from);
            pages.insert(to, page);
        }
        if let Some(current) = self.current.get() {
            let moved = if current == from {
                to
            } else if from < current && current <= to {
                current - 1
            } else if to <= current && current < from {
                current + 1
            } else {
                current
            };
            self.current.set(Some(moved));
        }
        self.save_session();
    }

    /// Tab titles from core: repeated filenames gain parent folders until they differ.
    fn relabel(&self) {
        let Some(bar) = self.tab_bar.get() else {
            return;
        };
        let paths: Vec<PathBuf> = self.pages.borrow().iter().map(|p| p.path.clone()).collect();
        for (index, label) in tab_labels(&paths).iter().enumerate() {
            if index < bar.count() {
                bar.set_label(index, label, &paths[index].display().to_string());
            }
        }
    }

    /// The page shown with no tab open.
    fn show_empty(&self, status: &str) {
        self.current.set(None);
        *self.path.borrow_mut() = None;
        *self.document.borrow_mut() = Some(parse_bytes(WELCOME.as_bytes()));
        *self.renderer.borrow_mut() = DocumentRenderer::new();
        self.show_page(0.0, None);
        if let Some(window) = self.window.get() {
            window.widget().set_title(&strings::window_title(None));
        }
        self.message(status);
    }

    /// Writes tabs, the current tab and Recent at once, so a crash loses nothing.
    fn save_session(&self) {
        let mut session = self.session.borrow_mut();
        let Some((path, session)) = session.as_mut() else {
            return;
        };
        session.tabs = self.pages.borrow().iter().map(|p| p.path.clone()).collect();
        session.active = self.current.get().unwrap_or(0);
        session.recent = self.recent.borrow().clone();
        session.theme = self.choice.get();
        session.zoom = (self.zoom.get() * 100.0).round() as u16;
        if let Err(e) = session.save(path) {
            eprintln!("mdview-qt: could not save {}: {e}", path.display());
        }
    }

    /// Rebuilds File > Open recent as it opens, so missing files are marked as of now.
    fn fill_recent(self: &Rc<Self>, menu: H<QMenu>) {
        menu.clear();
        let entries = self.recent.borrow().entries();
        if entries.is_empty() {
            menu.add_action(strings::RECENT_EMPTY).set_enabled(false);
        }
        for entry in entries {
            let action = menu.add_action(&entry.label.replace('&', "&&"));
            action.set_tool_tip(&entry.path.display().to_string());
            let s = self.clone();
            action.on_triggered(move |_| {
                if entry.missing {
                    s.recent.borrow_mut().remove(&entry.path);
                    s.save_session();
                    s.message(strings::RECENT_REMOVED_MISSING);
                } else {
                    s.open(&entry.path);
                }
            });
        }
        menu.add_separator();
        let clear = menu.add_action("&Clear recent files");
        let s = self.clone();
        clear.on_triggered(move |_| {
            s.recent.borrow_mut().clear();
            s.save_session();
            s.message(strings::RECENT_CLEARED);
        });
    }

    fn blocks(&self) -> usize {
        self.document
            .borrow()
            .as_ref()
            .map_or(0, |doc| doc.blocks.len())
    }

    /// Applies a theme choice; System follows the desktop's colour scheme.
    fn set_theme_choice(&self, choice: ThemeChoice) {
        self.choice.set(choice);
        self.theme.set(match choice {
            ThemeChoice::Light => LIGHT,
            ThemeChoice::Dark => DARK,
            ThemeChoice::System => {
                if system_is_dark() {
                    DARK
                } else {
                    LIGHT
                }
            }
        });
        *self.layout.borrow_mut() = None;
        self.redraw();
        self.save_session();
    }

    /// Sets the zoom (clamped to 60-250%), keeping the reading position roughly in place.
    fn set_zoom(&self, zoom: f64) {
        let zoom = ((zoom * 10.0).round() / 10.0).clamp(ZOOM_MIN, ZOOM_MAX);
        let old = self.zoom.get();
        if (zoom - old).abs() < 1e-9 {
            return;
        }
        self.zoom.set(zoom);
        self.scroll.set(self.scroll.get() * zoom / old);
        *self.layout.borrow_mut() = None;
        self.message(&format!("Zoom {}%", (zoom * 100.0).round() as i32));
        self.redraw();
        self.save_session();
    }

    /// Focus reading mode: hides the menu bar, status bar, outline and find bar, and restores
    /// exactly what was showing. The tab bar stays. Ctrl+Shift+F toggles; Escape leaves.
    fn toggle_focus(&self) {
        let (Some(menu), Some(status)) = (self.menu_bar.get(), self.status.get()) else {
            return;
        };
        let outline = self.outline_dock.get().map(|d| d.widget());
        let find = self.find_bar.get().map(|b| b.widget());
        match self.focus.take() {
            None => {
                let seen = |w: Option<H<QWidget>>| w.is_some_and(|w| w.is_visible());
                self.focus.set(Some([
                    menu.widget().is_visible(),
                    status.widget().is_visible(),
                    seen(outline),
                    seen(find),
                ]));
                menu.widget().set_visible(false);
                status.widget().set_visible(false);
                for w in [outline, find].into_iter().flatten() {
                    w.set_visible(false);
                }
            }
            Some([m, st, o, f]) => {
                menu.widget().set_visible(m);
                status.widget().set_visible(st);
                if let Some(w) = outline {
                    w.set_visible(o);
                }
                if let Some(w) = find {
                    w.set_visible(f);
                }
            }
        }
        if let Some(canvas) = self.canvas.get() {
            canvas.widget().set_focus();
        }
        self.redraw();
    }

    /// Re-reads the showing file from disk, keeping the scroll offset. F5 and File > Refresh.
    fn refresh(&self) {
        if let Some(index) = self.current.get() {
            self.reload_page(index, true);
        }
    }

    /// Re-reads tab `index` from disk. The showing tab keeps its scroll offset; others get a fresh
    /// renderer and are laid out when next shown. A file that cannot be read keeps its content.
    fn reload_page(&self, index: usize, manual: bool) {
        let Some(path) = self.pages.borrow().get(index).map(|p| p.path.clone()) else {
            return;
        };
        let loaded = match read_stable(&path) {
            Ok(loaded) => loaded,
            Err(e) => {
                self.message(&format!("{}: {e}", path.display()));
                return;
            }
        };
        let document = parse_bytes(&loaded.bytes);
        let blocks = document.blocks.len();
        if let Some(page) = self.pages.borrow_mut().get_mut(index) {
            page.stamp = loaded.stamp;
        }
        if self.current.get() == Some(index) {
            let scroll = self.scroll.get();
            *self.document.borrow_mut() = Some(document);
            *self.renderer.borrow_mut() = DocumentRenderer::new();
            self.show_page(scroll, None);
        } else if let Some(page) = self.pages.borrow_mut().get_mut(index) {
            page.document = Some(document);
            page.renderer = DocumentRenderer::new();
            page.selection = None;
        }
        if manual || self.current.get() == Some(index) {
            let text = if manual {
                format!("Refreshed {}: {blocks} blocks", file_name(&path))
            } else {
                strings::REFRESHED.to_string()
            };
            self.message(&text);
        }
    }

    /// The 800 ms poll: reloads any open file whose stamp changed, background tabs included. A
    /// file that disappears keeps its last content and says so.
    fn poll_files(&self) {
        let update = self.updating.borrow().as_ref().and_then(|r| r.try_recv().ok());
        if let Some(status) = update {
            use crate::appimage_update::Status;
            self.updating.borrow_mut().take();
            self.message(&match status {
                Status::UpToDate => "mdview is up to date".to_string(),
                Status::Available => {
                    "A new mdview is available: Help > Update mdview now".to_string()
                }
                Status::Updated => "mdview was updated; restart it to use the new version".into(),
                Status::Failed(why) => format!("Update failed: {why}"),
            });
        }
        let sent: Vec<PathBuf> = self
            .incoming
            .borrow()
            .as_ref()
            .map(|r| r.try_iter().collect())
            .unwrap_or_default();
        if !sent.is_empty() {
            if let Some(window) = self.window.get() {
                window.widget().bring_to_front();
            }
        }
        for path in sent {
            self.open(&path);
        }
        let count = self.pages.borrow().len();
        for index in 0..count {
            let (path, old) = {
                let pages = self.pages.borrow();
                (pages[index].path.clone(), pages[index].stamp)
            };
            let now = stamp(&path);
            if now == old {
                continue;
            }
            if now == FileStamp::Missing {
                if let Some(page) = self.pages.borrow_mut().get_mut(index) {
                    page.stamp = now;
                }
                if self.current.get() == Some(index) {
                    self.message(strings::FILE_DELETED);
                }
            } else {
                self.reload_page(index, false);
            }
        }
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
        if code == KEY_ESCAPE && self.focus.get().is_some() {
            self.toggle_focus();
            return;
        }
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
            KEY_F5 => self.refresh(),
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
        let doc_dir = self
            .path
            .borrow()
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        // Core decides what a link may do: it ignores unknown schemes and refuses UNC paths, so a
        // document can never make the machine contact another host.
        let (resolved, fragment) = match classify_link(destination, &doc_dir) {
            LinkAction::Fragment(fragment) => {
                self.jump_fragment(&fragment);
                return;
            }
            LinkAction::External(url) => {
                if !open_url(&url) {
                    self.message(&format!("could not open {url}"));
                }
                return;
            }
            LinkAction::OpenMarkdown { path, fragment } => (path, fragment),
            LinkAction::OtherLocalFile(_) => {
                self.message(strings::ONLY_MARKDOWN_LINKS);
                return;
            }
            LinkAction::Ignored => return,
        };
        if !resolved.is_file() {
            self.message(&format!("not a file: {}", resolved.display()));
            return;
        }
        if !self.open(&resolved) {
            return;
        }
        if let Some(fragment) = fragment {
            self.pending_anchor.borrow_mut().replace(fragment);
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

    fn control_key(&self, code: i32, shift: bool) -> bool {
        match code {
            KEY_TAB => self.cycle(1),
            KEY_BACKTAB => self.cycle(-1),
            KEY_PLUS | KEY_EQUAL => self.set_zoom(self.zoom.get() + ZOOM_STEP),
            KEY_MINUS => self.set_zoom(self.zoom.get() - ZOOM_STEP),
            KEY_0 => self.set_zoom(1.0),
            KEY_F if shift => self.toggle_focus(),
            KEY_W if shift => self.close_all(),
            KEY_W | KEY_F4 => {
                if let Some(current) = self.current.get() {
                    self.close_tab(current);
                }
            }
            KEY_O if shift => self.choose_folder(),
            KEY_O => {
                if let Some(window) = self.window.get() {
                    let dir = self
                        .path
                        .borrow()
                        .as_ref()
                        .and_then(|path| path.parent().map(Path::to_path_buf))
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_default();
                    if let Some(path) = choose_file(window.widget(), &dir) {
                        self.open(&path);
                    }
                }
            }
            _ => return false,
        }
        true
    }

    fn drag_enter(&self, _text: String) -> bool {
        true
    }

    fn dropped(&self, text: String) {
        let first = text.lines().next().unwrap_or("").trim();
        if first.is_empty() {
            return;
        }
        // A drop is a file URI or a plain path; core's link rules refuse UNC hosts here too.
        let path = match classify_link(first, Path::new("")) {
            LinkAction::OpenMarkdown { path, .. } | LinkAction::OtherLocalFile(path) => path,
            _ => {
                self.message(&format!("cannot open {first}"));
                return;
            }
        };
        if path.is_file() {
            self.open(&path);
        } else {
            self.message(&format!("not a file: {}", path.display()));
        }
    }

    fn timer(&self, _id: i32) {
        self.poll_files();
    }

    fn update_request(&self) {}
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
    /// Open, switch, cycle and close tabs and check what each shows, then exit.
    Tabs,
}

/// Shown by Help > Keyboard shortcuts (the status bar; there is no dialog binding yet).
const SHORTCUTS: &str = "Ctrl+O open | Ctrl+W close | Ctrl+Tab next tab | Ctrl+F find, F3 next | F5 refresh | Ctrl++ / Ctrl+- / Ctrl+0 zoom | Ctrl+C copy";

/// Markdown extensions the Files dock lists, as in the original.
fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|e| {
        let e = e.to_string_lossy().to_ascii_lowercase();
        matches!(e.as_str(), "md" | "markdown" | "mdown" | "mkd")
    })
}

/// Collects `dir`'s folders (first) and Markdown files, depth first, at most 4000 rows and 8 deep.
fn scan_folder(dir: &Path, level: u8, rows: &mut Vec<(u8, String, Option<PathBuf>)>) {
    if rows.len() >= 4000 || level > 8 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    entries.sort_by_key(|e| {
        (
            !e.path().is_dir(),
            e.file_name().to_string_lossy().to_lowercase(),
        )
    });
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            rows.push((level, name, None));
            scan_folder(&path, level + 1, rows);
        } else if is_markdown(&path) {
            rows.push((level, name, Some(path)));
        }
    }
}

/// Opens the folder holding `path`, with the file selected where the platform supports it.
fn reveal_in_file_manager(path: &Path) {
    if cfg!(windows) {
        let _ = std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    } else if let Some(parent) = path.parent() {
        open_url(&parent.display().to_string());
    }
}

/// `--tabs-test`: with one document open, opens `linked.md` beside it, switches back by opening the
/// first again, cycles with Ctrl+Tab, closes both, and checks tabs, scroll, labels and Recent.
fn run_tabs_check(shell: &Rc<Shell>) -> ! {
    let mut failures = Vec::new();
    let mut expect = |ok: bool, what: String| {
        if !ok {
            failures.push(what);
        }
    };
    let Some(first) = shell.path.borrow().clone() else {
        println!("tabs test failed: start with a document");
        std::process::exit(1);
    };
    let bar = shell.tab_bar.get().expect("tab bar");
    let other = first.with_file_name("linked.md");
    shell.scroll_to(500.0);
    let scrolled = shell.scroll.get();
    expect(scrolled > 0.0, format!("first page did not scroll ({scrolled})"));
    shell.open(&other);
    expect(shell.pages.borrow().len() == 2, "second tab not added".into());
    expect(shell.current.get() == Some(1), "second tab not current".into());
    expect(bar.count() == 2 && bar.current() == Some(1), "bar out of step after open".into());
    expect(shell.scroll.get() == 0.0, "new tab did not start at the top".into());
    shell.open(&first);
    expect(shell.pages.borrow().len() == 2, "reopening added a duplicate".into());
    expect(shell.current.get() == Some(0), "reopening did not switch".into());
    expect(
        shell.scroll.get() == scrolled,
        format!("scroll not kept: {} vs {scrolled}", shell.scroll.get()),
    );
    expect(
        bar.text(0) == "showcase.md" && bar.text(1) == "linked.md",
        format!("labels {:?} {:?}", bar.text(0), bar.text(1)),
    );
    shell.control_key(KEY_TAB, false);
    expect(shell.current.get() == Some(1) && bar.current() == Some(1), "Ctrl+Tab".into());
    shell.control_key(KEY_TAB, false);
    expect(shell.current.get() == Some(0), "Ctrl+Tab did not wrap".into());
    shell.control_key(KEY_BACKTAB, true);
    expect(shell.current.get() == Some(1), "Ctrl+Shift+Tab".into());
    shell.tab_moved(1, 0);
    expect(
        shell.current.get() == Some(0) && shell.pages.borrow()[0].path == canonical_or_absolute(&other),
        "move did not carry the current tab".into(),
    );
    shell.tab_moved(0, 1);
    shell.set_zoom(1.0);
    shell.control_key(KEY_EQUAL, false);
    expect((shell.zoom.get() - 1.1).abs() < 1e-9, "zoom in".into());
    shell.control_key(KEY_0, false);
    expect((shell.zoom.get() - 1.0).abs() < 1e-9, "zoom reset".into());
    for _ in 0..40 {
        shell.control_key(KEY_MINUS, false);
    }
    expect((shell.zoom.get() - ZOOM_MIN).abs() < 1e-9, "zoom floor".into());
    for _ in 0..40 {
        shell.control_key(KEY_PLUS, false);
    }
    expect((shell.zoom.get() - ZOOM_MAX).abs() < 1e-9, "zoom ceiling".into());
    shell.set_zoom(1.0);
    // Theme and zoom reach the saved session file.
    let scratch = std::env::temp_dir().join(format!("mdview-theme-{}.ini", std::process::id()));
    let kept = shell.session.borrow_mut().replace((scratch.clone(), Session::default()));
    shell.set_theme_choice(ThemeChoice::Dark);
    shell.set_zoom(1.3);
    let saved = Session::load(&scratch);
    _ = std::fs::remove_file(&scratch);
    *shell.session.borrow_mut() = kept;
    expect(
        saved.theme == ThemeChoice::Dark && saved.zoom == 130 && shell.theme.get().name == DARK.name,
        format!("theme/zoom not saved: {:?} {}", saved.theme, saved.zoom),
    );
    shell.set_theme_choice(ThemeChoice::System);
    shell.set_theme_choice(ThemeChoice::Light);
    shell.set_zoom(1.0);
    // Manage recent: select a row, remove it; clear the rest.
    let gone = std::env::temp_dir().join(format!("mdview-gone-{}.md", std::process::id()));
    shell.recent.borrow_mut().add(gone.clone());
    shell.manage_show(true);
    let listed = shell.manage_rows.borrow().len();
    expect(listed == shell.recent.borrow().paths().len() && listed >= 3, "manage list".into());
    shell.manage_sel.set(Some(0));
    shell.manage_remove();
    expect(!shell.recent.borrow().paths().contains(&gone), "manage remove".into());
    shell.manage_show(false);
    let backup = shell.recent.borrow().clone();
    shell.manage_clear();
    expect(shell.recent.borrow().is_empty(), "manage clear".into());
    *shell.recent.borrow_mut() = backup;
    // Files dock: folders first, Markdown only, hidden skipped; a click opens the file.
    let root = std::env::temp_dir().join(format!("mdview-folder-{}", std::process::id()));
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::create_dir_all(root.join(".hidden")).unwrap();
    std::fs::write(root.join("b.md"), "# B\n").unwrap();
    std::fs::write(root.join("a.txt"), "no").unwrap();
    std::fs::write(root.join("sub").join("c.markdown"), "# C\n").unwrap();
    std::fs::write(root.join(".hidden").join("h.md"), "# H\n").unwrap();
    shell.open_folder(&root);
    let names: Vec<Option<String>> = shell
        .folder_rows
        .borrow()
        .iter()
        .map(|p| p.as_ref().map(|p| file_name(p)))
        .collect();
    expect(
        names == vec![None, Some("c.markdown".to_string()), Some("b.md".to_string())],
        format!("folder rows {names:?}"),
    );
    let tabs_before = shell.pages.borrow().len();
    shell.files_click(2);
    expect(shell.pages.borrow().len() == tabs_before + 1, "folder click opens".into());
    let opened = shell.current.get().unwrap();
    shell.close_tab(opened);
    shell.recent.borrow_mut().remove(&canonical_or_absolute(&root.join("b.md")));
    _ = std::fs::remove_dir_all(&root);
    // Auto-refresh: an edited file reloads (even in the background), a deleted one keeps its text.
    let watched = std::env::temp_dir().join(format!("mdview-watch-{}.md", std::process::id()));
    std::fs::write(&watched, "# One\n\nfirst\n").unwrap();
    shell.open(&watched);
    let index = shell.current.get().unwrap();
    let before = shell.blocks();
    std::fs::write(&watched, "# One\n\nfirst\n\nsecond\n\nthird\n").unwrap();
    shell.activate(0);
    shell.poll_files();
    shell.activate(index);
    expect(shell.blocks() > before, format!("auto-refresh: {} blocks, was {before}", shell.blocks()));
    let kept = shell.blocks();
    std::fs::remove_file(&watched).unwrap();
    shell.poll_files();
    expect(shell.blocks() == kept, "a deleted file kept its content".into());
    std::fs::write(&watched, "# Back\n").unwrap();
    shell.poll_files();
    expect(shell.blocks() < kept, "a recreated file reloaded".into());
    shell.close_tab(index);
    shell.recent.borrow_mut().remove(&canonical_or_absolute(&watched));
    _ = std::fs::remove_file(&watched);
    shell.control_key(KEY_F, true);
    expect(shell.focus.get().is_some(), "focus mode on".into());
    shell.key(KEY_ESCAPE);
    expect(shell.focus.get().is_none(), "Escape leaves focus mode".into());
    let blocks = shell.blocks();
    shell.key(KEY_F5);
    expect(shell.blocks() == blocks && shell.document.borrow().is_some(), "refresh".into());
    shell.tab_menu(Some(0), (20, 20)); // builds and pops the menu; must not crash or change tabs
    shell.tab_menu(None, (20, 20));
    expect(shell.pages.borrow().len() == 2, "tab menu changed the tabs".into());
    // "Close other tabs" from the first tab's menu leaves just that tab; reopen the second.
    shell.close_others(0);
    expect(
        shell.pages.borrow().len() == 1 && bar.count() == 1 && shell.current.get() == Some(0),
        "Close other tabs".into(),
    );
    shell.open(&other);
    expect(shell.pages.borrow().len() == 2 && shell.current.get() == Some(1), "reopen".into());
    shell.open(&first); // keeps `first` the most recent, as the Recent check below expects
    shell.activate(1);
    shell.control_key(KEY_W, false);
    expect(
        shell.pages.borrow().len() == 1
            && shell.path.borrow().as_deref().is_some_and(|p| same_file(p, &first)),
        "Ctrl+W closed the wrong tab".into(),
    );
    expect(shell.current.get() == Some(0) && bar.count() == 1, "after close".into());
    shell.control_key(KEY_W, true);
    expect(shell.pages.borrow().is_empty() && bar.count() == 0, "Ctrl+Shift+W".into());
    expect(shell.path.borrow().is_none(), "empty page still has a path".into());
    expect(shell.blocks() > 0, "empty page shows nothing".into());
    let recent: Vec<PathBuf> = shell.recent.borrow().paths().to_vec();
    expect(
        recent.len() == 2 && same_file(&recent[0], &first),
        format!("recent {recent:?}"),
    );
    if failures.is_empty() {
        println!("tabs test ok: 2 tabs, scroll {scrolled} kept, recent {}", recent.len());
        std::process::exit(0);
    }
    println!("tabs test failed: {}", failures.join("; "));
    std::process::exit(1);
}

fn same_file(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
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
    if check == Check::Tabs {
        run_tabs_check(shell);
    }
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
        let entries = shell.outline_blocks.borrow().len();
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
        | Check::Copy
        | Check::Tabs => {
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
    files: Vec<PathBuf>,
    drop_test: Option<PathBuf>,
    check: Option<Check>,
    scroll: f64,
) {
    let t0 = std::time::Instant::now();
    let trace = std::env::var_os("MDVIEW_STARTUP_TRACE").is_some();
    let mark = |what: &str| {
        if trace {
            eprintln!("mdview-qt: build {what} at {:.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
        }
    };
    let window = H::<QMainWindow>::new();
    window.widget().set_title("mdview");
    window.widget().resize(1100, 900);

    let menus = window.menu_bar();
    let file_menu = menus.add_menu("&File");
    let open = file_menu.add_action("&Open...\tCtrl+O");
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
                s.open(&path);
            }
        }
    });
    let refresh = file_menu.add_action("&Refresh\tF5");
    let s = shell.clone();
    refresh.on_triggered(move |_| s.refresh());
    let recent_menu = file_menu.add_menu("Open &recent");
    recent_menu.show_tool_tips();
    let s = shell.clone();
    recent_menu.on_about_to_show(move || s.fill_recent(recent_menu));
    // Filled once now too, so the submenu is never empty before its first opening.
    shell.fill_recent(recent_menu);
    file_menu.add_separator();
    let close_tab = file_menu.add_action("&Close tab\tCtrl+W");
    let s = shell.clone();
    close_tab.on_triggered(move |_| {
        if let Some(current) = s.current.get() {
            s.close_tab(current);
        }
    });
    let close_all = file_menu.add_action("Close &all tabs\tCtrl+Shift+W");
    let s = shell.clone();
    close_all.on_triggered(move |_| s.close_all());
    file_menu.add_separator();
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

    mark("menus (File, Open recent)");
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
    let outline_tree = Tree::new();
    outline_dock.set_widget(outline_tree.widget());
    window.add_dock(true, outline_dock);
    outline_dock.widget().set_visible(false);
    outline_dock.widget().resize(240, 700);
    let s = shell.clone();
    outline_tree.on_item_clicked(move |index| s.outline_click(index));
    shell.outline_dock.set(Some(outline_dock));
    shell.outline_tree.replace(Some(outline_tree));

    let files_dock = H::<QDockWidget>::new("Files", window.clone());
    let files_tree = Tree::new();
    files_dock.set_widget(files_tree.widget());
    window.add_dock(true, files_dock);
    files_dock.widget().set_visible(false);
    files_dock.widget().resize(240, 700);
    let s = shell.clone();
    files_tree.on_item_clicked(move |index| s.files_click(index));
    shell.files_dock.set(Some(files_dock));
    shell.files_tree.replace(Some(files_tree));

    let manage_dock = H::<QDockWidget>::new("Manage recent files", window.clone());
    let manage_tree = Tree::new();
    manage_dock.set_widget(manage_tree.widget());
    window.add_dock(true, manage_dock);
    manage_dock.widget().set_visible(false);
    manage_dock.widget().resize(320, 360);
    let manage_bar = window.add_tool_bar("Manage recent");
    manage_bar.widget().set_visible(false);
    let (open_sel, remove_sel, clear_all, done) = (
        manage_bar.add_action("Open"),
        manage_bar.add_action("Remove selected"),
        manage_bar.add_action("Clear list"),
        manage_bar.add_action("Done"),
    );
    let s = shell.clone();
    manage_tree.on_item_clicked(move |index| s.manage_sel.set(Some(index)));
    let s = shell.clone();
    open_sel.on_triggered(move |_| s.manage_open());
    let s = shell.clone();
    remove_sel.on_triggered(move |_| s.manage_remove());
    let s = shell.clone();
    clear_all.on_triggered(move |_| s.manage_clear());
    let s = shell.clone();
    done.on_triggered(move |_| s.manage_show(false));
    shell.manage_dock.set(Some(manage_dock));
    shell.manage_bar.set(Some(manage_bar));
    shell.manage_tree.replace(Some(manage_tree));
    let manage = file_menu.add_action("&Manage recent files...");
    let s = shell.clone();
    manage.on_triggered(move |_| s.manage_show(true));

    let open_folder = file_menu.add_action("Open &folder...\tCtrl+Shift+O");
    let s = shell.clone();
    open_folder.on_triggered(move |_| s.choose_folder());

    let view_menu = menus.add_menu("&View");
    let files_toggle = view_menu.add_action("&Files");
    let s = shell.clone();
    files_toggle.on_triggered(move |_| {
        if let Some(dock) = s.files_dock.get() {
            dock.widget().set_visible(!dock.widget().is_visible());
        }
    });
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
    view_menu.add_separator();
    let zoom_in = view_menu.add_action("Zoom &in\tCtrl++");
    let zoom_out = view_menu.add_action("Zoom &out\tCtrl+-");
    let zoom_reset = view_menu.add_action("&Reset zoom\tCtrl+0");
    let s = shell.clone();
    zoom_in.on_triggered(move |_| s.set_zoom(s.zoom.get() + ZOOM_STEP));
    let s = shell.clone();
    zoom_out.on_triggered(move |_| s.set_zoom(s.zoom.get() - ZOOM_STEP));
    let s = shell.clone();
    zoom_reset.on_triggered(move |_| s.set_zoom(1.0));
    let focus_mode = view_menu.add_action("&Focus reading mode\tCtrl+Shift+F");
    let s = shell.clone();
    focus_mode.on_triggered(move |_| s.toggle_focus());
    let help = menus.add_menu("&Help");
    let shortcuts = help.add_action("&Keyboard shortcuts");
    let s = shell.clone();
    shortcuts.on_triggered(move |_| s.message(SHORTCUTS));
    if crate::appimage_update::appimage().is_some() {
        let update_now = help.add_action("&Update mdview now");
        let s = shell.clone();
        update_now.on_triggered(move |_| {
            if s.updating.borrow().is_some() {
                return;
            }
            match crate::appimage_update::spawn_apply() {
                Some(receiver) => {
                    s.message("Updating mdview...");
                    *s.updating.borrow_mut() = Some(receiver);
                }
                None => s.message("appimageupdatetool is not installed"),
            }
        });
    }
    let about = help.add_action("&About");
    let s = shell.clone();
    about.on_triggered(move |_| s.message("mdview 0.1 - Qt shell"));
    let system = view_menu.add_action("&System theme");
    for (action, choice) in [
        (system, ThemeChoice::System),
        (light, ThemeChoice::Light),
        (dark, ThemeChoice::Dark),
    ] {
        let s = shell.clone();
        action.on_triggered(move |_| s.set_theme_choice(choice));
    }

    let canvas = H::<Canvas>::new(shell.clone() as Rc<dyn CanvasHandler>);
    canvas.widget().accept_drops();
    canvas.widget().track_mouse();
    canvas.start_timer(800); // the file poll, as in the original
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
    mark("docks and view menu");
    let tabs = H::<QTabBar>::new();
    let s = shell.clone();
    tabs.on_current_changed(move |index| {
        if let Ok(index) = usize::try_from(index) {
            if !s.tabs_busy.get() {
                s.activate(index);
            }
        }
    });
    let s = shell.clone();
    tabs.on_close_requested(move |index| {
        if let Ok(index) = usize::try_from(index) {
            s.close_tab(index);
        }
    });
    let s = shell.clone();
    tabs.on_moved(move |from, to| {
        if let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) {
            s.tab_moved(from, to);
        }
    });
    let s = shell.clone();
    tabs.on_context_menu(move |tab, at| s.tab_menu(tab, at));
    shell.tab_bar.set(Some(tabs));
    let central = new_plain_widget();
    layout_column(central, tabs.widget(), page);
    mark("canvas and tabs");
    window.set_central(central);
    shell.canvas.set(Some(canvas));
    shell.scrollbar.set(Some(bar));
    shell.window.set(Some(window));
    shell.status.set(Some(window.status_bar()));
    shell.menu_bar.set(Some(window.menu_bar()));
    window.widget().show();
    mark("window shown");
    if let Some(canvas) = shell.canvas.get() {
        canvas.widget().set_focus();
    }

    // Saved tabs come back first, without reordering Recent; files on the command line follow.
    let saved = shell.session.borrow().as_ref().map(|(_, s)| s.clone());
    if let Some(saved) = saved {
        *shell.recent.borrow_mut() = saved.recent.clone();
        for path in &saved.tabs {
            shell.restore_page(path);
        }
        if !shell.pages.borrow().is_empty() {
            let last = shell.pages.borrow().len() - 1;
            shell.switch_to(saved.active.min(last));
        }
        if let Some(dir) = saved.folder.filter(|d| d.is_dir()) {
            shell.open_folder(&dir);
        }
    }
    mark("session restored");
    for path in &files {
        shell.open(path);
    }
    if shell.pages.borrow().is_empty() {
        shell.show_empty(strings::STATUS_STARTUP);
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
        let loaded = shell
            .path
            .borrow()
            .as_deref()
            .is_some_and(|path| same_file(path, &canonical_or_absolute(&target)));
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

/// `mdview-qt [FILE...] [--light|--dark] [--no-session] [--select-test|--link-test|...]`.
/// `MDVIEW_SETTINGS` names the session file instead of the per-user default.
pub fn run() -> i32 {
    let mut files = Vec::new();
    let mut use_session = true;
    let mut drop_test = None;
    let mut check = None;
    let mut theme = LIGHT;
    let mut zoom = 1.0f64;
    let mut theme_flag = false;
    let mut zoom_flag = false;
    let mut scroll = 0.0f64;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--light" => {
                theme = LIGHT;
                theme_flag = true;
            }
            "--dark" => {
                theme = DARK;
                theme_flag = true;
            }
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
            "--tabs-test" => check = Some(Check::Tabs),
            "--no-session" => use_session = false,
            "--zoom" => {
                zoom_flag = true;
                zoom = args.next().expect("--zoom needs a number").parse().unwrap();
            }
            "--drop-test" => {
                drop_test = Some(PathBuf::from(
                    args.next().expect("--drop-test needs a path"),
                ));
            }
            other if !other.starts_with("--") => files.push(PathBuf::from(other)),
            other => eprintln!("mdview-qt: ignoring {other}"),
        }
    }
    let shell = Rc::new(Shell::new(theme, zoom.clamp(0.5, 3.0)));
    // Scripted checks start from a known state, so they never read or write the user's session
    // unless `MDVIEW_SETTINGS` points them at a scratch one.
    let explicit = std::env::var_os("MDVIEW_SETTINGS").map(PathBuf::from);
    let scripted = check.is_some() || drop_test.is_some();
    let settings = match explicit {
        Some(path) => Some(path),
        None if scripted => None,
        None => mdview_core::session::default_settings_path(),
    };
    if let Some(path) = settings.filter(|_| use_session) {
        // A window is already open: hand it the files and quit. Scripted checks skip this.
        if !scripted && crate::single::forward(&path, &files) {
            eprintln!("mdview-qt: sent {} file(s) to the running window", files.len());
            std::process::exit(0);
        }
        if !scripted {
            *shell.incoming.borrow_mut() = crate::single::listen(&path);
            *shell.updating.borrow_mut() = crate::appimage_update::spawn_check();
        }
        let saved = Session::load(&path);
        if !theme_flag {
            shell.set_theme_choice(saved.theme);
        }
        if !zoom_flag {
            shell.zoom.set(f64::from(saved.zoom) / 100.0);
        }
        *shell.session.borrow_mut() = Some((path, saved));
    }
    run_app(move || build(&shell, files, drop_test, check, scroll))
}
