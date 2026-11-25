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
use mdshared_ui::anim::{Animator, Id};
use mdshared_ui::corners::Corners;
use mdshared_ui::menu::Entrance;
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
use winit::event::{
    ElementState, KeyEvent, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, ResizeDirection, Theme as WinTheme, Window, WindowId};

mod cards;
mod commands;
mod crash;
pub use crash::crash_dir;
mod export;
#[cfg(test)]
mod frost_tests;
mod input;
mod library;
#[cfg(test)]
mod library_tests;
mod modal;
#[cfg(test)]
mod motion_tests;
mod slides;
#[cfg(test)]
mod tests;

/// Over a vertical scrollbar at the window's right edge the east resize band is only this wide
/// (px), so the thumb, which starts 3 px in, can be grabbed.
const SCROLLBAR_EDGE: f32 = 3.0;
/// The editor, as the hand-off names it.
const EDITOR: &str = "mdedit";
/// How long a closing window waits for the editor to be told of the tabs it asked for: the
/// process ends after it, and its listener with it.
const EDITOR_CLOSE_WAIT: Duration = Duration::from_millis(500);
const TIP_DELAY: Duration = Duration::from_millis(500);
const AUTOSCROLL_TICK: Duration = Duration::from_millis(16);
const IDLE_CHECK: Duration = Duration::from_secs(30);
/// While a tab shows the editor's text: how often to ask whether the editor still answers.
const SISTER_CHECK: Duration = Duration::from_secs(5);
/// While animated images are on the page and the window may be covered: how often to look
/// again, and how long an answer is kept. Nothing runs without an animated image.
const COVER_CHECK: Duration = Duration::from_secs(1);
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
/// The most folders below the open one that are followed, for a note or a folder made in them.
const WATCHED_NOTE_FOLDERS: usize = 1_000;

/// How long a file watcher wake waits for the rest of a burst of changes.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

pub enum Wake {
    /// The single-instance listener queued paths.
    Instance,
    /// macOS asked the application to open files (Finder, Open With, the Dock icon).
    Open(Vec<PathBuf>),
    /// An item of the macOS menu bar the application answers.
    Menu(mdshared_ui::menu_bar::AppCommand),
    /// The file watcher saw a change.
    Files,
    /// A remote image finished downloading.
    Images,
    /// The note index (wiki-links, backlinks) was built or refreshed.
    Notes,
    /// Find in folder has results waiting.
    FolderSearch,
    /// The animation thread has a frame ready.
    Animation,
    /// The system's wish for less motion changed (`platform::watch_motion_reduced`).
    Motion,
    /// A helper thread has read a file for a hover card.
    Preview,
    /// An updater thread has an answer.
    Update(updater::Event),
    /// The helper thread asked whether a window of mdedit still answers: true if one does.
    Sister(bool),
    /// A file or folder dialog, which runs on a helper thread off Windows, was closed.
    #[cfg(not(windows))]
    Dialog(DialogAnswer),
    /// Export as HTML or Print made progress on its helper thread.
    Export(export::Progress),
    /// The helper thread found where the notes library is, as the window started.
    Library(crate::notes::Found),
}

/// What another thread wakes the loop with: winit's proxy in a window; in a test, a channel the
/// test reads, so that the window's own start (`loop_ready`) is driven as `run` makes it (review
/// of the audit 4 fixes, W2).
#[derive(Clone)]
pub(crate) enum LoopProxy {
    Loop(EventLoopProxy<Wake>),
    #[cfg(test)]
    Test(std::sync::mpsc::Sender<Wake>),
}

impl LoopProxy {
    /// Sends `wake` to the loop: false when the loop has ended.
    pub(crate) fn send_event(&self, wake: Wake) -> bool {
        match self {
            LoopProxy::Loop(proxy) => proxy.send_event(wake).is_ok(),
            #[cfg(test)]
            LoopProxy::Test(tx) => tx.send(wake).is_ok(),
        }
    }
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

/// A tab the editor asked this window to show ([`App::editor_asked`]).
#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorTab {
    /// The file as the editor named it, which its `closed` names back; empty for a document
    /// that has no file.
    path: PathBuf,
    /// The editor's name for a document that has no file.
    doc: Option<String>,
    /// `path` resolved when the editor asked, which is what the tabs are compared with on every
    /// turn of the loop: nothing is asked of the disk there (audit 4, E1).
    file: PathBuf,
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
    /// Where the window was before the editor's "Preview side by side" placed it, while it is
    /// still there.
    tiling: mdshared_ui::place::Tiling,
    /// What the window was placed for: the file (resolved when it was placed, as it is looked
    /// for on every turn of the loop), or the editor's name for a document that has no file.
    /// When no tab shows it any more the arrangement is over.
    placed_for: Option<(std::path::PathBuf, Option<String>)>,
    /// The files, and the editor's names of documents that have no file, that the editor asked
    /// this window to show, while a tab shows each. When one is closed the editor is told
    /// (`closed`), so that its preview of it ends.
    editor_shown: Vec<EditorTab>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    proxy: Option<LoopProxy>,
    /// The settings file of mdedit's that names the notes library, decided from the environment
    /// as the window is made and read only on the helper thread (`look_for_library`); `None`
    /// when this run has no library (a scratch or session-less run without `MDEDIT_SETTINGS`).
    library_settings: Option<PathBuf>,
    /// A file or folder dialog is open (off Windows, where it runs beside the window); a second
    /// request is ignored until its answer arrives.
    #[cfg(not(windows))]
    dialog_open: bool,
    /// An export or print is under way on its helper thread; a second request waits for it.
    exporting: bool,
    incoming: Option<Receiver<Request>>,
    watcher: Option<FileWatcher>,
    /// The folders the watcher follows for the Files pane: the tree's root and expanded folders
    /// while the pane is open.
    tree_dirs: Vec<PathBuf>,
    session: Option<PathBuf>,
    /// What a crash report would say mdview was doing, as last set.
    crash_key: crash::CrashKey,
    screen: Bitmap,
    gfx: Gfx,
    pal: Palette,
    system_dark: bool,
    scale: f32,
    /// Device px.
    size: (u32, u32),
    maximized: bool,
    /// The maximize button's rectangle last given to the snap-layouts hook (`None` is a value
    /// too: "not answering"); the outer `None` is "nothing given yet".
    snap_sent: Option<Option<Rect>>,
    /// The window's drop shadow (Linux: KWin's protocol, or the client-drawn margin where the
    /// compositor has none); `None` everywhere else. With the margin, `size` is the CONTENT size
    /// (the surface minus the margin on every side) and `present` puts the content inside it.
    /// Dropping it would remove the KWin shadow, so it lives as long as the window.
    shadow: Option<mdshared_ui::shadow::Shadow>,
    /// The "Window backdrop" setting at work (Windows 11): idle unless the setting is on.
    frost: crate::frost::Frost,
    /// Whether the window has the keyboard; a window that never heard otherwise has it.
    focused: bool,
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
    /// The file to start once the window is closed: set by a Restart that was carried out.
    restart: Option<PathBuf>,
    mods: Mods,
    /// macOS: the left press held now was made with the Control key, which a Mac takes for a
    /// right click; its release is not a left release.
    control_click: bool,
    /// A trackpad pinch on its way to the next zoom step.
    pinch: mdshared_ui::window::PinchZoom,
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
    /// When the watcher's wake is looked at: a short wait that folds a burst of changes into one check.
    watch_due: Option<Instant>,
    /// A helper thread is asking now.
    sister_asking: bool,
    /// Rows of the page viewport (device px, first and one past the last) where an animated
    /// image went to its next frame since the last frame was drawn.
    anim_rows: Option<(u32, u32)>,
    /// A partial page draw found more changed than its rows; the page is drawn whole next.
    page_again: bool,
    /// The system says the window is completely covered.
    occluded: bool,
    /// An animated image was on the page at the last look: whether the window is covered is
    /// worth asking (Windows never reports it).
    anim_seen: bool,
    /// The last answer to "is the window covered", and when it was asked.
    covered: Option<(Instant, bool)>,
    /// `--scroll-steps`: steps left and the frame times taken.
    steps: Option<u32>,
    step_times: Vec<FrameTimes>,
    trace: bool,
    /// Ctrl+wheel and trackpad deltas below one notch, kept until they add up.
    wheel_rest: f32,
    /// The window's own motion (`crate::motion`): what is on its way, and when the next frame
    /// of it is due. `None` once nothing moves, so an idle window keeps no timer.
    anim: Animator,
    anim_due: Option<Instant>,
    /// A run that must not depend on time (a shot, the scroll steps, a test): nothing moves.
    still: bool,
    /// The frames' time when it is not the real one: a test's, or a shot of a menu mid-fade.
    frame_clock: Option<Instant>,
    /// How present the open menu and its submenu are this frame, 0 to 1.
    menu_t: (f32, f32),
    /// The system's wish for less motion is watched (from the first frame on).
    motion_watched: bool,
    /// The toasts on screen and leaving (`crate::motion::toasts`), the status line they were
    /// last brought up to date with, and this frame's: each one's key, text, presence and lift.
    toasts: mdshared_ui::anim::stack::Stack<String>,
    toast_seen: Option<(String, Instant, bool)>,
    toast_frame: Vec<(u64, String, f32, f32)>,
    /// Which stack the toasts are: a message that takes the others' place starts a new one, so
    /// the old go at once and the new enters as new.
    toast_gen: u64,
    /// A shot part way through a motion: it moves whatever the setting and the system say.
    forced_motion: bool,
}

