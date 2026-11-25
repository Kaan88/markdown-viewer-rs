//! Headless tests of the window's own motion (`crate::motion`): menus fading in as they open,
//! the frames that asks for and when they stop, and when nothing moves at all. The frames' time
//! is moved by hand (`App::frame_clock`).

use super::tests::app;
use super::*;
use crate::settings as s;
use crate::ui::PopupKind;
use mdshared_ui::settings::Value;

const FRAME: Duration = Duration::from_millis(16);

/// An app that animates, as a window does, on a clock moved by hand.
fn moving(files: &[&str]) -> App {
    let mut a = app(files);
    a.still = false;
    a.frame_clock = Some(Instant::now());
    a
}

/// Moves the frames' time on and composes a frame.
fn step(a: &mut App, by: Duration) {
    a.frame_clock = a.frame_clock.map(|t| t + by);
    a.compose();
}

/// The colour of the pixel at logical (x, y).
fn pixel(a: &App, x: f32, y: f32) -> [u8; 4] {
    let (x, y) = ((x * a.scale) as usize, (y * a.scale) as usize);
    let i = (y * a.screen.width as usize + x) * 4;
    a.screen.pixels[i..i + 4].try_into().unwrap()
}

fn menu_middle(a: &App) -> (f32, f32) {
    let r = a.ui.popup.as_ref().expect("a menu").menu.rect();
    (r.cx(), r.cy())
}

#[test]
fn a_menu_that_opens_fades_in_and_then_leaves_no_timer() {
    let mut a = moving(&["showcase.md"]);
    assert!(a.anim_due.is_none(), "nothing moves before the menu");
    a.open_popup(PopupKind::Main);
    a.compose();
    assert_eq!(
        a.menu_t.0, 0.0,
        "the menu's first frame is the start of its fade"
    );
    assert!(a.anim_due.is_some(), "a frame is asked for");
    // The deadline is the timer the loop waits for, handled as the others are.
    let due = a.anim_due.unwrap();
    a.wants_frame = false;
    a.timers(due - Duration::from_millis(1));
    assert!(
        !a.wants_frame && a.anim_due == Some(due),
        "not before its time"
    );
    a.timers(due);
    assert!(
        a.wants_frame && a.anim_due.is_none(),
        "the timer asks for a frame once"
    );
    step(&mut a, FRAME);
    let (x, y) = menu_middle(&a);
    let (mid_t, mid) = (a.menu_t.0, pixel(&a, x, y));
    assert!(mid_t > 0.0 && mid_t < 1.0, "on its way: {mid_t}");
    let mut frames = 1;
    while a.anim_due.is_some() {
        step(&mut a, FRAME);
        frames += 1;
        assert!(frames < 30, "the fade ends");
    }
    assert_eq!(a.menu_t.0, 1.0);
    // 167 ms at 16 ms a frame, and one frame more at most.
    assert!(frames <= 12, "{frames} frames");
    assert!(a.anim.ungrouped_deadline().is_none());
    // Each frame of the fade was drawn: the menu's middle is not what it was half way.
    assert_ne!(pixel(&a, x, y), mid, "the frames of the fade are drawn");
    // At rest it is the menu a still run draws.
    let mut still = app(&["showcase.md"]);
    still.open_popup(PopupKind::Main);
    still.compose();
    assert_eq!(pixel(&a, x, y), pixel(&still, x, y));
    // Nothing moves any more: no timer is left.
    step(&mut a, Duration::from_secs(3));
    assert!(a.anim_due.is_none());
}

#[test]
fn a_submenu_fades_in_beside_its_menu() {
    let mut a = moving(&["showcase.md", "tables.md"]);
    a.open_popup(PopupKind::Main);
    a.compose();
    for _ in 0..20 {
        step(&mut a, FRAME);
    }
    assert_eq!(a.menu_t, (1.0, 1.0));
    a.open_popup(PopupKind::Recent);
    a.compose();
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_some()));
    assert_eq!(a.menu_t, (1.0, 0.0), "the parent stays; the submenu starts");
    step(&mut a, FRAME);
    assert!(a.menu_t.1 > 0.0 && a.menu_t.1 < 1.0);
    while a.anim_due.is_some() {
        step(&mut a, FRAME);
    }
    assert_eq!(a.menu_t, (1.0, 1.0));
    // The same submenu put in its own place again does not fade in again.
    a.open_submenu(false);
    a.compose();
    assert_eq!(a.menu_t.1, 1.0);
    assert!(a.anim_due.is_none());
}

#[test]
fn a_menu_opened_again_fades_in_again_and_closing_is_instant() {
    let mut a = moving(&["showcase.md"]);
    let mut fresh = app(&["showcase.md"]);
    fresh.full = true;
    fresh.compose();
    a.open_popup(PopupKind::Main);
    a.compose();
    step(&mut a, FRAME);
    // Closed half way: gone on the next frame, with nothing of it left on screen.
    a.ui.popup = None;
    a.compose();
    assert!(a.anim_due.is_none(), "closing plays nothing");
    assert!(
        a.screen.pixels == fresh.screen.pixels,
        "nothing of the menu is left"
    );
    a.open_popup(PopupKind::Main);
    a.compose();
    assert_eq!(a.menu_t.0, 0.0, "a new opening fades in from the start");
}

