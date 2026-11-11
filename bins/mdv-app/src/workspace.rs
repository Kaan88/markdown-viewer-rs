//! The headless state under the window: open tabs, the session, view settings, scrolling, hit
//! testing, selection, find, links, the outline and the status line. Nothing here touches a window,
//! a drawing toolkit, the clipboard or a dialog; what needs the OS comes back to the caller as a
//! value (`LinkOutcome::OpenUrl`, text to copy, paths to watch).
//!
//! The UI is immediate mode: it calls in every frame and on every input event, and asks
//! [`Workspace::take_dirty`] whether something changed that needs a redraw.
//!
//! Coordinates: the page viewport is in device px. `x`/`y` given to hit tests and returned in
//! rectangles are relative to the viewport's top-left corner; scroll offsets are device px from the
//! top of the page.
//!
//! **One renderer for the workspace.** Only the active tab is ever laid out or painted, so one
//! `DocumentRenderer` serves every tab: its glyph cache stays warm across tab switches, and an
//! inactive tab costs only its parsed `Document`. The renderer owns the active tab's `PageLayout`
//! and corrects it in place (a `<details>` toggle, an arrived image), keeping the content under
//! the viewport still; the workspace follows with [`Workspace::adopt`]. Leaving a tab drops its
//! view; the next `renderer.layout` call for the new tab replaces the layout, paint cache and
//! frame. `<details>` toggles are view state the renderer holds, so each tab keeps a copy and the
//! copy is applied to the renderer when the tab becomes active.
//!
//! Ported from `bins/mdv-qt/src/shell.rs`; differences are noted where they are made.

use mdview_core::commands::strings;
use mdview_core::files::{FileStamp, canonical_or_absolute, read_stable, stamp, tab_labels};
use mdview_core::links::{LinkAction, classify_link};
use mdview_core::model::{NONE, ScrollPos};
use mdview_core::session::{self, RecentEntry, Recents, Session};
use mdview_core::{Document, parse_bytes};
use mdview_fetch::{Fetcher, State as Fetched};
use mdview_render::{
    DARK, DocumentRenderer, Frame, Hit, Jump, LIGHT, Options, PageLayout, Rect, RemoteState,
    SourceKind, Theme,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The View menu's theme choice; `System` follows the desktop.
pub use mdview_core::session::Theme as ThemeChoice;

/// Space left above a jumped-to heading, footnote or match, in logical px.
const JUMP_MARGIN: f32 = 24.0;
/// One arrow-key or wheel-notch step, in logical px (the Qt shell used 48 device px).
const LINE_STEP: f32 = 48.0;
/// Page Up/Down move this share of the viewport, as in the Qt shell.
const PAGE_SHARE: f32 = 0.9;
/// Reading speed for the time estimate, in words per minute.
const WORDS_PER_MINUTE: usize = 230;
/// At most this many find marks are drawn per frame (only visible matches are considered).
const MAX_MARKS: usize = 1000;

/// Counts for the active document, computed once per load.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocStats {
    pub blocks: usize,
    pub words: usize,
    /// Rounded up; 0 only for a document without words.
    pub reading_minutes: usize,
}

impl DocStats {
    fn of(doc: &Document) -> DocStats {
        let words = doc.text.split_whitespace().count();
        DocStats {
            blocks: doc.blocks.len(),
            words,
            reading_minutes: words.div_ceil(WORDS_PER_MINUTE),
        }
    }
}

/// What [`Workspace::paint`] drew: the renderer's frame and where it goes.
pub struct PageView<'a> {
    /// The viewport-sized bitmap, with `band`/`shift` describing what changed since the last frame.
    pub frame: &'a Frame,
    /// Device px from the viewport's left edge at which to blit the frame (the renderer centres
    /// the reading column inside the bitmap itself, so this is 0 unless rounding differs).
    pub x: i32,
    /// The scroll offset the frame shows, device px.
    pub scroll: f32,
    /// The whole page's height, device px, for a scrollbar.
    pub total_height: f32,
    /// The page background, for any area the frame does not cover.
    pub background: [u8; 3],
}

/// What following a link did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    /// Scrolled to a fragment in this document (or will once it is laid out).
    Scrolled,
    /// Opened, or switched to, the Markdown file at this tab index.
    OpenedTab(usize),
    /// The caller should hand this http/https/mailto URL to the system.
    OpenUrl(String),
    /// Refused; the text is also the status message.
    Refused(String),
    /// A scheme the viewer never follows (`javascript:` and the like); nothing happens.
    Ignored,
}

/// What a pointer release meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Click {
    /// No press was in progress, or the press missed the page.
    Nothing,
    /// A plain click on text: the selection was cleared.
    Deselected,
    /// A drag (or a double/triple click) left a selection.
    Selected,
    Link(LinkOutcome),
    /// Jumped to a footnote.
    Footnote,
    /// Opened or closed a `<details>`.
    Details,
}

/// What is under the pointer, for the cursor shape and a hover hint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hover {
    /// The link's destination as written.
    pub link: Option<String>,
    /// A link, footnote mark or `<details>` summary: show the pointing hand.
    pub pointing: bool,
    /// The pointer is on a line of text: show the text cursor. Off it (a margin, the gap
    /// between blocks, a picture) the arrow stays.
    pub text: bool,
}

/// Somewhere the reader was: a file and how far down it. Back and Forward walk these.
#[derive(Debug, Clone, PartialEq)]
struct Place {
    path: PathBuf,
    scroll: f32,
}

/// The most places Back remembers.
const HISTORY_LIMIT: usize = 100;

/// What a right click offers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageContext {
    /// The destination of the link under the pointer (for Open link / Copy link address).
    pub link: Option<String>,
    pub has_selection: bool,
    /// The TeX or Mermaid text of the formula or diagram under the pointer.
    pub source: Option<(SourceKind, String)>,
}

/// Find counts for the find box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FindCounts {
    /// 1-based index of the current match; 0 when there is none.
    pub current: usize,
    pub total: usize,
    /// A limit stopped the search, so the real total may be higher.
    pub capped: bool,
}

/// Find marks in viewport coordinates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FindMarks {
    /// Every visible match except the current one.
    pub others: Vec<Rect>,
    pub current: Vec<Rect>,
}

/// One outline row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutlineEntry<'a> {
    pub level: u8,
    pub title: &'a str,
    pub block: u32,
}

/// The status line: a message and when it was set. It shows for `strings::TRANSIENT_MS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
    pub at: Instant,
}

/// Something to bring into view once the tab has a layout.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Reveal {
    /// A fragment, as the link wrote it.
    Anchor(String),
    /// A text range, such as the current find match.
    Range(u32, u32),
}

/// The active tab's layout: the options it was made for and what the workspace derives from it.
/// The layout itself is the renderer's, which corrects it in place (a `<details>` toggle, an
/// arrived image); `generation` says which version `heading_ys` was taken from.
struct View {
    options: Options,
    /// The page y of each heading in `Document::headings`, non-decreasing.
    heading_ys: Vec<f32>,
    generation: u64,
}

/// One open file. A tab restored from the session holds only its path until it is first shown.
pub struct Tab {
    path: PathBuf,
    doc: Option<Document>,
    /// What the file looked like when last read; `check_changed` compares against it.
    stamp: FileStamp,
    /// The file is gone; the last text read stays on screen.
    missing: bool,
    stats: DocStats,
    /// Device px from the page top.
    scroll: f32,
    /// The reading position to restore at the next layout: a block and an offset into it in
    /// logical px. Taken whenever a layout is dropped, so tab switches, resizes, zoom, theme and
    /// reloads keep the reader where they were.
    keep: Option<ScrollPos>,
    reveal: Option<Reveal>,
    /// Two offsets into `Document::text`, anchor first.
    selection: Option<(u32, u32)>,
    /// `<details>` toggles, by block.
    details: HashMap<u32, bool>,
    /// Only the active tab has one.
    view: Option<View>,
    /// When the tab was last left, for [`Workspace::drop_idle`].
    left_at: Option<Instant>,
    remote: Remote,
}

/// What a tab decided about its document's remote images.
#[derive(Default)]
struct Remote {
    /// The reader chose "Load remote images" for this document.
    allowed: bool,
    /// The reader closed the bar without loading.
    dismissed: bool,
    /// How many different remote images the document names, counted when it is read. The bar
    /// shows from the first frame, so it never pushes the page down while the reader scrolls.
    count: usize,
}

impl Tab {
    fn unloaded(path: PathBuf) -> Tab {
        Tab {
            path,
            doc: None,
            stamp: FileStamp::Missing,
            missing: false,
            stats: DocStats::default(),
            scroll: 0.0,
            keep: None,
            reveal: None,
            selection: None,
            details: HashMap::new(),
            view: None,
            left_at: None,
            remote: Remote::default(),
        }
    }

    fn set_document(&mut self, doc: Document, stamp: FileStamp) {
        self.stats = DocStats::of(&doc);
        self.remote.count = remote_images(&doc);
        self.doc = Some(doc);
        self.stamp = stamp;
    }

    /// The file this tab shows (canonical, or absolute when the file is missing).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// False for a restored tab that has not been shown yet (or was unloaded by `drop_idle`).
    pub fn is_loaded(&self) -> bool {
        self.doc.is_some()
    }

    /// The file was deleted (its last text is still shown) or could not be found.
    pub fn is_missing(&self) -> bool {
        self.missing
    }

    #[allow(dead_code)] // Workspace API the shell does not call yet.
    pub fn document(&self) -> Option<&Document> {
        self.doc.as_ref()
    }

