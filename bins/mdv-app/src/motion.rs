//! The window's own motion: which things move, and by what. The one place the timings are
//! chosen (owner, 2026-11-25: they are tuned later, here). The menu's entrance is the shared
//! `mdshared_ui::anim::document::MENU`, 117 ms since shared `6ec1dee`: the owner's word ("make
//! it slightly faster", 2026-11-25), first set here and now the shared preset.
//!
//! Menus, context menus and the Open recent submenu fade in as they open; closing is instant
//! (owner, 2026-11-25). A folder's chevron in the Files pane (and a group's or a section's in the
//! Notes pane) turns as it opens or closes, by [`DISCLOSURE`] (the shared `glyph::DISCLOSURE_MOTION`, 167 ms).
//! A toast rises and fades in, fades and sinks out as it leaves of itself, and a second one
//! stacks under the first (the shared `anim::Stack`), by `document::TOAST`.
//! Nothing moves in a `--shot`, a `--scroll-steps` run or a test, nor when
//! the "Animations" setting is off, nor when the system asks for less motion.

use mdshared_ui::anim::stack::Stack;
use mdshared_ui::anim::{Animator, Id, Motion, Policy, document, fluent};
use std::path::Path;

/// A menu's entrance: the shared preset (117 ms, decelerating). Its exit is not used: a
/// closing menu goes at once.
pub const MENU: document::Pair = document::MENU;

/// How long the pointer rests on a row that opens a submenu before it opens, or on another row
/// of its menu before the open one closes or gives way, as in Windows (`SPI_GETMENUSHOWDELAY`);
/// the shared `submenu::Cascade`'s delay (`Cascade::with_delay`).
pub const SUBMENU: std::time::Duration = std::time::Duration::from_millis(300);

/// The presence of the open menu, `serial` naming this opening of it: a menu opened again is
/// new and fades in again.
pub fn menu(anim: &mut Animator, serial: u64) -> f32 {
    anim.presence(("menu", serial), true, MENU.enter, MENU.exit)
}

/// The presence of the submenu opened beside the menu `serial`, `sub` naming this opening of it.
pub fn submenu(anim: &mut Animator, serial: u64, sub: u64) -> f32 {
    anim.presence(("submenu", serial, sub), true, MENU.enter, MENU.exit)
}

/// The presence of the palette, `serial` naming this opening of it: the card fades in and drops
/// into place (`Palette::draw_presence`), and a palette opened again fades in again. Closing is
/// instant, so its exit is not used.
pub fn palette(anim: &mut Animator, serial: u64) -> f32 {
    anim.presence(("palette", serial), true, MENU.enter, MENU.exit)
}

/// A chevron's turn in the Files and Notes panes: the shared `glyph::DISCLOSURE_MOTION` as it
/// is (`fluent::MOVE`, 167 ms), chosen here so every timing of the window is in this file.
pub const DISCLOSURE: Motion = fluent::MOVE;

/// How long the pointer rests on a control before its tooltip shows, counted from its last
/// move (owner, 2026-11-25: "From resting"): the shared preset, 500 ms, as before.
pub const TIP_DELAY: std::time::Duration = document::TIP_DELAY;

/// A tooltip's entrance: the shared preset (a fade, 83 ms). A tip goes at once.
pub const TIP: document::Pair = document::TIP;

/// The presence of the tooltip showing (asked on every frame one shows).
pub fn tip(anim: &mut Animator) -> f32 {
    anim.presence("tip", true, TIP.enter, TIP.exit)
}

/// The presence of a notice bar as it slides in, `key` naming which one (update, crash, stale);
/// a bar that goes is forgotten (`App::bar_t`), so a later one slides in again.
pub fn bar(anim: &mut Animator, key: &str) -> f32 {
    anim.presence(("bar", key), true, MENU.enter, MENU.exit)
}

/// The animator's group for the Files pane's values (the Notes pane is drawn in the same
/// region): a frame that leaves the region undrawn keeps them (`keep_group`), and the region is
/// drawn again while one of them moves.
pub const FILES: &str = "files";

/// The animator's group for the open menus' hover fills (a row, of the menu or its submenu): a
/// frame that leaves the menus undrawn keeps them, and the overlays are drawn again while one of
/// them moves.
pub const MENUS: &str = "menus";

/// The animator's group, and the name, of the settings dialog's values (`Form::draw_animated`:
/// a switch's knob and a chosen segment slide, the page list's hover fills and the open
/// dropdown's row fade, by the shared form's own presets): drawn again while one of them moves,
/// kept on a frame that leaves the dialog undrawn.
pub const SETTINGS: &str = "settings";

/// The animator's group for the title row's hover fills (the window's three buttons, a tab's, the
/// new tab button's): the row is drawn again while one of them moves.
pub const TITLE: &str = "title";

/// The name of the hover fill of the window's button `kind` (its function, so the button keeps
/// its fade when it turns from Maximize to Restore).
pub fn win_button(kind: &'static str) -> Id {
    Id::new(("win-hover", kind))
}

/// The name of a tab's hover fill: by the document it shows, so a tab that moves along the row
/// keeps its fade.
pub fn tab(key: &str) -> Id {
    Id::new(("tab-hover", key))
}

/// The name of the new tab button's hover fill.
pub fn new_tab() -> Id {
    Id::new(("new-tab", "hover"))
}

/// The name of the chevron of the folder at `path` in the Files tree: by its path, so a row that
/// moves up or down the list (a folder above it opened) keeps its turn.
pub fn folder(path: &Path) -> Id {
    Id::new(("folder", path))
}

/// The name of the chevron of the Notes pane's group or section `key`.
pub fn notes_fold(key: &str) -> Id {
    Id::new(("notes-fold", key))
}

/// A toast's entrance and exit: the shared preset as it is (167 ms each). A toast leaves of
/// itself when its time is up; it is not closed by the reader.
pub const TOAST: document::Pair = document::TOAST;

/// The gap between two stacked toasts.
pub const TOAST_GAP: f32 = 8.0;

/// The most toasts on screen at once, those leaving included.
pub const TOASTS_MAX: usize = 3;

/// The animator's name for the stack of toasts.
pub const TOASTS: &str = "toasts";

/// The toasts' stack, entering and leaving by [`TOAST`]; at most [`TOASTS_MAX`] are there at
/// once, those leaving included (a fourth sends the oldest away, and drops at once one already
/// leaving). A confirmation pushed onto others waits until they have lifted out of its way
/// (`with_wait_for_lift`, the owner's "New waits for the lift", 2026-11-25), so the one above
/// never covers it on the way in; onto an empty stack it enters at once. Under `Policy::Never`
/// the shared delay is kept even though nothing moves, so `App::show_toast` revives the new one
/// to drop the wait.
pub fn toasts() -> Stack<String> {
    Stack::new()
        .with_motion(TOAST.enter, TOAST.exit)
        .with_max(TOASTS_MAX)
        .with_max_drawn(TOASTS_MAX)
        .with_wait_for_lift()
}

/// What the animator may do: `still` is a run that must not depend on time (a shot, the
/// scroll steps, a test), `setting` the reader's "Animations" switch. The system's wish for less
/// motion is the animator's own (`set_system_reduced`) and wins under `Policy::System`.
/// `forced` is a shot of something part way through its motion (`--menu-at`, `--motion-at`):
/// it moves whatever the setting and the system say, so the picture is the same everywhere.
pub fn policy(still: bool, setting: bool, forced: bool) -> Policy {
    if forced {
        Policy::Always
    } else if still || !setting {
        Policy::Never
    } else {
        Policy::System
    }
}
