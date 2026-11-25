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
    // 117 ms at 16 ms a frame, and one frame more at most.
    assert!(frames <= 9, "{frames} frames");
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

#[test]
fn a_covered_window_is_left_with_no_deadline_once_the_timer_fired() {
    let mut a = moving(&["showcase.md"]);
    a.open_popup(PopupKind::Main);
    a.compose();
    let due = a.anim_due.expect("the fade asks for a frame");
    a.occluded = true;
    a.wants_frame = false;
    // The timer fires: one frame is asked for, and the deadline is gone.
    a.timers(due);
    assert!(a.wants_frame);
    assert!(a.anim_due.is_none());
    // A covered window draws nothing, so nothing sets it again: the loop goes back to waiting
    // instead of coming back at a deadline long past.
    let later = due + Duration::from_secs(1);
    assert_eq!(a.timers(later), None, "no deadline keeps the loop turning");
}

#[test]
fn a_change_of_the_systems_motion_setting_is_read_and_drawn() {
    let mut a = moving(&["showcase.md"]);
    // Taken as reduced before; the wake reads the system's answer again.
    a.anim.set_system_reduced(!platform::motion_reduced());
    a.wants_frame = false;
    a.woken(Wake::Motion);
    assert!(a.wants_frame, "a frame is asked for");
    // The frame that follows sets the policy over the system's answer.
    a.compose();
    assert_eq!(a.anim.animates(), !platform::motion_reduced());
}

#[test]
fn the_watch_on_the_systems_motion_starts_off_the_windows_thread() {
    use std::sync::mpsc;
    let mut a = moving(&["showcase.md"]);
    let (tx, rx) = mpsc::channel();
    a.proxy = Some(LoopProxy::Test(tx));
    let (seen_tx, seen_rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let started = Instant::now();
    a.watch_motion_with(move |wake| {
        let _ = seen_tx.send(std::thread::current().id());
        // A watch that takes its time, as the hidden window's may (up to 5 s).
        let _ = go_rx.recv_timeout(Duration::from_secs(5));
        wake();
    });
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the window's thread did not wait for the watch"
    );
    let thread = seen_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the watch ran");
    assert_ne!(thread, std::thread::current().id());
    go_tx.send(()).unwrap();
    assert!(matches!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Wake::Motion)
    ));
    // Once only.
    a.watch_motion_with(|_| panic!("a second watch"));
}

#[test]
fn the_appearance_page_fits_the_smallest_window_with_every_row() {
    let mut a = app(&["showcase.md"]);
    a.size = (520, 360);
    a.compose();
    // Every row, corners and (on Windows) the backdrop with their help lines included.
    let appearance = s::PAGES.iter().position(|p| *p == s::APPEARANCE).unwrap();
    a.ui.settings = Some(s::form(&a.ws, true, appearance));
    a.compose();
    let f = a.ui.settings.as_ref().unwrap();
    let ids: Vec<u32> = f.pages()[appearance]
        .items
        .iter()
        .filter_map(|it| it.row().id())
        .collect();
    assert_eq!(ids.last(), Some(&s::MOTION), "Animations is the last row");
    assert!(ids.contains(&s::WINDOW_CORNERS));
    let (row, card, page) = (f.row_rect(s::MOTION), f.rect(), f.page_rect());
    assert!(row.h > 0.0, "the row is laid out");
    assert!(
        row.bottom() <= card.bottom() && row.bottom() <= page.bottom(),
        "the last row {row:?} lies inside the card {card:?} and its page {page:?}"
    );
}

// ---- the Files pane's chevrons ---------------------------------------------------------------

/// A folder of two folders, each with a file, and a file: `a/`, `b/`, `c.md`.
fn folder_tree(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mdview-turn-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for sub in ["a", "b"] {
        std::fs::create_dir_all(d.join(sub)).unwrap();
        std::fs::write(d.join(sub).join("x.md"), "# x\n").unwrap();
    }
    std::fs::write(d.join("c.md"), "# c\n").unwrap();
    d
}