    /// Records the reading position from the current layout (the renderer's, which is this tab's
    /// while it has a view), then drops the view.
    fn drop_view(&mut self, layout: Option<&PageLayout>) {
        if let (Some(view), Some(_), Some(layout)) = (self.view.take(), self.doc.as_ref(), layout)
            && self.keep.is_none()
            && let Some(span) = layout.block_at(self.scroll)
        {
            self.keep = Some(ScrollPos {
                block: span.block,
                offset: (self.scroll - span.y) / view.options.scale.max(0.01),
            });
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Viewport {
    width: u32,
    height: u32,
    scale: f32,
}

#[derive(Default)]
struct Find {
    query: String,
    hits: Vec<(u32, u32)>,
    current: usize,
    capped: bool,
    /// The renderer's count of changes to its list of undrawn formulas and diagrams when the
    /// hits were found; a different count means their source came into or left the search.
    undrawn: u64,
}

/// A press in progress: what it started on, and whether the pointer has moved off it.
#[derive(Debug, Clone, Copy)]
struct Press {
    /// The range a drag extends from: one offset for a click, a word or block for a multi-click.
    anchor: (u32, u32),
    link: Option<u32>,
    footnote: Option<u32>,
    details: Option<u32>,
    moved: bool,
}

/// Every open tab plus the state shared between them.
pub struct Workspace {
    /// Where following a link, a footnote or an outline entry took the reader from, oldest
    /// first, and where Back took them from. One history for the window, across tabs.
    back: Vec<Place>,
    forward: Vec<Place>,
    tabs: Vec<Tab>,
    labels: Vec<String>,
    active: Option<usize>,
    renderer: DocumentRenderer,
    recents: Recents,
    /// The saved session, kept whole so the keys this module does not set round-trip.
    session: Session,
    session_path: Option<PathBuf>,
    choice: ThemeChoice,
    system_dark: bool,
    zoom: u16,
    viewport: Option<Viewport>,
    find: Find,
    press: Option<Press>,
    status: Option<StatusLine>,
    dirty: bool,
    watch_dirty: bool,
    /// Started by the first remote image that may load; nothing touches the network before.
    fetcher: Option<Fetcher>,
    /// Posts a message to the event loop when a download finishes (set by the window).
    remote_wake: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Remote SVGs delivered so far and the scale they were rasterized at, to deliver them again
    /// when the scale changes.
    remote_svgs: Vec<String>,
    remote_scale: f32,
    /// Remote URLs a page asked for and did not get, each once. The image store is shared by
    /// every tab, so this is too: a document's share is the URLs its own images name.
    held: Vec<String>,
}

impl Workspace {
    // ---- construction and session ------------------------------------------------------------

    /// Starts from the session at `session_path` (`None`: no session is read or written). Saved
    /// tabs come back holding only their paths; the active one is loaded.
    pub fn new(session_path: Option<PathBuf>) -> Workspace {
        let saved = session_path
            .as_deref()
            .map(Session::load)
            .unwrap_or_default();
        let mut ws = Workspace {
            back: Vec::new(),
            forward: Vec::new(),
            tabs: Vec::new(),
            labels: Vec::new(),
            active: None,
            renderer: DocumentRenderer::new(),
            recents: saved.recent.clone(),
            choice: saved.theme,
            system_dark: false,
            zoom: session::clamp_zoom(saved.zoom),
            session: saved,
            session_path,
            viewport: None,
            find: Find::default(),
            press: None,
            status: None,
            dirty: true,
            watch_dirty: true,
            fetcher: None,
            remote_wake: None,
            remote_svgs: Vec::new(),
            remote_scale: 0.0,
            held: Vec::new(),
        };
        for path in ws.session.tabs.clone() {
            let path = canonical_or_absolute(&path);
            if !ws.tabs.iter().any(|t| same_file(&t.path, &path)) {
                ws.tabs.push(Tab::unloaded(path));
            }
        }
        ws.relabel();
        if !ws.tabs.is_empty() {
            let active = ws.session.active.min(ws.tabs.len() - 1);
            ws.activate(active);
        }
        ws
    }

    /// [`new`](Self::new) with `mdview_core::session::default_settings_path()`.
    #[allow(dead_code)] // Workspace API the shell does not call yet.
    pub fn with_default_session() -> Workspace {
        Workspace::new(session::default_settings_path())
    }

    /// Writes tabs, the active tab, Recent, folder, theme and zoom (plus panes and window size as
    /// last set). Does nothing without a session path. Called after every open, close, move and
    /// setting change; call it once more on exit.
    pub fn save_session(&mut self) -> std::io::Result<()> {
        let Some(path) = self.session_path.as_deref() else {
            return Ok(());
        };
        self.session.tabs = self.tabs.iter().map(|t| t.path.clone()).collect();
        self.session.active = self.active.unwrap_or(0);
        self.session.recent = self.recents.clone();
        self.session.theme = self.choice;
        self.session.zoom = self.zoom;
        self.session.save(path)
    }

    /// Saves, reports a failure on stderr as the Qt shell did, and re-aims the file watcher.
    fn persist(&mut self) {
        self.watch_dirty = true;
        if let Err(e) = self.save_session() {
            eprintln!("mdview: could not save the session: {e}");
        }
    }

    /// Side panes as saved: `(files, outline)`.
    pub fn panes(&self) -> (bool, bool) {
        self.session.panes()
    }

    /// Records which side panes are showing, and saves.
    pub fn set_panes(&mut self, files: bool, outline: bool) {
        self.session.set_panes(files, outline);
        self.persist();
    }

    /// The saved place of the window's top-left corner, in desktop device px.
    pub fn window_position(&self) -> Option<(i32, i32)> {
        self.session.window_position()
    }

    /// Records where the window is. Not saved at once (a drag reports every step); the next
    /// save, the one at exit at the latest, writes it.
    pub fn set_window_position(&mut self, x: i32, y: i32) {
        self.session.set_window_position(x, y);
    }

    /// The saved widths of the Files and Outline panes, in logical px.
    pub fn pane_widths(&self) -> (Option<u32>, Option<u32>) {
        self.session.pane_widths()
    }

    /// Records the side panes' widths, and saves.
    pub fn set_pane_widths(&mut self, files: u32, outline: u32) {
        if self.session.pane_widths() != (Some(files), Some(outline)) {
            self.session.set_pane_widths(Some(files), Some(outline));
            self.persist();
        }
    }

    // ---- back and forward --------------------------------------------------------------------

    fn place(&self) -> Option<Place> {
        self.active_tab().map(|tab| Place {
            path: tab.path.clone(),
            scroll: tab.scroll,
        })
    }

    /// Notes that the reader has just left `from` by a jump, so Back returns there. A jump that
    /// did not move them is not a place to go back to.
    fn remember(&mut self, from: Option<Place>) {
        let Some(from) = from else { return };
        if self.place().as_ref() == Some(&from) {
            return;
        }
        self.back.push(from);
        if self.back.len() > HISTORY_LIMIT {
            self.back.remove(0);
        }
        self.forward.clear();
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Returns to where the last jump was made from: that file's tab (opened again if it was
    /// closed) at that scroll position. False when there is nowhere to go.
    pub fn go_back(&mut self) -> bool {
        self.travel(true)
    }

    /// Undoes [`go_back`](Self::go_back).
    pub fn go_forward(&mut self) -> bool {
        self.travel(false)
    }

    fn travel(&mut self, back: bool) -> bool {
        loop {
            let next = if back {
                self.back.pop()
            } else {
                self.forward.pop()
            };
            let Some(next) = next else { return false };
            let here = self.place();
            // A file that has gone since is skipped, not an error to show.
            if self.open(&next.path).is_err() {
                continue;
            }
            if let Some(here) = here {
                if back {
                    self.forward.push(here);
                } else {
                    self.back.push(here);
                }
            }
            self.ensure_layout();
            self.scroll_to(next.scroll);
            self.dirty = true;
            return true;
        }
    }

    /// Scrolls to an outline entry's heading, as a jump Back returns from.
    pub fn outline_jump(&mut self, block: u32) -> bool {
        let from = self.place();
        let moved = self.scroll_to_block(block);
        self.remember(from);
        moved
    }

    /// The saved window size in logical px and whether it was maximized.
    pub fn window_size(&self) -> Option<(u32, u32, bool)> {
        self.session.window_size()
    }

    /// Records the window size and saves. While maximized the last restored size is kept, so
    /// un-maximizing next time looks right (the Qt shell's rule).
    pub fn set_window_size(&mut self, width: u32, height: u32, maximized: bool) {
        let (width, height) = match (maximized, self.session.window_size()) {
            (true, Some((w, h, _))) => (w, h),
            _ => (width, height),
        };
        self.session.set_window_size(width, height, maximized);
        self.persist();
    }

    /// The folder shown in the Files pane.
    pub fn folder(&self) -> Option<&Path> {
        self.session.folder.as_deref()
    }

    pub fn set_folder(&mut self, folder: Option<PathBuf>) {
        self.session.folder = folder;
        self.persist();
    }

    // ---- redraw and watcher signals ----------------------------------------------------------

    /// True once after anything visible changed.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Marks the page for a redraw (for example after the window was uncovered).
    #[allow(dead_code)] // Workspace API the shell does not call yet.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// True once after the set of open files changed; then pass [`watched_paths`](Self::watched_paths)
    /// to `FileWatcher::set_files`.
    pub fn take_watch_changed(&mut self) -> bool {
        std::mem::take(&mut self.watch_dirty)
    }

    /// Every open tab's file, loaded or not.
    pub fn watched_paths(&self) -> impl Iterator<Item = &Path> {
        self.tabs.iter().map(|t| t.path.as_path())
    }

    // ---- tabs --------------------------------------------------------------------------------

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tab(&self, index: usize) -> Option<&Tab> {
        self.tabs.get(index)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// The showing tab; `None` means the empty state.
    pub fn active(&self) -> Option<usize> {
        self.active
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active?)
    }

    pub fn active_document(&self) -> Option<&Document> {
        self.active_tab()?.doc.as_ref()
    }

    /// Tab titles in tab order; repeated filenames gain parent folders (`files::tab_labels`).
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// `strings::window_title` with the active file's name.
    /// Images decoded and engine jobs sent so far, for traces.
    #[allow(dead_code)] // Only the windowed startup trace reads it.
    pub fn image_stats(&self) -> mdview_render::ImageStats {
        self.renderer.image_stats()
    }

    pub fn title(&self) -> String {
        strings::window_title(self.active_tab().map(|t| file_name(&t.path)).as_deref())
    }

    /// Opens `path` in a new tab and shows it, or shows the tab that already has it. Adds it to
    /// Recent either way. A file that cannot be read sets the status and opens nothing.
    pub fn open(&mut self, path: &Path) -> Result<usize, String> {
        let path = canonical_or_absolute(path);
        if let Some(index) = self.tabs.iter().position(|t| same_file(&t.path, &path)) {
            self.recents.add(path);
            self.activate(index);
            self.persist();
            return Ok(index);
        }
        match read_stable(&path) {
            Ok(loaded) => {
                let mut tab = Tab::unloaded(path.clone());
                tab.set_document(parse_bytes(&loaded.bytes), loaded.stamp);
                self.tabs.push(tab);
                self.recents.add(path);
                self.relabel();
                let index = self.tabs.len() - 1;
                self.activate(index);
                self.persist();
                Ok(index)
            }
            Err(e) => {
                let text = format!("{}: {e}", path.display());
                self.message(&text);
                Err(text)
            }
        }
    }

    /// Opens what was dropped on the window: the first line, a path or a `file:` URL. Core's link
    /// rules apply, so a UNC host is refused here too.
    #[allow(dead_code)] // Workspace API the shell does not call yet.
    pub fn open_dropped(&mut self, text: &str) -> Result<usize, String> {
        let first = text.lines().next().unwrap_or("").trim();
        if first.is_empty() {
            return Err(String::new());
        }
        let path = match classify_link(first, Path::new("")) {
            LinkAction::OpenMarkdown { path, .. } | LinkAction::OtherLocalFile(path) => path,
            _ => return Err(self.refuse(format!("cannot open {first}"))),
        };
        if !path.is_file() {
            return Err(self.refuse(format!("not a file: {}", path.display())));
        }
        self.open(&path)
    }

    /// Shows tab `index`, loading it first if it was restored lazily. The tab left behind keeps its
    /// document, scroll and selection but drops its layout. Find is cleared, as in the Qt shell.
    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || self.active == Some(index) {
            return false;
        }
        if let Some(old) = self.active.and_then(|i| self.tabs.get_mut(i)) {
            old.drop_view(self.renderer.page_layout());
            old.left_at = Some(Instant::now());
        }
        self.active = Some(index);
        self.load(index);
        let tab = &mut self.tabs[index];
        tab.left_at = None;
        self.renderer.clear_details();
        if let Some(doc) = tab.doc.as_ref() {
            for (&block, &open) in &tab.details {
                self.renderer.set_details(doc, block, open);
            }
        }
        self.find = Find::default();
        self.press = None;
        self.dirty = true;
        true
    }

    /// Closes tab `index`. Closing the showing tab shows the one that slides into its place (or the
    /// new last one); closing the last tab leaves the empty state.
    pub fn close(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        match self.active {
            Some(current) if current == index => self.active = None,
            Some(current) if current > index => self.active = Some(current - 1),
            _ => {}
        }
        self.relabel();
        if self.tabs.is_empty() {
            self.all_closed();
        } else if self.active.is_none() {
            self.activate(index.min(self.tabs.len() - 1));
        }
        self.dirty = true;
        self.persist();
        true
    }

    /// Closes the showing tab.
    pub fn close_active(&mut self) -> bool {
        self.active.is_some_and(|i| self.close(i))
    }

    /// Closes every tab but `keep`.
    pub fn close_others(&mut self, keep: usize) -> bool {
        if keep >= self.tabs.len() || self.tabs.len() == 1 {
            return false;
        }
        let was_active = self.active == Some(keep);
        let tab = self.tabs.swap_remove(keep);
        self.tabs.clear();
        self.tabs.push(tab);
        self.relabel();
        if was_active {
            self.active = Some(0);
        } else {
            self.active = None;
            self.activate(0);
        }
        self.dirty = true;
        self.persist();
        true
    }

    pub fn close_all(&mut self) -> bool {
        if self.tabs.is_empty() {
            return false;
        }
        self.tabs.clear();
        self.active = None;
        self.relabel();
        self.all_closed();
        self.dirty = true;
        self.persist();
        true
    }

    fn all_closed(&mut self) {
        self.find = Find::default();
        self.press = None;
        self.renderer.clear_details();
        self.message(strings::STATUS_ALL_CLOSED);
    }

    /// The next tab (`delta` 1) or the previous one (-1), wrapping. Nothing with one tab.
    pub fn cycle(&mut self, delta: isize) -> bool {
        let count = self.tabs.len();
        let Some(current) = self.active else {
            return false;
        };
        if count < 2 {
            return false;
        }
        let next = (current as isize + delta).rem_euclid(count as isize) as usize;
        self.activate(next)
    }

    pub fn next_tab(&mut self) -> bool {
        self.cycle(1)
    }

    pub fn previous_tab(&mut self) -> bool {
        self.cycle(-1)
    }

    /// Moves the tab at `from` to index `to`; the active tab stays active wherever it ends up.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if let Some(current) = self.active {
            self.active = Some(if current == from {
                to
            } else if from < current && current <= to {
                current - 1
            } else if to <= current && current < from {
                current + 1
            } else {
                current
            });
        }
        self.relabel();
        self.dirty = true;
        self.persist();
        true
    }

    fn relabel(&mut self) {
        let paths: Vec<PathBuf> = self.tabs.iter().map(|t| t.path.clone()).collect();
        self.labels = tab_labels(&paths);
    }

    /// Reads a lazily restored tab. A file that is gone gets a page saying so, as the original did.
    fn load(&mut self, index: usize) {
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        if tab.doc.is_some() {
            return;
        }
        match read_stable(&tab.path) {
            Ok(loaded) => {
                tab.missing = false;
                tab.set_document(parse_bytes(&loaded.bytes), loaded.stamp);
            }
            Err(e) => {
                let now = stamp(&tab.path);
                let missing = now == FileStamp::Missing;
                let why = if missing {
                    strings::FILE_NOT_FOUND.to_string()
                } else {
                    e.to_string()
                };
                let text = format!("# {}\n\n{why}\n", file_name(&tab.path));
                tab.missing = missing;
                tab.set_document(parse_bytes(text.as_bytes()), now);
            }
        }
    }

    /// Unloads the documents of tabs left more than `idle` ago (PLAN: model dropped after 60 s), so
    /// they read the file again when next shown. Tabs whose file is missing keep their text.
    /// Returns how many were unloaded.
    pub fn drop_idle(&mut self, now: Instant, idle: Duration) -> usize {
        let mut count = 0;
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            let stale = tab
                .left_at
                .is_some_and(|t| now.saturating_duration_since(t) >= idle);
            if Some(i) != self.active && stale && tab.doc.is_some() && !tab.missing {
                tab.doc = None;
                tab.view = None;
                tab.details.clear();
                tab.selection = None;
                count += 1;
            }
        }
        count
    }