impl App {
    fn new(args: Args, ws: Workspace, session: Option<PathBuf>) -> App {
        // Which settings file names the library is a matter of the environment alone: nothing
        // on disk is read here.
        let library_settings = crate::notes::library_settings(session.as_deref());
        App::with_library_settings(args, ws, session, library_settings)
    }

    /// [`new`](Self::new), with the settings file of mdedit's that names the library given
    /// rather than taken from the environment: a test's window has its own, and no other test
    /// sees it, as a process-wide `MDEDIT_SETTINGS` would be seen (review of the audit 4 fixes,
    /// T5).
    fn with_library_settings(
        args: Args,
        ws: Workspace,
        session: Option<PathBuf>,
        library_settings: Option<PathBuf>,
    ) -> App {
        let system_dark = platform::system_is_dark();
        let trace = std::env::var_os("MDVIEW_TRACE").is_some();
        let args_still = args.shot.is_some() || args.scroll_steps.is_some() || cfg!(test);
        let mut ui = Ui::new();
        // A shot has no loop to wait in, so it looks for the library now; a window looks on a
        // helper thread once it runs (`look_for_library`), so neither mdedit's settings file nor
        // the folder it names is on the way to the first frame (audit 4, E12).
        if args.shot.is_some() {
            let dir = crate::notes::library_dir(library_settings.as_deref());
            ui.notes = crate::notes::Notes::new(dir);
        }
        App {
            started: Instant::now(),
            steps: args.scroll_steps,
            args,
            ws,
            ui,
            window: None,
            tiling: Default::default(),
            placed_for: None,
            editor_shown: Vec::new(),
            surface: None,
            proxy: None,
            library_settings,
            #[cfg(not(windows))]
            dialog_open: false,
            exporting: false,
            incoming: None,
            watcher: None,
            watch_due: None,
            tree_dirs: Vec::new(),
            session,
            crash_key: crash::CrashKey::default(),
            screen: Bitmap::new(1, 1),
            gfx: Gfx::new(),
            pal: theme::DARK,
            system_dark,
            scale: 1.0,
            size: (1, 1),
            maximized: false,
            snap_sent: None,
            shadow: None,
            frost: crate::frost::Frost::default(),
            focused: true,
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
            restart: None,
            mods: Mods::default(),
            control_click: false,
            pinch: Default::default(),
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
            anim_seen: false,
            covered: None,
            step_times: Vec::new(),
            trace,
            wheel_rest: 0.0,
            anim: Animator::new(),
            anim_due: None,
            still: args_still,
            frame_clock: None,
            menu_t: (1.0, 1.0),
            motion_watched: false,
            toasts: crate::motion::toasts(),
            toast_seen: None,
            toast_frame: Vec::new(),
            toast_gen: 0,
            forced_motion: false,
        }
    }

    /// Applies the arguments that shape the first state: theme, zoom, folder, panes, files.
    fn setup(&mut self) {
        let a = self.args.clone();
        self.ws.set_system_dark(self.system_dark);
        self.ws.set_library_offered(self.ui.notes.available);
        if a.notes_face {
            self.ui.notes.show(true);
        }
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
        if let Some(n) = a.present {
            self.present_begin();
            if let Some(show) = self.ui.present.as_mut() {
                show.go(n - 1);
                if a.present_select {
                    show.select_all();
                }
                if a.present_overview {
                    show.open_overview();
                }
            }
        }
        if let Some(stage) = &a.update_bar {
            self.ws.preview_update(stage);
        }
        if let Some(backdrop) = a.backdrop {
            self.ws.set_backdrop(backdrop);
        }
        if a.load_remote {
            self.ws.allow_remote(false);
        }
        self.ws.clear_status();
        self.sync_theme();
    }