fn tree_row(a: &App, name: &str) -> usize {
    a.ui.tree
        .rows()
        .iter()
        .position(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row {name}"))
}

/// The pixels of the chevron of tree row `i`, where the Files pane draws it.
fn chevron(a: &App, i: usize) -> Vec<u8> {
    let row = a.hit_rect(Hit::FilesRow(i)).expect("the row is on screen");
    let files = a.areas.region(Region::Files).expect("the Files pane");
    let depth = a.ui.tree.rows()[i].depth as f32;
    let cx = files.x + 14.0 + (depth + 1.0) * 14.0 - 8.0;
    crop(a, Rect::new(cx - 7.0, row.cy() - 7.0, 14.0, 14.0))
}

fn crop(a: &App, r: Rect) -> Vec<u8> {
    let w = a.screen.width as usize;
    let (x0, y0) = ((r.x * a.scale) as usize, (r.y * a.scale) as usize);
    let (x1, y1) = (
        (r.right() * a.scale) as usize,
        (r.bottom() * a.scale) as usize,
    );
    let mut out = Vec::new();
    for y in y0..y1 {
        out.extend_from_slice(&a.screen.pixels[(y * w + x0) * 4..(y * w + x1) * 4]);
    }
    out
}

/// An app showing `dir` in the Files pane, moving or still.
fn with_tree(dir: &Path, moving_app: bool) -> App {
    let mut a = if moving_app {
        moving(&["showcase.md"])
    } else {
        app(&["showcase.md"])
    };
    a.ui.tree.set_root(Some(dir.to_path_buf()));
    a.compose();
    a
}

#[test]
fn a_folder_that_opens_turns_its_chevron_and_then_leaves_no_timer() {
    let dir = folder_tree("opens");
    let mut a = with_tree(&dir, true);
    let b = tree_row(&a, "b");
    let closed = chevron(&a, b);
    // Frames that leave the pane undrawn: its chevrons are kept, not forgotten.
    for _ in 0..3 {
        step(&mut a, FRAME);
    }
    assert!(a.anim_due.is_none(), "nothing moves at rest");
    a.ui.tree.toggle(b);
    step(&mut a, FRAME);
    assert!(a.anim_due.is_some(), "the turn asks for a frame");
    assert!(chevron(&a, b) == closed, "the turn starts where it was");
    step(&mut a, FRAME);
    let mid = chevron(&a, b);
    assert!(
        mid != closed,
        "on its way: drawn on a frame nothing else changed"
    );
    let mut frames = 0;
    while a.anim_due.is_some() {
        step(&mut a, FRAME);
        frames += 1;
        assert!(frames < 30, "the turn ends");
    }
    // At rest: the open chevron a still run draws, and no timer is left.
    let mut still = with_tree(&dir, false);
    still.ui.tree.toggle(tree_row(&still, "b"));
    still.compose();
    let open = chevron(&still, b);
    assert!(mid != open);
    assert!(chevron(&a, b) == open, "at rest, open");
    assert!(a.anim.group_deadlines().is_empty());
    step(&mut a, Duration::from_secs(1));
    assert!(a.anim_due.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_chevron_is_named_by_its_folder_and_keeps_turning_when_its_row_moves() {
    let dir = folder_tree("moves");
    let mut a = with_tree(&dir, true);
    let b = tree_row(&a, "b");
    a.ui.tree.toggle(b);
    step(&mut a, FRAME);
    step(&mut a, FRAME);
    // The folder above opens: "b" is now a row further down, still part of the way round.
    a.ui.tree.toggle(tree_row(&a, "a"));
    step(&mut a, FRAME);
    let b = tree_row(&a, "b");
    let mut still = with_tree(&dir, false);
    still.ui.tree.toggle(tree_row(&still, "b"));
    still.ui.tree.toggle(tree_row(&still, "a"));
    still.compose();
    assert_eq!(tree_row(&still, "b"), b);
    assert!(
        chevron(&a, b) != chevron(&still, b),
        "the moved row's chevron is still turning, not taken for new"
    );
    while a.anim_due.is_some() {
        step(&mut a, FRAME);
    }
    assert!(chevron(&a, b) == chevron(&still, b), "at rest, open");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_animations_off_or_a_still_run_a_folder_opens_at_once() {
    let dir = folder_tree("still");
    let mut still = with_tree(&dir, false);
    let b = tree_row(&still, "b");
    still.ui.tree.toggle(b);
    still.compose();
    assert!(still.anim_due.is_none());
    let open = chevron(&still, b);
    let mut off = with_tree(&dir, true);
    off.apply_setting(s::MOTION, Value::Bool(false));
    let mut reduced = with_tree(&dir, true);
    reduced.anim.set_system_reduced(true);
    for (name, a) in [("setting off", &mut off), ("reduced motion", &mut reduced)] {
        a.compose();
        a.ui.tree.toggle(b);
        step(a, FRAME);
        assert!(a.anim_due.is_none(), "{name}: no frame is asked for");
        assert!(chevron(a, b) == open, "{name}: open at once");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_group_of_the_notes_pane_turns_its_chevron_as_it_folds() {
    let root = std::env::temp_dir().join(format!("mdview-turn-notes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Personal")).unwrap();
    std::fs::write(root.join("Personal/Budget.md"), "# Budget\n").unwrap();
    std::fs::write(
        root.join("Personal.md"),
        "# Personal\n\n## Money\n\n- [Budget](<Personal/Budget.md>)\n",
    )
    .unwrap();
    let mut a = moving(&["showcase.md"]);
    a.ui.notes = crate::notes::Notes::new(Some(root.clone()));
    a.ws.set_library_offered(true);
    a.pane_face(1);
    a.compose();
    let i = (a.ui.notes.rows.iter())
        .position(|r| r.label == "Personal")
        .expect("the group's row");
    let row = a.hit_rect(Hit::NotesRow(i)).expect("on screen");
    let files = a.areas.region(Region::Files).unwrap();
    let cx = files.x + 14.0 + (a.ui.notes.rows[i].depth as f32 + 1.0) * 14.0 - 8.0;
    let at = Rect::new(cx - 7.0, row.cy() - 7.0, 14.0, 14.0);
    let open = crop(&a, at);
    step(&mut a, FRAME);
    a.notes_row(i);
    step(&mut a, FRAME);
    assert!(a.anim_due.is_some(), "the fold turns the chevron");
    step(&mut a, FRAME);
    let mid = crop(&a, at);
    assert!(mid != open, "on its way");
    while a.anim_due.is_some() {
        step(&mut a, FRAME);
    }
    let folded = crop(&a, at);
    assert!(mid != folded && open != folded, "folded at rest");
    let _ = std::fs::remove_dir_all(&root);
}

// ---- toasts ------------------------------------------------------------------------------------

/// Composes frames 16 ms apart until nothing moves.
fn settle(a: &mut App) -> usize {
    let mut frames = 0;
    while a.anim_due.is_some() {
        step(a, FRAME);
        frames += 1;
        assert!(frames < 60, "the motion ends");
    }
    frames
}

/// This frame's toasts: text, presence and lift, the bottom one first.
fn toasts(a: &App) -> Vec<(String, f32, f32)> {
    (a.toast_frame.iter())
        .map(|(_, text, t, lift)| (text.clone(), *t, *lift))
        .collect()
}

#[test]
fn a_toast_rises_in_stays_then_leaves_of_itself_with_no_timer_left() {
    let mut fresh = app(&["showcase.md"]);
    fresh.full = true;
    fresh.compose();
    let mut still = app(&["showcase.md"]);
    still.ws.set_status("Hello");
    still.compose();
    let mut a = moving(&["showcase.md"]);
    a.ws.set_status("Hello");
    a.compose();
    assert_eq!(toasts(&a), [("Hello".into(), 0.0, 0.0)], "it starts gone");
    assert!(a.anim_due.is_some(), "the entrance asks for frames");
    step(&mut a, FRAME);
    step(&mut a, FRAME);
    let t = toasts(&a)[0].1;
    assert!(t > 0.0 && t < 1.0, "on its way in: {t}");
    let mid = a.screen.pixels.clone();
    assert!(settle(&mut a) <= 12);
    assert_eq!(toasts(&a), [("Hello".into(), 1.0, 0.0)]);
    assert!(
        mid != a.screen.pixels,
        "each frame of the entrance is drawn"
    );
    assert!(
        a.screen.pixels == still.screen.pixels,
        "at rest it is the toast a still run draws"
    );
    // Its time to leave is the timer the loop waits for; nothing else is.
    let until = a.ws.status_expires().unwrap();
    assert_eq!(a.toast_until, Some(until));
    assert!(a.anim_due.is_none());
    a.wants_frame = false;
    a.timers(until);
    assert!(a.wants_frame && a.toast_until.is_none());
    a.frame_clock = Some(until);
    a.compose();
    assert!(a.anim_due.is_some(), "it leaves by an exit");
    step(&mut a, FRAME);
    step(&mut a, FRAME);
    let t = toasts(&a)[0].1;
    assert!(t > 0.0 && t < 1.0, "on its way out: {t}");
    settle(&mut a);
    assert!(a.toasts.is_empty() && a.toast_frame.is_empty());
    assert!(
        a.anim_due.is_none() && a.toast_until.is_none(),
        "no timer is left"
    );
    assert!(
        a.screen.pixels == fresh.screen.pixels,
        "nothing of it is left on screen"
    );
    assert_eq!(a.timers(until + Duration::from_secs(5)), None);
}

#[test]
fn a_second_toast_stacks_under_the_first_which_moves_up() {
    let mut a = moving(&["showcase.md"]);
    a.ws.confirm("First");
    a.compose();
    settle(&mut a);
    a.ws.confirm("Second");
    a.compose();
    step(&mut a, FRAME);
    step(&mut a, FRAME);
    let now = toasts(&a);
    assert_eq!(now.len(), 2);
    assert_eq!(
        (now[0].0.as_str(), now[0].2),
        ("Second", 0.0),
        "the new one at the bottom"
    );
    assert!(now[0].1 < 1.0, "coming in");
    let lifting = now[1].2;
    assert!(
        lifting > 0.0 && lifting < 44.0,
        "the first on its way up: {lifting}"
    );
    settle(&mut a);
    assert_eq!(
        toasts(&a),
        [("Second".into(), 1.0, 0.0), ("First".into(), 1.0, 44.0)]
    );
    // The same message said again keeps its toast, for longer: no third one.
    let before = a.toast_until;
    a.ws.confirm("Second");
    a.compose();
    assert_eq!(toasts(&a).len(), 2);
    assert!(a.toasts.current().count() == 2);
    assert_eq!(a.toast_until, before, "the first still leaves first");
    // A status cleared sends them all away.
    a.ws.clear_status();
    a.compose();
    settle(&mut a);
    assert!(a.toasts.is_empty());
    assert!(a.anim_due.is_none() && a.toast_until.is_none());
}

#[test]
fn the_redraw_covers_everything_a_toast_touches_as_it_rises() {
    let mut a = moving(&["showcase.md"]);
    a.ws.set_status("Hello");
    a.compose();
    step(&mut a, FRAME);
    let page = a.areas.page;
    let b = {
        let mut p = Painter::new(&mut a.screen, &mut a.gfx, a.scale);
        Bubble::toast(&mut p, "Hello", page)
    };
    assert!(a.overlay_bounds.contains(&b.presence_bounds()));
    assert!(
        b.presence_bounds().bottom() > b.bounds().bottom(),
        "the rise is held"
    );
}

#[test]
fn with_animations_off_reduced_motion_or_a_still_run_a_toast_comes_and_goes_at_once() {
    let mut still = app(&["showcase.md"]);
    still.ws.set_status("Hello");
    still.compose();
    assert_eq!(toasts(&still), [("Hello".into(), 1.0, 0.0)]);
    assert!(still.anim_due.is_none());
    let mut off = moving(&["showcase.md"]);
    off.apply_setting(s::MOTION, Value::Bool(false));
    let mut reduced = moving(&["showcase.md"]);
    reduced.anim.set_system_reduced(true);
    for (name, a) in [("setting off", &mut off), ("reduced motion", &mut reduced)] {
        a.ws.set_status("Hello");
        a.compose();
        assert_eq!(toasts(a), [("Hello".into(), 1.0, 0.0)], "{name}");
        assert!(a.anim_due.is_none(), "{name}: no frame is asked for");
        assert!(a.screen.pixels == still.screen.pixels, "{name}: at rest");
        // Its time up: gone on that frame.
        a.frame_clock = a.ws.status_expires();
        a.compose();
        assert!(a.toasts.is_empty(), "{name}: gone at once");
        assert!(a.anim_due.is_none(), "{name}");
    }
}

#[test]
fn zoom_steps_replace_the_toast_instead_of_stacking() {
    let mut a = moving(&["showcase.md"]);
    for _ in 0..3 {
        a.ws.zoom_in();
        a.compose();
        step(&mut a, FRAME);
    }
    let shown = toasts(&a);
    assert_eq!(shown.len(), 1, "one toast: {shown:?}");
    assert_eq!(shown[0].0, format!("Zoom {}%", a.ws.zoom()));
    assert_eq!(shown[0].2, 0.0, "nothing lifts");
    settle(&mut a);
    assert_eq!(toasts(&a).len(), 1);
}

#[test]
fn an_exports_progress_then_its_result_leave_one_toast() {
    let mut a = moving(&["showcase.md"]);
    a.export_progress(super::export::Progress::Working);
    a.compose();
    settle(&mut a);
    a.export_progress(super::export::Progress::Saved(PathBuf::from("x.html")));
    a.compose();
    let shown = toasts(&a);
    assert_eq!(
        shown,
        [("Exported to x.html".into(), 1.0, 0.0)],
        "in the progress's place, at once"
    );
    settle(&mut a);
    assert_eq!(toasts(&a), [("Exported to x.html".into(), 1.0, 0.0)]);
}

#[test]
fn never_more_than_three_toasts_show_those_leaving_included() {
    let mut a = moving(&["showcase.md"]);
    for text in ["One", "Two", "Three", "Four"] {
        a.ws.confirm(text);
        a.compose();
        step(&mut a, FRAME);
        assert!(a.toast_frame.len() <= 3, "{:?}", toasts(&a));
    }
    // "One" was sent away by the fourth and is leaving: it is not drawn as a fourth.
    let names: Vec<String> = toasts(&a).into_iter().map(|t| t.0).collect();
    assert_eq!(names, ["Four", "Three", "Two"]);
    settle(&mut a);
    assert_eq!(a.toasts.current().count(), 3);
    assert_eq!(a.toast_frame.len(), 3);
}

#[test]
fn the_same_confirmation_keeps_its_toast_wherever_it_is_in_the_stack() {
    let mut a = moving(&["showcase.md"]);
    for text in ["Copied", "Link address copied"] {
        a.ws.confirm(text);
        a.compose();
    }
    settle(&mut a);
    a.ws.confirm("Copied");
    a.compose();
    settle(&mut a);
    let names: Vec<String> = toasts(&a).into_iter().map(|t| t.0).collect();
    assert_eq!(names, ["Link address copied", "Copied"], "no second Copied");
}

#[test]
fn a_status_already_past_its_time_shows_no_toast() {
    let mut a = moving(&["showcase.md"]);
    a.ws.set_status("Long ago");
    // The first frame after it comes after its time (a window covered all along).
    a.frame_clock = a.ws.status_expires().map(|u| u + Duration::from_millis(1));
    a.compose();
    assert!(a.toasts.is_empty() && a.toast_frame.is_empty());
    assert!(a.anim_due.is_none() && a.toast_until.is_none());
}

#[test]
fn a_toast_leaves_while_a_menu_or_a_dialog_is_open() {
    for dialog in [false, true] {
        let mut a = moving(&["showcase.md"]);
        a.ws.set_status("Hello");
        a.compose();
        settle(&mut a);
        if dialog {
            a.open_dialog(DialogKind::About);
        } else {
            a.open_popup(PopupKind::Main);
        }
        a.compose();
        settle(&mut a);
        a.frame_clock = a.ws.status_expires();
        a.compose();
        assert!(a.anim_due.is_some(), "it leaves by its exit");
        settle(&mut a);
        assert!(a.toasts.is_empty(), "gone");
        assert!(a.anim_due.is_none(), "no timer left");
        // What is on screen is the menu or the dialog alone.
        let mut b = app(&["showcase.md"]);
        if dialog {
            b.open_dialog(DialogKind::About);
        } else {
            b.open_popup(PopupKind::Main);
        }
        b.full = true;
        b.compose();
        a.full = true;
        a.compose();
        assert!(
            a.screen.pixels == b.screen.pixels,
            "dialog {dialog}: nothing of it left"
        );
    }
}

#[test]
fn a_window_resized_while_a_toast_comes_in_shows_it_in_its_new_place() {
    let mut a = moving(&["showcase.md"]);
    a.ws.set_status("Hello");
    a.compose();
    step(&mut a, FRAME);
    step(&mut a, FRAME);
    a.size = (900, 600);
    settle(&mut a);
    let mut still = app(&["showcase.md"]);
    still.size = (900, 600);
    still.ws.set_status("Hello");
    still.compose();
    assert!(
        a.screen.pixels == still.screen.pixels,
        "at rest in the new window"
    );
}

#[test]
fn toasts_that_would_reach_above_the_page_are_left_out() {
    let mut a = app(&["showcase.md"]);
    a.size = (520, 200);
    a.compose();
    for text in ["One", "Two", "Three"] {
        a.ws.confirm(text);
        a.compose();
    }
    assert_eq!(a.toast_frame.len(), 3);
    let page = a.areas.page;
    let drawn: Vec<Rect> = (a.overlay_bounds.iter().copied())
        .filter(|b| b.y < page.bottom())
        .collect();
    assert!(
        drawn.len() < 3,
        "fewer than three fit: {drawn:?} in {page:?}"
    );
    for b in &drawn {
        assert!(b.y + 16.0 >= page.y, "{b:?} lies in the page {page:?}");
    }
}

#[test]
fn a_lifting_toast_is_drawn_at_a_whole_device_pixel() {
    for scale in [1.0_f32, 1.25, 1.5, 2.0] {
        for lift in [0.3_f32, 13.37, 29.9, 44.0] {
            let moved = (701.0 - super::toast_y(701.0, lift, scale)) * scale;
            assert!(
                (moved - moved.round()).abs() < 1e-3,
                "{lift} at {scale}: {moved}"
            );
            assert!((moved / scale - lift).abs() <= 0.5 / scale + 1e-3);
        }
    }
}

#[test]
fn moving_shots_move_whatever_the_setting_says() {
    let mut a = app(&["showcase.md"]);
    a.apply_setting(s::MOTION, Value::Bool(false));
    a.anim.set_system_reduced(true);
    a.open_popup(PopupKind::Main);
    let start = super::start_moving(&mut a);
    super::frames_until(&mut a, start, 21);
    let t = a.menu_t.0;
    assert!(t > 0.0 && t < 1.0, "half way: {t}");
}

#[test]
fn the_files_pane_hidden_mid_turn_leaves_no_timer() {
    let dir = folder_tree("hidden");
    let mut a = with_tree(&dir, true);
    a.ui.tree.toggle(tree_row(&a, "b"));
    step(&mut a, FRAME);
    step(&mut a, FRAME);
    assert!(a.anim_due.is_some());
    a.ui.panes.files = false;
    let mut frames = 0;
    while a.anim_due.is_some() {
        step(&mut a, FRAME);
        frames += 1;
        assert!(
            frames < 30,
            "the hidden pane's turn stops asking for frames"
        );
    }
    step(&mut a, Duration::from_secs(1));
    assert!(a.anim_due.is_none());
    assert!(a.anim.group_deadlines().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_window_too_short_for_a_toast_still_shows_its_one_status() {
    let mut a = app(&["showcase.md"]);
    a.size = (520, 120);
    a.compose();
    a.ws.set_status("Zoom 110%");
    a.compose();
    let page = a.areas.page;
    let mut p = Painter::new(&mut a.screen, &mut a.gfx, a.scale);
    let rest = Bubble::toast(&mut p, "Zoom 110%", page).rect;
    assert!(
        rest.y < page.y + crate::motion::TOAST_GAP,
        "too short: {rest:?} in {page:?}"
    );
    assert_eq!(a.overlay_bounds.len(), 1, "the bottom toast is drawn");
}

#[test]
fn a_zoom_step_while_its_toast_is_at_rest_changes_it_in_place() {
    let mut a = moving(&["showcase.md"]);
    a.ws.zoom_in();
    a.compose();
    settle(&mut a);
    assert_eq!(toasts(&a)[0].1, 1.0);
    a.ws.zoom_in();
    a.compose();
    let shown = toasts(&a);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(
        shown[0].0,
        format!("Zoom {}%", a.ws.zoom()),
        "the new text at once"
    );
    assert_eq!(shown[0].1, 1.0, "no frame without it, no entrance");
}

#[test]
fn under_repeated_zoom_steps_the_toast_never_fades() {
    let mut a = moving(&["showcase.md"]);
    let mut last = 0.0_f32;
    for i in 0..40 {
        // In and out, so every step is a new message (a step past the limit says nothing).
        if i % 2 == 0 {
            a.ws.zoom_in();
        } else {
            a.ws.zoom_out();
        }
        a.compose();
        step(&mut a, FRAME);
        let shown = toasts(&a);
        assert_eq!(shown.len(), 1, "step {i}: {shown:?}");
        assert!(shown[0].1 >= last, "step {i}: {} after {last}", shown[0].1);
        assert_eq!(shown[0].2, 0.0, "step {i}: nothing lifts");
        last = shown[0].1;
    }
    assert_eq!(last, 1.0, "in, and staying in");
}

#[test]
fn a_replacing_status_with_none_showing_comes_in_from_nothing() {
    let mut a = moving(&["showcase.md"]);
    a.ws.set_status("Hello");
    a.compose();
    assert_eq!(toasts(&a)[0].1, 0.0, "it enters");
    settle(&mut a);
    // Cleared, it leaves; the clock stays this side of the next message's time.
    a.ws.set_status("");
    a.compose();
    settle(&mut a);
    assert!(a.toasts.is_empty());
    a.ws.zoom_in();
    a.compose();
    assert_eq!(
        toasts(&a)[0].1,
        0.0,
        "after the last has gone, the next enters again"
    );
}

#[test]
fn a_confirmation_said_again_while_it_leaves_comes_back() {
    let mut a = moving(&["showcase.md"]);
    a.ws.confirm("Copied");
    a.compose();
    settle(&mut a);
    a.ws.set_status("");
    a.compose();
    step(&mut a, FRAME);
    step(&mut a, FRAME);
    let going = toasts(&a)[0].1;
    assert!(going > 0.0 && going < 1.0, "leaving: {going}");
    a.ws.confirm("Copied");
    a.compose();
    let shown = toasts(&a);
    assert_eq!(shown.len(), 1, "no second Copied: {shown:?}");
    assert!(
        shown[0].1 >= going - 0.2,
        "turns round where it is: {}",
        shown[0].1
    );
    settle(&mut a);
    assert_eq!(toasts(&a), [("Copied".into(), 1.0, 0.0)]);
}

#[test]
fn a_replacing_status_takes_the_place_of_the_confirmations_showing() {
    let mut a = moving(&["showcase.md"]);
    for text in ["Copied", "Link address copied"] {
        a.ws.confirm(text);
        a.compose();
    }
    settle(&mut a);
    a.ws.zoom_in();
    a.compose();
    let shown = toasts(&a);
    assert_eq!(shown.len(), 1, "the others went at once: {shown:?}");
    assert_eq!(shown[0].1, 1.0, "in the newest one's place");
    assert_eq!(shown[0].2, 0.0);
}

// ---- the settings dialog in motion ----------------------------------------------------------

/// Opens the settings dialog with the row `row` focused.
fn settings_at(a: &mut App, row: u32) {
    a.command(C::Settings);
    a.compose();
    assert!(a.ui.settings.as_mut().unwrap().focus_row(row));
    // Its page drawn (as a click on it in the list would), so its rows are seen at rest.
    a.full = true;
    a.compose();
}

#[test]
fn a_switch_in_the_settings_dialog_slides_then_rests_as_a_still_run_draws_it() {
    let mut a = moving(&["showcase.md"]);
    settings_at(&mut a, s::TICK_BOXES);
    settle(&mut a);
    let off = a.screen.pixels.clone();
    a.key_named(KeyName::Space, None);
    a.compose();
    step(&mut a, FRAME);
    step(&mut a, FRAME);
    assert!(a.anim_due.is_some(), "the knob is on its way");
    let mid = a.screen.pixels.clone();
    settle(&mut a);
    let on = a.screen.pixels.clone();
    assert!(mid != off && mid != on, "drawn part way");
    assert!(a.anim_due.is_none(), "no timer left");
    let mut b = app(&["showcase.md"]);
    settings_at(&mut b, s::TICK_BOXES);
    b.key_named(KeyName::Space, None);
    b.full = true;
    b.compose();
    a.full = true;
    a.compose();
    assert!(
        a.screen.pixels == b.screen.pixels,
        "at rest, what a still run draws"
    );
}

#[test]
fn with_animations_off_a_settings_switch_changes_at_once() {
    let mut a = moving(&["showcase.md"]);
    a.apply_setting(s::MOTION, Value::Bool(false));
    settings_at(&mut a, s::TICK_BOXES);
    a.key_named(KeyName::Space, None);
    a.compose();
    assert!(a.anim_due.is_none(), "nothing moves");
    let mut b = app(&["showcase.md"]);
    b.apply_setting(s::MOTION, Value::Bool(false));
    settings_at(&mut b, s::TICK_BOXES);
    b.key_named(KeyName::Space, None);
    b.full = true;
    b.compose();
    a.full = true;
    a.compose();
    assert!(
        a.screen.pixels == b.screen.pixels,
        "the switch is on at once"
    );
}

// ---- tooltips, from resting -----------------------------------------------------------------

/// The middle of the minimize button, which has a tip.
fn minimize(a: &mut App) -> (f32, f32) {
    a.compose();
    let r = a.hit_rect(Hit::WinMin).expect("the minimize button");
    (r.cx(), r.cy())
}

#[test]
fn a_move_on_the_same_control_starts_the_tips_wait_again() {
    let mut a = app(&["showcase.md"]);
    let (x, y) = minimize(&mut a);
    a.pointer_moved(x, y);
    let first = a.ui.tips.deadline().expect("it waits");
    std::thread::sleep(Duration::from_millis(5));
    a.pointer_moved(x + 1.0, y);
    let again = a.ui.tips.deadline().expect("it still waits");
    assert!(again > first, "counted from the last move");
    // A move event to where the pointer already is is not a move.
    a.pointer_moved(x + 1.0, y);
    assert_eq!(a.ui.tips.deadline(), Some(again));
    assert!(a.ui.tips.shown().is_none());
}

#[test]
fn resting_the_delay_shows_the_tip_and_leaving_hides_it() {
    let mut a = app(&["showcase.md"]);
    let (x, y) = minimize(&mut a);
    a.pointer_moved(x, y);
    let due = a.ui.tips.deadline().expect("it waits");
    a.timers(due - Duration::from_millis(1));
    assert!(a.ui.tips.shown().is_none(), "not before the delay");
    a.timers(due);
    assert_eq!(a.ui.tips.shown().map(|t| t.text.as_str()), Some("Minimize"));
    a.compose();
    assert_eq!(a.tip_t, 1.0);
    let page = a.areas.page;
    a.pointer_moved(page.cx(), page.cy());
    assert!(a.ui.tips.shown().is_none() && a.ui.tips.deadline().is_none());
    a.compose();
    assert_eq!(a.tip_t, 0.0, "gone at once");
}

#[test]
fn a_tip_fades_in_as_it_shows_and_with_animations_off_is_there_at_once() {
    for animations in [true, false] {
        let mut a = moving(&["showcase.md"]);
        a.apply_setting(s::MOTION, Value::Bool(animations));
        let (x, y) = minimize(&mut a);
        a.pointer_moved(x, y);
        let due = a.ui.tips.deadline().expect("it waits");
        a.timers(due);
        a.compose();
        if animations {
            assert_eq!(a.tip_t, 0.0, "it fades in");
            step(&mut a, FRAME);
            assert!(a.tip_t > 0.0 && a.tip_t < 1.0, "part way: {}", a.tip_t);
            settle(&mut a);
            assert_eq!(a.tip_t, 1.0);
        } else {
            assert_eq!(a.tip_t, 1.0, "at once");
            assert!(a.anim_due.is_none(), "no frames asked");
        }
    }
}

#[test]
fn a_frame_where_only_the_settings_move_redraws_the_card_alone_as_a_whole_redraw_would() {
    for (scale, size) in [(1.0_f32, (1280, 800)), (1.5, (1920, 1200))] {
        let mut a = moving(&["showcase.md"]);
        a.scale = scale;
        a.size = size;
        settings_at(&mut a, s::TICK_BOXES);
        settle(&mut a);
        a.key_named(KeyName::Space, None);
        a.compose();
        let card = a.ui.settings.as_ref().unwrap().rect();
        let mut clipped = 0;
        while a.anim_due.is_some() {
            a.frame_clock = a.frame_clock.map(|t| t + FRAME);
            let (damage, _) = a.compose();
            let part = a.screen.pixels.clone();
            let (x1, y1) = (
                (card.right() * scale) as i32,
                (card.bottom() * scale) as i32,
            );
            if damage
                .iter()
                .all(|d| d.0 >= (card.x * scale) as i32 && d.2 <= x1 + 1 && d.3 <= y1 + 1)
            {
                clipped += 1;
            }
            // The same frame drawn whole.
            a.full = true;
            a.compose();
            assert!(
                part == a.screen.pixels,
                "at {scale}: the card alone is the whole frame"
            );
        }
        assert!(
            clipped >= 3,
            "at {scale}: frames of the card alone: {clipped}"
        );
    }
}

#[test]
fn a_page_lists_hover_fades_in_frames_of_the_list_alone_as_a_whole_redraw_would() {
    let mut a = moving(&["showcase.md"]);
    settings_at(&mut a, s::TICK_BOXES);
    settle(&mut a);
    let item = a.ui.settings.as_ref().unwrap().list_item_rect(0);
    a.pointer_moved(item.cx(), item.cy());
    a.compose();
    let mut frames = 0;
    while a.anim_due.is_some() {
        a.frame_clock = a.frame_clock.map(|t| t + FRAME);
        a.compose();
        let part = a.screen.pixels.clone();
        a.full = true;
        a.compose();
        assert!(
            part == a.screen.pixels,
            "frame {frames}: the list alone is the whole frame"
        );
        frames += 1;
        assert!(frames < 60);
    }
    assert!(frames >= 2, "the hover fill fades in: {frames} frames");
}