    // ---- reloading ---------------------------------------------------------------------------

    /// F5: reads the showing file again, keeping the reading position.
    pub fn reload(&mut self) -> bool {
        match self.active {
            Some(index) => self.reload_tab(index, true),
            None => false,
        }
    }

    /// After a file-watcher wake (call `FileWatcher::woken` first): reloads every loaded tab whose
    /// file changed. A deleted file keeps its text and flags the tab; it reloads when it comes back.
    /// Tabs not loaded yet read the file when shown. True if anything changed.
    pub fn check_changed(&mut self) -> bool {
        let mut changed = false;
        for index in 0..self.tabs.len() {
            let tab = &mut self.tabs[index];
            if tab.doc.is_none() {
                continue;
            }
            let now = stamp(&tab.path);
            if now == tab.stamp {
                continue;
            }
            changed = true;
            if now == FileStamp::Missing {
                tab.stamp = now;
                tab.missing = true;
                if self.active == Some(index) {
                    self.message(strings::FILE_DELETED);
                }
                self.dirty = true;
            } else {
                self.reload_tab(index, false);
            }
        }
        changed
    }

    /// Re-reads tab `index`. The scroll position goes through `Document::remap_scroll`, so an edit
    /// above the reading position does not move the text the reader was looking at. A file that
    /// cannot be read keeps its content.
    fn reload_tab(&mut self, index: usize, manual: bool) -> bool {
        let Some(tab) = self.tabs.get_mut(index) else {
            return false;
        };
        if tab.doc.is_none() {
            self.load(index);
            self.dirty = true;
            return true;
        }
        let loaded = match read_stable(&tab.path) {
            Ok(loaded) => loaded,
            Err(e) => {
                let text = if stamp(&tab.path) == FileStamp::Missing {
                    tab.stamp = FileStamp::Missing;
                    tab.missing = true;
                    strings::FILE_DELETED.to_string()
                } else {
                    format!("{}: {e}", tab.path.display())
                };
                if manual || self.active == Some(index) {
                    self.message(&text);
                }
                self.dirty = true;
                return false;
            }
        };
        let active = self.active == Some(index);
        tab.drop_view(self.renderer.page_layout());
        let document = parse_bytes(&loaded.bytes);
        if let (Some(old), Some(pos)) = (tab.doc.as_ref(), tab.keep) {
            tab.keep = Some(document.remap_scroll(old, pos));
        }
        tab.set_document(document, loaded.stamp);
        tab.missing = false;
        tab.selection = None;
        tab.details.clear();
        let blocks = tab.stats.blocks;
        let name = file_name(&tab.path);
        if active {
            self.renderer.clear_details();
            self.press = None;
            // The Qt shell cleared find on reload; running it again keeps the find box honest.
            self.refind();
        }
        if manual {
            self.message(&format!("Refreshed {name}: {blocks} blocks"));
        } else if active {
            self.message(strings::REFRESHED);
        }
        self.dirty = true;
        true
    }

    // ---- recents -----------------------------------------------------------------------------

    /// Most recent first.
    pub fn recents(&self) -> &[PathBuf] {
        self.recents.paths()
    }

    /// Recent with missing files marked; checks the disk, so call it when the list opens.
    pub fn recent_entries(&self) -> Vec<RecentEntry> {
        self.recents.entries()
    }

    /// Opens a Recent entry; a missing one is removed from Recent instead, with a status message.
    pub fn open_recent(&mut self, path: &Path) -> Result<usize, String> {
        if !path.exists() {
            self.recents.remove(path);
            self.persist();
            self.message(strings::RECENT_REMOVED_MISSING);
            return Err(strings::RECENT_REMOVED_MISSING.to_string());
        }
        self.open(path)
    }

    pub fn remove_recent(&mut self, path: &Path) {
        self.recents.remove(path);
        self.persist();
    }

    pub fn clear_recents(&mut self) {
        self.recents.clear();
        self.persist();
        self.message(strings::RECENT_CLEARED);
    }

    /// Drops every Recent entry whose file is gone; returns how many.
    #[allow(dead_code)] // Workspace API the shell does not call yet.
    pub fn remove_missing_recents(&mut self) -> usize {
        let missing: Vec<PathBuf> = self
            .recents
            .entries()
            .into_iter()
            .filter(|e| e.missing)
            .map(|e| e.path)
            .collect();
        for path in &missing {
            self.recents.remove(path);
        }
        if !missing.is_empty() {
            self.persist();
        }
        missing.len()
    }

    // ---- theme and zoom ----------------------------------------------------------------------

    pub fn theme_choice(&self) -> ThemeChoice {
        self.choice
    }

    /// Sets System/Light/Dark and saves. The page is laid out again at the next paint.
    pub fn set_theme_choice(&mut self, choice: ThemeChoice) -> bool {
        if self.choice == choice {
            return false;
        }
        self.choice = choice;
        self.dirty = true;
        self.persist();
        true
    }

    /// Tells the workspace whether the desktop is dark, for the System choice.
    pub fn set_system_dark(&mut self, dark: bool) -> bool {
        if self.system_dark == dark {
            return false;
        }
        self.system_dark = dark;
        self.dirty |= self.choice == ThemeChoice::System;
        true
    }

    /// The theme the current choice gives on a desktop that is (or is not) dark.
    pub fn resolve_theme(&self, system_is_dark: bool) -> Theme {
        match self.choice {
            ThemeChoice::Light => LIGHT,
            ThemeChoice::Dark => DARK,
            ThemeChoice::System if system_is_dark => DARK,
            ThemeChoice::System => LIGHT,
        }
    }

    /// The theme in use, from the choice and the last [`set_system_dark`](Self::set_system_dark).
    pub fn theme(&self) -> Theme {
        self.resolve_theme(self.system_dark)
    }

    /// Zoom in percent, 60..=250.
    pub fn zoom(&self) -> u16 {
        self.zoom
    }

    /// Sets the zoom (clamped to 60-250%), keeping the reading position, and saves.
    pub fn set_zoom(&mut self, percent: u16) -> bool {
        let zoom = session::clamp_zoom(percent);
        if zoom == self.zoom {
            return false;
        }
        self.zoom = zoom;
        self.message(&format!("Zoom {zoom}%"));
        self.dirty = true;
        self.persist();
        true
    }

    pub fn zoom_in(&mut self) -> bool {
        self.set_zoom(session::zoom_in(self.zoom))
    }

    pub fn zoom_out(&mut self) -> bool {
        self.set_zoom(session::zoom_out(self.zoom))
    }

    pub fn zoom_reset(&mut self) -> bool {
        self.set_zoom(session::ZOOM_DEFAULT)
    }

    // ---- layout and painting -----------------------------------------------------------------

    /// Records the page viewport in device px and the display scale. `paint` does this too; call
    /// it directly to scroll or hit-test before the first paint.
    pub fn set_viewport(&mut self, width_px: u32, height_px: u32, scale: f32) -> bool {
        if width_px == 0 || height_px == 0 || !(scale.is_finite() && scale > 0.0) {
            return false;
        }
        let viewport = Viewport {
            width: width_px,
            height: height_px,
            scale,
        };
        if self.viewport == Some(viewport) {
            return false;
        }
        self.viewport = Some(viewport);
        self.dirty = true;
        true
    }

    /// Sets the viewport and lays the active page out now if it needs it, so what depends on the
    /// layout (the current heading, a pending jump) is settled before anything is drawn. False
    /// with no tab or an empty viewport. `paint` then reuses the layout.
    pub fn prepare(&mut self, width_px: u32, height_px: u32, scale: f32) -> bool {
        self.set_viewport(width_px, height_px, scale);
        self.ensure_layout()
    }

    /// The viewport height in device px, 0 before one is set.
    pub fn viewport_height(&self) -> f32 {
        self.viewport.map_or(0.0, |v| v.height as f32)
    }

