//! The window and its event loop: start-up, frame composition, input, commands and timers.
//!
//! **Frames.** The window is five regions (title row, command bar, Files, Outline, page) plus
//! overlays (menus, dialogs, toast, link hint, tooltip). Each region has a signature: a hash of
//! everything it shows. A frame redraws only the regions whose signature changed (the page also
//! whenever the workspace says it is dirty), redraws overlays together with every region under
//! them, and sends only the redrawn rows to the screen. A wheel step therefore costs the page
//! region and whatever overlays sit on it. The composed `screen` bitmap is kept between frames.
//!
//! **Idle.** The loop waits for events; it wakes on a timer only for something pending (caret
//! blink while the find box has focus, a tooltip delay, the toast expiring, drag auto-scroll,
//! unloading idle tabs, saving the window size after a resize, and every 5 s
//! while a tab shows the editor's live text: asking, on a helper thread, if the editor is there).

use crate::args::{Args, ShotDialog, ShotMenu};
use crate::chrome::{self, Ctx};
use crate::gfx::{Face, Gfx, Painter, Rect};
use crate::keys::{self, KeyName, Mods};
use crate::overlays::{self, Bubble};
use crate::page;
use crate::panes;
use crate::platform;
use crate::textfield::EditKey;
use crate::theme::{self, Palette};
use crate::ui::{
    Act, Areas, BarItem, Dialog, DialogKind, Drag, Hit, PAGE_MIN, PANE_MAX, PANE_MIN, Popup,
    PopupKind, REGIONS, Region, Scroller, Side, Tip, Ui,
};
use crate::updater;
use crate::workspace::{Click, LinkOutcome, ThemeChoice, Workspace};
use mdshared_app::sister::Request;
use mdview_core::commands::CommandId as C;
use mdview_core::watch::FileWatcher;
use mdview_render::Bitmap;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, ResizeDirection, Theme as WinTheme, Window, WindowId};

mod commands;
mod input;
mod modal;
#[cfg(test)]
mod tests;

const TIP_DELAY: Duration = Duration::from_millis(500);
const AUTOSCROLL_TICK: Duration = Duration::from_millis(16);
const IDLE_CHECK: Duration = Duration::from_secs(30);
/// While a tab shows the editor's text: how often to ask whether the editor still answers.
const SISTER_CHECK: Duration = Duration::from_secs(5);
const IDLE_UNLOAD: Duration = Duration::from_secs(60);
const SIZE_SAVE_DELAY: Duration = Duration::from_millis(800);
/// How long the page's refinement may run between events: short enough that input waiting
/// behind it is not noticed.
const REFINE_SLICE: Duration = Duration::from_millis(4);
/// One wheel notch scrolls this many of the workspace's line steps (48 logical px each).
const WHEEL_LINES: f32 = 2.0;
/// Pane wheel step, logical px per notch.
const PANE_WHEEL: f32 = 90.0;
/// Pointer travel that still counts as the same spot for a double click.
const CLICK_SLOP: f32 = 4.0;
/// The watcher follows the indexed notes of the open folder up to this many, so a note saved
/// anywhere in it refreshes the backlinks. A larger folder is refreshed by the other wakes only.
const WATCHED_NOTES: usize = 5_000;

pub enum Wake {
    /// The single-instance listener queued paths.
    Instance,
    /// The file watcher saw a change.
    Files,
    /// A remote image finished downloading.
    Images,
    /// The note index (wiki-links, backlinks) was built or refreshed.
    Notes,
    /// The animation thread has a frame ready.
    Animation,
    /// An updater thread has an answer.
    Update(updater::Event),
    /// The helper thread asked whether a window of mdedit still answers: true if one does.
    Sister(bool),
    /// A file or folder dialog, which runs on a helper thread off Windows, was closed.
    #[cfg(not(windows))]
    Dialog(DialogAnswer),
}

/// What a closed file dialog chose.
#[cfg(not(windows))]
pub enum DialogAnswer {
    Files(Vec<PathBuf>),
    Folder(Option<PathBuf>),
}