    /// Ends the app: the window's size and the session are saved and the loop is told to stop.
    /// Once only, however often it is asked.
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if event_loop.exiting() {
            return;
        }
        self.closing();
        event_loop.exit();
    }

    /// What the window does as it closes, before the loop stops: its size and the session are
    /// saved, and the editor is told that every tab it asked for is closed with it (its
    /// previews end now, not when it next finds this process gone).
    fn closing(&mut self) {
        if !self.leave_side_by_side() {
            self.save_size();
        }
        if let Err(e) = self.ws.save_session() {
            eprintln!("mdview: could not save the session: {e}");
        }
        self.tell_editor_all_closed(EDITOR_CLOSE_WAIT);
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
                style(w, dark, Corners::from_setting(self.ws.corners()));
            }
        }
    }

    /// The "Window backdrop" setting: asks the system when the setting or the focus changed and
    /// repaints everything when the chrome turns frosted or flat. One comparison when it is off.
    fn sync_frost(&mut self) {
        let want = self.ws.backdrop();
        if !self.frost.concerns(want) {
            return;
        }
        // Without a window (`--shot`) there is nothing to show a backdrop behind.
        let Some(window) = self.window.clone() else {
            return;
        };
        #[cfg(windows)]
        let mut sys = mdshared_ui::frost::WindowSystem(hwnd(&window));
        // KDE Plasma (Wayland) blurs behind the window by a request on its own connection; other
        // desktops answer "not available".
        #[cfg(not(windows))]
        let mut sys = mdshared_ui::frost::BlurSystem(&window);
        let changed = self.frost.sync(want, self.focused, &self.pal, &mut sys);
        // The window's pixels carry alpha while the chrome is frosted (the client-drawn margin of
        // the shadow needs it too), or the compositor blurs behind an opaque rectangle. Patched
        // softbuffer; Windows does not use it.
        #[cfg(not(windows))]
        if let Some(surface) = self.surface.as_mut() {
            surface.set_premultiplied_alpha(
                self.frost.wants_alpha() || self.shadow.as_ref().is_some_and(|s| s.wants_alpha()),
            );
        }
        if changed {
            // Every mark goes: the regions put theirs back when they are drawn frosted.
            for px in self.screen.pixels.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
            self.full = true;
            self.expose = true;
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
                (
                    ui.find_focused && ui.caret_on,
                    ui.find.preedit().map(|p| (&p.text, p.cursor)),
                ),
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
                    ui.notes.sig(),
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
                    ui.outline_focus,
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
                let update = self.ws.update_bar().map(|stage| {
                    // The Restart button comes and goes with the file there is to start.
                    (
                        format!("{stage:?}"),
                        self.ws.update_relaunch_target().is_some(),
                    )
                });
                let crash = (self.ws.crash_report().is_some(), self.ws.stale_offer());
                // The code box whose copy button shows, and its check mark.
                let vr = page::viewport(&self.ws, self.areas.page);
                let code = crate::codebtn::spot(&mut self.ws, &self.ui, vr, self.scale)
                    .map(|c| (c.start, self.ui.copied.is_some_and(|(s, _)| s == c.start)));
                sig((
                    common,
                    self.ws.active(),
                    missing,
                    drag,
                    recents,
                    (update, crash),
                    code,
                ))
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

    /// Composes one frame into `screen` and returns the device-px rows changed. The frame is
    /// one frame of the animator: every value of it is asked between its `begin_frame` and
    /// `end_frame`, and the deadline that gives is the timer `timers` waits for.
    fn compose(&mut self) -> (Vec<(i32, i32, i32, i32)>, FrameTimes) {
        let now = self.frame_clock.unwrap_or_else(Instant::now);
        let policy = crate::motion::policy(self.still, self.ws.motion(), self.forced_motion);
        self.anim.set_policy(policy);
        self.anim.begin_frame(now);
        // The open menus are asked for on every frame they are open, whatever else the frame
        // draws, so none is forgotten and taken for new on a frame that skips the overlays.
        self.menu_t = match self.ui.popup.as_ref() {
            Some(p) => (
                crate::motion::menu(&mut self.anim, p.serial),
                match p.sub {
                    Some(_) => crate::motion::submenu(&mut self.anim, p.serial, p.sub_serial),
                    None => 1.0,
                },
            ),
            None => (1.0, 1.0),
        };
        self.sync_toasts(now);
        let done = self.compose_frame();
        self.anim_due = self.anim.end_frame();
        done
    }

    /// Brings the toasts up to date with the status line at `now`, the frame's time, and asks
    /// for this frame of them. Asked on every frame, whatever it draws, so none is forgotten and
    /// taken for new. Each leaves of itself when its time is up; a status cleared sends them
    /// all away.
    ///
    /// A message set by `set_status` takes the place of the toasts showing: they go at once,
    /// with no exit and no lift, and it comes in as a new toast (a zoom step, a width, a
    /// progress and its result). One set by `confirm`, a one-shot confirmation, comes in under
    /// them. Either way the same text still showing keeps its toast for longer instead.
    fn sync_toasts(&mut self, now: Instant) {
        self.toasts.tick(now);
        let line = (self.ws.status_line()).map(|s| (s.text.clone(), s.at, s.stack));
        if line != self.toast_seen {
            match (&line, self.ws.status_expires()) {
                // A message already past its time (set long before this frame) is not shown.
                (Some((text, _, stack)), Some(until)) if until > now => {
                    self.show_toast(text, until, *stack)
                }
                (None, _) => self.toasts.hide_all(),
                _ => {}
            }
            self.toast_seen = line;
        }
        self.toast_frame.clear();
        if self.toasts.is_empty() {
            return;
        }
        // The height of a toast as `Bubble::toast` makes it (the same for every text).
        let h = {
            let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
            Bubble::toast(&mut p, "", self.areas.page).rect.h
        };
        let gap = crate::motion::TOAST_GAP;
        let id = (crate::motion::TOASTS, self.toast_gen);
        let shown = (self.toasts).frame(&mut self.anim, id, |_| h, gap);
        // The newest first; never more than the most, those leaving included.
        self.toast_frame = (shown.into_iter())
            .take(crate::motion::TOASTS_MAX)
            .map(|s| (s.key, s.item.clone(), s.t, s.lift))
            .collect();
    }

    /// Shows `text` until `until`: under the others when `stack`, in their place otherwise.
    fn show_toast(&mut self, text: &str, until: Instant, stack: bool) {
        let same = (self.toasts.current()).find(|(_, t)| *t == text);
        let same = same.map(|(k, _)| k);
        let alone = self.toasts.current().count() == 1;
        match same {
            Some(key) if stack || alone => self.toasts.keep(key, Some(until)),
            _ if stack => _ = self.toasts.push(text.to_string(), Some(until)),
            _ => {
                // In their place: a stack of its own, under a new name in the animator, so
                // the old toasts are not drawn again and the new one enters from nothing.
                self.toasts = crate::motion::toasts();
                self.toast_gen += 1;
                self.toasts.push(text.to_string(), Some(until));
            }
        }
    }

    fn compose_frame(&mut self) -> (Vec<(i32, i32, i32, i32)>, FrameTimes) {
        self.crash_context();
        let mut times = FrameTimes::default();
        let t0 = Instant::now();
        let (pw, ph) = (self.size.0.max(1), self.size.1.max(1));
        if self.screen.width != pw || self.screen.height != ph {
            self.screen = Bitmap::new(pw, ph);
            self.full = true;
        }
        self.sync_theme();
        self.sync_frost();
        // Text the editor sent since the last frame: one parse, of the newest.
        self.ws.apply_live();
        // A presentation is the whole frame.
        if self.ui.present.is_some()
            && let Some(done) = self.compose_present()
        {
            return done;
        }
        let (w, h) = self.logical();
        let areas = Areas::compute(w, h, &self.ui);
        if areas != self.areas {
            self.areas = areas;
            self.full = true;
        }
        // A zoom view is a picture of the tab it was opened on: another tab, a new text of it or
        // another theme under it, and it closes (audit 4, E8). Otherwise it follows the window:
        // its diagram is kept inside the new area, at the new device scale.
        if let Some(view) = &self.ui.zoom
            && let Some(origin) = &view.origin
            && !(self.ws.active_tab())
                .is_some_and(|t| origin.is(t.path(), t.revision(), self.pal.dark))
        {
            self.close_zoom();
        }
        if let Some(view) = self.ui.zoom.as_mut() {
            let area = Rect::new(0.0, 0.0, w, h);
            view.set_scale(area, self.scale);
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
            // known yet (not drawn): then the row is revealed on a later frame. A hidden outline
            // is left as it is, and catches up on the frame that shows it again.
            if self.areas.outline.is_none() {
                // Nothing to move.
            } else if panes::follow_outline_row(&mut self.ui, current.unwrap_or(0)) {
                // Above the first heading nothing is current: the outline is at its top.
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
        // A chevron turning in the Files pane: the pane is drawn on each frame of the turn,
        // whenever the frame comes (one a little before the deadline included).
        let files = Id::new(crate::motion::FILES);
        if self.anim.group_deadlines().iter().any(|(g, _)| *g == files) {
            dirty[Region::Files.index()] = true;
        }
        for r in REGIONS {
            if self.areas.region(r).is_none() {
                dirty[r.index()] = false;
                self.hits[r.index()].clear();
            }
        }

        // Overlays: lay them out, then redraw every region under them if they or those changed.
        let pal = self.pal;
        let mut bubbles = Vec::new();
        {
            let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
            let page = self.areas.page;
            if let Some(text) = &self.ui.link_hint {
                bubbles.push((Bubble::hint(&mut p, text, page), 1.0));
            }
            // The toasts, each lifted above the ones under it to a whole device pixel (so its
            // text stays on the grid), at its presence. One that would reach above the page
            // (a short window) is left out.
            for (_, text, t, lift) in &self.toast_frame {
                let mut b = Bubble::toast(&mut p, text, page);
                b.rect.y = toast_y(b.rect.y, *lift, self.scale);
                if b.rect.y < page.y + crate::motion::TOAST_GAP {
                    continue;
                }
                bubbles.push((b, *t));
            }
            if let Some(tip) = self.ui.tip.as_ref().filter(|t| t.shown) {
                bubbles.push((Bubble::tip(&mut p, &tip.text, tip.anchor, w, h), 1.0));
            }
            if let Some(popup) = self.ui.popup.as_mut() {
                let room = menu_room(self.areas.title, w, h);
                popup.menu.place_in(&mut p, room);
                if let Some(s) = popup.sub.as_mut() {
                    s.place_in(&mut p, room);
                }
            }
        }
        self.toast_until = self.toasts.deadline();
        let card = self.card_layout(w, h);
        // A toast touches more as it rises and sinks than at rest.
        let mut bounds: Vec<Rect> = (bubbles.iter())
            .map(|(b, _)| match b.kind {
                mdshared_ui::overlay::Kind::Toast => b.presence_bounds(),
                _ => b.bounds(),
            })
            .collect();
        bounds.extend(card.iter().map(crate::preview::Layout::bounds));
        if let Some(popup) = &self.ui.popup {
            // Everything a menu touches as it fades in, its shadow and its drop included.
            bounds.push(popup.menu.presence_bounds());
            if let Some(s) = &popup.sub {
                bounds.push(s.presence_bounds());
            }
        }
        if self.ui.dialog.is_some() || self.ui.settings.is_some() {
            bounds.push(Rect::new(0.0, 0.0, w, h));
        }
        if self.ui.zoom.is_some() {
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
                .map(|(b, t)| {
                    (
                        b.kind,
                        b.text.as_str(),
                        b.rect.x.to_bits(),
                        b.rect.y.to_bits(),
                        // A toast coming or going is drawn anew on each frame of it.
                        t.to_bits(),
                    )
                })
                .collect::<Vec<_>>(),
            card.as_ref().map(crate::preview::Layout::key),
            self.ui.popup.as_ref().map(|p| {
                (
                    p.kind,
                    (p.menu.hover, p.menu.scroll().to_bits()),
                    p.menu.items.len(),
                    p.access,
                    p.sub
                        .as_ref()
                        .map(|s| (s.hover, s.items.len(), s.scroll().to_bits())),
                    // A menu fading in is drawn anew on each frame of it.
                    (self.menu_t.0.to_bits(), self.menu_t.1.to_bits()),
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
            self.ui
                .zoom
                .as_ref()
                .map(|z| (z.zoom.to_bits(), z.pan.0.to_bits(), z.pan.1.to_bits())),
            self.ui
                .zoom
                .is_some()
                .then_some((self.ui.hover, self.ui.pressed)),
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
        let mut files_drawn = false;
        {
            let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
            // A menu fading in over frosted chrome lets the backdrop through less and less.
            p.set_frost_key(self.frost.key().copied());
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
                    Region::Files => {
                        self.anim.group(crate::motion::FILES);
                        panes::draw_files(&mut c, &mut self.anim, &self.ws, &mut self.ui, rect);
                        self.anim.ungroup();
                        files_drawn = true;
                    }
                    Region::Outline => panes::draw_outline(&mut c, &self.ws, &mut self.ui, rect),
                    Region::Page => {
                        times.paint = page::draw_page(&mut c, &mut self.ws, &self.ui, rect)
                    }
                }
                p.restore(old);
                // The chrome lets the backdrop through: its pixels are marked for the present.
                if self.frost.key().is_some() && crate::frost::frosted(r) {
                    mdshared_ui::frost::mark(p.bmp, p.dev_rect(clip));
                }
            }
            if self.frost.key().is_some() {
                self.frost.areas.clear();
                for r in REGIONS {
                    if let Some(rect) = self.areas.region(r).filter(|_| crate::frost::frosted(r)) {
                        let d = p.dev_rect(rect);
                        let u = |v: i32| v.max(0) as usize;
                        self.frost.areas.push((u(d.0), u(d.1), u(d.2), u(d.3)));
                    }
                }
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
                    overlays::draw_dialog(
                        &mut c,
                        &self.ws,
                        &self.ui.notes,
                        d,
                        w,
                        h,
                        self.ui.dlg_hover_row,
                    );
                }
                if let Some(popup) = self.ui.popup.as_mut() {
                    let room = menu_room(self.areas.title, w, h);
                    // At rest (1) each is `draw_in`, pixel for pixel.
                    let (t, ts) = self.menu_t;
                    popup.menu.draw_in_presence(c.p, &pal, room, t);
                    if let Some(s) = popup.sub.as_mut() {
                        let leftward = s.rect().x < popup.menu.rect().x;
                        s.draw_in_entering(c.p, &pal, room, ts, Entrance::Side { leftward });
                    }
                }
                for (b, t) in &bubbles {
                    if *t >= 1.0 {
                        b.draw(c.p, &pal);
                    } else {
                        b.draw_presence(c.p, &pal, *t);
                    }
                }
                if let Some(l) = &card {
                    crate::preview::draw(l, c.p, &pal);
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
                // The diagram view covers everything, so it is drawn after them all.
                if let Some(z) = self.ui.zoom.as_ref() {
                    crate::zoom::draw(&mut c, z, window);
                }
            }
        }
        // A pane left undrawn keeps its chevrons: a folder clicked on a later frame turns from
        // where it was drawn, instead of being taken for new (and shown at rest).
        if !files_drawn {
            self.anim.keep_group(crate::motion::FILES);
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
    fn watch_tree(&mut self, files_changed: bool) {
        let Some(w) = self.watcher.as_mut() else {
            return;
        };
        // The folders the Files pane lists, while it is open or Find in folder is: a file made
        // or removed there changes what a search finds.
        let searching = self
            .ui
            .palette
            .as_ref()
            .is_some_and(|p| p.is_folder_search());
        let dirs = if self.ui.panes.files || searching {
            self.ui.tree.watched_dirs()
        } else {
            Vec::new()
        };
        // The folders the notes need are asked for only when the open files or the index changed
        // (`files_changed`), not on every pass.
        if dirs != self.tree_dirs || files_changed {
            let notes = self.ws.note_folders(WATCHED_NOTE_FOLDERS);
            let all: std::collections::BTreeSet<&Path> =
                dirs.iter().chain(&notes).map(PathBuf::as_path).collect();
            w.set_folders(all);
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
        self.sync_snap();
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
            // After the first frame, never before it: the system's wish for less motion, and
            // the watch for its changes (a helper thread on Linux, a hidden window on Windows).
            self.read_motion();
            self.watch_motion();
        }
    }

    /// Reads the system's wish for less motion again: after the first frame, on focus, on a
    /// change of theme and when the watch says it changed.
    fn read_motion(&mut self) {
        if !self.still {
            self.anim.set_system_reduced(platform::motion_reduced());
        }
    }

    fn watch_motion(&mut self) {
        self.watch_motion_with(platform::watch_motion_reduced);
    }

    /// Starts `watch` with a wake that sends `Wake::Motion`, on a short-lived helper thread:
    /// on Windows the watch makes a hidden window on a thread of its own and waits up to 5 s
    /// for it, which must never be the window's own thread. A thread that cannot be started
    /// leaves the window unwatched; it still reads the setting on focus and theme changes.
    fn watch_motion_with(
        &mut self,
        watch: impl FnOnce(Box<dyn Fn() + Send + Sync>) + Send + 'static,
    ) {
        if self.still || self.motion_watched {
            return;
        }
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        self.motion_watched = true;
        let wake: Box<dyn Fn() + Send + Sync> = Box::new(move || {
            let _ = proxy.send_event(Wake::Motion);
        });
        let _ = std::thread::Builder::new()
            .name("mdview-motion-watch".into())
            .spawn(move || watch(wake));
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
        let frost = &self.frost;
        let copy = |buffer: &mut [u32], (x0, y0, x1, y1): (usize, usize, usize, usize)| {
            let bytes: &mut [u8] = bytemuck::cast_slice_mut(buffer);
            for y in y0..y1 {
                let (a, b) = ((y * stride + x0) * 4, (y * stride + x1) * 4);
                let to = (((y + m) * surface_stride) + x0 + m) * 4;
                bytes[to..to + (b - a)].copy_from_slice(&self.screen.pixels[a..b]);
            }
            // A frosted chrome: the flat colour is taken out of its marked pixels, in the
            // window's copy only (the bitmap keeps what was drawn).
            if let Some(key) = frost.key() {
                for area in &frost.areas {
                    if let Some(r) = mdshared_ui::frost::overlap((x0, y0, x1, y1), *area) {
                        let r = (r.0 + m, r.1 + m, r.2 + m, r.3 + m);
                        mdshared_ui::frost::key_out(key, buffer, surface_stride, r);
                    }
                }
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
        // The rounded corners are cut out of the content just copied: only the corner squares the
        // copied rectangles touch are redone (all four after a whole copy, or when the margin was
        // painted again and the shadow under the corners changed with it). Nothing without
        // rounding.
        if let Some(s) = self.shadow.as_ref().filter(|s| s.rounds()) {
            let touched: Vec<_> = damage.iter().map(&clamp).collect();
            let all = age != 1 || margin_painted;
            s.paint_corners(
                &mut buffer,
                sw,
                sh,
                &self.screen.pixels,
                (!all).then_some(&touched[..]),
            );
        }
        let copied = t.elapsed();
        let _ = buffer.present_with_damage(&rects);
        copied
    }

    // ---- hit testing ---------------------------------------------------------------------------

    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        // The diagram view covers the window; only its close button is a target.
        if self.ui.zoom.is_some() {
            return self
                .overlay_hits
                .iter()
                .rev()
                .find(|(_, r)| r.contains(x, y))
                .map(|(h, _)| *h);
        }
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

    /// Tells the system where the maximize button is, so Windows 11 shows the snap-layouts flyout
    /// over it; `None` while the title row's buttons are not there to be pressed: a dialog, the
    /// settings or the palette over it, a presentation, full screen. Runs after every composed
    /// frame, which every resize, maximize and scale change brings, and sends only a change.
    fn sync_snap(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let covered = self.ui.dialog.is_some()
            || self.ui.modal()
            || self.ui.zoom.is_some()
            || self.ui.present.is_some()
            || window.fullscreen().is_some();
        let rect = (!covered).then(|| chrome::max_button_rect(self.logical().0, self.maximized));
        if self.snap_sent != Some(rect) {
            self.snap_sent = Some(rect);
            mdshared_ui::snap::set_maximize_button_rect(&window, rect);
        }
    }

    fn edge_at(&self, x: f32, y: f32) -> Option<ResizeDirection> {
        // macOS resizes the window itself (`chrome::NATIVE_FRAME`).
        if self.maximized || self.window.is_none() || mdshared_ui::chrome::NATIVE_FRAME {
            return None;
        }
        self.resize_band(x, y)
    }

    /// The resize direction at the point for a restored window, without asking the window.
    ///
    /// The band is 8 px wide, and a vertical scrollbar at the window's right edge (the page with
    /// the Outline pane closed, or the Outline pane's list) has its thumb 3 to 11 px from that
    /// edge: the east band over such a track shrinks to the outermost [`SCROLLBAR_EDGE`] px, which
    /// the thumb never covers, so the bar can be grabbed and the edge still resizes. The corners
    /// keep their full band.
    fn resize_band(&self, x: f32, y: f32) -> Option<ResizeDirection> {
        let (w, h) = self.logical();
        let dir = mdshared_ui::window::edge_at(x, y, w, h);
        if dir == Some(ResizeDirection::East)
            && x < w - SCROLLBAR_EDGE
            && matches!(
                self.hit_at(x, y),
                Some(Hit::ScrollBar(Scroller::Page | Scroller::Pane(_)))
            )
        {
            return None;
        }
        dir
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

/// The dark title bar and the corner rounding (Windows 11 draws it; Windows 10 stays square).
#[cfg(windows)]
fn style(w: &Window, dark: bool, corners: Corners) {
    if let Some(h) = hwnd(w) {
        platform::style_window(h, dark);
        mdshared_ui::corners::set_window_corners(h, corners);
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
        // (not on Windows; on KDE only with rounded corners, which need the alpha channel): the
        // attributes come back unchanged elsewhere.
        let corners = Corners::from_setting(self.ws.corners());
        let attributes = mdshared_ui::shadow::Shadow::prepare_corners(
            attributes,
            mdshared_ui::shadow::Mode::from_env(),
            corners,
            event_loop,
        );
        // Linux: the Wayland app id, X11 class and window icon; a no-op on Windows.
        let attributes = mdshared_ui::window::identify(
            attributes,
            "mdview",
            Some(include_bytes!("../../../assets/mdview.png")),
        );
        let window = Rc::new(event_loop.create_window(attributes).expect("window"));
        mdshared_ui::window::created(&window);
        // macOS: the menu bar, with Quit as the Exit command (winit's ends the process at once).
        if let Some(p) = self.proxy.clone() {
            mdshared_ui::menu_bar::install("mdview", move |c| {
                let _ = p.send_event(Wake::Menu(c));
            });
        }
        stage(self.started, "window created");
        self.scale = window.scale_factor() as f32;
        self.maximized = window.is_maximized();
        self.shadow = mdshared_ui::shadow::Shadow::attach_corners(
            &window,
            mdshared_ui::shadow::Mode::from_env(),
            corners,
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
        style(&window, self.pal.dark, corners);
        let context = softbuffer::Context::new(window.clone()).expect("display");
        let mut surface = softbuffer::Surface::new(&context, window.clone()).expect("surface");
        // The client-drawn margin is transparent: Wayland buffers must carry alpha (the content
        // bitmap's alpha is always 255). A no-op elsewhere.
        if self.shadow.as_ref().is_some_and(|s| s.wants_alpha()) {
            surface.set_premultiplied_alpha(true);
        }
        self.surface = Some(surface);
        self.window = Some(window);
        self.present_window();
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
            let file = self.args.files.first().cloned().unwrap_or_default();
            self.place(rect, (file, None));
        }
        self.started_by_editor();
    }

    fn user_event(&mut self, _: &ActiveEventLoop, wake: Wake) {
        self.woken(wake);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.exit = true;
                // Ended here, not in `about_to_wait`: the loop reads an exit asked for there only
                // after the next event, and a close that comes from the system alone (the
                // taskbar's "Close window") is followed by none, so the window stayed open.
                self.finish(event_loop);
            }
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
                self.read_motion();
                self.system_dark = t == WinTheme::Dark;
                self.ws.set_system_dark(self.system_dark);
                self.full = true;
                self.redraw();
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                // Seen again: whatever changed while covered is drawn, and the animations start
                // again (`about_to_wait` follows this event and arms their timer).
                self.wants_frame |= !occluded;
            }
            WindowEvent::Focused(focused) => {
                self.update_shadow(Some(focused));
                self.focused = focused;
                // The backdrop shows only behind the window that has the keyboard.
                if self.ws.backdrop() != mdshared_app::backdrop::Backdrop::Off {
                    self.redraw();
                }
                if focused {
                    // The system's "animation effects" setting, read again: for the images here,
                    // and for the window's motion (as also after the first frame, on a theme
                    // change and when the watch wakes the window).
                    self.ws.set_system_animations(platform::shows_animations());
                    self.read_motion();
                }
                if focused && self.ui.tree.refresh() {
                    self.redraw();
                }
                if !focused {
                    self.ui.caret_on = true;
                    self.ui.tip = None;
                    self.ui.card = None;
                }
                self.redraw();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = Mods::from_winit(m.state());
                // Alt shows the underlined access keys of an open menu.
                if self.mods.alt
                    && let Some(p) = self.ui.popup.as_mut()
                    && !p.access
                {
                    p.show_access();
                    self.redraw();
                }
            }
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
                    self.ui.card = None;
                    self.ui.link_hint = None;
                    self.redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                // On a Mac, Control and a click is a right click (`Mods::control` is never set
                // elsewhere).
                (ElementState::Pressed, MouseButton::Left) if self.mods.control => {
                    self.control_click = true;
                    self.right_press();
                }
                (ElementState::Released, MouseButton::Left)
                    if std::mem::take(&mut self.control_click) => {}
                (ElementState::Pressed, MouseButton::Left) => self.left_press(),
                (ElementState::Released, MouseButton::Left) => self.left_release(),
                (ElementState::Pressed, MouseButton::Right) => self.right_press(),
                (ElementState::Pressed, MouseButton::Middle) => self.middle_press(),
                (ElementState::Pressed, MouseButton::Back) => self.travel(true),
                (ElementState::Pressed, MouseButton::Forward) => self.travel(false),
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => self.wheel(delta),
            // A trackpad: a pinch zooms in steps; the smart-zoom tap (two fingers, twice) is the
            // actual size. winit reports both on macOS only.
            WindowEvent::PinchGesture { delta, phase, .. } => self.pinch(
                delta,
                matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled),
            ),
            WindowEvent::DoubleTapGesture { .. } => self.double_tap(),

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
            // Text an input method made: an accent typed with a dead key or chosen by holding a
            // key, or Chinese, Japanese or Korean input. Input methods are on only on macOS.
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => self.ime_commit(&text),
            WindowEvent::Ime(winit::event::Ime::Preedit(text, cursor)) => {
                self.ime_preedit(text, cursor)
            }
            WindowEvent::Ime(winit::event::Ime::Enabled | winit::event::Ime::Disabled) => {
                self.ime_preedit(String::new(), None)
            }
            WindowEvent::DroppedFile(path) => self.dropped(path),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Focused time: the tab in front while the window has the focus.
        let showing = self.focused && !self.occluded;
        self.ws.focus_tick(showing, std::time::Instant::now());
        if self.exit {
            self.finish(event_loop);
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
        let files_changed = self.ws.take_watch_changed();
        if files_changed && let Some(w) = self.watcher.as_mut() {
            w.set_files(
                self.ws
                    .watched_paths()
                    .chain(self.ws.note_files(WATCHED_NOTES)),
            );
        }
        self.watch_tree(files_changed);
        // The tab the window was placed for is closed: the side-by-side arrangement is over.
        if self.tiling.is_some() && self.placed_tab_gone() {
            self.end_side_by_side(true);
        }
        // A tab the editor asked for is closed: its preview of it is over.
        self.tell_editor_closed();
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
        // A window that is fully covered is not drawn: the frame waits until it is seen again.
        // (The first frame is drawn whatever is reported: it is what shows the window.)
        if self.wants_frame && (!self.occluded || self.first) {
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
    // A document mdedit has not saved, as its "Preview in mdview" would hand it over.
    if let Some(text) =
        (app.args.demo_untitled.as_deref()).and_then(|file| std::fs::read_to_string(file).ok())
    {
        (app.ws).document_text("mdedit-0-1", Some("Untitled 1"), Some(text), None, true);
    }
    app.compose();
    if let Some(px) = app.args.scroll {
        app.ws.scroll_to(px * app.scale);
    }
    app.compose();
    let page = app.areas.page;
    match app.args.menu {
        Some(ShotMenu::Main) => app.open_popup(PopupKind::Main),
        Some(ShotMenu::MainKeys) => {
            app.open_popup(PopupKind::Main);
            if let Some(p) = app.ui.popup.as_mut() {
                p.show_access();
            }
        }
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
        Some(ShotMenu::Library) => app.library_groups_for_shot(),
        Some(ShotMenu::Find) => {
            if let Some(r) = app.hit_rect(Hit::Find) {
                app.ui.mouse = (r.x + 60.0, r.cy());
                app.right_press();
            }
        }
        None => {}
    }
    // A menu part of the way into its fade-in: its first frame, then the frames' time moved on.
    if let Some(ms) = app.args.menu_at
        && app.ui.popup.is_some()
    {
        let start = start_moving(&mut app);
        frames_until(&mut app, start, ms);
    }
    match app.args.dialog {
        Some(ShotDialog::About) => app.open_dialog(DialogKind::About),
        Some(ShotDialog::Shortcuts) => app.open_dialog(DialogKind::Shortcuts),
        Some(ShotDialog::Recent) => app.open_dialog(DialogKind::Recent),
        Some(ShotDialog::Edit) => app.open_dialog(DialogKind::AllowEdit),
        Some(ShotDialog::AddLibrary) => app.library_add_for_shot(),
        Some(ShotDialog::Settings) => {
            app.open_settings();
            if let Some(name) = app.args.settings_section.clone()
                && !app.show_settings_section(&name)
            {
                eprintln!("mdview: no settings page {name}");
            }
        }
        None => {}
    }
    if let Some(n) = app.args.outline_focus {
        app.compose();
        app.ui.outline_focus = Some(n);
        app.reveal_focused_outline();
    }
    if let Some(query) = app.args.palette.clone() {
        app.open_palette(&query);
    }
    if let Some(query) = app.args.folder_search.clone() {
        app.open_folder_search();
        if let Some(p) = app.ui.palette.as_mut() {
            p.palette.set_query(&query);
            p.sync(&app.ws);
            // A shot has no event loop to hand the results over: wait for the search to end.
            while p.folder_running() {
                std::thread::sleep(std::time::Duration::from_millis(5));
                p.poll_folder();
            }
            p.poll_folder();
        }
    }
    // The diagram zoom view, at the size and place the arguments ask for.
    if let Some(n) = app.args.zoom_diagram {
        if !app.open_zoom_nth(n) {
            eprintln!("mdview: no diagram {n}");
        }
        let area = app.zoom_area();
        if let Some(view) = app.ui.zoom.as_mut() {
            if let Some(level) = app.args.zoom_level {
                view.zoom_to(area, level, (area.cx(), area.cy()));
            }
            if let Some(pan) = app.args.zoom_pan {
                view.pan = pan;
                view.clamp(area);
            }
        }
        app.compose();
    }
    if app.ui.dialog.is_some() || app.ui.popup.is_some() || app.ui.modal() {
        app.compose();
    }
    if let Some((x, y)) = app.args.hover {
        app.pointer_moved(x, y);
        // A control that shows only under the pointer (the code block's copy button) is on screen
        // after the first frame: the pointer is over it from then on.
        if app.args.copied || app.ui.hover == Some(Hit::Page) {
            app.compose();
            if app.hit_at(x, y) != app.ui.hover {
                app.pointer_moved(x, y);
            }
        }
        if app.args.copied {
            app.compose();
            let vr = page::viewport(&app.ws, app.areas.page);
            if let Some(spot) = crate::codebtn::spot(&mut app.ws, &app.ui, vr, app.scale) {
                app.ui.copied = Some((spot.start, Instant::now() + crate::codebtn::COPIED_FOR));
            }
        }
        if let Some(t) = app.ui.tip.as_mut() {
            t.shown = true;
        }
        app.show_card_now();
        if let Some(p) = app.ui.popup.as_mut() {
            p.menu.pointer(x, y);
            if let Some(s) = p.sub.as_mut() {
                s.pointer(x, y);
            }
        }
    }
    // What a shot sets moving: what is on screen now is at rest, then the change comes and
    // the frames of its motion run up to `--motion-at` (without it the change is at rest).
    let toasts = app.args.toasts.clone();
    let (last_toast, earlier) = match toasts.split_last() {
        Some((last, earlier)) => (Some(last), earlier),
        None => (None, &[][..]),
    };
    // Confirmations, so that they stack.
    for text in earlier {
        app.ws.confirm(text);
        app.compose();
    }
    let moving = app.args.motion_at.map(|ms| (start_moving(&mut app), ms));
    if let Some(text) = last_toast {
        app.ws.confirm(text);
    }
    if let Some(name) = app.args.turn.clone() {
        match (app.ui.tree.rows().iter()).position(|r| r.is_dir && r.depth == 0 && r.name == name) {
            Some(i) => {
                app.ui.tree.toggle(i);
            }
            None => {
                eprintln!("mdview: no folder {name} in the Files pane");
                return 2;
            }
        }
    }
    if let Some((start, ms)) = moving {
        frames_until(&mut app, start, ms);
    }
    app.full = true;
    app.compose();
    // `--frame`: the window as the client-drawn margin puts it on screen.
    let framed = app
        .args
        .frame
        .map(|c| frame_of(&app.screen, c, app.scale as f64, app.pal.dark));
    match framed.as_ref().unwrap_or(&app.screen).write_png(path) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("mdview: could not write {}: {e}", path.display());
            1
        }
    }
}

/// Where a toast whose place at rest is `rest` stands `lift` logical px above it: moved by a
/// whole number of device pixels at `scale`, so its text stays on the grid it has at rest.
fn toast_y(rest: f32, lift: f32, scale: f32) -> f32 {
    rest - (lift * scale).round() / scale
}

/// A shot from here on moves, as a window does, on a clock set by hand: the frame drawn now is
/// the start. Returns the start.
fn start_moving(app: &mut App) -> Instant {
    app.still = false;
    app.forced_motion = true;
    let start = Instant::now();
    app.frame_clock = Some(start);
    app.compose();
    start
}

/// Draws a moving shot's frames up to `ms` after `start`, in frames of 16 ms, as a window draws
/// them: one step longer than the animator's 50 ms would be taken for a stall and cut short. It
/// stops once nothing moves, so no number of milliseconds, however large, makes a long loop or
/// overflows the clock.
fn frames_until(app: &mut App, start: Instant, ms: u64) {
    let end = Duration::from_millis(ms);
    let mut at = Duration::ZERO;
    loop {
        // The frame that starts the motion asks for the next one; the change made since the
        // last frame is drawn first.
        if at > Duration::ZERO && app.anim_due.is_none() {
            break;
        }
        at = (at + Duration::from_millis(16)).min(end);
        app.frame_clock = Some(start.checked_add(at).unwrap_or(start));
        let (damage, times) = app.compose();
        // `MDVIEW_TRACE`: what each frame of the motion costs, as a window's trace says it
        // (without the copy to the window and its present, which a shot does not make).
        if app.trace {
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            eprintln!(
                "mdview: motion frame at {} ms: layout {:.2} paint {:.2} compose {:.2} ms, {} rows in {} rects",
                at.as_millis(),
                ms(times.layout),
                ms(times.paint),
                ms(times.compose),
                times.rows,
                damage.len()
            );
        }
        if at >= end {
            break;
        }
    }
}

/// What `--frame` renders: `content` with the shadow margin and `corners`, composited over a flat
/// backdrop (a mid grey, lighter for a dark window so its shadow shows), by the same code the
/// client-drawn window uses (`shadow::Frame`). The picture is larger than the content by the margin.
fn frame_of(content: &Bitmap, corners: Corners, scale: f64, dark: bool) -> Bitmap {
    let frame = mdshared_ui::shadow::Frame::with_radius(scale, true, corners.px(scale));
    let backdrop = if dark {
        [0x5a, 0x5a, 0x5f]
    } else {
        [0xc8, 0xc8, 0xcd]
    };
    let (width, height, pixels) =
        frame.preview(&content.pixels, content.width, content.height, backdrop);
    Bitmap {
        width,
        height,
        pixels,
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
    if args.install_update {
        return mdshared_app::updater::install_and_restart(&updater::APP, &updater::fetch);
    }
    let explicit = std::env::var_os("MDVIEW_SETTINGS").map(PathBuf::from);
    // A shot never touches the user's session; it may use a scratch one named by MDVIEW_SETTINGS.
    let mut session = match (args.no_session, args.shot.is_some()) {
        (true, _) => None,
        (false, true) => explicit,
        (false, false) => explicit.or_else(mdview_core::session::default_settings_path),
    };
    // Held while the window runs: this launch has the settings folder
    // (mdshared_app::folder_lock, shared with mdedit).
    let mut _folder_lock = None;
    // Why this window runs without its session, shown once it is up.
    let mut no_session_note: Option<String> = None;
    if let Some(settings) = session.clone().filter(|_| args.shot.is_none()) {
        // Started by a restart: the window that started it is still closing; wait it out, or the
        // hand-over below would go to it and leave no window.
        mdshared_app::relaunch::wait_for_predecessor(std::env::args().skip(1), &settings);
        // A window is already open: hand it the files and quit, before any window work. A launch
        // that asks for a line, a place or a follow hands each file over with that. One that is
        // still starting holds the folder and is waited for, not run beside.
        let requests = args.requests();
        let forward = || {
            if requests.is_empty() {
                mdview_core::single::forward(&settings, &args.files)
            } else {
                (requests.iter()).all(|r| mdview_core::single::forward_request(&settings, r))
            }
        };
        use mdshared_app::folder_lock::{self, Claim};
        match folder_lock::claim(&settings, forward, folder_lock::WAIT) {
            Claim::Own(lock) => _folder_lock = lock,
            Claim::Handed => return 0,
            Claim::Busy => {
                session = None;
                no_session_note = Some(folder_lock::busy_note("mdview"));
            }
        }
    }
    // A live update is for a window that shows the file already: with none, there is nothing to do.
    if args.follow && args.shot.is_none() {
        return 0;
    }
    stage(started, "single-instance check");
    // A session that cannot be read is left as it is: the window runs without it.
    let saved = match session
        .as_deref()
        .map(mdview_core::session::Session::read_patiently)
    {
        Some(Err(e)) => {
            session = None;
            no_session_note = Some(mdshared_app::folder_lock::unreadable_note(&e));
            mdview_core::session::Session::default()
        }
        Some(Ok(s)) => s,
        None => mdview_core::session::Session::default(),
    };
    let mut ws = Workspace::from_session(saved, session.clone());
    if let Some(note) = &no_session_note {
        eprintln!("mdview: {note}");
        ws.set_status(note);
    }
    stage(started, "workspace (session, file read, parse)");
    let mut app = App::new(args, ws, session);
    app.started = started;
    app.setup();
    stage(started, "app setup");
    app.look_for_crash();
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
    // macOS hands a running bundle its files as an event, also the files it was launched with.
    let p = proxy.clone();
    mdshared_ui::open_documents::on_open(move |paths| {
        let _ = p.send_event(Wake::Open(paths));
    });
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
    let p = proxy.clone();
    app.ui.previews.set_wake(move || {
        let _ = p.send_event(Wake::Preview);
    });
    app.loop_ready(LoopProxy::Loop(proxy));
    app.updates_at_launch();
    stage(started, "ready to run");
    let _ = event_loop.run_app(&mut app);
    // A restart starts the new version last, with the window and the engine gone and the session
    // written. The new process waits for this one to end before it looks for a running instance.
    let restart = app.restart.take();
    drop(app);
    if let Some(target) = restart
        && let Err(e) = mdshared_app::relaunch::spawn(&target)
    {
        eprintln!("mdview: could not start {}: {e}", target.display());
    }
    0
}
