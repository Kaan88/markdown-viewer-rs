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
//! unloading idle tabs, saving the window size after a resize).

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
use crate::workspace::{Click, LinkOutcome, ThemeChoice, Workspace};
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
#[cfg(test)]
mod tests;

/// How close to the window edge a press resizes, in logical px, and how far along an edge from a
/// corner still resizes diagonally. The window has no frame outside its pixels to grab, so the
/// band has to be wide enough to hit without aiming.
const EDGE: f32 = 8.0;
const CORNER: f32 = 20.0;
const TIP_DELAY: Duration = Duration::from_millis(500);
const AUTOSCROLL_TICK: Duration = Duration::from_millis(16);
const IDLE_CHECK: Duration = Duration::from_secs(30);
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

pub enum Wake {
    /// The single-instance listener queued paths.
    Instance,
    /// The file watcher saw a change.
    Files,
    /// A remote image finished downloading.
    Images,
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
    incoming: Option<Receiver<PathBuf>>,
    watcher: Option<FileWatcher>,
    session: Option<PathBuf>,
    screen: Bitmap,
    gfx: Gfx,
    pal: Palette,
    system_dark: bool,
    scale: f32,
    /// Device px.
    size: (u32, u32),
    maximized: bool,
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
    toast_until: Option<Instant>,
    size_save_due: Option<Instant>,
    idle_due: Option<Instant>,
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
            incoming: None,
            watcher: None,
            session,
            screen: Bitmap::new(1, 1),
            gfx: Gfx::new(),
            pal: theme::DARK,
            system_dark,
            scale: 1.0,
            size: (1, 1),
            maximized: false,
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
            toast_until: None,
            size_save_due: None,
            idle_due: None,
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
        if let Some(q) = &a.find {
            self.ui.find.set_text(q);
            self.ws.find_set(q);
        }
        self.ui.focus_mode = a.focus_mode;
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
                let missing: Vec<bool> = self.ws.tabs().iter().map(|t| t.is_missing()).collect();
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
                sig((
                    common,
                    self.ws.active(),
                    titles,
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
                sig((common, self.ws.active(), missing, drag, recents))
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
        let current = self.ws.current_heading();
        if current != self.last_heading {
            self.last_heading = current;
            if let Some(i) = current {
                panes::reveal_outline_row(&mut self.ui, i);
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
        if self.ws.take_dirty() {
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
                popup.menu.place(&mut p, w, h);
            }
        }
        self.toast_until = self.ws.status(now).and(self.ws.status_expires());
        let mut bounds: Vec<Rect> = bubbles.iter().map(Bubble::bounds).collect();
        if let Some(popup) = &self.ui.popup {
            bounds.push(popup.menu.rect().inset(-20.0));
        }
        if self.ui.dialog.is_some() {
            bounds.push(Rect::new(0.0, 0.0, w, h));
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
            self.ui
                .popup
                .as_ref()
                .map(|p| (p.kind, p.menu.hover, p.menu.items.len())),
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
        ));
        let overlay_changed = osig != self.overlay_sig;
        self.overlay_sig = osig;
        let mut under: Vec<Rect> = bounds.clone();
        under.extend(self.overlay_bounds.iter().copied());
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
        {
            let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
            for r in REGIONS {
                if !dirty[r.index()] {
                    continue;
                }
                let Some(rect) = self.areas.region(r) else {
                    continue;
                };
                damage.push(p.dev_rect(rect));
                let hits = &mut self.hits[r.index()];
                hits.clear();
                let old = p.clip(rect);
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
                    popup.menu.draw(c.p, &pal, w, h);
                }
                for b in &bubbles {
                    b.draw(c.p, &pal);
                }
            }
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
        if surface
            .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
            .is_err()
        {
            return Duration::ZERO;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return Duration::ZERO;
        };
        let t = Instant::now();
        let whole = buffer.age() == 0;
        let clamp = |d: &(i32, i32, i32, i32)| {
            let x0 = d.0.clamp(0, w as i32) as usize;
            let x1 = d.2.clamp(0, w as i32) as usize;
            let y0 = d.1.clamp(0, h as i32) as usize;
            let y1 = d.3.clamp(0, h as i32) as usize;
            (x0, y0, x1, y1)
        };
        let stride = w as usize;
        // Both are BGRX in memory; the alpha byte is ignored by the blit to the window.
        let copy = |buffer: &mut [u32], (x0, y0, x1, y1): (usize, usize, usize, usize)| {
            let bytes: &mut [u8] = bytemuck::cast_slice_mut(buffer);
            for y in y0..y1 {
                let (a, b) = ((y * stride + x0) * 4, (y * stride + x1) * 4);
                bytes[a..b].copy_from_slice(&self.screen.pixels[a..b]);
            }
        };
        let rects: Vec<softbuffer::Rect> = if whole {
            copy(&mut buffer, (0, 0, w as usize, h as usize));
            vec![softbuffer::Rect {
                x: 0,
                y: 0,
                width: NonZeroU32::new(w).unwrap(),
                height: NonZeroU32::new(h).unwrap(),
            }]
        } else {
            damage
                .iter()
                .filter_map(|d| {
                    let r = clamp(d);
                    copy(&mut buffer, r);
                    Some(softbuffer::Rect {
                        x: r.0 as u32,
                        y: r.1 as u32,
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
        if !(x < EDGE || x >= w - EDGE || y < EDGE || y >= h - EDGE) {
            return None;
        }
        // On an edge: the corner band along it turns the resize diagonal.
        let (l, r, t, b) = (x < CORNER, x >= w - CORNER, y < CORNER, y >= h - CORNER);
        let (l, r) = (l && (x < EDGE || t || b), r && (x >= w - EDGE || t || b));
        let (t, b) = (t && (y < EDGE || l || r), b && (y >= h - EDGE || l || r));
        Some(match (l, r, t, b) {
            (true, _, true, _) => ResizeDirection::NorthWest,
            (_, true, true, _) => ResizeDirection::NorthEast,
            (true, _, _, true) => ResizeDirection::SouthWest,
            (_, true, _, true) => ResizeDirection::SouthEast,
            (true, ..) => ResizeDirection::West,
            (_, true, ..) => ResizeDirection::East,
            (_, _, true, _) => ResizeDirection::North,
            (_, _, _, true) => ResizeDirection::South,
            _ => return None,
        })
    }

    /// The page viewport (logical) and a logical point in it as device px.
    fn page_point(&self, x: f32, y: f32) -> (Rect, f32, f32) {
        let vr = page::viewport(&self.ws, self.areas.page);
        (vr, (x - vr.x) * self.scale, (y - vr.y) * self.scale)
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
            .with_min_inner_size(LogicalSize::new(520.0, 360.0))
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
        let window = Rc::new(event_loop.create_window(attributes).expect("window"));
        stage(self.started, "window created");
        self.scale = window.scale_factor() as f32;
        let size = window.inner_size();
        self.size = (size.width.max(1), size.height.max(1));
        self.maximized = window.is_maximized();
        #[cfg(windows)]
        style(&window, self.pal.dark);
        let context = softbuffer::Context::new(window.clone()).expect("display");
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).expect("surface"));
        self.window = Some(window);
        stage(self.started, "surface");
        self.draw();
        stage(self.started, "first draw returned");
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
                self.size = (size.width.max(1), size.height.max(1));
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
                }
                self.size_save_due = Some(Instant::now() + SIZE_SAVE_DELAY);
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor as f32;
                self.full = true;
                self.redraw();
            }
            WindowEvent::ThemeChanged(t) => {
                self.system_dark = t == WinTheme::Dark;
                self.ws.set_system_dark(self.system_dark);
                self.full = true;
                self.redraw();
            }
            WindowEvent::Focused(focused) => {
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
            w.set_files(self.ws.watched_paths());
        }
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
        Some(ShotMenu::Recent) => app.open_popup(PopupKind::Recent),
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
        None => {}
    }
    match app.args.dialog {
        Some(ShotDialog::About) => app.open_dialog(DialogKind::About),
        Some(ShotDialog::Shortcuts) => app.open_dialog(DialogKind::Shortcuts),
        Some(ShotDialog::Recent) => app.open_dialog(DialogKind::Recent),
        None => {}
    }
    if app.ui.dialog.is_some() || app.ui.popup.is_some() {
        app.compose();
    }
    if let Some((x, y)) = app.args.hover {
        app.pointer_moved(x, y);
        if let Some(t) = app.ui.tip.as_mut() {
            t.shown = true;
        }
        if let Some(p) = app.ui.popup.as_mut() {
            p.menu.pointer(x, y);
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

pub fn run() -> i32 {
    let started = Instant::now();
    let args = Args::parse(std::env::args().skip(1));
    let explicit = std::env::var_os("MDVIEW_SETTINGS").map(PathBuf::from);
    // A shot never touches the user's session; it may use a scratch one named by MDVIEW_SETTINGS.
    let session = match (args.no_session, args.shot.is_some()) {
        (true, _) => None,
        (false, true) => explicit,
        (false, false) => explicit.or_else(mdview_core::session::default_settings_path),
    };
    if let Some(settings) = session.as_ref().filter(|_| args.shot.is_none()) {
        // A window is already open: hand it the files and quit, before any window work.
        if mdview_core::single::forward(settings, &args.files) {
            return 0;
        }
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
        app.incoming = mdview_core::single::listen_waking(settings, move || {
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
    app.proxy = Some(proxy);
    stage(started, "ready to run");
    let _ = event_loop.run_app(&mut app);
    0
}