fn sig<T: Hash>(t: T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

#[derive(Default, Clone, Copy)]
struct FrameTimes {
    layout: Duration,
    paint: Duration,
    compose: Duration,
    /// Copying changed rows into the window's buffer (ours) and handing them to the OS.
    copy: Duration,
    present: Duration,
    rows: u32,
}

struct App {
    started: Instant,
    args: Args,
    ws: Workspace,
    ui: Ui,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    proxy: Option<EventLoopProxy<Wake>>,
    /// A file or folder dialog is open (off Windows, where it runs beside the window); a second
    /// request is ignored until its answer arrives.
    #[cfg(not(windows))]
    dialog_open: bool,
    incoming: Option<Receiver<Request>>,
    watcher: Option<FileWatcher>,
    /// The folders the watcher follows for the Files pane: the tree's root and expanded folders
    /// while the pane is open.
    tree_dirs: Vec<PathBuf>,
    session: Option<PathBuf>,
    screen: Bitmap,
    gfx: Gfx,
    pal: Palette,
    system_dark: bool,
    scale: f32,
    /// Device px.
    size: (u32, u32),
    maximized: bool,
    /// The window's drop shadow (Linux: KWin's protocol, or the client-drawn margin where the
    /// compositor has none); `None` everywhere else. With the margin, `size` is the CONTENT size
    /// (the surface minus the margin on every side) and `present` puts the content inside it.
    /// Dropping it would remove the KWin shadow, so it lives as long as the window.
    shadow: Option<mdshared_ui::shadow::Shadow>,
    areas: Areas,
    hits: [Vec<(Hit, Rect)>; 5],
    overlay_hits: Vec<(Hit, Rect)>,
    sigs: [u64; 5],
    overlay_sig: u64,
    overlay_bounds: Vec<Rect>,
    /// Redraw everything next frame (size, scale or theme changed).
    full: bool,
    /// Something happened that may change what is shown.
    wants_frame: bool,
    /// A redraw this app asked for is on its way. A `RedrawRequested` that arrives without it
    /// comes from the system (the window was shown, restored or uncovered), which has dropped
    /// what the window showed, so the whole frame goes out again even though nothing changed.
    requested: bool,
    expose: bool,
    first: bool,
    exit: bool,
    mods: Mods,
    cursor: CursorIcon,
    last_title: String,
    /// The active file, to follow it in the Files tree when it changes.
    last_active: Option<PathBuf>,
    last_heading: Option<usize>,
    /// The tab `last_heading` was computed for: a heading index means nothing across tabs.
    last_heading_tab: Option<usize>,
    toast_until: Option<Instant>,
    size_save_due: Option<Instant>,
    idle_due: Option<Instant>,
    /// When to ask next whether the editor answers; `Some` only while a tab is live.
    sister_due: Option<Instant>,
    /// A helper thread is asking now.
    sister_asking: bool,
    /// Rows of the page viewport (device px, first and one past the last) where an animated
    /// image went to its next frame since the last frame was drawn.
    anim_rows: Option<(u32, u32)>,
    /// A partial page draw found more changed than its rows; the page is drawn whole next.
    page_again: bool,
    /// The system says the window is completely covered.
    occluded: bool,
    /// `--scroll-steps`: steps left and the frame times taken.
    steps: Option<u32>,
    step_times: Vec<FrameTimes>,
    trace: bool,
    /// Ctrl+wheel and trackpad deltas below one notch, kept until they add up.
    wheel_rest: f32,
}

impl App {
    fn new(args: Args, ws: Workspace, session: Option<PathBuf>) -> App {
        let system_dark = platform::system_is_dark();
        let trace = std::env::var_os("MDVIEW_TRACE").is_some();
        App {
            started: Instant::now(),
            steps: args.scroll_steps,
            args,
            ws,
            ui: Ui::new(),
            window: None,
            surface: None,
            proxy: None,
            #[cfg(not(windows))]
            dialog_open: false,
            incoming: None,
            watcher: None,
            tree_dirs: Vec::new(),
            session,
            screen: Bitmap::new(1, 1),
            gfx: Gfx::new(),
            pal: theme::DARK,
            system_dark,
            scale: 1.0,
            size: (1, 1),
            maximized: false,
            shadow: None,
            areas: Areas::default(),
            hits: Default::default(),
            overlay_hits: Vec::new(),
            sigs: [0; 5],
            overlay_sig: 0,
            overlay_bounds: Vec::new(),
            full: true,
            wants_frame: true,
            requested: false,
            expose: false,
            first: true,
            exit: false,
            mods: Mods::default(),
            cursor: CursorIcon::Default,
            last_title: String::new(),
            last_active: None,
            last_heading: None,
            last_heading_tab: None,
            toast_until: None,
            size_save_due: None,
            idle_due: None,
            sister_due: None,
            sister_asking: false,
            anim_rows: None,
            page_again: false,
            occluded: false,
            step_times: Vec::new(),
            trace,
            wheel_rest: 0.0,
        }
    }

    /// Applies the arguments that shape the first state: theme, zoom, folder, panes, files.
    fn setup(&mut self) {
        let a = self.args.clone();
        self.ws.set_system_dark(self.system_dark);
        if let Some(dark) = a.dark {
            self.ws.set_theme_choice(if dark {
                ThemeChoice::Dark
            } else {
                ThemeChoice::Light
            });
        }
        if let Some(z) = a.zoom {
            self.ws.set_zoom(z);
        }
        if let Some(font) = &a.page_font {
            self.ws.set_font(false, font);
        }
        if let Some(font) = &a.code_font {
            self.ws.set_font(true, font);
        }
        if let Some(width) = a.page_width {
            self.ws.set_page_width(width);
        }
        let (files, outline) = self.ws.panes();
        self.ui.panes.files = a.files_pane.unwrap_or(files);
        self.ui.panes.outline = a.outline_pane.unwrap_or(outline);
        let (files_w, outline_w) = self.ws.pane_widths();
        let width = |saved: Option<u32>, default: f32| {
            saved.map_or(default, |w| (w as f32).clamp(PANE_MIN, PANE_MAX))
        };
        self.ui.panes.files_w = width(files_w, self.ui.panes.files_w);
        self.ui.panes.outline_w = width(outline_w, self.ui.panes.outline_w);
        if let Some(folder) = &a.folder {
            self.ws
                .set_folder(Some(mdview_core::files::canonical_or_absolute(folder)));
        }
        self.ui
            .tree
            .set_root(self.ws.folder().map(Path::to_path_buf));
        for f in &a.files {
            let _ = self.ws.open(f);
        }
        if a.shot.is_some() {
            // A shot has no event loop to wait for the indexer in.
            self.ws.index_notes_now();
        }
        if let Some(q) = &a.find {
            self.ui.find.set_text(q);
            self.ws.find_set(q);
        }
        self.ui.focus_mode = a.focus_mode;
        if let Some(stage) = &a.update_bar {
            self.ws.preview_update(stage);
        }
        if a.load_remote {
            self.ws.allow_remote(false);
        }
        self.ws.clear_status();
        self.sync_theme();
    }

    fn redraw(&mut self) {
        self.wants_frame = true;
    }

    fn logical(&self) -> (f32, f32) {
        (
            self.size.0 as f32 / self.scale,
            self.size.1 as f32 / self.scale,
        )
    }

    fn sync_theme(&mut self) {
        let dark = self.ws.theme().name == mdview_render::DARK.name;
        let pal = if dark { theme::DARK } else { theme::LIGHT };
        if pal != self.pal {
            self.pal = pal;
            self.full = true;
            #[cfg(windows)]
            if let Some(w) = &self.window {
                style(w, dark);
            }
        }
    }

    // ---- frame ---------------------------------------------------------------------------------

    fn region_sig(&mut self, r: Region, rect: Option<Rect>) -> u64 {
        let ui = &self.ui;
        let hov = |h: Option<Hit>| h.filter(|h| h.region() == Some(r));
        let common = (
            rect.map(|r| (r.x.to_bits(), r.y.to_bits(), r.w.to_bits(), r.h.to_bits())),
            hov(ui.hover),
            hov(ui.pressed),
        );
        match r {
            Region::Title => {
                let missing: Vec<(bool, bool)> = self
                    .ws
                    .tabs()
                    .iter()
                    .map(|t| (t.is_missing(), t.is_live()))
                    .collect();
                let drag = match ui.drag {
                    Some(Drag::Tab { index, moving, .. }) => Some((index, moving)),
                    _ => None,
                };
                sig((
                    common,
                    self.maximized,
                    ui.focus_mode,
                    self.ws.labels(),
                    self.ws.active(),
                    missing,
                    drag,
                ))
            }
            Region::Bar => sig((
                common,
                (self.ws.zoom(), self.ws.page_width() as u8),
                self.ws.theme_choice() as u8,
                ui.find.text(),
                ui.find.caret(),
                ui.find_focused,
                ui.find_focused && ui.caret_on,
                self.ws.find_label(),
                ui.panes.files,
                ui.panes.outline,
                ui.focus_mode,
                ui.popup_kind(),
            )),
            Region::Files => {
                let active = self.ws.active_tab().map(|t| t.path().to_path_buf());
                let drag = matches!(
                    ui.drag,
                    Some(Drag::Splitter {
                        side: Side::Files,
                        ..
                    }) | Some(Drag::Thumb {
                        target: Scroller::Pane(Side::Files),
                        ..
                    })
                );
                sig((
                    common,
                    ui.tree.root(),
                    ui.tree.rows().len(),
                    self.tree_sig(),
                    active,
                    ui.panes.files_scroll.to_bits(),
                    drag,
                ))
            }
            Region::Outline => {
                let titles: Vec<(u8, &str)> = self
                    .ws
                    .outline()
                    .iter()
                    .map(|e| (e.level, e.title))
                    .collect();
                let drag = matches!(
                    ui.drag,
                    Some(Drag::Splitter {
                        side: Side::Outline,
                        ..
                    }) | Some(Drag::Thumb {
                        target: Scroller::Pane(Side::Outline),
                        ..
                    })
                );
                let backlinks: Vec<(&str, &str)> = self
                    .ws
                    .backlinks()
                    .iter()
                    .map(|b| (b.shown.as_str(), b.context.as_str()))
                    .collect();
                sig((
                    common,
                    self.ws.active(),
                    titles,
                    backlinks,
                    self.ws.current_heading(),
                    ui.panes.outline_scroll.to_bits(),
                    drag,
                ))
            }
            Region::Page => {
                let drag = matches!(
                    ui.drag,
                    Some(Drag::Thumb {
                        target: Scroller::Page,
                        ..
                    })
                );
                let missing = self.ws.active_tab().map(|t| t.is_missing());
                let recents = if self.ws.active().is_none() {
                    sig(self.ws.recents())
                } else {
                    0
                };
                let update = self.ws.update_bar().map(|stage| format!("{stage:?}"));
                sig((common, self.ws.active(), missing, drag, recents, update))
            }
        }
    }

    /// The tree's visible rows, cheaply: expansion changes alter the count or the order.
    fn tree_sig(&self) -> u64 {
        let rows = self.ui.tree.rows();
        sig(rows
            .iter()
            .map(|r| (r.depth, r.expanded))
            .collect::<Vec<_>>())
    }

    /// The page viewport in device px, and whether there is an active tab to lay out.
    fn page_device(&self) -> (u32, u32) {
        let vr = page::viewport(&self.ws, self.areas.page);
        let d = |v: f32| (v * self.scale).round() as i32;
        (
            (d(vr.right()) - d(vr.x)).max(0) as u32,
            (d(vr.bottom()) - d(vr.y)).max(0) as u32,
        )
    }

    /// Composes one frame into `screen` and returns the device-px rows changed.
    fn compose(&mut self) -> (Vec<(i32, i32, i32, i32)>, FrameTimes) {
        let mut times = FrameTimes::default();
        let t0 = Instant::now();
        let (pw, ph) = (self.size.0.max(1), self.size.1.max(1));
        if self.screen.width != pw || self.screen.height != ph {
            self.screen = Bitmap::new(pw, ph);
            self.full = true;
        }
        self.sync_theme();
        // Text the editor sent since the last frame: one parse, of the newest.
        self.ws.apply_live();
        let (w, h) = self.logical();
        let areas = Areas::compute(w, h, &self.ui);
        if areas != self.areas {
            self.areas = areas;
            self.full = true;
        }
        // Lay the page out first: the outline's current heading and a pending jump depend on it.
        let (vw, vh) = self.page_device();
        if vw > 0 && vh > 0 {
            self.ws.prepare(vw, vh, self.scale);
        }
        self.ws.update_backlinks();
        let current = self.ws.current_heading();
        let tab = self.ws.active();
        if current != self.last_heading || tab != self.last_heading_tab {
            // The outline follows the page, except while the pointer is on it or its size is not
            // known yet (not drawn): then the row is revealed on a later frame.
            let shown = self.areas.outline.is_some();
            let done = !shown
                // Above the first heading nothing is current: the outline returns to its top.
                || panes::follow_outline_row(&mut self.ui, current.unwrap_or(0));
            if done {
                self.last_heading = current;
                self.last_heading_tab = tab;
            } else if self.ui.panes.outline_view <= 0.0
                && panes::outline_has_room(self.areas.outline)
            {
                self.wants_frame = true;
            }
        }
        self.follow_active_file();
        times.layout = t0.elapsed();

        let t1 = Instant::now();
        let mut dirty = [self.full; 5];
        for r in REGIONS {
            let rect = self.areas.region(r);
            let s = self.region_sig(r, rect);
            if s != self.sigs[r.index()] {
                self.sigs[r.index()] = s;
                dirty[r.index()] = true;
            }
        }
        if self.ws.take_dirty() || std::mem::take(&mut self.page_again) {
            dirty[Region::Page.index()] = true;
        }
        for r in REGIONS {
            if self.areas.region(r).is_none() {
                dirty[r.index()] = false;
                self.hits[r.index()].clear();
            }
        }

        // Overlays: lay them out, then redraw every region under them if they or those changed.
        let now = Instant::now();
        let pal = self.pal;
        let mut bubbles = Vec::new();
        {
            let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
            let page = self.areas.page;
            if let Some(text) = &self.ui.link_hint {
                bubbles.push(Bubble::hint(&mut p, text, page));
            }
            if let Some(text) = self.ws.status(now) {
                bubbles.push(Bubble::toast(&mut p, text, page));
            }
            if let Some(tip) = self.ui.tip.as_ref().filter(|t| t.shown) {
                bubbles.push(Bubble::tip(&mut p, &tip.text, tip.anchor, w, h));
            }
            if let Some(popup) = self.ui.popup.as_mut() {
                let room = menu_room(self.areas.title, w, h);
                popup.menu.place_in(&mut p, room);
                if let Some(s) = popup.sub.as_mut() {
                    s.place_in(&mut p, room);
                }
            }
        }
        self.toast_until = self.ws.status(now).and(self.ws.status_expires());
        let mut bounds: Vec<Rect> = bubbles.iter().map(Bubble::bounds).collect();
        if let Some(popup) = &self.ui.popup {
            bounds.push(popup.menu.rect().inset(-20.0));
            if let Some(s) = &popup.sub {
                bounds.push(s.rect().inset(-20.0));
            }
        }
        if self.ui.dialog.is_some() || self.ui.settings.is_some() {
            bounds.push(Rect::new(0.0, 0.0, w, h));
        }
        if let Some(p) = self.ui.palette.as_mut() {
            // The card's place is needed here, to redraw what it covers; drawing lays it out again.
            let card = p
                .palette
                .layout(Rect::new(0.0, 0.0, w, h), crate::ui::TITLE_H);
            bounds.push(card.inset(-20.0));
        }
        let osig = sig((
            bubbles
                .iter()
                .map(|b| {
                    (
                        b.kind,
                        b.text.as_str(),
                        b.rect.x.to_bits(),
                        b.rect.y.to_bits(),
                    )
                })
                .collect::<Vec<_>>(),
            self.ui.popup.as_ref().map(|p| {
                (
                    p.kind,
                    (p.menu.hover, p.menu.scroll().to_bits()),
                    p.menu.items.len(),
                    p.sub
                        .as_ref()
                        .map(|s| (s.hover, s.items.len(), s.scroll().to_bits())),
                )
            }),
            self.ui.dialog.as_ref().map(|d| {
                (
                    d.kind,
                    d.scroll.to_bits(),
                    self.ui.dlg_hover_row,
                    self.ws.recents().len(),
                )
            }),
            self.ui
                .dialog
                .as_ref()
                .map(|_| (self.ui.hover, self.ui.pressed)),
            // Neither the palette nor the settings dialog says what changed inside: every event
            // they handled raised the tick; the caret's blink is the one thing that changes alone.
            self.ui.modal().then_some((
                self.ui.overlay_tick,
                self.ui.palette.is_some().then_some(self.ui.caret_on),
            )),
        ));
        let overlay_changed = osig != self.overlay_sig;
        self.overlay_sig = osig;
        let mut under: Vec<Rect> = bounds.clone();
        under.extend(self.overlay_bounds.iter().copied());
        // An animated image's next frame and nothing else: only its rows of the page are drawn
        // and presented, unless an overlay lies over them (then the page goes the usual way).
        let mut page_band: Option<(Rect, (u32, u32))> = None;
        if let Some((a, b)) = self.anim_rows.take()
            && !dirty[Region::Page.index()]
            && self.areas.region(Region::Page).is_some()
            && self.ws.active().is_some()
        {
            let vr = page::viewport(&self.ws, self.areas.page);
            let y0 = (vr.y * self.scale).round();
            // A device row to spare on each side, against rounding back and forth.
            let band = Rect::new(
                vr.x,
                (y0 + a as f32 - 1.0) / self.scale,
                vr.w,
                (b.saturating_sub(a) as f32 + 2.0) / self.scale,
            );
            if under.iter().any(|u| overlaps(*u, band)) {
                dirty[Region::Page.index()] = true;
            } else {
                page_band = Some((band, (a, b)));
            }
        }
        let touches = |r: Region, dirty: &[bool; 5]| dirty[r.index()];
        let any_under_dirty = REGIONS.iter().any(|&r| {
            touches(r, &dirty)
                && self
                    .areas
                    .region(r)
                    .is_some_and(|rr| under.iter().any(|b| overlaps(*b, rr)))
        });
        let draw_overlays = !bounds.is_empty() && (overlay_changed || any_under_dirty || self.full);
        if overlay_changed || any_under_dirty {
            for r in REGIONS {
                if let Some(rr) = self.areas.region(r)
                    && under.iter().any(|b| overlaps(*b, rr))
                {
                    dirty[r.index()] = true;
                }
            }
        }
        self.overlay_bounds = bounds;
        if !self.overlay_bounds.is_empty() || self.ui.dialog.is_none() {
            // Dialog hits are rebuilt whenever overlays draw; with no dialog there are none.
        }
        if self.ui.dialog.is_none() {
            self.overlay_hits.clear();
        }

        // Draw.
        let maximized = self.maximized;
        let (hover, pressed) = (self.ui.hover, self.ui.pressed);
        let mut damage = Vec::new();
        // The page rows a draw of an animation's next frame is limited to.
        let mut partial: Option<(u32, u32)> = None;
        {
            let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
            for r in REGIONS {
                let band = page_band.filter(|_| r.index() == Region::Page.index());
                if !dirty[r.index()] && band.is_none() {
                    continue;
                }
                let Some(rect) = self.areas.region(r) else {
                    continue;
                };
                // The whole region, or only the rows an animation changed. Hits are recorded
                // for the whole region either way.
                let band = band.filter(|_| !dirty[r.index()]);
                let clip = band.map_or(rect, |(clip, _)| clip);
                partial = band.map(|(_, rows)| rows);
                damage.push(p.dev_rect(clip));
                let hits = &mut self.hits[r.index()];
                hits.clear();
                let old = p.clip(clip);
                let mut c = Ctx {
                    p: &mut p,
                    pal: &pal,
                    hits,
                    hover,
                    pressed,
                };
                match r {
                    Region::Title => {
                        chrome::draw_title(&mut c, &self.ws, &self.ui, rect, maximized)
                    }
                    Region::Bar => chrome::draw_bar(&mut c, &self.ws, &mut self.ui, rect),
                    Region::Files => panes::draw_files(&mut c, &self.ws, &mut self.ui, rect),
                    Region::Outline => panes::draw_outline(&mut c, &self.ws, &mut self.ui, rect),
                    Region::Page => {
                        times.paint = page::draw_page(&mut c, &mut self.ws, &self.ui, rect)
                    }
                }
                p.restore(old);
            }
            if draw_overlays {
                let mut c = Ctx {
                    p: &mut p,
                    pal: &pal,
                    hits: &mut self.overlay_hits,
                    hover,
                    pressed,
                };
                c.hits.clear();
                if let Some(d) = self.ui.dialog.as_mut() {
                    overlays::draw_dialog(&mut c, &self.ws, d, w, h, self.ui.dlg_hover_row);
                }
                if let Some(popup) = self.ui.popup.as_mut() {
                    let room = menu_room(self.areas.title, w, h);
                    popup.menu.draw_in(c.p, &pal, room);
                    if let Some(s) = popup.sub.as_mut() {
                        s.draw_in(c.p, &pal, room);
                    }
                }
                for b in &bubbles {
                    b.draw(c.p, &pal);
                }
                // Last, over everything.
                let window = Rect::new(0.0, 0.0, w, h);
                if let Some(p) = self.ui.palette.as_mut() {
                    p.palette
                        .draw(c.p, &pal, window, crate::ui::TITLE_H, self.ui.caret_on);
                }
                if let Some(f) = self.ui.settings.as_mut() {
                    f.draw(c.p, &pal, window, self.ui.caret_on);
                }
            }
        }
        // A partial draw trusted that only those rows of the page changed. If the page's frame
        // says otherwise (something else moved in between), the page is drawn whole next.
        if let Some((a, b)) = partial
            && !self.ws.frame_changed_within(a, b)
        {
            self.page_again = true;
            self.wants_frame = true;
        }
        // Painting may have settled a jump; that is already on screen.
        self.ws.take_dirty();
        self.full = false;
        times.compose = t1.elapsed().saturating_sub(times.paint);
        times.rows = damage.iter().map(|d| (d.3 - d.1).max(0) as u32).sum();
        (damage, times)
    }

    /// Selects the active file in the Files tree when it changes.
    fn follow_active_file(&mut self) {
        let active = self.ws.active_tab().map(|t| t.path().to_path_buf());
        if active == self.last_active {
            return;
        }
        self.last_active = active.clone();
        if let Some(path) = active
            && let Some(i) = self.ui.tree.reveal(&path)
        {
            let top = i as f32 * panes::ROW;
            let view = self.ui.panes.files_view;
            let s = &mut self.ui.panes.files_scroll;
            if view > 0.0 && (top < *s || top + panes::ROW > *s + view) {
                *s = (top - view / 3.0).max(0.0);
            }
        }
    }

    /// Gives the page's refinement one slice between events (the first layout only estimated the
    /// blocks off screen). True while there is more; once the page is refined this does nothing,
    /// so an idle window costs no CPU.
    /// Keeps the watcher on the folders the Files pane lists, so a file created, removed or renamed
    /// there shows without a refresh. A closed pane watches nothing.
    fn watch_tree(&mut self) {
        let Some(w) = self.watcher.as_mut() else {
            return;
        };
        let dirs = if self.ui.panes.files {
            self.ui.tree.watched_dirs()
        } else {
            Vec::new()
        };
        if dirs != self.tree_dirs {
            w.set_folders(dirs.iter().map(PathBuf::as_path));
            self.tree_dirs = dirs;
        }
    }

    fn refine_slice(&mut self) -> bool {
        if self.first || !self.ws.refining() {
            return false;
        }
        let more = self.ws.refine(REFINE_SLICE);
        if !more && (self.trace || std::env::var_os("MDVIEW_STARTUP_TRACE").is_some()) {
            let images = self.ws.image_stats();
            eprintln!(
                "mdview: page refined after {:.1} ms ({} image decodes, {} engine jobs)",
                self.started.elapsed().as_secs_f64() * 1000.0,
                images.decodes,
                images.engine_jobs
            );
        }
        more
    }

    fn draw(&mut self) {
        let (mut damage, mut times) = self.compose();
        if std::mem::take(&mut self.expose) {
            damage = vec![(0, 0, self.screen.width as i32, self.screen.height as i32)];
        }
        let t = Instant::now();
        if !damage.is_empty() {
            times.copy = self.present(&damage);
        }
        times.present = t.elapsed().saturating_sub(times.copy);
        if self.trace && !damage.is_empty() {
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            eprintln!(
                "mdview: frame layout {:.2} paint {:.2} compose {:.2} copy {:.2} present {:.2} ms, {} rows",
                ms(times.layout),
                ms(times.paint),
                ms(times.compose),
                ms(times.copy),
                ms(times.present),
                times.rows
            );
        }
        if self.steps.is_some() && !self.first && !self.ws.refining() {
            self.step_times.push(times);
        }
        let title = self.ws.title();
        if title != self.last_title {
            if let Some(w) = &self.window {
                w.set_title(&title);
            }
            self.last_title = title;
        }
        if self.first {
            self.first = false;
            stage(self.started, "first frame drawn and presented hidden");
            if let Some(w) = &self.window {
                w.set_visible(true);
            }
            stage(self.started, "window shown");
            // The first frame was presented to a hidden window, which shows nothing: send it
            // again now that there is a window to draw on.
            let whole = (0, 0, self.screen.width as i32, self.screen.height as i32);
            self.present(&[whole]);
            if std::env::var_os("MDVIEW_STARTUP_TRACE").is_some() || self.trace {
                let images = self.ws.image_stats();
                eprintln!(
                    "mdview: first frame after {:.1} ms ({} image decodes, {} engine jobs)",
                    self.started.elapsed().as_secs_f64() * 1000.0,
                    images.decodes,
                    images.engine_jobs
                );
            }
            #[cfg(windows)]
            if self.trace
                && let Some(w) = &self.window
            {
                eprintln!("mdview: {}", frame_report(w));
            }
        }
    }

    /// Copies the damaged rows to the window and presents them. Returns the copy time.
    fn present(&mut self, damage: &[(i32, i32, i32, i32)]) -> Duration {
        let Some(surface) = self.surface.as_mut() else {
            return Duration::ZERO;
        };
        let (w, h) = (self.screen.width, self.screen.height);
        // The surface is the content plus the shadow's margin on every side (0 without one:
        // everything below then is as it always was).
        let m = self.shadow.as_ref().map_or(0, |s| s.margin());
        let (sw, sh) = (w + 2 * m, h + 2 * m);
        if surface
            .resize(NonZeroU32::new(sw).unwrap(), NonZeroU32::new(sh).unwrap())
            .is_err()
        {
            return Duration::ZERO;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return Duration::ZERO;
        };
        let t = Instant::now();
        // The back buffer holds the previous frame only when its age is 1. Age 0 is undefined
        // contents; on Wayland softbuffer swaps two buffers (age 2 in steady state) and a
        // resized buffer keeps its age, so any other age would present stale regions. On Windows
        // the age is only 0 or 1, so this changes nothing there.
        let age = buffer.age();
        let mut whole = age != 1;
        // The margin is painted when this buffer's one is stale (a new size, scale, focus or
        // margin: both buffers, one frame each); then the compositor gets the whole surface.
        let margin_painted = self
            .shadow
            .as_mut()
            .is_some_and(|s| s.paint_frame(&mut buffer, sw, sh, age));
        let clamp = |d: &(i32, i32, i32, i32)| {
            let x0 = d.0.clamp(0, w as i32) as usize;
            let x1 = d.2.clamp(0, w as i32) as usize;
            let y0 = d.1.clamp(0, h as i32) as usize;
            let y1 = d.3.clamp(0, h as i32) as usize;
            (x0, y0, x1, y1)
        };
        let stride = w as usize;
        let (surface_stride, m) = (sw as usize, m as usize);
        // Both are BGRX in memory (the content's alpha byte is always 255: the blit ignores it,
        // the client-drawn margin needs it). A content row lands `m` rows down and `m` columns in.
        let copy = |buffer: &mut [u32], (x0, y0, x1, y1): (usize, usize, usize, usize)| {
            let bytes: &mut [u8] = bytemuck::cast_slice_mut(buffer);
            for y in y0..y1 {
                let (a, b) = ((y * stride + x0) * 4, (y * stride + x1) * 4);
                let to = (((y + m) * surface_stride) + x0 + m) * 4;
                bytes[to..to + (b - a)].copy_from_slice(&self.screen.pixels[a..b]);
            }
        };
        whole |= margin_painted;
        let rects: Vec<softbuffer::Rect> = if whole {
            // Content: the whole bitmap when the buffer is stale, else only the damage (the
            // buffer holds the last frame); the margin only changes with `margin_painted`.
            if age != 1 {
                copy(&mut buffer, (0, 0, w as usize, h as usize));
            } else {
                for d in damage {
                    copy(&mut buffer, clamp(d));
                }
            }
            vec![softbuffer::Rect {
                x: 0,
                y: 0,
                width: NonZeroU32::new(sw).unwrap(),
                height: NonZeroU32::new(sh).unwrap(),
            }]
        } else {
            damage
                .iter()
                .filter_map(|d| {
                    let r = clamp(d);
                    copy(&mut buffer, r);
                    Some(softbuffer::Rect {
                        x: (r.0 + m) as u32,
                        y: (r.1 + m) as u32,
                        width: NonZeroU32::new((r.2 - r.0) as u32)?,
                        height: NonZeroU32::new((r.3 - r.1) as u32)?,
                    })
                })
                .collect()
        };
        let copied = t.elapsed();
        let _ = buffer.present_with_damage(&rects);
        copied
    }

    // ---- hit testing ---------------------------------------------------------------------------

    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        // The palette and the settings dialog do their own hit testing.
        if self.ui.modal() {
            return None;
        }
        if self.ui.dialog.is_some() {
            return self
                .overlay_hits
                .iter()
                .rev()
                .find(|(_, r)| r.contains(x, y))
                .map(|(h, _)| *h);
        }
        for r in REGIONS {
            if let Some(h) = self.hits[r.index()]
                .iter()
                .rev()
                .find(|(_, rr)| rr.contains(x, y))
            {
                return Some(h.0);
            }
        }
        None
    }

    fn hit_rect(&self, hit: Hit) -> Option<Rect> {
        self.hits
            .iter()
            .chain(std::iter::once(&self.overlay_hits))
            .flat_map(|v| v.iter())
            .find(|(h, _)| *h == hit)
            .map(|(_, r)| *r)
    }

    fn edge_at(&self, x: f32, y: f32) -> Option<ResizeDirection> {
        if self.maximized || self.window.is_none() {
            return None;
        }
        let (w, h) = self.logical();
        mdshared_ui::window::edge_at(x, y, w, h)
    }

    /// The page viewport (logical) and a logical point in it as device px.
    fn page_point(&self, x: f32, y: f32) -> (Rect, f32, f32) {
        let vr = page::viewport(&self.ws, self.areas.page);
        (vr, (x - vr.x) * self.scale, (y - vr.y) * self.scale)
    }

    /// Brings the shadow in line with the window's scale, focus and the margin in force (0 when
    /// maximized, fullscreen or tiled). Cheap: it returns at once when none of them changed, and
    /// does nothing without a shadow. `focused` is the new focus when the event says it, otherwise
    /// the window's own. A change of the look is drawn by the next `present` (the margin is
    /// repainted for two frames); a focus change redraws through its own handler.
    fn update_shadow(&mut self, focused: Option<bool>) {
        let mut changed = false;
        if let (Some(shadow), Some(w)) = (self.shadow.as_mut(), self.window.as_ref()) {
            let active = focused.unwrap_or_else(|| w.has_focus());
            changed = shadow.update(w, w.scale_factor(), active);
        }
        if changed {
            // The margin's look changed: a frame must reach the compositor even if the content
            // has nothing to redraw.
            self.expose = true;
            self.wants_frame = true;
        }
    }

    /// The content size inside a surface of `w` x `h` device px: the whole of it without a margin.
    pub(super) fn content_size(&self, w: u32, h: u32) -> (u32, u32) {
        let (w, h) = match &self.shadow {
            Some(s) => s.content_size(w, h),
            None => (w, h),
        };
        (w.max(1), h.max(1))
    }

    fn set_cursor(&mut self, c: CursorIcon) {
        if c != self.cursor {
            self.cursor = c;
            if let Some(w) = &self.window {
                w.set_cursor(c);
            }
        }
    }
}