    /// Lays out (if needed) and draws the active page for a viewport of `width_px` by `height_px`
    /// device px at display `scale`. `None` with no tab (draw the empty state) or an empty viewport.
    /// Layout is redone only when the width, scale, theme, zoom, document or `<details>` changed.
    pub fn paint(&mut self, width_px: u32, height_px: u32, scale: f32) -> Option<PageView<'_>> {
        self.find_again_if_stale();
        self.set_viewport(width_px, height_px, scale);
        if !self.ensure_layout() {
            return None;
        }
        let height = self.viewport?.height as f32;
        let tab = &mut self.tabs[self.active?];
        let doc = tab.doc.as_ref()?;
        let view = tab.view.as_ref()?;
        let background = view.options.theme.bg;
        // The frame measures what it shows first, which may correct the layout above the top; it
        // keeps the anchored content still and reports the top it drew.
        let top = self
            .renderer
            .view_frame(doc, &view.options, tab.scroll, height)
            .top;
        tab.scroll = top;
        self.adopt();
        self.pump_remote();
        let frame = self.renderer.frame()?;
        let total = self.renderer.page_layout()?.total_height();
        let x = ((width_px as i32 - frame.bitmap.width as i32) / 2).max(0);
        Some(PageView {
            frame,
            x,
            scroll: top,
            total_height: total,
            background,
        })
    }

    /// Draws page text for the display's subpixel order, as the chrome does.
    pub fn set_subpixel(&mut self, subpixel: mdview_render::Subpixel) {
        self.renderer.set_subpixel(subpixel);
        self.dirty = true;
    }

    // ---- remote images --------------------------------------------------------------------------

    /// Sets what runs (on a download thread) when a remote image finishes; it must only post a
    /// message that makes the window call [`remote_completed`](Self::remote_completed).
    pub fn set_remote_wake(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.remote_wake = Some(Arc::new(wake));
    }

    /// Whether remote images load in every document without asking.
    pub fn remote_always(&self) -> bool {
        self.session.remote_images
    }

    /// How many remote images the active document is holding back, when the bar offering to
    /// load them should show: none allowed yet, and the reader has not closed the bar.
    pub fn remote_blocked(&self) -> usize {
        match self.active_tab() {
            Some(tab) if !tab.remote.allowed && !tab.remote.dismissed => tab.remote.count,
            _ => 0,
        }
    }

    /// Loads the active document's remote images; with `always`, every document's from now on.
    pub fn allow_remote(&mut self, always: bool) {
        if always && !self.session.remote_images {
            self.session.remote_images = true;
            self.persist();
        }
        let urls: Vec<String> = if always {
            self.tabs.iter_mut().for_each(|t| t.remote.allowed = true);
            std::mem::take(&mut self.held)
        } else {
            let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) else {
                return;
            };
            tab.remote.allowed = true;
            let doc = tab.doc.as_ref();
            let (mine, rest) = std::mem::take(&mut self.held)
                .into_iter()
                .partition(|url| doc.is_some_and(|doc| names_image(doc, url)));
            self.held = rest;
            mine
        };
        for url in urls {
            self.fetch_remote(&url);
        }
        self.dirty = true;
    }

    /// Turns "always load remote images" on or off. Images already shown stay.
    pub fn set_remote_always(&mut self, always: bool) {
        if always {
            self.allow_remote(true);
        } else if self.session.remote_images {
            self.session.remote_images = false;
            self.persist();
        }
    }

    /// Closes the bar for the active document without loading anything.
    pub fn dismiss_remote(&mut self) {
        if let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) {
            tab.remote.dismissed = true;
            self.dirty = true;
        }
    }

    /// After the wake: hands every finished download to the page. True when something finished.
    pub fn remote_completed(&mut self) -> bool {
        let done = match &self.fetcher {
            Some(fetcher) => fetcher.take_completed(),
            None => return false,
        };
        for url in &done {
            self.fetch_remote(url);
        }
        !done.is_empty()
    }

    /// Decides about the remote images the last frame met: fetch them when this document may,
    /// otherwise hold them back and count them for the bar.
    fn pump_remote(&mut self) {
        let scale = self.scale();
        if scale != self.remote_scale {
            // SVGs were rasterized for the old scale; the bytes are still in the fetcher.
            self.remote_scale = scale;
            for url in std::mem::take(&mut self.remote_svgs) {
                self.fetch_remote(&url);
            }
        }
        let asked = self.renderer.take_remote_requests();
        if asked.is_empty() {
            return;
        }
        let always = self.session.remote_images;
        let allowed = always || self.active_tab().is_some_and(|t| t.remote.allowed);
        if allowed {
            for url in asked {
                self.fetch_remote(&url);
            }
            return;
        }
        for url in asked {
            self.renderer.set_remote(&url, RemoteState::Blocked);
            if !self.held.contains(&url) {
                self.held.push(url);
            }
        }
        self.dirty = true;
    }

    /// Asks the fetcher for one URL and tells the page where it stands.
    fn fetch_remote(&mut self, url: &str) {
        if self.fetcher.is_none() {
            let cache = self
                .session_path
                .as_deref()
                .and_then(Path::parent)
                .map(|dir| dir.join("cache").join("images"));
            let wake = self.remote_wake.clone();
            self.fetcher = Some(Fetcher::new(cache, move || {
                if let Some(wake) = &wake {
                    wake();
                }
            }));
        }
        let Some(fetcher) = &self.fetcher else {
            return;
        };
        let scale = self.remote_scale.max(0.25);
        let changed = match fetcher.request(url) {
            Fetched::Ready(bytes) => {
                let svg = bytes.starts_with(b"<") || bytes.starts_with(b"\xEF\xBB\xBF<");
                if svg && !self.remote_svgs.iter().any(|u| u == url) {
                    self.remote_svgs.push(url.to_string());
                }
                self.renderer.remote_arrived(url, Some(&bytes), scale)
            }
            Fetched::Pending => self.renderer.set_remote(url, RemoteState::Pending),
            Fetched::Failed => self.renderer.remote_arrived(url, None, 1.0),
        };
        self.dirty |= changed;
    }

    /// The device scale pages are drawn at: the display's times the zoom.
    fn scale(&self) -> f32 {
        self.active_tab()
            .and_then(|t| t.view.as_ref())
            .map_or(1.0, |v| v.options.scale)
    }

    /// Takes the renderer's corrections to the active layout since the last call: the scroll
    /// offset follows the anchored content and the heading positions are taken again. True when
    /// the layout changed.
    fn adopt(&mut self) -> bool {
        let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) else {
            return false;
        };
        let (Some(doc), Some(view)) = (tab.doc.as_ref(), tab.view.as_mut()) else {
            return false;
        };
        let Some(layout) = self.renderer.page_layout() else {
            return false;
        };
        if layout.generation() == view.generation {
            return false;
        }
        view.generation = layout.generation();
        view.heading_ys = heading_ys(doc, layout);
        if let Some(top) = self.renderer.top() {
            tab.scroll = top.round();
        }
        self.dirty = true;
        true
    }

    /// Measures more of the active page's estimated blocks, for about `budget`; true while some
    /// are left. The window calls this between events until it returns false, then stops (a
    /// refined page needs no idle work). Corrections above the viewport move the scroll offset
    /// with the content and mark the page dirty.
    pub fn refine(&mut self, budget: Duration) -> bool {
        let Some(tab) = self.active.and_then(|i| self.tabs.get(i)) else {
            return false;
        };
        let (Some(doc), Some(view)) = (tab.doc.as_ref(), tab.view.as_ref()) else {
            return false;
        };
        let more = self.renderer.refine(doc, &view.options, budget);
        self.adopt();
        self.find_again_if_stale();
        more
    }

    /// Searches again when a formula or diagram has turned out not to be drawn since the last
    /// search (or the layout was dropped): its source is on the page and must be found.
    fn find_again_if_stale(&mut self) {
        if !self.find.query.is_empty() && self.renderer.undrawn().1 != self.find.undrawn {
            self.refind();
            self.dirty = true;
        }
    }

    /// Whether the active page still has estimated heights for [`refine`](Self::refine).
    pub fn refining(&self) -> bool {
        self.layout().is_some() && self.renderer.estimating()
    }

    /// The active tab's layout, while it has one.
    fn layout(&self) -> Option<&PageLayout> {
        self.active_tab()?.view.as_ref()?;
        self.renderer.page_layout()
    }

    /// Makes sure the active tab has a layout for the current viewport, theme and zoom, then
    /// applies a pending reading position or jump. False with no tab or no viewport yet.
    fn ensure_layout(&mut self) -> bool {
        let (Some(viewport), Some(index)) = (self.viewport, self.active) else {
            return false;
        };
        self.load(index);
        let theme = self.theme();
        let zoom = f32::from(self.zoom) / 100.0;
        let width = viewport.width as f32 / viewport.scale / zoom;
        let scale = viewport.scale * zoom;
        let tab = &mut self.tabs[index];
        let fresh = tab.view.as_ref().is_some_and(|v| {
            v.options.width == width
                && (v.options.scale - scale).abs() < 0.001
                && v.options.theme.name == theme.name
        });
        if !fresh {
            tab.drop_view(self.renderer.page_layout());
            let Some(doc) = tab.doc.as_ref() else {
                return false;
            };
            let options = Options {
                width,
                scale,
                theme,
                max_height: None,
                base_dir: tab.path.parent().map(Path::to_path_buf),
            };
            let layout = self.renderer.layout(doc, &options);
            tab.view = Some(View {
                heading_ys: heading_ys(doc, layout),
                generation: layout.generation(),
                options,
            });
            self.dirty = true;
        }
        let height = viewport.height as f32;
        let Some(doc) = tab.doc.as_ref() else {
            return false;
        };
        let Some(view) = tab.view.as_ref() else {
            return false;
        };
        let margin = JUMP_MARGIN * view.options.scale;
        // A new layout moves the renderer's anchor to the tab's offset, unless a reading position
        // or a jump puts it on a block; otherwise the anchor leads, since the renderer may have
        // corrected the layout around it. A jump measures the viewport it lands on first, so it
        // is exact while the rest of the page is still estimated.
        let mut moved = !fresh;
        let renderer = &mut self.renderer;
        let mut jump = |target: Jump, margin: f32, scroll: &mut f32, moved: &mut bool| {
            if let Some(top) = renderer.jump_to(doc, &view.options, target, margin, height) {
                *scroll = top.round();
                *moved = false;
            }
        };
        if !fresh && let Some(pos) = tab.keep.take() {
            let below = pos.offset * view.options.scale;
            jump(Jump::Block(pos.block), -below, &mut tab.scroll, &mut moved);
        }
        match tab.reveal.take() {
            Some(Reveal::Anchor(fragment)) => {
                if let Some(block) = doc.resolve_fragment(&fragment) {
                    jump(Jump::Block(block), margin, &mut tab.scroll, &mut moved);
                }
            }
            Some(Reveal::Range(from, to)) => {
                let rects = self.renderer.selection_rects(doc, &view.options, from, to);
                if !moved && let Some(top) = self.renderer.top() {
                    tab.scroll = top.round();
                }
                if into_view(&rects, tab.scroll, height, margin).is_some() {
                    let renderer = &mut self.renderer;
                    if let Some(top) = renderer.jump_to(
                        doc,
                        &view.options,
                        Jump::Text(from.min(to)),
                        margin,
                        height,
                    ) {
                        tab.scroll = top.round();
                        moved = false;
                    }
                }
            }
            None => {}
        }
        let Some(total) = self.renderer.page_layout().map(PageLayout::total_height) else {
            return false;
        };
        let clamped = tab.scroll.clamp(0.0, (total - height).max(0.0)).round();
        if moved || clamped != tab.scroll {
            tab.scroll = clamped;
            self.renderer.set_top(clamped);
        }
        self.adopt();
        true
    }

    // ---- scrolling ---------------------------------------------------------------------------

    /// The active tab's scroll offset, device px.
    pub fn scroll(&self) -> f32 {
        self.active_tab().map_or(0.0, |t| t.scroll)
    }

    /// The largest scroll offset; 0 before the page is laid out.
    pub fn max_scroll(&self) -> f32 {
        let (Some(layout), Some(viewport)) = (self.layout(), self.viewport) else {
            return 0.0;
        };
        (layout.total_height() - viewport.height as f32).max(0.0)
    }

    /// The page height, device px; 0 before the page is laid out.
    pub fn total_height(&self) -> f32 {
        self.layout().map_or(0.0, PageLayout::total_height)
    }

    /// Scrolls to `y` device px, clamped. True if the offset changed.
    pub fn scroll_to(&mut self, y: f32) -> bool {
        let laid_out = self.ensure_layout();
        let max = self.max_scroll();
        let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) else {
            return false;
        };
        tab.keep = None;
        let y = if y.is_nan() { 0.0 } else { y };
        let next = if laid_out {
            y.clamp(0.0, max).round()
        } else {
            y.max(0.0).round()
        };
        if next == tab.scroll {
            return false;
        }
        tab.scroll = next;
        if laid_out {
            self.renderer.set_top(next);
        }
        self.dirty = true;
        true
    }

    /// Scrolls by `dy` device px (positive goes down).
    pub fn scroll_by(&mut self, dy: f32) -> bool {
        self.scroll_to(self.scroll() + dy)
    }

    /// Scrolls by `lines` arrow-key steps (negative goes up).
    pub fn scroll_lines(&mut self, lines: f32) -> bool {
        let scale = self.viewport.map_or(1.0, |v| v.scale);
        self.scroll_by(lines * LINE_STEP * scale)
    }

    /// Page Down / Space.
    pub fn page_down(&mut self) -> bool {
        self.scroll_by(self.viewport_height() * PAGE_SHARE)
    }

    /// Page Up.
    pub fn page_up(&mut self) -> bool {
        self.scroll_by(-self.viewport_height() * PAGE_SHARE)
    }

    pub fn scroll_home(&mut self) -> bool {
        self.scroll_to(0.0)
    }

    pub fn scroll_end(&mut self) -> bool {
        self.scroll_to(f32::MAX)
    }

    /// Scrolls so `block` (nested blocks too) sits just below the top edge, as an outline click or
    /// an anchor does. False if the block is not on the page or there is no layout yet.
    pub fn scroll_to_block(&mut self, block: u32) -> bool {
        self.jump(Jump::Block(block))
    }

    /// Brings a block or footnote entry to just below the top edge through the renderer, which
    /// measures the viewport it lands on first. False if the target is not on the page or there
    /// is no layout yet.
    fn jump(&mut self, target: Jump) -> bool {
        if !self.ensure_layout() {
            return false;
        }
        let Some(viewport) = self.viewport else {
            return false;
        };
        let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) else {
            return false;
        };
        let (Some(doc), Some(view)) = (tab.doc.as_ref(), tab.view.as_ref()) else {
            return false;
        };
        let margin = JUMP_MARGIN * view.options.scale;
        let height = viewport.height as f32;
        let Some(top) = self
            .renderer
            .jump_to(doc, &view.options, target, margin, height)
        else {
            return false;
        };
        tab.keep = None;
        tab.scroll = top.round();
        self.dirty = true;
        self.adopt();
        true
    }

    // ---- hit testing and the pointer ---------------------------------------------------------

    /// What is under viewport point `x, y` (device px).
    pub fn hit(&mut self, x: f32, y: f32) -> Option<Hit> {
        if !self.ensure_layout() {
            return None;
        }
        let tab = self.tabs.get(self.active?)?;
        let (doc, view) = (tab.doc.as_ref()?, tab.view.as_ref()?);
        let hit = self
            .renderer
            .hit_test(doc, &view.options, x, y + tab.scroll);
        // A pending correction may have moved the content: the next frame shows where.
        self.adopt();
        hit
    }

    /// What is under the pointer while no button is down.
    pub fn hover(&mut self, x: f32, y: f32) -> Hover {
        let Some(hit) = self.hit(x, y) else {
            return Hover::default();
        };
        Hover {
            link: hit.link.and_then(|i| self.link_destination(i)),
            pointing: hit.link.is_some() || hit.footnote.is_some() || hit.details.is_some(),
            text: hit.on_text,
        }
    }

    /// What a right click at `x, y` offers.
    pub fn context_at(&mut self, x: f32, y: f32) -> PageContext {
        let hit = self.hit(x, y);
        let link = hit
            .and_then(|hit| hit.link)
            .and_then(|i| self.link_destination(i));
        // A diagram's source is its block text, which ends in the fence's newline.
        let source = hit.and_then(|hit| hit.source).and_then(|s| {
            let text = self
                .active_document()?
                .text
                .get(s.start as usize..s.end as usize)?;
            Some((s.kind, text.trim().to_string()))
        });
        PageContext {
            link,
            has_selection: self.has_selection(),
            source,
        }
    }

    /// The destination of link `index` in the active document, as written.
    pub fn link_destination(&self, index: u32) -> Option<String> {
        let doc = self.active_document()?;
        doc.links
            .get(index as usize)
            .map(|l| doc.str(l.dest).to_string())
    }

    /// A button went down at `x, y`. `clicks` is 1 for a click, 2 for a double click (selects a
    /// word), 3 or more for a triple click (selects the paragraph or other innermost block).
    pub fn pointer_press(&mut self, x: f32, y: f32, clicks: u32) -> bool {
        self.press = None;
        let Some(hit) = self.hit(x, y) else {
            return false;
        };
        let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) else {
            return false;
        };
        let Some(doc) = tab.doc.as_ref() else {
            return false;
        };
        let (anchor, multi) = match clicks {
            0 | 1 => ((hit.offset, hit.offset), false),
            2 => (word_range(doc, hit.offset), true),
            _ => (block_range(doc, hit.offset), true),
        };
        tab.selection = Some(anchor);
        self.press = Some(Press {
            anchor,
            link: hit.link.filter(|_| !multi),
            footnote: hit.footnote.filter(|_| !multi),
            details: hit.details.filter(|_| !multi),
            moved: multi,
        });
        self.dirty = true;
        true
    }

    /// The pointer moved with the button down: extends the selection. The caller scrolls first when
    /// the pointer is past the viewport's edge. False when no press is in progress.
    pub fn pointer_drag(&mut self, x: f32, y: f32) -> bool {
        let Some(press) = self.press else {
            return false;
        };
        let Some(hit) = self.hit(x, y) else {
            return false;
        };
        let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) else {
            return false;
        };
        let (a, b) = press.anchor;
        let selection = if hit.offset < a {
            (b, hit.offset)
        } else if hit.offset > b {
            (a, hit.offset)
        } else {
            (a, b)
        };
        if let Some(press) = self.press.as_mut() {
            press.moved |= hit.offset != a;
        }
        if tab.selection != Some(selection) {
            tab.selection = Some(selection);
            self.dirty = true;
        }
        true
    }

    /// The button came up. A release where the press landed follows the link, footnote or
    /// `<details>` under it, or clears the selection; after a drag the selection stays.
    pub fn pointer_release(&mut self) -> Click {
        let Some(press) = self.press.take() else {
            return Click::Nothing;
        };
        if press.moved {
            return Click::Selected;
        }
        if let Some(link) = press.link {
            self.clear_selection();
            return Click::Link(self.follow_link(link));
        }
        if let Some(note) = press.footnote {
            self.clear_selection();
            self.follow_footnote(note);
            return Click::Footnote;
        }
        if let Some(block) = press.details {
            self.clear_selection();
            self.toggle_details(block);
            return Click::Details;
        }
        self.clear_selection();
        Click::Deselected
    }

    /// Drops a press without acting on it (the pointer left the window, focus was lost).
    #[allow(dead_code)] // Workspace API the shell does not call yet.
    pub fn pointer_cancel(&mut self) {
        self.press = None;
    }

    /// Whether a press is in progress, so the caller can auto-scroll a drag.
    pub fn is_pressing(&self) -> bool {
        self.press.is_some()
    }

    // ---- links, footnotes, details -----------------------------------------------------------

    /// Follows link `index` of the active document.
    pub fn follow_link(&mut self, index: u32) -> LinkOutcome {
        match self.link_destination(index) {
            Some(destination) => self.open_link(&destination),
            None => LinkOutcome::Ignored,
        }
    }

    /// Follows a destination: a fragment here, another local Markdown file (opened in a tab, then
    /// its fragment), or a web/mail address for the caller to open. Core's `classify_link` decides,
    /// so unknown schemes and UNC hosts never do anything.
    pub fn open_link(&mut self, destination: &str) -> LinkOutcome {
        let from = self.place();
        let outcome = self.open_link_from_here(destination);
        if matches!(outcome, LinkOutcome::Scrolled | LinkOutcome::OpenedTab(_)) {
            self.remember(from);
        }
        outcome
    }

    fn open_link_from_here(&mut self, destination: &str) -> LinkOutcome {
        let doc_dir = self
            .active_tab()
            .and_then(|t| t.path.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        match classify_link(destination, &doc_dir) {
            LinkAction::Fragment(fragment) => self.jump_fragment(&fragment),
            LinkAction::External(url) => LinkOutcome::OpenUrl(url),
            LinkAction::OpenMarkdown { path, fragment } => {
                if !path.is_file() {
                    return LinkOutcome::Refused(
                        self.refuse(format!("not a file: {}", path.display())),
                    );
                }
                match self.open(&path) {
                    Ok(index) => {
                        if let Some(fragment) = fragment {
                            self.tabs[index].reveal = Some(Reveal::Anchor(fragment));
                            self.ensure_layout();
                        }
                        self.dirty = true;
                        LinkOutcome::OpenedTab(index)
                    }
                    Err(e) => LinkOutcome::Refused(e),
                }
            }
            LinkAction::OtherLocalFile(_) => {
                LinkOutcome::Refused(self.refuse(strings::ONLY_MARKDOWN_LINKS.to_string()))
            }
            LinkAction::Ignored => LinkOutcome::Ignored,
        }
    }

    fn jump_fragment(&mut self, fragment: &str) -> LinkOutcome {
        let block = self
            .active_document()
            .and_then(|doc| doc.resolve_fragment(fragment));
        let Some(block) = block else {
            return LinkOutcome::Refused(self.refuse(format!("no anchor named {fragment}")));
        };
        if !self.scroll_to_block(block) {
            // Not laid out yet: jump once it is.
            if let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) {
                tab.reveal = Some(Reveal::Anchor(fragment.to_string()));
            }
        }
        LinkOutcome::Scrolled
    }

    /// Scrolls to footnote `index`'s entry at the end of the page.
    pub fn follow_footnote(&mut self, index: u32) -> bool {
        let number = self
            .active_document()
            .and_then(|doc| doc.footnotes.get(index as usize))
            .map(|note| note.number);
        let from = self.place();
        let moved = number.is_some_and(|number| self.jump(Jump::Footnote(number)));
        self.remember(from);
        moved
    }

    /// Opens or closes a `<details>`; the renderer measures its block again at the next paint
    /// (the rest of the layout stays), and find runs again because the visible text changed.
    pub fn toggle_details(&mut self, block: u32) -> bool {
        let Some(index) = self.active else {
            return false;
        };
        let tab = &mut self.tabs[index];
        let Some(doc) = tab.doc.as_ref() else {
            return false;
        };
        let is_details = doc
            .blocks
            .get(block as usize)
            .is_some_and(|b| matches!(b.kind, mdview_core::model::BlockKind::Details { .. }));
        if !is_details {
            return false;
        }
        let open = !self.renderer.details_open(doc, block);
        self.renderer.set_details(doc, block, open);
        tab.details.insert(block, open);
        self.refind();
        self.dirty = true;
        true
    }

    // ---- selection ---------------------------------------------------------------------------

    /// The selection as ordered offsets into `Document::text`, if it is not empty.
    pub fn selection(&self) -> Option<(u32, u32)> {
        let (a, b) = self.active_tab()?.selection?;
        (a != b).then(|| (a.min(b), a.max(b)))
    }

    pub fn has_selection(&self) -> bool {
        self.selection().is_some()
    }

    /// The selected text, for Copy.
    pub fn selected_text(&self) -> Option<String> {
        let (from, to) = self.selection()?;
        let doc = self.active_document()?;
        doc.text.get(from as usize..to as usize).map(str::to_string)
    }

    pub fn select_all(&mut self) -> bool {
        let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) else {
            return false;
        };
        let Some(len) = tab.doc.as_ref().map(|d| d.text.len() as u32) else {
            return false;
        };
        tab.selection = Some((0, len));
        self.dirty = true;
        true
    }

    pub fn clear_selection(&mut self) -> bool {
        let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) else {
            return false;
        };
        if tab.selection.take().is_some() {
            self.dirty = true;
            return true;
        }
        false
    }

    /// The selection's rectangles on screen, viewport coordinates (only the visible part).
    pub fn selection_rects(&mut self) -> Vec<Rect> {
        match self.selection() {
            Some((from, to)) => self.visible_rects(&[(from, to)]).0,
            None => Vec::new(),
        }
    }

    // ---- find --------------------------------------------------------------------------------

    /// Runs find for `query` (case-insensitive, visible text only) and brings the first match into
    /// view. An empty query clears the marks. The same query again does nothing.
    pub fn find_set(&mut self, query: &str) -> bool {
        if query == self.find.query {
            return false;
        }
        self.find = Find {
            query: query.to_string(),
            ..Find::default()
        };
        self.refind();
        self.reveal_current();
        self.dirty = true;
        true
    }

    /// Moves to the next match, wrapping, and brings it into view.
    pub fn find_next(&mut self) -> bool {
        self.find_step(1)
    }

    /// Moves to the previous match, wrapping, and brings it into view.
    pub fn find_previous(&mut self) -> bool {
        self.find_step(-1)
    }

    fn find_step(&mut self, delta: i64) -> bool {
        let len = self.find.hits.len();
        if len == 0 {
            return false;
        }
        self.find.current = (self.find.current as i64 + delta).rem_euclid(len as i64) as usize;
        self.reveal_current();
        self.dirty = true;
        true
    }

    /// Ends the find: the query and marks go.
    pub fn find_clear(&mut self) -> bool {
        if self.find.query.is_empty() && self.find.hits.is_empty() {
            return false;
        }
        self.find = Find::default();
        self.dirty = true;
        true
    }

    pub fn find_query(&self) -> &str {
        &self.find.query
    }

    pub fn find_counts(&self) -> FindCounts {
        let total = self.find.hits.len();
        FindCounts {
            current: if total == 0 { 0 } else { self.find.current + 1 },
            total,
            capped: self.find.capped,
        }
    }

    /// The counter text beside the find box (`strings::find_label`).
    pub fn find_label(&self) -> String {
        let c = self.find_counts();
        strings::find_label(self.find.query.is_empty(), c.current, c.total, c.capped)
    }

    /// The visible matches' rectangles in viewport coordinates, the current one apart.
    pub fn find_marks(&mut self) -> FindMarks {
        if self.find.hits.is_empty() {
            return FindMarks::default();
        }
        let hits = std::mem::take(&mut self.find.hits);
        let (others, current) = self.visible_rects_marked(&hits, Some(self.find.current));
        self.find.hits = hits;
        FindMarks { others, current }
    }

    /// Runs the current query again on the active document, keeping the current index in range.
    fn refind(&mut self) {
        let current = self.find.current;
        self.find.hits.clear();
        self.find.capped = false;
        if self.find.query.is_empty() {
            self.find.current = 0;
            return;
        }
        let Some(doc) = self.active.and_then(|i| self.tabs[i].doc.as_ref()) else {
            return;
        };
        let renderer = &self.renderer;
        let (shown, changes) = renderer.undrawn();
        let result = mdview_core::find::find_shown(
            doc,
            &self.find.query,
            |block| renderer.details_open(doc, block),
            shown,
        );
        self.find.undrawn = changes;
        self.find.hits = result.hits.iter().map(|h| (h.start, h.end)).collect();
        self.find.capped = result.capped;
        self.find.current = current.min(self.find.hits.len().saturating_sub(1));
    }

    /// Scrolls the current match into view if it is not already, now or once laid out.
    fn reveal_current(&mut self) {
        let Some(&(from, to)) = self.find.hits.get(self.find.current) else {
            return;
        };
        if let Some(tab) = self.active.and_then(|i| self.tabs.get_mut(i)) {
            tab.reveal = Some(Reveal::Range(from, to));
        }
        self.ensure_layout();
    }

    // ---- outline -----------------------------------------------------------------------------

    /// The active document's headings in order.
    pub fn outline(&self) -> Vec<OutlineEntry<'_>> {
        let Some(doc) = self.active_document() else {
            return Vec::new();
        };
        doc.headings
            .iter()
            .map(|h| OutlineEntry {
                level: h.level,
                title: doc.str(h.title),
                block: h.block,
            })
            .collect()
    }

    /// The heading the reader is in: the last one at or above the viewport's top edge (allowing
    /// for the margin a jump leaves). `None` above the first heading or before layout.
    pub fn current_heading(&self) -> Option<usize> {
        self.current_heading_at(self.scroll())
    }

    /// [`current_heading`](Self::current_heading) for a viewport top of `top` device px. Scrolled to
    /// the bottom, the last heading on screen counts, since headings near the end of a page can
    /// never reach the top.
    pub fn current_heading_at(&self, top: f32) -> Option<usize> {
        let view = self.active_tab()?.view.as_ref()?;
        let height = self.viewport_height();
        let max = (self.layout()?.total_height() - height).max(0.0);
        let line = if max > 0.0 && top >= max - 1.0 {
            top + height
        } else {
            top + JUMP_MARGIN * view.options.scale + 1.0
        };
        view.heading_ys
            .partition_point(|&y| y <= line)
            .checked_sub(1)
    }

    // ---- status ------------------------------------------------------------------------------

    /// The status text while it is fresh (`strings::TRANSIENT_MS` after it was set).
    pub fn status(&self, now: Instant) -> Option<&str> {
        let status = self.status.as_ref()?;
        (now.saturating_duration_since(status.at) < transient()).then_some(status.text.as_str())
    }

    /// The status line as set, fresh or not.
    #[allow(dead_code)] // Workspace API the shell does not call yet.
    pub fn status_line(&self) -> Option<&StatusLine> {
        self.status.as_ref()
    }

    /// When the current status stops showing, so the caller can schedule a redraw.
    pub fn status_expires(&self) -> Option<Instant> {
        self.status.as_ref().map(|s| s.at + transient())
    }

    pub fn set_status(&mut self, text: &str) {
        self.message(text);
    }

    pub fn clear_status(&mut self) {
        if self.status.take().is_some() {
            self.dirty = true;
        }
    }

    /// Counts for the active document.
    #[allow(dead_code)] // Workspace API the shell does not call yet.
    pub fn stats(&self) -> Option<DocStats> {
        self.active_tab()
            .filter(|t| t.doc.is_some())
            .map(|t| t.stats)
    }

    fn message(&mut self, text: &str) {
        self.status = (!text.is_empty()).then(|| StatusLine {
            text: text.to_string(),
            at: Instant::now(),
        });
        self.dirty = true;
    }

    /// Sets the status and hands the text back, for refusals.
    fn refuse(&mut self, text: String) -> String {
        self.message(&text);
        text
    }

    // ---- geometry helpers --------------------------------------------------------------------

    /// Rectangles of `ranges` in viewport coordinates, clipped to what is on screen.
    fn visible_rects(&mut self, ranges: &[(u32, u32)]) -> (Vec<Rect>, Vec<Rect>) {
        self.visible_rects_marked(ranges, None)
    }

    /// As `visible_rects`, with range `marked` (an index into `ranges`) returned apart. `ranges`
    /// must be sorted and not overlap.
    fn visible_rects_marked(
        &mut self,
        ranges: &[(u32, u32)],
        marked: Option<usize>,
    ) -> (Vec<Rect>, Vec<Rect>) {
        let mut out = (Vec::new(), Vec::new());
        if !self.ensure_layout() {
            return out;
        }
        let Some(viewport) = self.viewport else {
            return out;
        };
        let Some(tab) = self.active.and_then(|i| self.tabs.get(i)) else {
            return out;
        };
        let (Some(doc), Some(view)) = (tab.doc.as_ref(), tab.view.as_ref()) else {
            return out;
        };
        let height = viewport.height as f32;
        let Some(layout) = self.renderer.page_layout() else {
            return out;
        };
        let Some((lo, hi)) = visible_text(doc, layout, tab.scroll, height) else {
            return out;
        };
        let first = ranges.partition_point(|r| r.0.max(r.1) <= lo);
        let mut drawn = 0;
        for (i, &(a, b)) in ranges.iter().enumerate().skip(first) {
            let (from, to) = (a.min(b).max(lo), a.max(b).min(hi));
            if a.min(b) >= hi || drawn >= MAX_MARKS {
                break;
            }
            if from >= to {
                continue;
            }
            drawn += 1;
            let rects = self.renderer.selection_rects(doc, &view.options, from, to);
            let target = if Some(i) == marked {
                &mut out.1
            } else {
                &mut out.0
            };
            target.extend(
                rects
                    .into_iter()
                    .filter(|r| r.y + r.h > tab.scroll && r.y < tab.scroll + height)
                    .map(|r| Rect {
                        y: r.y - tab.scroll,
                        ..r
                    }),
            );
        }
        out
    }
}

