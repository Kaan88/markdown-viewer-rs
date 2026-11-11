//! The Qt shell: a window, a menu bar and a canvas that shows the page `mdview-render` drew, with
//! the wheel scrolling it. Linux is the real target; this also builds against Qt on Windows so the
//! variant can be tested there. Safe Rust only; `qt/mod.rs` holds every `unsafe`.
#![forbid(unsafe_code)]

use crate::qt::ffi::*;
use crate::qt::*;
use mdview_core::commands::strings;
use mdview_core::files::{canonical_or_absolute, read_stable, stamp, tab_labels, FileStamp};
use mdview_core::links::{classify_link, LinkAction};
use mdview_core::parse_bytes;
use mdview_core::session::{Recents, Session, Theme as ThemeChoice};
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
    /// The command bar (layout A), which also holds the find box.
    find_bar: Cell<Option<H<QToolBar>>>,
    /// The command bar's main menu (☰), filled once the actions it repeats exist.
    main_menu: Cell<Option<H<QMenu>>>,
    /// The match count beside the find box ("2 of 4").
    find_count: Cell<Option<H<QAction>>>,
    /// The command bar's zoom percentage; clicking it resets to 100%.
    zoom_label: Cell<Option<H<QAction>>>,
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
    /// The folder behind each tree row (None = file), and the folders the user has expanded.
    /// Only expanded folders are read, so a big folder no longer slows the launch.
    folder_dirs: RefCell<Vec<Option<PathBuf>>>,
    folder_open: RefCell<std::collections::HashSet<PathBuf>>,
    /// Watches the open files' folders; `None` before the window is built or when the OS watcher
    /// failed, in which case a 2 s poll timer runs instead.
    watcher: RefCell<Option<mdview_core::watch::FileWatcher>>,
    /// The canvas's waker, set once the canvas exists. Worker threads started before that (the
    /// single-instance listener, the update check) call through this slot.
    wake: std::sync::Arc<std::sync::OnceLock<Box<dyn Fn() + Send + Sync>>>,
    /// Paths sent by later launches, drained when the listener wakes the GUI thread.
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
            choice: Cell::new(if theme.name == DARK.name {
                ThemeChoice::Dark
            } else {
                ThemeChoice::Light
            }),
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
            zoom_label: Cell::new(None),
            main_menu: Cell::new(None),
            find_count: Cell::new(None),
            find_edit: Cell::new(None),
            find: RefCell::new(Find::default()),
            outline_dock: Cell::new(None),
            outline_tree: RefCell::new(None),
            files_dock: Cell::new(None),
            files_tree: RefCell::new(None),
            folder: RefCell::new(None),
            folder_rows: RefCell::new(Vec::new()),
            folder_dirs: RefCell::new(Vec::new()),
            folder_open: RefCell::new(std::collections::HashSet::new()),
            watcher: RefCell::new(None),
            wake: Default::default(),
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
        // The find box lives in the command bar, which stays; emptying it ends the find.
        if let Some(edit) = self.find_edit.get() {
            edit.set_text("");
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
            if let Some(count) = self.find_count.get() {
                count.set_text("");
            }
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
        // Also beside the find box, where WinUI shows it.
        if let Some(count) = self.find_count.get() {
            count.set_text(&text);
        }
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
        self.folder_open.borrow_mut().clear();
        *self.folder.borrow_mut() = Some(dir.to_path_buf());
        self.fill_folder();
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

    /// Rebuilds the Files tree: the folder's top level, plus the folders the user expanded.
    fn fill_folder(&self) {
        let Some(dir) = self.folder.borrow().clone() else {
            return;
        };
        let mut rows: Vec<FolderRow> = Vec::new();
        scan_folder(&dir, 1, &self.folder_open.borrow(), &mut rows);
        let titles: Vec<(u8, &str)> = rows.iter().map(|r| (r.level, r.title.as_str())).collect();
        if let Some(tree) = self.files_tree.borrow().as_ref() {
            tree.set_items(&titles);
        }
        *self.folder_dirs.borrow_mut() = rows.iter().map(|r| r.dir.clone()).collect();
        *self.folder_rows.borrow_mut() = rows.into_iter().map(|r| r.file).collect();
    }

    /// A file row opens the file; a folder row expands or collapses it.
    fn files_click(&self, index: usize) {
        let dir = self.folder_dirs.borrow().get(index).cloned().flatten();
        if let Some(dir) = dir {
            let mut open = self.folder_open.borrow_mut();
            if !open.remove(&dir) {
                open.insert(dir);
            }
            drop(open);
            self.fill_folder();
            return;
        }
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
                    eprintln!(
                        "mdview-qt: open: add_page {:.1} ms",
                        t.elapsed().as_secs_f64() * 1000.0
                    );
                }
                self.switch_to(index);
                if trace {
                    eprintln!(
                        "mdview-qt: open: switch_to {:.1} ms",
                        t.elapsed().as_secs_f64() * 1000.0
                    );
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
            eprintln!(
                "mdview-qt: add_page: bar.add {:.1} ms",
                t.elapsed().as_secs_f64() * 1000.0
            );
        }
        self.relabel();
        if trace {
            eprintln!(
                "mdview-qt: add_page: relabel {:.1} ms",
                t.elapsed().as_secs_f64() * 1000.0
            );
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
        if let Some(edit) = self.find_edit.get() {
            edit.set_text("");
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

    /// Right click on the page: a link offers Open and Copy link address; a selection offers Copy.
    fn canvas_menu(self: &Rc<Self>, local: (i32, i32), at: (i32, i32)) {
        let link = self
            .hit(local.0 as f64, local.1 as f64)
            .and_then(|hit| hit.link);
        let has_selection = self.selection.get().is_some_and(|(a, b)| a != b);
        let menu = H::<QMenu>::new_popup();
        if let Some(index) = link {
            let destination = self.document.borrow().as_ref().and_then(|d| {
                d.links
                    .get(index as usize)
                    .map(|l| d.str(l.dest).to_string())
            });
            let s = self.clone();
            menu.add_action("&Open link")
                .on_triggered(move |_| s.follow_link(index));
            if let Some(destination) = destination {
                menu.add_action("Copy link &address")
                    .on_triggered(move |_| set_clipboard(&destination));
            }
        }
        let s = self.clone();
        let copy = menu.add_action("&Copy\tCtrl+C");
        copy.set_enabled(has_selection);
        copy.on_triggered(move |_| s.copy_selection());
        let s = self.clone();
        menu.add_action("&Find...\tCtrl+F")
            .on_triggered(move |_| s.show_find());
        let s = self.clone();
        menu.add_action("&Reload\tF5")
            .on_triggered(move |_| s.refresh());
        // Leaks one small popup per use at most; the popup deletes itself when it closes.
        menu.popup(at);
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
        menu.add_action("&Close tab")
            .on_triggered(move |_| s.close_tab(index));
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

    /// A thread-safe wake for workers; does nothing until the canvas exists.
    fn waker(&self) -> impl Fn() + Send + Sync + 'static {
        let slot = self.wake.clone();
        move || {
            if let Some(wake) = slot.get() {
                wake();
            }
        }
    }

    /// Points the file watcher at the open tabs' files.
    fn rewatch(&self) {
        if let Some(watcher) = self.watcher.borrow_mut().as_mut() {
            let pages = self.pages.borrow();
            watcher.set_files(pages.iter().map(|page| page.path.as_path()));
        }
    }

    /// Writes tabs, the current tab and Recent at once, so a crash loses nothing. Called after
    /// every open, close and move, so it also re-aims the file watcher.
    fn save_session(&self) {
        self.rewatch();
        let mut session = self.session.borrow_mut();
        let Some((path, session)) = session.as_mut() else {
            return;
        };
        session.tabs = self.pages.borrow().iter().map(|p| p.path.clone()).collect();
        session.active = self.current.get().unwrap_or(0);
        session.recent = self.recent.borrow().clone();
        session.theme = self.choice.get();
        session.zoom = (self.zoom.get() * 100.0).round() as u16;
        let (files, outline) = self.panes();
        session.set_panes(files, outline);
        if let Err(e) = session.save(path) {
            eprintln!("mdview-qt: could not save {}: {e}", path.display());
        }
    }

    /// Which panes are open; in focus mode, the outline it will bring back.
    fn panes(&self) -> (bool, bool) {
        let seen = |dock: Option<H<QDockWidget>>| dock.is_some_and(|d| d.widget().is_visible());
        let outline = match self.focus.get() {
            Some([_, _, outline, _]) => outline,
            None => seen(self.outline_dock.get()),
        };
        (seen(self.files_dock.get()), outline)
    }

    /// Saves the panes, or, given the window, only its size: by the time the window is closed
    /// its docks report hidden, so the panes saved on the last toggle stand.
    fn save_layout(&self, window: Option<H<QWidget>>) {
        let (files, outline) = self.panes();
        let mut session = self.session.borrow_mut();
        let Some((path, session)) = session.as_mut() else {
            return;
        };
        if window.is_none() {
            session.set_panes(files, outline);
        }
        if let Some(w) = window {
            let maximized = w.is_maximized();
            // Maximized: keep the last restored size, so un-maximizing next time looks right.
            let (width, height) = match (maximized, session.window_size()) {
                (true, Some((sw, sh, _))) => (sw, sh),
                (true, None) => (1100, 900),
                _ => (w.width().max(0) as u32, w.height().max(0) as u32),
            };
            session.set_window_size(width, height, maximized);
        }
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
        self.style_chrome();
        self.redraw();
        self.save_session();
    }

    /// Styles the window's own controls in the page's colours, so the title row, command bar, tabs,
    /// panes and menus look like one surface with the page (layout A), whatever the platform style.
    fn style_chrome(&self) {
        let Some(window) = self.window.get() else {
            return;
        };
        let sheet = chrome_style(self.theme.get());
        let mode = theme_label(self.choice.get());
        window
            .widget()
            .set_style_sheet(&format!("/* {mode} */\n{sheet}"));
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
        self.refresh_zoom_label();
        self.redraw();
        self.save_session();
    }

    fn refresh_zoom_label(&self) {
        if let Some(label) = self.zoom_label.get() {
            label.set_text(&format!("{}%", (self.zoom.get() * 100.0).round() as i32));
        }
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

    /// Runs when the file watcher, the single-instance listener or the update check wakes the GUI
    /// thread (or on the 2 s fallback timer): reloads any open file whose stamp changed,
    /// background tabs included. A file that disappears keeps its last content and says so.
    fn poll_files(&self) {
        if let Some(watcher) = self.watcher.borrow().as_ref() {
            watcher.woken();
        }
        let update = self
            .updating
            .borrow()
            .as_ref()
            .and_then(|r| r.try_recv().ok());
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
                eprintln!(
                    "mdview-qt: {} was deleted, keeping its text",
                    path.display()
                );
                if self.current.get() == Some(index) {
                    self.message(strings::FILE_DELETED);
                }
            } else {
                eprintln!("mdview-qt: {} changed on disk, reloading", path.display());
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
        if code == KEY_ESCAPE && !self.find.borrow().query.is_empty() {
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
                let page = self.renderer.borrow_mut().layout(document, &options).clone();
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
        self.layout.borrow().as_ref()?;
        self.renderer.borrow_mut().hit_test(
            document,
            &options,
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
        if self.layout.borrow().is_none() {
            return Vec::new();
        }
        self.renderer
            .borrow_mut()
            .selection_rects(document, &options, from, to)
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
        if let Some(document) = self.document.borrow().as_ref() {
            self.renderer.borrow_mut().set_details(document, block, !open);
        }
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
            let page = self.renderer.borrow_mut().layout(document, &options).clone();
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
        if self.layout.borrow().is_none() {
            return;
        }
        let Some(document) = &*self.document.borrow() else {
            return;
        };
        let started = std::time::Instant::now();
        self.renderer.borrow_mut().view_frame(
            document,
            &options,
            scroll as f32,
            device_height as f32,
        );
        // The frame may have corrected the layout (measured what it shows): keep the copy the
        // scroll bar, anchors and footnotes read in step with it.
        let generation = |page: Option<&PageLayout>| page.map(PageLayout::generation);
        let corrected = generation(self.renderer.borrow().page_layout())
            != generation(self.layout.borrow().as_ref());
        if corrected {
            *self.layout.borrow_mut() = self.renderer.borrow().page_layout().cloned();
        }
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

    fn woken(&self) {
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

/// One row of the Files tree: a folder (`dir`) or a Markdown file (`file`).
struct FolderRow {
    level: u8,
    title: String,
    file: Option<PathBuf>,
    dir: Option<PathBuf>,
}

/// Collects `dir`'s folders (first) and Markdown files; only folders in `open` are read further.
/// At most 4000 rows and 8 deep.
fn scan_folder(
    dir: &Path,
    level: u8,
    open: &std::collections::HashSet<PathBuf>,
    rows: &mut Vec<FolderRow>,
) {
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
            let expanded = open.contains(&path);
            rows.push(FolderRow {
                level,
                title: format!("{} {name}", if expanded { "\u{25be}" } else { "\u{25b8}" }),
                file: None,
                dir: Some(path.clone()),
            });
            if expanded {
                scan_folder(&path, level + 1, open, rows);
            }
        } else if is_markdown(&path) {
            rows.push(FolderRow {
                level,
                title: name,
                file: Some(path),
                dir: None,
            });
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
    expect(
        scrolled > 0.0,
        format!("first page did not scroll ({scrolled})"),
    );
    shell.open(&other);
    expect(
        shell.pages.borrow().len() == 2,
        "second tab not added".into(),
    );
    expect(
        shell.current.get() == Some(1),
        "second tab not current".into(),
    );
    expect(
        bar.count() == 2 && bar.current() == Some(1),
        "bar out of step after open".into(),
    );
    expect(
        shell.scroll.get() == 0.0,
        "new tab did not start at the top".into(),
    );
    shell.open(&first);
    expect(
        shell.pages.borrow().len() == 2,
        "reopening added a duplicate".into(),
    );
    expect(
        shell.current.get() == Some(0),
        "reopening did not switch".into(),
    );
    expect(
        shell.scroll.get() == scrolled,
        format!("scroll not kept: {} vs {scrolled}", shell.scroll.get()),
    );
    expect(
        bar.text(0) == "showcase.md" && bar.text(1) == "linked.md",
        format!("labels {:?} {:?}", bar.text(0), bar.text(1)),
    );
    shell.control_key(KEY_TAB, false);
    expect(
        shell.current.get() == Some(1) && bar.current() == Some(1),
        "Ctrl+Tab".into(),
    );
    shell.control_key(KEY_TAB, false);
    expect(
        shell.current.get() == Some(0),
        "Ctrl+Tab did not wrap".into(),
    );
    shell.control_key(KEY_BACKTAB, true);
    expect(shell.current.get() == Some(1), "Ctrl+Shift+Tab".into());
    shell.tab_moved(1, 0);
    expect(
        shell.current.get() == Some(0)
            && shell.pages.borrow()[0].path == canonical_or_absolute(&other),
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
    expect(
        (shell.zoom.get() - ZOOM_MIN).abs() < 1e-9,
        "zoom floor".into(),
    );
    for _ in 0..40 {
        shell.control_key(KEY_PLUS, false);
    }
    expect(
        (shell.zoom.get() - ZOOM_MAX).abs() < 1e-9,
        "zoom ceiling".into(),
    );
    shell.set_zoom(1.0);
    // Theme and zoom reach the saved session file.
    let scratch = std::env::temp_dir().join(format!("mdview-theme-{}.ini", std::process::id()));
    let kept = shell
        .session
        .borrow_mut()
        .replace((scratch.clone(), Session::default()));
    shell.set_theme_choice(ThemeChoice::Dark);
    shell.set_zoom(1.3);
    let saved = Session::load(&scratch);
    _ = std::fs::remove_file(&scratch);
    *shell.session.borrow_mut() = kept;
    expect(
        saved.theme == ThemeChoice::Dark
            && saved.zoom == 130
            && shell.theme.get().name == DARK.name,
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
    expect(
        listed == shell.recent.borrow().paths().len() && listed >= 3,
        "manage list".into(),
    );
    shell.manage_sel.set(Some(0));
    shell.manage_remove();
    expect(
        !shell.recent.borrow().paths().contains(&gone),
        "manage remove".into(),
    );
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
    let names = |shell: &Shell| -> Vec<Option<String>> {
        shell
            .folder_rows
            .borrow()
            .iter()
            .map(|p| p.as_ref().map(|p| file_name(p)))
            .collect()
    };
    let collapsed = names(shell);
    expect(
        collapsed == vec![None, Some("b.md".to_string())],
        format!("folder rows collapsed {collapsed:?}"),
    );
    shell.files_click(0);
    let names = names(shell);
    expect(
        names
            == vec![
                None,
                Some("c.markdown".to_string()),
                Some("b.md".to_string()),
            ],
        format!("folder rows {names:?}"),
    );
    let tabs_before = shell.pages.borrow().len();
    shell.files_click(2);
    expect(
        shell.pages.borrow().len() == tabs_before + 1,
        "folder click opens".into(),
    );
    let opened = shell.current.get().unwrap();
    shell.close_tab(opened);
    shell
        .recent
        .borrow_mut()
        .remove(&canonical_or_absolute(&root.join("b.md")));
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
    expect(
        shell.blocks() > before,
        format!("auto-refresh: {} blocks, was {before}", shell.blocks()),
    );
    let kept = shell.blocks();
    std::fs::remove_file(&watched).unwrap();
    shell.poll_files();
    expect(
        shell.blocks() == kept,
        "a deleted file kept its content".into(),
    );
    std::fs::write(&watched, "# Back\n").unwrap();
    shell.poll_files();
    expect(shell.blocks() < kept, "a recreated file reloaded".into());
    shell.close_tab(index);
    shell
        .recent
        .borrow_mut()
        .remove(&canonical_or_absolute(&watched));
    _ = std::fs::remove_file(&watched);
    shell.control_key(KEY_F, true);
    expect(shell.focus.get().is_some(), "focus mode on".into());
    shell.key(KEY_ESCAPE);
    expect(
        shell.focus.get().is_none(),
        "Escape leaves focus mode".into(),
    );
    let blocks = shell.blocks();
    shell.key(KEY_F5);
    expect(
        shell.blocks() == blocks && shell.document.borrow().is_some(),
        "refresh".into(),
    );
    shell.tab_menu(Some(0), (20, 20)); // builds and pops the menu; must not crash or change tabs
    shell.tab_menu(None, (20, 20));
    expect(
        shell.pages.borrow().len() == 2,
        "tab menu changed the tabs".into(),
    );
    // "Close other tabs" from the first tab's menu leaves just that tab; reopen the second.
    shell.close_others(0);
    expect(
        shell.pages.borrow().len() == 1 && bar.count() == 1 && shell.current.get() == Some(0),
        "Close other tabs".into(),
    );
    shell.open(&other);
    expect(
        shell.pages.borrow().len() == 2 && shell.current.get() == Some(1),
        "reopen".into(),
    );
    shell.open(&first); // keeps `first` the most recent, as the Recent check below expects
    shell.activate(1);
    shell.control_key(KEY_W, false);
    expect(
        shell.pages.borrow().len() == 1
            && shell
                .path
                .borrow()
                .as_deref()
                .is_some_and(|p| same_file(p, &first)),
        "Ctrl+W closed the wrong tab".into(),
    );
    expect(
        shell.current.get() == Some(0) && bar.count() == 1,
        "after close".into(),
    );
    shell.control_key(KEY_W, true);
    expect(
        shell.pages.borrow().is_empty() && bar.count() == 0,
        "Ctrl+Shift+W".into(),
    );
    expect(
        shell.path.borrow().is_none(),
        "empty page still has a path".into(),
    );
    expect(shell.blocks() > 0, "empty page shows nothing".into());
    let recent: Vec<PathBuf> = shell.recent.borrow().paths().to_vec();
    expect(
        recent.len() == 2 && same_file(&recent[0], &first),
        format!("recent {recent:?}"),
    );
    if failures.is_empty() {
        println!(
            "tabs test ok: 2 tabs, scroll {scrolled} kept, recent {}",
            recent.len()
        );
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
            eprintln!(
                "mdview-qt: build {what} at {:.1} ms",
                t0.elapsed().as_secs_f64() * 1000.0
            );
        }
    };
    let window = H::<QMainWindow>::new();
    set_menu_parent(window.widget());
    window.widget().set_title("mdview");
    window.widget().resize(1100, 900);

    // The menus live in the command bar's main menu (☰); the menu bar itself is hidden.
    let menus = H::<QMenu>::new_popup();
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
    // The command bar (layout A): Open, Reload, Recent | Files, Outline | zoom on the left; the find
    // box, Theme and the main menu on the right. It is the window's only toolbar and cannot move.
    // The title row comes first: the window is frameless, and this row holds the logo, the tabs, the
    // drag area and the window buttons (filled once the tab bar exists, below).
    window.widget().set_frameless();
    let title = window.add_tool_bar("Title");
    title.set_movable(false);
    window.add_tool_bar_break();
    let bar = window.add_tool_bar("Commands");
    bar.set_movable(false);
    let open_button = bar.add_action("Open");
    open_button.set_tool_tip(TIP_OPEN);
    open_button.on_triggered(move |_| open.trigger());
    let reload_button = bar.add_action("↻");
    reload_button.set_tool_tip(TIP_RELOAD);
    reload_button.on_triggered(move |_| refresh.trigger());
    let recent_button = bar.add_action("Recent");
    recent_button.set_tool_tip(TIP_RECENT);
    let s = shell.clone();
    recent_button.on_triggered(move |_| {
        s.fill_recent(recent_menu);
        if let Some(at) = bar.below(recent_button) {
            recent_menu.popup(at);
        }
    });
    bar.add_separator();
    let files_button = bar.add_action("Files");
    files_button.set_tool_tip(TIP_FILES);
    let outline_button = bar.add_action("Outline");
    outline_button.set_tool_tip(TIP_OUTLINE);
    bar.add_separator();
    let zoom_out_button = bar.add_action("−");
    zoom_out_button.set_tool_tip(TIP_ZOOM_OUT);
    let s = shell.clone();
    zoom_out_button.on_triggered(move |_| s.set_zoom(s.zoom.get() - ZOOM_STEP));
    let zoom_label = bar.add_action("100%");
    zoom_label.set_tool_tip("Reset zoom (Ctrl+0)");
    let s = shell.clone();
    zoom_label.on_triggered(move |_| s.set_zoom(1.0));
    let zoom_in_button = bar.add_action("+");
    zoom_in_button.set_tool_tip(TIP_ZOOM_IN);
    let s = shell.clone();
    zoom_in_button.on_triggered(move |_| s.set_zoom(s.zoom.get() + ZOOM_STEP));
    shell.zoom_label.set(Some(zoom_label));
    shell.refresh_zoom_label();
    bar.add_spacer();
    let find_count = bar.add_action("");
    find_count.set_enabled(false);
    shell.find_count.set(Some(find_count));
    let find_edit = H::<QLineEdit>::new();
    find_edit.set_clear_button(true);
    find_edit.set_placeholder("Find in document  (Ctrl+F)");
    find_edit.widget().set_fixed_width(260);
    bar.add_widget(find_edit.widget());
    let previous = bar.add_action("↑");
    previous.set_tool_tip(TIP_PREVIOUS);
    let next = bar.add_action("↓");
    next.set_tool_tip(TIP_NEXT);
    let s = shell.clone();
    find_edit.on_text_changed(move |text| s.find_update(&text));
    let s = shell.clone();
    find_edit.on_return_pressed(move || s.find_step(1));
    let s = shell.clone();
    previous.on_triggered(move |_| s.find_step(-1));
    let s = shell.clone();
    next.on_triggered(move |_| s.find_step(1));
    bar.add_separator();
    // Theme cycles System -> Light -> Dark -> System; its label names the current mode.
    let theme_button = bar.add_action(theme_label(shell.choice.get()));
    theme_button.set_tool_tip(theme_tip(shell.choice.get()));
    let s = shell.clone();
    theme_button.on_triggered(move |_| {
        let next = match s.choice.get() {
            ThemeChoice::System => ThemeChoice::Light,
            ThemeChoice::Light => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::System,
        };
        // Text and tooltip first: the restyle in `set_theme_choice` then gives the button the new
        // mode's icon (a text change alone would drop the icon the style sheet set).
        theme_button.set_text(theme_label(next));
        theme_button.set_tool_tip(theme_tip(next));
        s.set_theme_choice(next);
    });
    let main_button = bar.add_action("☰");
    main_button.set_tool_tip(TIP_MENU);
    let main_menu = H::<QMenu>::new_popup();
    shell.main_menu.set(Some(main_menu));
    main_button.on_triggered(move |_| {
        if let Some(at) = bar.below_right(main_button, main_menu) {
            main_menu.popup(at);
        }
    });
    shell.find_bar.set(Some(bar));
    shell.find_edit.set(Some(find_edit));

    let outline_dock = H::<QDockWidget>::new("Outline", window.clone());
    let outline_tree = Tree::new();
    outline_dock.set_widget(outline_tree.widget());
    window.add_dock(false, outline_dock);
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
        // No folder yet: show the one the current file is in; ask only with no file open.
        if s.folder.borrow().is_none() {
            let dir = s
                .path
                .borrow()
                .as_ref()
                .and_then(|path| path.parent().map(Path::to_path_buf));
            match dir {
                Some(dir) => s.open_folder(&dir),
                None => s.choose_folder(),
            }
            return;
        }
        if let Some(dock) = s.files_dock.get() {
            dock.widget().set_visible(!dock.widget().is_visible());
        }
        s.save_layout(None);
    });
    let outline = view_menu.add_action("&Outline");
    let s = shell.clone();
    outline.on_triggered(move |_| {
        let visible = s
            .outline_dock
            .get()
            .is_some_and(|dock| dock.widget().is_visible());
        s.show_outline(!visible);
        s.save_layout(None);
    });
    files_button.on_triggered(move |_| files_toggle.trigger());
    outline_button.on_triggered(move |_| outline.trigger());
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
    shortcuts.on_triggered(move |_| {
        let rows: String = mdview_core::commands::shortcut_help()
            .into_iter()
            .map(|(keys, label)| {
                format!("<tr><td><b>{keys}</b>&nbsp;&nbsp;</td><td>{label}</td></tr>")
            })
            .collect();
        if let Some(window) = s.window.get() {
            about_box(
                window.widget(),
                strings::SHORTCUTS_TITLE,
                &format!("<table cellspacing=\"4\">{rows}</table>"),
            );
        }
    });
    if crate::appimage_update::appimage().is_some() {
        let update_now = help.add_action("&Update mdview now");
        let s = shell.clone();
        update_now.on_triggered(move |_| {
            if s.updating.borrow().is_some() {
                return;
            }
            match crate::appimage_update::spawn_apply(s.waker()) {
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
    about.on_triggered(move |_| {
        if let Some(window) = s.window.get() {
            about_box(
                window.widget(),
                strings::ABOUT_TITLE,
                &format!(
                    "<table><tr><td valign=\"top\">{}</td><td style=\"padding-left: 16px\"><h3>mdview {}</h3><p>{}</p><p>WinUI 3 on Windows, Qt on Linux; one renderer, no webview.</p></td></tr></table>",
                    about_logo(),
                    env!("CARGO_PKG_VERSION"),
                    strings::ABOUT
                ),
            );
        }
    });
    // The ☰ menu: the same flat list as WinUI's "More", then what only this shell has.
    if let Some(main_menu) = shell.main_menu.get() {
        let entries = [
            ("Open folder...\tCtrl+Shift+O", open_folder),
            ("Manage recent files...", manage),
            ("Close tab\tCtrl+W", close_tab),
            ("Close all tabs\tCtrl+Shift+W", close_all),
            ("Copy\tCtrl+C", copy),
            ("Actual size\tCtrl+0", zoom_reset),
            ("Focus reading mode\tCtrl+Shift+F", focus_mode),
            ("Keyboard shortcuts", shortcuts),
            ("About mdview", about),
            ("Exit", quit),
        ];
        for (index, (label, action)) in entries.into_iter().enumerate() {
            if index == 6 || index == 9 {
                main_menu.add_separator();
            }
            main_menu
                .add_action(label)
                .on_triggered(move |_| action.trigger());
        }
    }
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
    // OS file watching instead of the original's 800 ms poll; a 2 s poll only if it cannot start.
    let wake = canvas.waker();
    *shell.watcher.borrow_mut() = mdview_core::watch::FileWatcher::new(wake.clone());
    if shell.watcher.borrow().is_none() {
        eprintln!("mdview-qt: file watching unavailable, polling every 2 s");
        canvas.start_timer(2000);
    }
    _ = shell.wake.set(Box::new(wake.clone()));
    shell.rewatch();
    // Anything a worker sent before the slot was set is picked up by one check after start-up.
    wake();
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
    let s = shell.clone();
    canvas
        .widget()
        .on_context_menu_at(move |local, global| s.canvas_menu(local, global));
    shell.tab_bar.set(Some(tabs));
    let brand = title.add_action("mdview");
    brand.set_tool_tip(TIP_BRAND);
    title.add_widget(tabs.widget());
    let drag = title.add_spacer();
    drag.set_tool_tip("Drag to move the window; double-click to maximize");
    canvas.watch_title_drag(drag);
    let top = window.widget();
    let minimize = title.add_action("—");
    minimize.set_tool_tip(TIP_MINIMIZE);
    minimize.on_triggered(move |_| top.minimize());
    let maximize = title.add_action("☐");
    maximize.set_tool_tip(TIP_MAXIMIZE);
    maximize.on_triggered(move |_| top.toggle_maximized());
    let close_window = title.add_action("✕");
    close_window.set_tool_tip(TIP_CLOSE);
    close_window.on_triggered(move |_| top.close());
    mark("canvas and tabs");
    window.set_central(page);
    shell.canvas.set(Some(canvas));
    shell.scrollbar.set(Some(bar));
    shell.window.set(Some(window));
    shell.style_chrome();
    shell.status.set(Some(window.status_bar()));
    window.menu_bar().widget().set_visible(false);
    shell.menu_bar.set(Some(window.menu_bar()));
    let saved_size = shell
        .session
        .borrow()
        .as_ref()
        .and_then(|(_, saved)| saved.window_size());
    if let Some((w, h, _)) = saved_size {
        window.widget().resize(w as i32, h as i32);
    }
    if saved_size.is_some_and(|(_, _, max)| max) {
        window.widget().maximize();
    } else {
        window.widget().show();
    }
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
        let folder = saved.folder.clone().filter(|d| d.is_dir());
        if let Some(dir) = &folder {
            shell.open_folder(dir);
        }
        // Panes as they were left; a session from before they were saved keeps the old behaviour.
        if !saved.layout.is_empty() {
            let (files, outline) = saved.panes();
            if files && folder.is_none() {
                let dir = shell
                    .path
                    .borrow()
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf));
                if let Some(dir) = dir {
                    shell.open_folder(&dir);
                }
            } else if !files {
                if let Some(dock) = shell.files_dock.get() {
                    dock.widget().set_visible(false);
                }
            }
            if outline {
                shell.show_outline(true);
            }
            shell.save_layout(None);
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

/// The logo as an `<img>` for the About dialog: a 64 px PNG drawn beside the bar icons.
fn about_logo() -> String {
    let dir = std::env::temp_dir().join("mdview-icons");
    let path = dir.join("logo-about.png");
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><rect x="2" y="2" width="20" height="20" rx="5" fill="#2980b9"/><path d="M6.5 16V8.5l3 3.5 3-3.5V16M16.5 8.5v7.5m-2-2 2 2.2 2-2.2" fill="none" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
    if std::fs::create_dir_all(&dir).is_err() || write_png_sized(&path, svg, 64).is_none() {
        return String::new();
    }
    let src = path.to_string_lossy().replace('\\', "/");
    format!("<img src=\"{src}\" width=\"64\" height=\"64\">")
}

fn theme_label(choice: ThemeChoice) -> &'static str {
    match choice {
        ThemeChoice::System => "System",
        ThemeChoice::Light => "Light",
        ThemeChoice::Dark => "Dark",
    }
}

const TIP_BRAND: &str = "mdview";
const TIP_OPEN: &str = "Open a file (Ctrl+O)";
const TIP_RELOAD: &str = "Reload (F5)";
const TIP_RECENT: &str = "Recent files";
const TIP_FILES: &str = "Files pane (Ctrl+B)";
const TIP_OUTLINE: &str = "Outline pane";
const TIP_ZOOM_OUT: &str = "Zoom out (Ctrl+-)";
const TIP_ZOOM_IN: &str = "Zoom in (Ctrl++)";
const TIP_PREVIOUS: &str = "Previous match (Shift+F3)";
const TIP_NEXT: &str = "Next match (F3)";
const TIP_THEME_SYSTEM: &str = "Theme: System (click for Light)";
const TIP_THEME_LIGHT: &str = "Theme: Light (click for Dark)";
const TIP_THEME_DARK: &str = "Theme: Dark (click for System)";

fn theme_tip(choice: ThemeChoice) -> &'static str {
    match choice {
        ThemeChoice::System => TIP_THEME_SYSTEM,
        ThemeChoice::Light => TIP_THEME_LIGHT,
        ThemeChoice::Dark => TIP_THEME_DARK,
    }
}
const TIP_MENU: &str = "Main menu";
const TIP_MINIMIZE: &str = "Minimize";
const TIP_MAXIMIZE: &str = "Maximize";
const TIP_CLOSE: &str = "Close";

/// Line icons (24 px grid) for the bar buttons, as SVG bodies drawn in `{c}`.
const ICONS: &[(&str, &str)] = &[
    (
        "open",
        r#"<path d="M3.5 7.5v10a1.5 1.5 0 0 0 1.5 1.5h14a1.5 1.5 0 0 0 1.5-1.5v-8a1.5 1.5 0 0 0-1.5-1.5h-7l-2-2H5a1.5 1.5 0 0 0-1.5 1.5z"/>"#,
    ),
    (
        "reload",
        r#"<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3"/><path d="M19.5 4.5v4h-4"/>"#,
    ),
    (
        "recent",
        r#"<circle cx="12" cy="12" r="8"/><path d="M12 7.5V12l3 2"/>"#,
    ),
    (
        "files",
        r#"<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M9.5 4.5v15"/>"#,
    ),
    ("outline", r#"<path d="M4 6.5h16M7 12h13M7 17.5h13"/>"#),
    ("zoom_out", r#"<path d="M6 12h12"/>"#),
    ("zoom_in", r#"<path d="M6 12h12M12 6v12"/>"#),
    ("up", r#"<path d="M6.5 14.5 12 9l5.5 5.5"/>"#),
    ("down", r#"<path d="M6.5 9.5 12 15l5.5-5.5"/>"#),
    (
        "theme",
        r#"<circle cx="12" cy="12" r="7.5"/><path d="M12 4.5a7.5 7.5 0 0 1 0 15z" fill="{c}"/>"#,
    ),
    ("menu", r#"<path d="M4.5 7h15M4.5 12h15M4.5 17h15"/>"#),
    (
        "mode_system",
        r#"<rect x="3.5" y="4.5" width="17" height="11.5" rx="1.5"/><path d="M9 19.5h6M12 16v3.5"/>"#,
    ),
    (
        "mode_light",
        r#"<circle cx="12" cy="12" r="3.5"/><path d="M12 3.5v2M12 18.5v2M3.5 12h2M18.5 12h2M6 6l1.4 1.4M16.6 16.6 18 18M6 18l1.4-1.4M16.6 7.4 18 6"/>"#,
    ),
    (
        "mode_dark",
        r#"<path d="M19 14.5A7.5 7.5 0 0 1 9.5 5a7.5 7.5 0 1 0 9.5 9.5z"/>"#,
    ),
    ("minimize", r#"<path d="M6 12h12"/>"#),
    (
        "maximize",
        r#"<rect x="6.5" y="6.5" width="11" height="11" rx="1"/>"#,
    ),
    ("close", r#"<path d="M7 7l10 10M17 7 7 17"/>"#),
    ("tab_close", r#"<path d="M8.5 8.5l7 7M15.5 8.5l-7 7"/>"#),
];

/// Writes the bar icons in the theme's text colour (and the logo in its accent) as PNGs to a
/// per-theme folder in the temp directory, and returns that folder with forward slashes, as a style
/// sheet `url()` wants it.
fn write_icons(theme: Theme) -> Option<String> {
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    let dir = std::env::temp_dir().join("mdview-icons").join(theme.name);
    std::fs::create_dir_all(&dir).ok()?;
    let colour = hex(theme.text);
    let svg = |body: &str, c: &str| {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="{c}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
            body.replace("{c}", c)
        )
    };
    for (name, body) in ICONS {
        write_png(&dir.join(format!("{name}.png")), &svg(body, &colour))?;
    }
    let muted = hex(theme.muted);
    write_png(&dir.join("tab_close_muted.png"), &svg(ICONS[14].1, &muted))?;
    write_png(&dir.join("close_white.png"), &svg(ICONS[13].1, "#ffffff"))?;
    let accent = hex(theme.accent);
    let logo = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><rect x="2" y="2" width="20" height="20" rx="5" fill="{accent}"/><path d="M6.5 16V8.5l3 3.5 3-3.5V16M16.5 8.5v7.5m-2-2 2 2.2 2-2.2" fill="none" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>"##
    );
    write_png(&dir.join("logo.png"), &logo)?;
    Some(dir.to_string_lossy().replace('\\', "/"))
}

/// Renders an SVG icon to a 48 px PNG (2x, so it stays sharp at 200%).
fn write_png(path: &Path, svg: &str) -> Option<()> {
    write_png_sized(path, svg, 48)
}

/// Renders a 24 px SVG icon to a `size` px square PNG.
fn write_png_sized(path: &Path, svg: &str, size: u32) -> Option<()> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / 24.0;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(path).ok()
}

/// Gives each bar button its icon, picked out by its tooltip.
fn icon_rules(dir: &str) -> String {
    let rules: [(&str, &str, bool); 17] = [
        (TIP_BRAND, "logo", true),
        (TIP_OPEN, "open", true),
        (TIP_RELOAD, "reload", false),
        (TIP_RECENT, "recent", true),
        (TIP_FILES, "files", false),
        (TIP_OUTLINE, "outline", false),
        (TIP_ZOOM_OUT, "zoom_out", false),
        (TIP_ZOOM_IN, "zoom_in", false),
        (TIP_PREVIOUS, "up", false),
        (TIP_NEXT, "down", false),
        (TIP_THEME_SYSTEM, "mode_system", true),
        (TIP_THEME_LIGHT, "mode_light", true),
        (TIP_THEME_DARK, "mode_dark", true),
        (TIP_MENU, "menu", false),
        (TIP_MINIMIZE, "minimize", false),
        (TIP_MAXIMIZE, "maximize", false),
        (TIP_CLOSE, "close", false),
    ];
    let mut out = String::new();
    for (tip, icon, with_text) in rules {
        let style = if with_text {
            "ToolButtonTextBesideIcon"
        } else {
            "ToolButtonIconOnly"
        };
        out.push_str(&format!(
            "QToolButton[toolTip=\"{tip}\"] {{ qproperty-icon: url({dir}/{icon}.png); qproperty-toolButtonStyle: {style}; }}\n"
        ));
    }
    out
}

/// The style sheet for the window chrome in `theme`'s colours.
fn chrome_style(theme: Theme) -> String {
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    let mix = |a: [u8; 3], b: [u8; 3], t: f32| {
        let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        hex([m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])])
    };
    let (page, text) = (theme.bg, theme.text);
    let bar = mix(page, text, 0.06);
    let title = mix(page, text, 0.10);
    // Hover and press as WinUI shows them on its bar: a clear fill, darker while pressed.
    let hover = mix(page, text, 0.20);
    let press = mix(page, text, 0.28);
    let line = mix(page, text, 0.18);
    let muted = hex(theme.muted);
    let accent = hex(theme.accent);
    let page = hex(page);
    let text = hex(text);
    let icons = write_icons(theme)
        .map(|dir| (icon_rules(&dir), dir))
        .unwrap_or_default();
    let (icon_rules, dir) = icons;
    let tab_close = if dir.is_empty() {
        String::new()
    } else {
        format!(
            "QTabBar::close-button {{ image: url({dir}/tab_close_muted.png); subcontrol-position: right; width: 16px; height: 16px; margin: 0 2px; border-radius: 3px; }}\n\
             QTabBar::close-button:hover {{ image: url({dir}/tab_close.png); background: {hover}; }}\n\
             QToolButton[toolTip=\"{TIP_CLOSE}\"]:hover {{ background: #c42b1c; qproperty-icon: url({dir}/close_white.png); }}\n"
        )
    };
    format!(
        "QMainWindow {{ background: {bar}; }}
QMainWindow::separator {{ background: {line}; width: 1px; height: 1px; }}
QToolBar {{ background: {bar}; border: 0; padding: 6px 8px; spacing: 2px; qproperty-iconSize: 18px; }}
QToolBar:first {{ background: {title}; padding: 6px 4px 0 8px; spacing: 0; }}
QToolBar::separator {{ background: {line}; width: 1px; margin: 8px 6px; }}
QToolButton {{ color: {text}; background: transparent; border: 0; border-radius: 4px; padding: 6px 8px; font-size: 13px; }}
QToolButton:hover {{ background: {hover}; }}
QToolButton:pressed {{ background: {press}; }}
QToolButton[toolTip=\"{TIP_BRAND}\"] {{ font-weight: 600; padding: 4px 12px 8px 6px; }}
QToolButton[toolTip=\"{TIP_BRAND}\"]:hover {{ background: transparent; }}
QToolButton[toolTip=\"{TIP_MINIMIZE}\"], QToolButton[toolTip=\"{TIP_MAXIMIZE}\"], QToolButton[toolTip=\"{TIP_CLOSE}\"] {{ border-radius: 0; padding: 8px 16px; margin-top: -6px; }}
QLineEdit {{ background: {page}; color: {text}; border: 1px solid {line}; border-radius: 4px; padding: 6px 10px; font-size: 13px; selection-background-color: {accent}; }}
QLineEdit:hover {{ border: 1px solid {press}; }}
QLineEdit:focus {{ border: 1px solid {accent}; border-bottom: 2px solid {accent}; }}
QTabBar {{ background: transparent; qproperty-drawBase: 0; }}
QTabBar::tab {{ background-color: {title}; color: {muted}; border: 1px solid {title}; border-top: 2px solid {title}; border-bottom: 0; padding: 6px 10px 7px 14px; margin-right: 2px; min-width: 120px; border-top-left-radius: 6px; border-top-right-radius: 6px; }}
QTabBar::tab:selected {{ background-color: {bar}; color: {text}; border: 1px solid {line}; border-top: 2px solid {accent}; border-bottom: 0; }}
QTabBar::tab:hover:!selected {{ background-color: {hover}; color: {text}; border-color: {hover}; }}
QStatusBar {{ background: {bar}; color: {muted}; border-top: 1px solid {line}; }}
QStatusBar::item {{ border: 0; }}
QDockWidget {{ color: {muted}; font-size: 12px; }}
QDockWidget::title {{ background: {bar}; padding: 10px 12px 6px 12px; text-transform: uppercase; }}
QTreeWidget {{ background: {bar}; color: {text}; border: 0; padding: 2px 6px; font-size: 13px; outline: 0; }}
QTreeWidget::item {{ padding: 5px 4px; border-radius: 4px; }}
QTreeWidget::item:hover {{ background: {hover}; }}
QTreeWidget::item:selected {{ background: {accent}; color: {page}; }}
QMenu {{ background: {page}; color: {text}; border: 1px solid {line}; padding: 4px 0; font-size: 13px; min-width: 200px; }}
QMenu::item {{ padding: 8px 32px 8px 16px; background: transparent; }}
QMenu::item:selected:disabled {{ background: transparent; }}
QMenu::item:selected {{ background: {hover}; }}
QMenu::item:disabled {{ color: {muted}; }}
QMenu::separator {{ height: 1px; background: {line}; margin: 4px 0; }}
QMenu::right-arrow {{ width: 8px; height: 8px; }}
QToolTip {{ background: {page}; color: {text}; border: 1px solid {line}; padding: 4px 6px; }}
QScrollBar:vertical {{ background: {page}; width: 12px; margin: 0; }}
QScrollBar::handle:vertical {{ background: {line}; border-radius: 4px; min-height: 32px; margin: 2px 3px; }}
QScrollBar::handle:vertical:hover {{ background: {press}; }}
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{ height: 0; }}
QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical {{ background: transparent; }}
{tab_close}{icon_rules}"
    )
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
    if !theme_flag {
        // Follows the system unless a flag or the saved session (below) says otherwise.
        shell.set_theme_choice(ThemeChoice::System);
    }
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
        if !scripted && mdview_core::single::forward(&path, &files) {
            eprintln!(
                "mdview-qt: sent {} file(s) to the running window",
                files.len()
            );
            std::process::exit(0);
        }
        if !scripted {
            *shell.incoming.borrow_mut() = mdview_core::single::listen_waking(&path, shell.waker());
            *shell.updating.borrow_mut() = crate::appimage_update::spawn_check(shell.waker());
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
    let closing = shell.clone();
    run_app(
        move || build(&shell, files, drop_test, check, scroll),
        move || {
            if let Some(window) = closing.window.get() {
                closing.save_layout(Some(window.widget()));
            }
        },
    )
}