/// The part of the window a popup menu may use: all of it below the title row, so that the
/// caption buttons stay uncovered; a menu taller than this scrolls.
fn menu_room(title: Rect, w: f32, h: f32) -> Rect {
    let top = title.bottom();
    Rect::new(0.0, top, w, h - top)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

#[cfg(windows)]
fn hwnd(w: &Window) -> Option<isize> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

#[cfg(windows)]
fn style(w: &Window, dark: bool) {
    if let Some(h) = hwnd(w) {
        platform::style_window(h, dark);
    }
}

#[cfg(windows)]
fn frame_report(w: &Window) -> String {
    hwnd(w).map(platform::frame_report).unwrap_or_default()
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        stage(self.started, "resumed");
        let (w, h, max) = self.ws.window_size().unwrap_or((1280, 800, false));
        let mut attributes = Window::default_attributes()
            .with_title(self.ws.title())
            .with_decorations(false)
            .with_visible(false)
            .with_inner_size(LogicalSize::new(w.max(520) as f64, h.max(360) as f64))
            .with_maximized(max || self.args.maximize);
        // Back where it was, if a screen still shows that place: the title row's left end has
        // to be on one, or the window could not be reached.
        if let Some((x, y)) = self.ws.window_position() {
            let reachable = event_loop.available_monitors().any(|monitor| {
                let (origin, size) = (monitor.position(), monitor.size());
                let (px, py) = (x + 60, y + 16);
                px >= origin.x
                    && px < origin.x + size.width as i32
                    && py >= origin.y
                    && py < origin.y + size.height as i32
            });
            if reachable {
                attributes = attributes.with_position(PhysicalPosition::new(x, y));
            }
        }
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes.with_undecorated_shadow(true);
        }
        // Transparent and grown by the margin only where the client-drawn shadow will be used
        // (not on KDE, not on Windows): the attributes come back unchanged there.
        let attributes = mdshared_ui::shadow::Shadow::prepare(
            attributes,
            mdshared_ui::shadow::Mode::from_env(),
            event_loop,
        );
        // Linux: the Wayland app id, X11 class and window icon; a no-op on Windows.
        let attributes = mdshared_ui::window::identify(
            attributes,
            "mdview",
            Some(include_bytes!("../../../assets/mdview.png")),
        );
        let window = Rc::new(event_loop.create_window(attributes).expect("window"));
        stage(self.started, "window created");
        self.scale = window.scale_factor() as f32;
        self.maximized = window.is_maximized();
        self.shadow = mdshared_ui::shadow::Shadow::attach_mode(
            &window,
            mdshared_ui::shadow::Mode::from_env(),
        );
        // The minimum is a content size whatever the route.
        let min = LogicalSize::new(520.0, 360.0);
        match &self.shadow {
            Some(s) => s.set_min_content_size(&window, min),
            None => window.set_min_inner_size(Some(min)),
        }
        let size = window.inner_size();
        self.size = self.content_size(size.width, size.height);
        #[cfg(windows)]
        style(&window, self.pal.dark);
        let context = softbuffer::Context::new(window.clone()).expect("display");
        let mut surface = softbuffer::Surface::new(&context, window.clone()).expect("surface");
        // The client-drawn margin is transparent: Wayland buffers must carry alpha (the content
        // bitmap's alpha is always 255). A no-op elsewhere.
        if self.shadow.as_ref().is_some_and(|s| s.wants_alpha()) {
            surface.set_premultiplied_alpha(true);
        }
        self.surface = Some(surface);
        self.window = Some(window);
        stage(self.started, "surface");
        self.draw();
        stage(self.started, "first draw returned");
        // `--scroll` in a window too, so a check can start with the page scrolled.
        if let Some(px) = self.args.scroll {
            self.ws.scroll_to(px * self.scale);
            self.redraw();
        }
        // `--line` and `--place`, for the launch that opened this window.
        if let Some(line) = self.args.line {
            self.ws.scroll_to_line(line);
            self.redraw();
        }
        if let Some(rect) = self.args.place {
            self.place(rect);
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, wake: Wake) {
        self.woken(wake);
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.exit = true,
            WindowEvent::RedrawRequested => {
                self.wants_frame = false;
                if !std::mem::take(&mut self.requested) {
                    self.expose = true;
                }
                self.draw();
            }
            WindowEvent::Moved(position) => {
                // The restored window's place only: maximized and minimized windows report
                // where the system put them.
                if let Some(w) = &self.window
                    && !w.is_maximized()
                    && w.is_minimized() != Some(true)
                    && w.fullscreen().is_none()
                {
                    self.ws.set_window_position(position.x, position.y);
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(w) = &self.window {
                    let max = w.is_maximized();
                    if max != self.maximized && self.trace {
                        eprintln!("mdview: maximized {max}: {}", {
                            #[cfg(windows)]
                            {
                                frame_report(w)
                            }
                            #[cfg(not(windows))]
                            {
                                String::new()
                            }
                        });
                    }
                    self.maximized = max;
                    self.update_shadow(None);
                }
                // The window reports the surface; the app lives in the content inside the margin
                // (the margin in force was just re-read by `update_shadow`).
                self.size = self.content_size(size.width, size.height);
                self.size_save_due = Some(Instant::now() + SIZE_SAVE_DELAY);
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor as f32;
                self.update_shadow(None);
                if let Some(w) = &self.window {
                    let s = w.inner_size();
                    self.size = self.content_size(s.width, s.height);
                }
                self.full = true;
                self.redraw();
            }
            WindowEvent::ThemeChanged(t) => {
                self.system_dark = t == WinTheme::Dark;
                self.ws.set_system_dark(self.system_dark);
                self.full = true;
                self.redraw();
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
            }
            WindowEvent::Focused(focused) => {
                self.update_shadow(Some(focused));
                if focused {
                    // The one place the system's "animation effects" setting is read again.
                    self.ws.set_system_animations(platform::shows_animations());
                }
                if focused && self.ui.tree.refresh() {
                    self.redraw();
                }
                if !focused {
                    self.ui.caret_on = true;
                    self.ui.tip = None;
                }
                self.redraw();
            }
            WindowEvent::ModifiersChanged(m) => self.mods = Mods::from_winit(m.state()),
            WindowEvent::CursorMoved {
                position: PhysicalPosition { x, y },
                ..
            } => {
                // Surface coordinates to content coordinates (the margin is 0 without the
                // client-drawn shadow): negative in the outer resize band.
                let (x, y) = match &self.shadow {
                    Some(s) => s.to_content(x, y),
                    None => (x, y),
                };
                self.pointer_moved(x as f32 / self.scale, y as f32 / self.scale);
            }
            WindowEvent::CursorLeft { .. } => {
                if self.ui.drag.is_none() {
                    self.ui.hover = None;
                    self.ui.tip = None;
                    self.ui.link_hint = None;
                    self.redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => self.left_press(),
                (ElementState::Released, MouseButton::Left) => self.left_release(),
                (ElementState::Pressed, MouseButton::Right) => self.right_press(),
                (ElementState::Pressed, MouseButton::Middle) => self.middle_press(),
                (ElementState::Pressed, MouseButton::Back) => self.travel(true),
                (ElementState::Pressed, MouseButton::Forward) => self.travel(false),
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => self.wheel(delta),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                if matches!(event.logical_key, Key::Named(NamedKey::Alt)) {
                    return;
                }
                self.key(&event);
            }
            WindowEvent::DroppedFile(path) => self.dropped(path),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.exit {
            self.save_size();
            if let Err(e) = self.ws.save_session() {
                eprintln!("mdview: could not save the session: {e}");
            }
            event_loop.exit();
            return;
        }
        if self.steps.is_some() && !self.first {
            // The steps measure scrolling a refined page, as a reader who scrolls after the
            // first second sees it; refinement itself runs first, in its usual slices.
            if self.refine_slice() {
                event_loop.set_control_flow(ControlFlow::Poll);
                return;
            }
            self.step();
            if self.exit {
                return;
            }
            event_loop.set_control_flow(ControlFlow::Poll);
            return;
        }
        if self.ws.take_watch_changed()
            && let Some(w) = self.watcher.as_mut()
        {
            w.set_files(
                self.ws
                    .watched_paths()
                    .chain(self.ws.note_files(WATCHED_NOTES)),
            );
        }
        self.watch_tree();
        let refining = self.refine_slice();
        let next = self.timers(Instant::now());
        if refining {
            // More to measure: come straight back after any events that are waiting.
            event_loop.set_control_flow(ControlFlow::Poll);
            if let Some(w) = &self.window {
                self.requested = true;
                w.request_redraw();
            }
            return;
        }
        if self.wants_frame {
            self.wants_frame = false;
            if let Some(w) = &self.window {
                self.requested = true;
                w.request_redraw();
            }
        }
        event_loop.set_control_flow(match next {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }
}

/// Renders one state to a PNG without a window (`--shot`).
fn shot(mut app: App, path: &Path) -> i32 {
    let (lw, lh) = app.args.size.unwrap_or((1280, 800));
    app.scale = app.args.scale.unwrap_or_else(platform::system_scale);
    app.size = (
        (lw as f32 * app.scale).round() as u32,
        (lh as f32 * app.scale).round() as u32,
    );
    app.compose();
    if let Some(px) = app.args.scroll {
        app.ws.scroll_to(px * app.scale);
    }
    app.compose();
    let page = app.areas.page;
    match app.args.menu {
        Some(ShotMenu::Main) => app.open_popup(PopupKind::Main),
        Some(ShotMenu::MainEnd) => {
            app.open_popup(PopupKind::Main);
            if let Some(p) = app.ui.popup.as_mut() {
                p.menu.scroll_to_end();
            }
        }
        Some(ShotMenu::Recent) => app.open_popup(PopupKind::Recent),
        Some(ShotMenu::Cascade) => {
            app.open_popup(PopupKind::Main);
            app.compose();
            app.open_popup(PopupKind::Recent);
        }
        Some(ShotMenu::Tab) => {
            if let Some(r) = app.hit_rect(Hit::Tab(0)) {
                app.ui.mouse = (r.x + 30.0, r.cy());
                app.right_press();
            }
        }
        Some(ShotMenu::Page) => {
            app.ui.mouse = (page.x + page.w * 0.3, page.y + 160.0);
            app.right_press();
        }
        Some(ShotMenu::Find) => {
            if let Some(r) = app.hit_rect(Hit::Find) {
                app.ui.mouse = (r.x + 60.0, r.cy());
                app.right_press();
            }
        }
        None => {}
    }
    match app.args.dialog {
        Some(ShotDialog::About) => app.open_dialog(DialogKind::About),
        Some(ShotDialog::Shortcuts) => app.open_dialog(DialogKind::Shortcuts),
        Some(ShotDialog::Recent) => app.open_dialog(DialogKind::Recent),
        Some(ShotDialog::Edit) => app.open_dialog(DialogKind::AllowEdit),
        Some(ShotDialog::Settings) => app.open_settings(),
        None => {}
    }
    if let Some(query) = app.args.palette.clone() {
        app.open_palette(&query);
    }
    if app.ui.dialog.is_some() || app.ui.popup.is_some() || app.ui.modal() {
        app.compose();
    }
    if let Some((x, y)) = app.args.hover {
        app.pointer_moved(x, y);
        if let Some(t) = app.ui.tip.as_mut() {
            t.shown = true;
        }
        if let Some(p) = app.ui.popup.as_mut() {
            p.menu.pointer(x, y);
            if let Some(s) = p.sub.as_mut() {
                s.pointer(x, y);
            }
        }
    }
    app.full = true;
    app.compose();
    match app.screen.write_png(path) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("mdview: could not write {}: {e}", path.display());
            1
        }
    }
}

/// `MDVIEW_STARTUP_TRACE`: prints how long after launch each stage of startup finished.
fn stage(started: Instant, what: &str) {
    if std::env::var_os("MDVIEW_STARTUP_TRACE").is_some() {
        eprintln!(
            "mdview: {:7.1} ms  {what}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// `--check-update`: looks for a newer version as the application would and prints the answer.
fn check_update() -> i32 {
    let current = updater::current_version();
    eprintln!("mdview {current}; manifest {}", updater::manifest_url());
    match updater::check(&updater::fetch) {
        Ok(Some(offer)) => {
            eprintln!("mdview {} is available", offer.version);
            eprintln!("  {}", offer.app.url);
            if let Some(engine) = &offer.engine {
                eprintln!("  {}", engine.url);
            }
            0
        }
        Ok(None) => {
            eprintln!("nothing newer");
            0
        }
        Err(e) => {
            eprintln!("could not look: {e}");
            1
        }
    }
}

pub fn run() -> i32 {
    let started = Instant::now();
    let args = Args::parse(std::env::args().skip(1));
    if args.check_update {
        return check_update();
    }
    let explicit = std::env::var_os("MDVIEW_SETTINGS").map(PathBuf::from);
    // A shot never touches the user's session; it may use a scratch one named by MDVIEW_SETTINGS.
    let session = match (args.no_session, args.shot.is_some()) {
        (true, _) => None,
        (false, true) => explicit,
        (false, false) => explicit.or_else(mdview_core::session::default_settings_path),
    };
    if let Some(settings) = session.as_ref().filter(|_| args.shot.is_none()) {
        // A window is already open: hand it the files and quit, before any window work. A launch
        // that asks for a line, a place or a follow hands each file over with that.
        let requests = args.requests();
        let handed = if requests.is_empty() {
            mdview_core::single::forward(settings, &args.files)
        } else {
            requests
                .iter()
                .all(|r| mdview_core::single::forward_request(settings, r))
        };
        if handed {
            return 0;
        }
    }
    // A live update is for a window that shows the file already: with none, there is nothing to do.
    if args.follow && args.shot.is_none() {
        return 0;
    }
    stage(started, "single-instance check");
    let ws = Workspace::new(session.clone());
    stage(started, "workspace (session, file read, parse)");
    let mut app = App::new(args, ws, session);
    app.started = started;
    app.setup();
    stage(started, "app setup");
    if let Some(path) = app.args.shot.clone() {
        return shot(app, &path);
    }
    // Linux AppImage: the launcher entry and icon; does nothing elsewhere.
    mdshared_ui::desktop_entry::ensure(mdshared_ui::desktop_entry::Spec {
        name: "mdview",
        display_name: "mdview",
        comment: "Markdown viewer",
        icon_png: include_bytes!("../../../assets/mdview.png"),
        mime_types: mdshared_ui::desktop_entry::VIEWER_MIME_TYPES,
        categories: &["Utility", "Viewer", "Office"],
    });
    let event_loop = match EventLoop::<Wake>::with_user_event().build() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("mdview: no event loop: {e}");
            return 1;
        }
    };
    stage(started, "event loop");
    let proxy = event_loop.create_proxy();
    if let Some(settings) = &app.session {
        let p = proxy.clone();
        app.incoming = mdview_core::single::listen_requests_waking(settings, move || {
            let _ = p.send_event(Wake::Instance);
        });
    }
    let p = proxy.clone();
    app.watcher = FileWatcher::new(move || {
        let _ = p.send_event(Wake::Files);
    });
    stage(started, "listener and file watcher");
    app.ws.set_subpixel(crate::platform::subpixel_order());
    let p = proxy.clone();
    app.ws.set_remote_wake(move || {
        let _ = p.send_event(Wake::Images);
    });
    let p = proxy.clone();
    app.ws.set_notes_wake(move || {
        let _ = p.send_event(Wake::Notes);
    });
    // Animated images play only in a window: `--shot` renders above never get here.
    let p = proxy.clone();
    app.ws.set_system_animations(platform::shows_animations());
    app.ws.set_animation_wake(move || {
        let _ = p.send_event(Wake::Animation);
    });
    app.proxy = Some(proxy);
    app.updates_at_launch();
    stage(started, "ready to run");
    let _ = event_loop.run_app(&mut app);
    0
}