/// `strings::TRANSIENT_MS` as a duration.
fn transient() -> Duration {
    Duration::from_millis(u64::from(strings::TRANSIENT_MS))
}

/// Where to scroll so `rects` (page coordinates) are in view, or `None` if they already are.
fn into_view(rects: &[Rect], scroll: f32, height: f32, margin: f32) -> Option<f32> {
    let top = rects.iter().map(|r| r.y).reduce(f32::min)?;
    let bottom = rects.iter().map(|r| r.y + r.h).reduce(f32::max)?;
    (top < scroll || bottom > scroll + height).then_some(top - margin)
}

/// The text range of the top-level blocks that reach into `top..top + height`.
fn visible_text(doc: &Document, layout: &PageLayout, top: f32, height: f32) -> Option<(u32, u32)> {
    let first = layout.block_at(top)?;
    let last = layout.block_at(top + height)?;
    let start = doc.blocks.get(first.block as usize)?.text.start;
    let end = doc.blocks.get(last.block as usize)?.text.end;
    Some((start.min(end), end.max(start)))
}

/// The page y of every heading, non-decreasing, from the layout's top-level blocks. A heading
/// nested in a list or quote takes its container's y.
fn heading_ys(doc: &Document, layout: &PageLayout) -> Vec<f32> {
    let spans = layout.blocks();
    let mut last = 0.0f32;
    doc.headings
        .iter()
        .map(|heading| {
            let mut block = heading.block;
            while let Some(parent) = doc
                .blocks
                .get(block as usize)
                .map(|b| b.parent)
                .filter(|&p| p != NONE)
            {
                block = parent;
            }
            if let Ok(i) = spans.binary_search_by_key(&block, |s| s.block) {
                last = last.max(spans[i].y);
            }
            last
        })
        .collect()
}

