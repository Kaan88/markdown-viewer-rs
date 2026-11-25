//! The window's own motion: which things move, and by what. The one place the timings are
//! chosen (owner, 2026-11-25: they are tuned later, here). The menu's entrance is the shared
//! `mdshared_ui::anim::document::MENU`'s curve made quicker, at the owner's word ("make it
//! slightly faster", 2026-11-25): 117 ms instead of 167.
//!
//! Menus, context menus and the Open recent submenu fade in as they open; closing is instant
//! (owner, 2026-11-25). Nothing moves in a `--shot`, a `--scroll-steps` run or a test, nor when
//! the "Animations" setting is off, nor when the system asks for less motion.

use mdshared_ui::anim::{Animator, Motion, Policy, document, fluent};
use std::time::Duration;

/// How long a menu takes to come in: seven frames at 60 Hz.
pub const MENU_IN: Duration = Duration::from_millis(117);

/// A menu's entrance: the shared preset's decelerating curve, over [`MENU_IN`]. Its exit is not
/// used: a closing menu goes at once.
pub const MENU: document::Pair = document::Pair {
    enter: Motion::tween(MENU_IN, fluent::DECELERATE),
    exit: document::MENU.exit,
};

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