#[test]
fn a_menu_that_takes_another_ones_place_fades_in() {
    let mut a = moving(&["showcase.md"]);
    a.open_popup(PopupKind::Recent);
    a.compose();
    while a.anim_due.is_some() {
        step(&mut a, FRAME);
    }
    assert_eq!(a.menu_t.0, 1.0);
    // The main menu put in its place in one event, with no frame between.
    a.open_popup(PopupKind::Main);
    assert_eq!(a.ui.popup_kind(), Some(PopupKind::Main));
    a.compose();
    assert_eq!(a.menu_t.0, 0.0, "the new menu is new");
}

#[test]
fn the_redraw_covers_everything_the_menu_touches_while_it_fades() {
    let mut a = moving(&["showcase.md", "tables.md"]);
    a.open_popup(PopupKind::Main);
    a.compose();
    a.open_popup(PopupKind::Recent);
    a.compose();
    step(&mut a, FRAME);
    let p = a.ui.popup.as_ref().unwrap();
    let (menu, sub) = (
        p.menu.presence_bounds(),
        p.sub.as_ref().unwrap().presence_bounds(),
    );
    for b in [menu, sub] {
        assert!(a.overlay_bounds.contains(&b), "{b:?} is redrawn");
    }
    // And they hold the shadow and the drop: wider than the old 20 px round the menu.
    assert!(menu.y < p.menu.rect().y - 20.0 || menu.bottom() > p.menu.rect().bottom() + 20.0);
}

#[test]
fn with_animations_off_reduced_motion_or_a_still_run_a_menu_is_whole_at_once() {
    let mut still = app(&["showcase.md"]);
    still.open_popup(PopupKind::Main);
    still.compose();
    let at_rest = still.screen.pixels.clone();
    assert_eq!(still.menu_t, (1.0, 1.0), "a test or a shot does not move");
    assert!(still.anim_due.is_none());

    // The "Animations" setting off.
    let mut off = moving(&["showcase.md"]);
    off.apply_setting(s::MOTION, Value::Bool(false));
    assert!(!off.ws.motion());
    off.open_popup(PopupKind::Main);
    off.compose();
    // The system asks for less motion; the setting is on.
    let mut reduced = moving(&["showcase.md"]);
    assert!(reduced.ws.motion());
    reduced.anim.set_system_reduced(true);
    reduced.open_popup(PopupKind::Main);
    reduced.compose();
    for (name, a) in [("setting off", &off), ("reduced motion", &reduced)] {
        assert_eq!(a.menu_t, (1.0, 1.0), "{name}");
        assert!(a.anim_due.is_none(), "{name}: no frame is asked for");
        assert!(a.screen.pixels == at_rest, "{name}: the menu at rest");
    }
}

#[test]
fn the_animations_setting_is_on_by_default_and_apart_from_the_images() {
    let mut a = app(&["showcase.md"]);
    assert!(a.ws.motion());
    a.apply_setting(s::ANIMATIONS, Value::Bool(false));
    assert!(a.ws.motion(), "turning the images off leaves the motion");
    a.apply_setting(s::MOTION, Value::Bool(false));
    assert!(!a.ws.motion());
    assert!(!a.ws.animations());
    a.apply_setting(s::ANIMATIONS, Value::Bool(true));
    assert!(
        !a.ws.motion(),
        "turning the images on leaves the motion off"
    );
}

#[test]
fn a_menu_fading_over_frosted_chrome_lets_the_backdrop_through_less() {
    use mdshared_app::backdrop::Backdrop;
    use mdshared_ui::frost::{MARK, System};
    struct Yes;
    impl System for Yes {
        fn available(&mut self) -> bool {
            true
        }
        fn set(&mut self, _: Backdrop) -> bool {
            true
        }
    }
    let mut a = moving(&["showcase.md"]);
    a.ws.set_backdrop(Backdrop::Mica);
    a.focused = true;
    let pal = a.pal;
    assert!(a.frost.sync(Backdrop::Mica, true, &pal, &mut Yes));
    a.full = true;
    a.compose();
    a.open_popup(PopupKind::Main);
    a.compose();
    step(&mut a, FRAME);
    let t = a.menu_t.0;
    assert!(t > 0.0 && t < 1.0);
    // Inside the menu, where it lies over the chrome, a pixel is neither chrome (keyed out)
    // nor opaque: it is the menu, part of the way in, over the backdrop.
    let menu = a.ui.popup.as_ref().unwrap().menu.rect().inset(8.0);
    let (w, h) = (a.screen.width as usize, a.screen.height as usize);
    let mut between = 0;
    for y in 0..h {
        for x in 0..w {
            let chrome = a
                .frost
                .areas
                .iter()
                .any(|r| x >= r.0 && x < r.2 && y >= r.1 && y < r.3);
            let (lx, ly) = (x as f32 / a.scale, y as f32 / a.scale);
            let alpha = a.screen.pixels[(y * w + x) * 4 + 3];
            if chrome && menu.contains(lx, ly) && alpha < MARK {
                between += 1;
            }
        }
    }
    assert!(between > 0, "the menu over the chrome is part way in");
}