/// The word (or run of spaces, or single punctuation mark) at `offset`, within its block.
fn word_range(doc: &Document, offset: u32) -> (u32, u32) {
    let text = doc.text.as_str();
    let (lo, hi) = block_bounds(doc, offset);
    let mut pos = (offset as usize).clamp(lo, hi);
    while !text.is_char_boundary(pos) {
        pos -= 1;
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    };
    // At the end of a line, the character before the caret is the one meant.
    let c = match text[pos..hi].chars().next() {
        Some(c) => c,
        None => match text[lo..pos].chars().next_back() {
            Some(c) => {
                pos -= c.len_utf8();
                c
            }
            None => return (offset, offset),
        },
    };
    let kind = class(c);
    if kind == 2 {
        return (pos as u32, (pos + c.len_utf8()) as u32);
    }
    let start = text[lo..pos]
        .char_indices()
        .rev()
        .take_while(|&(_, ch)| class(ch) == kind)
        .last()
        .map_or(pos, |(i, _)| lo + i);
    let end = text[pos..hi]
        .char_indices()
        .find(|&(_, ch)| class(ch) != kind)
        .map_or(hi, |(i, _)| pos + i);
    (start as u32, end as u32)
}

/// The innermost block at `offset`, trailing whitespace left out.
fn block_range(doc: &Document, offset: u32) -> (u32, u32) {
    let (lo, hi) = block_bounds(doc, offset);
    let trimmed = doc.text.get(lo..hi).map_or(0, |s| s.trim_end().len());
    (lo as u32, (lo + trimmed) as u32)
}

/// The byte range of the innermost block containing `offset` (or ending at it), or the whole text.
fn block_bounds(doc: &Document, offset: u32) -> (usize, usize) {
    let len = doc.text.len();
    let block = doc
        .block_at(offset)
        .or_else(|| offset.checked_sub(1).and_then(|o| doc.block_at(o)));
    let (lo, hi) = block
        .and_then(|b| doc.blocks.get(b as usize))
        .map_or((0, len), |b| (b.text.start as usize, b.text.end as usize));
    let hi = hi.min(len);
    let lo = lo.min(hi);
    let lo = (0..=lo)
        .rev()
        .find(|&i| doc.text.is_char_boundary(i))
        .unwrap_or(0);
    let hi = (hi..=len)
        .find(|&i| doc.text.is_char_boundary(i))
        .unwrap_or(len);
    (lo, hi)
}

