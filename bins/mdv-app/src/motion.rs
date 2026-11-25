//! The window's own motion: which things move, and by what. The one place the timings are
//! chosen (owner, 2026-11-25: they are tuned later, here), all of them the shared
//! `mdshared_ui::anim::document` presets as they are.
//!
//! Menus, context menus and the Open recent submenu fade in as they open; closing is instant
//! (owner, 2026-11-25). Nothing moves in a `--shot`, a `--scroll-steps` run or a test, nor when
//! the "Animations" setting is off, nor when the system asks for less motion.

use mdshared_ui::anim::{Animator, Policy, document};

/// A menu's entrance. Its exit is not used: a closing menu goes at once.
pub const MENU: document::Pair = document::MENU;

/// The presence of the open menu, `serial` naming this opening of it: a menu opened again is
/// new and fades in again.
pub fn menu(anim: &mut Animator, serial: u64) -> f32 {
    anim.presence(("menu", serial), true, MENU.enter, MENU.exit)
}

/// The presence of the submenu opened beside the menu `serial`, `sub` naming this opening of it.
pub fn submenu(anim: &mut Animator, serial: u64, sub: u64) -> f32 {
    anim.presence(("submenu", serial, sub), true, MENU.enter, MENU.exit)
}

/// What the animator may do: `still` is a run that must not depend on time (a shot, the
/// scroll steps, a test), `setting` the reader's "Animations" switch. The system's wish for less
/// motion is the animator's own (`set_system_reduced`) and wins under `Policy::System`.
pub fn policy(still: bool, setting: bool) -> Policy {
    if still || !setting {
        Policy::Never
    } else {
        Policy::System
    }
}