/// How many different `http`/`https` images the document names.
fn remote_images(doc: &Document) -> usize {
    let mut urls: Vec<&str> = doc
        .images
        .iter()
        .map(|image| doc.str(image.src).trim())
        .filter(|src| {
            ["http://", "https://"].iter().any(|scheme| {
                src.get(..scheme.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
            })
        })
        .collect();
    urls.sort_unstable();
    urls.dedup();
    urls.len()
}

/// Whether one of the document's images is `url`, as the page asks for it (trimmed; any of the
/// plain, dark and light sources).
fn names_image(doc: &Document, url: &str) -> bool {
    doc.images.iter().any(|image| {
        [image.src, image.dark_src, image.light_src]
            .into_iter()
            .any(|src| doc.str(src).trim() == url)
    })
}

/// Whether two paths name the same file (case-insensitive on Windows).
pub fn same_file(a: &Path, b: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// A fresh, empty, canonical folder under the temp dir.
    fn temp_dir(name: &str) -> PathBuf {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("mdview-ws-{name}-{}-{n}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        canonical_or_absolute(&dir)
    }

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    fn workspace() -> Workspace {
        let mut ws = Workspace::new(None);
        ws.set_viewport(800, 600, 1.0);
        ws
    }

    /// Remote images are held back and counted until the reader allows them, per document. The
    /// URLs carry userinfo, which the fetcher refuses without starting a thread or a connection.
    #[test]
    fn remote_images_wait_for_the_reader() {
        let dir = temp_dir("remote");
        let text = "![a](https://u@example.invalid/a.png) ![b](https://u@example.invalid/b.png)\n";
        let a = write(&dir, "a.md", text);
        let b = write(&dir, "b.md", text);
        let mut ws = workspace();
        ws.open(&a).unwrap();
        assert_eq!(ws.remote_blocked(), 2, "counted when the file is read");
        ws.paint(800, 600, 1.0);
        assert_eq!(ws.remote_blocked(), 2);
        assert!(ws.fetcher.is_none(), "a held-back image starts nothing");
        ws.paint(800, 600, 1.0);
        assert_eq!(ws.remote_blocked(), 2, "each URL is counted once");

        // Closing the bar keeps the images back for this document only.
        ws.dismiss_remote();
        assert_eq!(ws.remote_blocked(), 0);
        assert!(ws.fetcher.is_none());

        // A second document naming the same images gets its own bar, and allowing them there
        // asks the fetcher and clears it.
        ws.open(&b).unwrap();
        ws.paint(800, 600, 1.0);
        assert_eq!(ws.remote_blocked(), 2);
        ws.allow_remote(false);
        assert_eq!(ws.remote_blocked(), 0);
        assert!(ws.fetcher.is_some());
        assert!(!ws.remote_always());

        // "Always" is a saved setting and can be turned off again.
        ws.set_remote_always(true);
        assert!(ws.remote_always());
        ws.set_remote_always(false);
        assert!(!ws.remote_always());
    }

    fn paragraphs(prefix: &str, n: usize) -> String {
        (0..n)
            .map(|i| format!("{prefix} paragraph number {i} with some words.\n\n"))
            .collect()
    }

    /// Scrolls `needle` into view and returns a viewport point inside its first rectangle.
    fn point_of(ws: &mut Workspace, needle: &str) -> (f32, f32) {
        let (start, end) = {
            let doc = ws.active_document().unwrap();
            let start = doc.text.find(needle).expect("needle") as u32;
            (start, start + needle.len() as u32)
        };
        assert!(ws.scroll_to_block(ws.active_document().unwrap().block_at(start).unwrap()));
        let rects = ws.visible_rects(&[(start, end)]).0;
        let r = rects.first().copied().expect("rect on screen");
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    fn click(ws: &mut Workspace, needle: &str) -> Click {
        let (x, y) = point_of(ws, needle);
        assert!(ws.pointer_press(x, y, 1));
        ws.pointer_release()
    }

    #[test]
    fn open_dedupe_close_others_and_move() {
        let dir = temp_dir("tabs");
        let a = write(&dir, "a.md", "# A\n");
        let b = write(&dir, "b.md", "# B\n");
        let c = write(&dir, "c.md", "# C\n");
        let mut ws = workspace();
        assert_eq!(ws.open(&a), Ok(0));
        assert_eq!(ws.open(&b), Ok(1));
        assert_eq!(ws.open(&c), Ok(2));
        assert_eq!(ws.active(), Some(2));
        // Opening an open file shows its tab.
        assert_eq!(ws.open(&a), Ok(0));
        assert_eq!(ws.tab_count(), 3);
        assert_eq!(ws.active(), Some(0));
        assert_eq!(ws.labels(), ["a.md", "b.md", "c.md"]);
        assert!(ws.title().starts_with("a.md"));
        // a moves to the end and stays active.
        assert!(ws.move_tab(0, 2));
        assert_eq!(ws.labels(), ["b.md", "c.md", "a.md"]);
        assert_eq!(ws.active(), Some(2));
        assert!(ws.next_tab());
        assert_eq!(ws.active(), Some(0));
        assert!(ws.previous_tab());
        assert_eq!(ws.active(), Some(2));
        // Closing the active last tab shows the new last one.
        assert!(ws.close(2));
        assert_eq!(ws.active(), Some(1));
        assert_eq!(ws.labels(), ["b.md", "c.md"]);
        assert!(ws.open(&a).is_ok());
        assert!(ws.close_others(0));
        assert_eq!(ws.labels(), ["b.md"]);
        assert_eq!(ws.active(), Some(0));
        assert!(ws.active_document().is_some());
        assert!(ws.close_all());
        assert_eq!(ws.active(), None);
        assert_eq!(ws.status(Instant::now()), Some(strings::STATUS_ALL_CLOSED));
        // Nothing open: nothing panics.
        assert!(!ws.close(3));
        assert!(!ws.activate(0));
        assert!(!ws.move_tab(0, 1));
        assert!(!ws.close_others(0));
        assert!(ws.paint(800, 600, 1.0).is_none());
        assert!(!ws.scroll_by(100.0));
        assert!(ws.hit(10.0, 10.0).is_none());
        assert!(!ws.pointer_press(1.0, 1.0, 1));
        assert_eq!(ws.pointer_release(), Click::Nothing);
        assert!(ws.selected_text().is_none());
        ws.find_set("x");
        assert_eq!(ws.find_counts().total, 0);
        assert!(ws.outline().is_empty());
        // A file that cannot be read opens nothing.
        assert!(ws.open(&dir.join("nope.md")).is_err());
        assert_eq!(ws.tab_count(), 0);
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restored_tabs_load_when_shown_and_drop_layout_when_left() {
        let dir = temp_dir("lazy");
        let a = write(&dir, "a.md", "# A\n\ntext\n");
        let b = write(&dir, "b.md", "# B\n\ntext\n");
        let settings = dir.join("settings.ini");
        let mut saved = Session::default();
        saved.tabs = vec![a.clone(), b.clone(), dir.join("gone.md")];
        saved.active = 1;
        saved.save(&settings).unwrap();

        let mut ws = Workspace::new(Some(settings));
        assert_eq!(ws.tab_count(), 3);
        assert_eq!(ws.active(), Some(1));
        assert!(!ws.tabs()[0].is_loaded());
        assert!(ws.tabs()[1].is_loaded());
        assert!(!ws.tabs()[2].is_loaded());
        ws.set_viewport(800, 600, 1.0);
        assert!(ws.paint(800, 600, 1.0).is_some());
        assert!(ws.tabs()[1].view.is_some());

        assert!(ws.activate(0));
        assert!(ws.tabs()[0].is_loaded());
        assert!(ws.tabs()[1].is_loaded(), "the left tab keeps its document");
        assert!(ws.tabs()[1].view.is_none(), "but not its layout");
        // A missing restored file gets a page saying so.
        assert!(ws.activate(2));
        assert!(ws.tabs()[2].is_missing());
        assert!(
            ws.active_document()
                .unwrap()
                .text
                .contains(strings::FILE_NOT_FOUND)
        );
        // Idle tabs drop their documents and load again when shown.
        let later = Instant::now() + Duration::from_secs(120);
        assert_eq!(ws.drop_idle(later, Duration::from_secs(60)), 2);
        assert!(!ws.tabs()[0].is_loaded());
        assert!(ws.activate(0));
        assert!(ws.tabs()[0].is_loaded());
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reload_keeps_the_reading_position() {
        let dir = temp_dir("reload");
        let body = paragraphs("Body", 200);
        let path = write(&dir, "doc.md", &format!("# Title\n\n{body}"));
        let mut ws = workspace();
        ws.open(&path).unwrap();
        ws.paint(800, 600, 1.0).unwrap();
        let target = "Body paragraph number 100 ";
        let y_of = |ws: &Workspace| {
            let doc = ws.active_document().unwrap();
            let block = doc.block_at(doc.text.find(target).unwrap() as u32).unwrap();
            ws.layout().unwrap().y_of(doc, block).unwrap()
        };
        let before = y_of(&ws);
        assert!(ws.scroll_to(before));
        let offset = ws.scroll() - before;

        // Ten paragraphs appear above the reading position.
        let extra = paragraphs("Inserted", 10);
        std::fs::write(&path, format!("# Title\n\n{extra}{body}")).unwrap();
        assert!(ws.reload());
        ws.paint(800, 600, 1.0).unwrap();
        let after = y_of(&ws);
        assert!(after > before + 100.0, "the block moved down the page");
        assert!(
            (ws.scroll() - (after + offset)).abs() <= 1.0,
            "scroll {} follows the block to {after}",
            ws.scroll()
        );
        // A tab switch and a zoom change keep it too.
        let other = write(&dir, "other.md", "# Other\n");
        ws.open(&other).unwrap();
        ws.activate(0);
        ws.paint(800, 600, 1.0).unwrap();
        assert!((ws.scroll() - (after + offset)).abs() <= 1.0);
        assert!(ws.zoom_in());
        ws.paint(800, 600, 1.0).unwrap();
        let zoomed = y_of(&ws);
        assert!((ws.scroll() - zoomed).abs() <= 2.0);
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_deleted_file_keeps_its_text_and_reloads_when_it_returns() {
        let dir = temp_dir("deleted");
        let path = write(&dir, "doc.md", "# Before\n\nold text\n");
        let mut ws = workspace();
        ws.open(&path).unwrap();
        assert!(!ws.check_changed(), "nothing changed yet");

        std::fs::remove_file(&path).unwrap();
        assert!(ws.check_changed());
        assert!(ws.tabs()[0].is_missing());
        assert!(ws.active_document().unwrap().text.contains("old text"));
        assert_eq!(ws.status(Instant::now()), Some(strings::FILE_DELETED));
        assert!(!ws.check_changed(), "still gone is not a change");

        std::fs::write(&path, "# After\n\nnew text, longer than before\n").unwrap();
        assert!(ws.check_changed());
        assert!(!ws.tabs()[0].is_missing());
        assert!(ws.active_document().unwrap().text.contains("new text"));
        assert_eq!(ws.status(Instant::now()), Some(strings::REFRESHED));
        assert_eq!(ws.watched_paths().collect::<Vec<_>>(), [path.as_path()]);
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_counts_and_wraps() {
        let dir = temp_dir("find");
        let path = write(
            &dir,
            "doc.md",
            "# Fruit\n\napple banana Apple cherry\n\nlast apple\n",
        );
        let mut ws = workspace();
        ws.open(&path).unwrap();
        assert!(ws.find_set("apple"));
        assert_eq!(
            ws.find_counts(),
            FindCounts {
                current: 1,
                total: 3,
                capped: false
            }
        );
        assert_eq!(ws.find_label(), "1 of 3");
        let marks = ws.find_marks();
        assert_eq!(marks.current.len(), 1);
        assert_eq!(marks.others.len(), 2);
        assert!(ws.find_next());
        assert!(ws.find_next());
        assert_eq!(ws.find_counts().current, 3);
        assert!(ws.find_next());
        assert_eq!(ws.find_counts().current, 1, "wraps forward");
        assert!(ws.find_previous());
        assert_eq!(ws.find_counts().current, 3, "wraps backward");
        assert!(
            !ws.find_set("apple"),
            "the same query again changes nothing"
        );
        assert!(ws.find_set("durian"));
        assert_eq!(ws.find_label(), strings::FIND_NO_MATCHES);
        assert!(!ws.find_next());
        assert!(ws.find_clear());
        assert_eq!(ws.find_counts(), FindCounts::default());
        assert_eq!(ws.find_label(), "");
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_scrolls_a_far_match_into_view() {
        let dir = temp_dir("findfar");
        let path = write(
            &dir,
            "doc.md",
            &format!("# Top\n\n{}needle here\n", paragraphs("Filler", 150)),
        );
        let mut ws = workspace();
        ws.open(&path).unwrap();
        ws.paint(800, 600, 1.0).unwrap();
        assert_eq!(ws.scroll(), 0.0);
        ws.find_set("needle");
        assert!(ws.scroll() > 1000.0);
        assert_eq!(ws.find_marks().current.len(), 1, "the match is on screen");
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn selection_by_drag_double_click_and_select_all() {
        let dir = temp_dir("select");
        let path = write(&dir, "doc.md", "# Head\n\nalpha bravo charlie delta\n");
        let mut ws = workspace();
        ws.open(&path).unwrap();
        ws.paint(800, 600, 1.0).unwrap();
        let (start, end) = {
            let doc = ws.active_document().unwrap();
            let s = doc.text.find("bravo charlie").unwrap() as u32;
            (s, s + "bravo charlie".len() as u32)
        };
        let rects = ws.visible_rects(&[(start, end)]).0;
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        let y = r.y + r.h / 2.0;
        assert!(ws.pointer_press(r.x + 1.0, y, 1));
        assert!(ws.pointer_drag(r.x + r.w - 1.0, y));
        assert_eq!(ws.pointer_release(), Click::Selected);
        assert_eq!(ws.selected_text().as_deref(), Some("bravo charlie"));
        assert_eq!(ws.selection_rects().len(), 1);

        // A plain click clears it.
        assert!(ws.pointer_press(r.x + 1.0, y, 1));
        assert_eq!(ws.pointer_release(), Click::Deselected);
        assert!(!ws.has_selection());

        // A double click takes the word, a triple click the paragraph.
        let (x, y) = point_of(&mut ws, "charlie");
        assert!(ws.pointer_press(x, y, 2));
        assert_eq!(ws.pointer_release(), Click::Selected);
        assert_eq!(ws.selected_text().as_deref(), Some("charlie"));
        assert!(ws.pointer_press(x, y, 3));
        assert_eq!(
            ws.selected_text().as_deref(),
            Some("alpha bravo charlie delta")
        );
        ws.pointer_cancel();

        assert!(ws.select_all());
        let all = ws.selected_text().unwrap();
        assert!(all.contains("Head") && all.contains("delta"));
        assert!(ws.context_at(x, y).has_selection);
        assert!(ws.clear_selection());
        assert!(ws.selected_text().is_none());
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zoom_is_clamped_and_stepped() {
        let mut ws = Workspace::new(None);
        assert_eq!(ws.zoom(), 100);
        assert!(ws.set_zoom(500));
        assert_eq!(ws.zoom(), 250);
        assert!(!ws.zoom_in());
        assert_eq!(ws.zoom(), 250);
        assert!(ws.set_zoom(10));
        assert_eq!(ws.zoom(), 60);
        assert!(!ws.zoom_out());
        assert!(ws.zoom_in());
        assert_eq!(ws.zoom(), 70);
        assert!(ws.zoom_reset());
        assert_eq!(ws.zoom(), 100);
    }

    #[test]
    fn theme_choice_resolves() {
        let mut ws = Workspace::new(None);
        assert_eq!(ws.theme_choice(), ThemeChoice::System);
        assert_eq!(ws.resolve_theme(true).name, DARK.name);
        assert_eq!(ws.resolve_theme(false).name, LIGHT.name);
        ws.set_theme_choice(ThemeChoice::Light);
        assert_eq!(ws.resolve_theme(true).name, LIGHT.name);
        ws.set_theme_choice(ThemeChoice::Dark);
        assert_eq!(ws.theme().name, DARK.name);
    }

    #[test]
    fn links_scroll_open_tabs_hand_out_urls_and_refuse() {
        let dir = temp_dir("links");
        let a = write(
            &dir,
            "a.md",
            &format!(
                "# Top\n\n[fragment](#target) [other](b.md#bee) [web](https://example.com/x) \
                 [pdf](c.pdf) [nowhere](#no-such-anchor)\n\n{}## Target\n\nend\n",
                paragraphs("Filler", 120)
            ),
        );
        write(
            &dir,
            "b.md",
            &format!(
                "# B\n\n{}## Bee\n\n{}",
                paragraphs("Filler", 120),
                paragraphs("After", 60)
            ),
        );
        let mut ws = workspace();
        ws.open(&a).unwrap();
        ws.paint(800, 600, 1.0).unwrap();

        assert_eq!(
            click(&mut ws, "fragment"),
            Click::Link(LinkOutcome::Scrolled)
        );
        assert!(ws.scroll() > 1000.0);
        let outline = ws.outline();
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[1].title, "Target");
        // "Target" is too near the end to reach the top; at the bottom the last heading on
        // screen counts.
        assert_eq!(ws.scroll(), ws.max_scroll());
        assert_eq!(ws.current_heading(), Some(1));

        assert_eq!(
            click(&mut ws, "web"),
            Click::Link(LinkOutcome::OpenUrl("https://example.com/x".into()))
        );
        assert_eq!(
            click(&mut ws, "pdf"),
            Click::Link(LinkOutcome::Refused(strings::ONLY_MARKDOWN_LINKS.into()))
        );
        assert!(matches!(
            click(&mut ws, "nowhere"),
            Click::Link(LinkOutcome::Refused(_))
        ));
        let (x, y) = point_of(&mut ws, "web");
        assert_eq!(
            ws.hover(x, y).link.as_deref(),
            Some("https://example.com/x")
        );
        assert!(ws.hover(x, y).pointing);
        assert_eq!(
            ws.context_at(x, y).link.as_deref(),
            Some("https://example.com/x")
        );

        assert_eq!(
            click(&mut ws, "other"),
            Click::Link(LinkOutcome::OpenedTab(1))
        );
        assert_eq!(ws.active(), Some(1));
        assert!(ws.labels()[1] == "b.md");
        assert!(ws.scroll() > 1000.0, "b.md opened at its #bee heading");
        assert_eq!(ws.current_heading(), Some(1));
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn back_and_forward_return_to_where_a_jump_was_made() {
        let dir = temp_dir("history");
        let a = write(
            &dir,
            "a.md",
            &format!(
                "# Top

A line of text here.

{}## Target

{}",
                paragraphs("Filler", 120),
                paragraphs("After", 60)
            ),
        );
        let b = write(
            &dir,
            "b.md",
            &format!(
                "# B

{}## Bee

{}",
                paragraphs("Filler", 120),
                paragraphs("After", 60)
            ),
        );
        let mut ws = workspace();
        ws.open(&a).unwrap();
        ws.paint(800, 600, 1.0).unwrap();
        assert!(!ws.can_go_back() && !ws.go_back());

        // The text cursor is for text: on a line, not in the margin beside it.
        let (x, y) = point_of(&mut ws, "line");
        assert!(ws.hover(x, y).text);
        assert!(!ws.hover(790.0, y).text);
        assert!(!ws.hover(2.0, y).text);

        ws.scroll_to(30.0);
        assert_eq!(ws.open_link("#target"), LinkOutcome::Scrolled);
        let at_target = ws.scroll();
        assert!(at_target > 1000.0);
        assert_eq!(ws.open_link("b.md#bee"), LinkOutcome::OpenedTab(1));
        ws.paint(800, 600, 1.0).unwrap();
        let at_bee = ws.scroll();

        // Back crosses the tab, to the place in a.md the link was clicked from; then to where
        // the first jump was made.
        assert!(ws.go_back());
        assert_eq!((ws.active(), ws.scroll()), (Some(0), at_target));
        assert!(ws.go_back());
        assert_eq!((ws.active(), ws.scroll()), (Some(0), 30.0));
        assert!(!ws.go_back());
        assert!(ws.go_forward());
        assert_eq!(ws.scroll(), at_target);
        assert!(ws.go_forward());
        assert_eq!((ws.active(), ws.scroll()), (Some(1), at_bee));
        assert!(!ws.go_forward());

        // A tab closed since is opened again.
        assert!(ws.go_back());
        ws.close(1);
        assert_eq!(ws.tab_count(), 1);
        assert!(ws.go_forward());
        assert_eq!(
            ws.active_tab().map(|t| t.path().to_path_buf()),
            Some(b.clone())
        );
        ws.paint(800, 600, 1.0).unwrap();
        assert_eq!(ws.scroll(), at_bee);

        // A new jump drops what Forward held.
        assert!(ws.go_back());
        assert_eq!(ws.open_link("#top"), LinkOutcome::Scrolled);
        assert!(!ws.can_go_forward());
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_round_trip() {
        let dir = temp_dir("session");
        let a = write(&dir, "a.md", "# A\n");
        let b = write(&dir, "b.md", "# B\n");
        let settings = dir.join("settings.ini");
        {
            let mut ws = Workspace::new(Some(settings.clone()));
            assert_eq!(ws.tab_count(), 0);
            ws.open(&a).unwrap();
            ws.open(&b).unwrap();
            ws.set_zoom(130);
            ws.set_theme_choice(ThemeChoice::Dark);
            ws.set_panes(true, false);
            ws.set_window_size(1000, 700, false);
            ws.set_window_size(1600, 1000, true);
            ws.set_folder(Some(dir.clone()));
            ws.save_session().unwrap();
        }
        let ws = Workspace::new(Some(settings));
        let tabs: Vec<&Path> = ws.tabs().iter().map(Tab::path).collect();
        assert_eq!(tabs, [a.as_path(), b.as_path()]);
        assert_eq!(ws.active(), Some(1));
        assert!(!ws.tabs()[0].is_loaded());
        assert_eq!(ws.zoom(), 130);
        assert_eq!(ws.theme_choice(), ThemeChoice::Dark);
        assert_eq!(ws.panes(), (true, false));
        assert_eq!(ws.window_size(), Some((1000, 700, true)));
        assert_eq!(ws.folder(), Some(dir.as_path()));
        assert_eq!(ws.recents(), [b.clone(), a.clone()]);
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recents_remove_missing_and_clear() {
        let dir = temp_dir("recent");
        let a = write(&dir, "a.md", "# A\n");
        let b = write(&dir, "b.md", "# B\n");
        let mut ws = workspace();
        ws.open(&a).unwrap();
        ws.open(&b).unwrap();
        ws.close_all();
        std::fs::remove_file(&a).unwrap();
        assert!(ws.recent_entries()[1].missing);
        assert!(ws.open_recent(&a).is_err());
        assert_eq!(ws.recents(), [b.clone()]);
        ws.remove_recent(&b);
        assert!(ws.recents().is_empty());
        ws.open(&b).unwrap();
        std::fs::remove_file(&b).unwrap();
        assert_eq!(ws.remove_missing_recents(), 1);
        ws.clear_recents();
        assert_eq!(ws.status(Instant::now()), Some(strings::RECENT_CLEARED));
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn showcase_paints_and_has_stats() {
        let showcase = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/showcase.md");
        let mut ws = Workspace::new(None);
        ws.open(&showcase).unwrap();
        let view = ws.paint(900, 700, 1.5).expect("a frame");
        assert_eq!(view.frame.bitmap.width, 900);
        assert_eq!(view.frame.bitmap.height, 700);
        assert_eq!(view.x, 0);
        assert!(view.total_height > 700.0);
        let stats = ws.stats().unwrap();
        assert!(stats.words > 100 && stats.reading_minutes >= 1 && stats.blocks > 10);
        assert!(!ws.outline().is_empty());
        assert!(ws.page_down());
        assert!(ws.scroll() > 0.0);
        assert!(ws.scroll_end());
        assert_eq!(ws.scroll(), ws.max_scroll().round());
        assert!(ws.paint(900, 700, 1.5).is_some());
        // The status line fades.
        ws.set_status("hello");
        let later = Instant::now() + Duration::from_secs(10);
        assert_eq!(ws.status(Instant::now()), Some("hello"));
        assert_eq!(ws.status(later), None);
    }

    /// An outline jump right after opening lands exactly while most of the page is estimated, and
    /// refining the rest keeps the heading where it is on screen.
    #[test]
    fn a_jump_before_refinement_stays_put_through_it() {
        let dir = temp_dir("jump-refine");
        let mut text = String::new();
        for i in 0..60 {
            text.push_str(&format!(
                "## Part {i}\n\n{}\n",
                paragraphs("Words", i % 4 + 1)
            ));
        }
        let path = write(&dir, "doc.md", &text);
        let mut ws = workspace();
        ws.open(&path).unwrap();
        ws.paint(800, 600, 1.0).unwrap();
        assert!(ws.refining(), "the first layout is estimated");
        let block = ws.outline()[45].block;
        assert!(ws.scroll_to_block(block));
        let at = |ws: &Workspace| {
            let doc = ws.active_document().unwrap();
            ws.layout().unwrap().y_of(doc, block).unwrap() - ws.scroll()
        };
        assert_eq!(at(&ws), JUMP_MARGIN);
        ws.paint(800, 600, 1.0).unwrap();
        while ws.refine(Duration::from_millis(1)) {}
        assert!(!ws.refining());
        ws.paint(800, 600, 1.0).unwrap();
        assert_eq!(at(&ws), JUMP_MARGIN);
        assert_eq!(ws.current_heading(), Some(45));
        _ = std::fs::remove_dir_all(&dir);
    }

    /// A `<details>` opened above the viewport (as a click on its summary would, then scrolling
    /// on) moves the page under the reader, not the reader: the scroll offset grows by the body's
    /// height and the text at the top stays where it was. The headings move with it.
    #[test]
    fn a_details_toggle_above_keeps_the_reading_position() {
        let dir = temp_dir("details-above");
        let body = paragraphs("Body", 120);
        let path = write(
            &dir,
            "doc.md",
            &format!(
                "<details>\n<summary>More</summary>\n\n{}\n</details>\n\n# Middle\n\n{body}",
                paragraphs("Hidden", 6)
            ),
        );
        let mut ws = workspace();
        ws.open(&path).unwrap();
        ws.paint(800, 600, 1.0).unwrap();
        let details = {
            let doc = ws.active_document().unwrap();
            (0..doc.blocks.len() as u32)
                .find(|&b| {
                    matches!(
                        doc.blocks[b as usize].kind,
                        mdview_core::model::BlockKind::Details { .. }
                    )
                })
                .unwrap()
        };
        let target = "Body paragraph number 60 ";
        let top_of = |ws: &mut Workspace| {
            let doc = ws.active_document().unwrap();
            let start = doc.text.find(target).unwrap() as u32;
            let rects = ws.visible_rects(&[(start, start + 4)]).0;
            rects.first().map(|r| r.y)
        };
        assert!(ws.scroll_to(ws.layout().unwrap().total_height() / 2.0));
        ws.paint(800, 600, 1.0).unwrap();
        let (scroll, at, total) = (ws.scroll(), top_of(&mut ws), ws.total_height());
        let heading = ws.current_heading();
        assert!(at.is_some(), "the target is on screen");
        assert!(ws.toggle_details(details));
        let view = ws.paint(800, 600, 1.0).unwrap();
        assert!(view.total_height > total + 100.0, "the body opened");
        assert!(ws.scroll() > scroll + 100.0, "the offset followed");
        assert_eq!(top_of(&mut ws), at, "the text stayed put");
        assert_eq!(ws.current_heading(), heading);
        _ = std::fs::remove_dir_all(&dir);
    }

    /// Whether the engine process that draws math and diagrams is built beside the tests.
    fn engine_available() -> bool {
        if let Some(path) = std::env::var_os("MDV_ENGINE") {
            return Path::new(&path).is_file();
        }
        let name = if cfg!(windows) {
            "mdview-engine.exe"
        } else {
            "mdview-engine"
        };
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release")
            .join(name)
            .is_file()
    }

    #[test]
    fn a_right_click_on_a_formula_or_diagram_offers_its_source() {
        if !engine_available() {
            eprintln!("skipping: no mdview-engine in target/release");
            return;
        }
        let dir = temp_dir("sources");
        let path = write(
            &dir,
            "a.md",
            "Inline $x^2 + 1$ in a sentence.\n\n$$\nE = mc^2\n$$\n\n```mermaid\ngraph TD\n  A --> B\n```\n",
        );
        let mut ws = workspace();
        ws.open(&path).unwrap();
        ws.paint(800, 600, 1.0).unwrap();
        let mut found: Vec<(SourceKind, String)> = Vec::new();
        let mut y = 2.0;
        while y < 600.0 {
            let mut x = 2.0;
            while x < 800.0 {
                if let Some(item) = ws.context_at(x, y).source
                    && !found.contains(&item)
                {
                    found.push(item);
                }
                x += 6.0;
            }
            y += 3.0;
        }
        assert_eq!(
            found,
            [
                (SourceKind::Formula, "x^2 + 1".to_string()),
                (SourceKind::Formula, "E = mc^2".to_string()),
                (SourceKind::Diagram, "graph TD\n  A --> B".to_string()),
            ]
        );
        // Plain text offers no source.
        let (x, y) = point_of(&mut ws, "sentence");
        assert_eq!(ws.context_at(x, y).source, None);
        _ = std::fs::remove_dir_all(&dir);
    }
}
