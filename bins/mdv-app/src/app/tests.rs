//! Headless interaction tests: a real `App` without a window, composed at 1280x800 and driven
//! through the same pointer and key handlers the event loop calls.

use super::*;
use mdview_core::commands::CommandId;
use mdview_core::session::PageWidth;

pub(super) fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

pub(super) fn app(files: &[&str]) -> App {
    let args = Args {
        files: files.iter().map(|f| fixture(f)).collect(),
        dark: Some(true),
        files_pane: Some(true),
        outline_pane: Some(true),
        no_session: true,
        ..Args::default()
    };
    let mut app = App::new(args, Workspace::new(None), None);
    app.scale = 1.0;
    app.size = (1280, 800);
    app.setup();
    app.compose();
    app
}

fn rect(app: &App, hit: Hit) -> Rect {
    app.hit_rect(hit)
        .unwrap_or_else(|| panic!("{hit:?} is not on screen"))
}

fn click(app: &mut App, hit: Hit) {
    let r = rect(app, hit);
    app.pointer_moved(r.cx(), r.cy());
    app.left_press();
    app.left_release();
    // Clicks in tests are far apart in intent: never count them as a double click.
    app.ui.last_press = None;
    app.compose();
}

fn key(app: &mut App, name: KeyName, typed: Option<&str>) {
    app.key_named(name, typed);
    app.compose();
}

pub(super) fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        let s = c.to_string();
        key(app, KeyName::Char(c), Some(&s));
    }
}

#[test]
fn tab_close_button_and_middle_click_close_tabs() {
    let mut a = app(&["linked.md", "math.md", "images.md"]);
    assert_eq!(a.ws.tab_count(), 3);
    let active = a.ws.active().unwrap();
    click(&mut a, Hit::TabClose(active));
    assert_eq!(a.ws.tab_count(), 2);
    let r = rect(&a, Hit::Tab(0));
    a.pointer_moved(r.cx(), r.cy());
    a.middle_press();
    a.compose();
    assert_eq!(a.ws.tab_count(), 1);
}

#[test]
fn dragging_a_tab_reorders_tabs() {
    let mut a = app(&["linked.md", "math.md"]);
    let before = a.ws.labels().to_vec();
    let (from, to) = (rect(&a, Hit::Tab(0)), rect(&a, Hit::Tab(1)));
    a.pointer_moved(from.cx(), from.cy());
    a.left_press();
    a.pointer_moved(to.cx(), to.cy());
    a.left_release();
    a.compose();
    let after = a.ws.labels().to_vec();
    assert_eq!(after, vec![before[1].clone(), before[0].clone()]);
    assert!(a.ui.drag.is_none());
}

#[test]
fn toolbar_toggles_panes_and_the_layout_follows() {
    let mut a = app(&["linked.md"]);
    assert!(a.areas.files.is_some() && a.areas.outline.is_some());
    click(&mut a, Hit::Bar(BarItem::Files));
    assert!(!a.ui.panes.files);
    assert!(a.areas.files.is_none());
    assert_eq!(a.ws.panes(), (false, true));
    click(&mut a, Hit::Bar(BarItem::Outline));
    assert!(a.areas.outline.is_none());
    assert_eq!(a.ws.panes(), (false, false));
    click(&mut a, Hit::Bar(BarItem::Files));
    assert!(a.areas.files.is_some());
}

#[test]
fn the_page_width_button_steps_through_the_widths() {
    use mdview_core::session::PageWidth;
    let mut a = app(&["linked.md"]);
    assert_eq!(a.ws.page_width(), PageWidth::Narrow);
    click(&mut a, Hit::Bar(BarItem::PageWidth));
    assert_eq!(a.ws.page_width(), PageWidth::Wide);
    click(&mut a, Hit::Bar(BarItem::PageWidth));
    assert_eq!(a.ws.page_width(), PageWidth::Full);
    click(&mut a, Hit::Bar(BarItem::PageWidth));
    assert_eq!(a.ws.page_width(), PageWidth::Narrow);
}

#[test]
fn right_click_in_the_find_box_opens_its_edit_menu() {
    use crate::ui::EditAct;
    let mut a = app(&["linked.md"]);
    click(&mut a, Hit::Find);
    type_text(&mut a, "abc");
    let r = rect(&a, Hit::Find);
    a.pointer_moved(r.cx(), r.cy());
    a.right_press();
    a.compose();
    assert_eq!(a.ui.popup_kind(), Some(PopupKind::Find));
    assert!(a.ui.find_focused);
    a.act(Act::Edit(EditAct::SelectAll));
    assert_eq!(a.ui.find.selected_text(), "abc");
    assert!(a.ui.find_focused);
}

#[test]
fn a_task_box_click_asks_and_cancel_changes_nothing() {
    use crate::workspace::TaskClick;
    let mut a = app(&["linked.md"]);
    a.handle_click(Click::Task(TaskClick::Ask));
    a.compose();
    assert_eq!(
        a.ui.dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::AllowEdit)
    );
    click(&mut a, Hit::DlgButton(0));
    assert!(a.ui.dialog.is_none());
    assert!(!a.ws.edit_always());
}

#[test]
fn splitter_drag_resizes_within_limits() {
    let mut a = app(&["linked.md"]);
    let w0 = a.areas.files.unwrap().w;
    let s = rect(&a, Hit::Splitter(Side::Files));
    a.pointer_moved(s.cx(), s.cy());
    a.left_press();
    a.pointer_moved(s.cx() + 100.0, s.cy());
    a.left_release();
    a.compose();
    assert_eq!(a.areas.files.unwrap().w, w0 + 100.0);
    // Far past the limit: clamped to the pane maximum.
    let s = rect(&a, Hit::Splitter(Side::Files));
    a.pointer_moved(s.cx(), s.cy());
    a.left_press();
    a.pointer_moved(s.cx() + 2000.0, s.cy());
    a.left_release();
    a.compose();
    assert_eq!(a.areas.files.unwrap().w, PANE_MAX);
}

#[test]
fn find_box_types_searches_and_escape_clears() {
    let mut a = app(&["showcase.md"]);
    click(&mut a, Hit::Find);
    assert!(a.ui.find_focused);
    type_text(&mut a, "the");
    assert_eq!(a.ui.find.text(), "the");
    assert_eq!(a.ws.find_query(), "the");
    let counts = a.ws.find_counts();
    assert!(counts.total > 1, "{counts:?}");
    assert_eq!(counts.current, 1);
    key(&mut a, KeyName::Enter, None);
    assert_eq!(a.ws.find_counts().current, 2);
    key(&mut a, KeyName::Backspace, None);
    assert_eq!(a.ws.find_query(), "th");
    key(&mut a, KeyName::Escape, None);
    assert!(!a.ui.find_focused);
    assert_eq!(a.ws.find_query(), "");
    assert!(a.ui.find.is_empty());
}

#[test]
fn escape_closes_one_layer_at_a_time() {
    let mut a = app(&["showcase.md"]);
    click(&mut a, Hit::Find);
    type_text(&mut a, "mark");
    a.open_popup(PopupKind::Main);
    a.compose();
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.popup.is_none());
    assert!(a.ui.find_focused, "the menu goes first");
    assert_eq!(a.ws.find_query(), "mark");
    key(&mut a, KeyName::Escape, None);
    assert!(!a.ui.find_focused);
    assert_eq!(a.ws.find_query(), "");
}

#[test]
fn menus_take_the_keyboard() {
    let mut a = app(&["linked.md"]);
    click(&mut a, Hit::Bar(BarItem::More));
    assert!(matches!(
        a.ui.popup.as_ref().map(|p| p.kind),
        Some(PopupKind::Main)
    ));
    let zoom = a.ws.zoom();
    // Ctrl+= would zoom without the menu; with it open the keys go to the menu.
    a.mods = Mods {
        ctrl: true,
        ..Mods::default()
    };
    key(&mut a, KeyName::Char('='), Some("="));
    a.mods = Mods::default();
    assert_eq!(a.ws.zoom(), zoom);
    key(&mut a, KeyName::Down, None);
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.popup.is_none());
    // A click outside closes a menu without acting.
    click(&mut a, Hit::Bar(BarItem::More));
    a.pointer_moved(640.0, 600.0);
    a.left_press();
    a.left_release();
    assert!(a.ui.popup.is_none());
}

#[test]
fn shortcuts_open_and_close_dialogs() {
    let mut a = app(&["linked.md"]);
    a.command(CommandId::ShowShortcuts);
    a.compose();
    assert!(matches!(
        a.ui.dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::Shortcuts)
    ));
    // Only the dialog's hits are live.
    assert!(matches!(a.hit_at(5.0, 300.0), Some(Hit::Backdrop)));
    a.pointer_moved(5.0, 300.0);
    a.left_press();
    a.left_release();
    assert!(a.ui.dialog.is_none());
    a.command(CommandId::About);
    a.compose();
    key(&mut a, KeyName::Enter, None);
    assert!(a.ui.dialog.is_none());
}

#[test]
fn ctrl_wheel_zooms_and_plain_wheel_scrolls() {
    let mut a = app(&["showcase.md"]);
    let page = a.areas.page;
    a.pointer_moved(page.cx(), page.cy());
    a.wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
    a.compose();
    assert!(a.ws.scroll() > 0.0);
    let zoom = a.ws.zoom();
    a.mods = Mods {
        ctrl: true,
        ..Mods::default()
    };
    a.wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
    a.compose();
    assert!(a.ws.zoom() > zoom);
}

#[test]
fn focus_mode_hides_chrome_and_escape_leaves() {
    let mut a = app(&["linked.md"]);
    a.command(CommandId::FocusMode);
    a.compose();
    assert!(a.ui.focus_mode);
    assert!(a.areas.files.is_none() && a.areas.outline.is_none());
    // Always leavable by mouse.
    assert!(a.hit_rect(Hit::LeaveFocus).is_some());
    key(&mut a, KeyName::Escape, None);
    assert!(!a.ui.focus_mode);
    assert!(a.areas.files.is_some());
}

#[test]
fn the_update_bar_pushes_the_page_down_and_closes() {
    let mut a = app(&["linked.md"]);
    let before = page::viewport(&a.ws, a.areas.page);
    a.ws.preview_update("available");
    a.compose();
    let with_bar = page::viewport(&a.ws, a.areas.page);
    assert_eq!(with_bar.y, before.y + crate::ui::BANNER_H);
    assert_eq!(with_bar.h, before.h - crate::ui::BANNER_H);
    assert!(rect(&a, Hit::UpdateGo).y < with_bar.y);
    // While it downloads there is nothing to press.
    a.ws.preview_update("installing");
    a.compose();
    assert!(a.hit_rect(Hit::UpdateGo).is_none());
    assert!(a.hit_rect(Hit::UpdateClose).is_none());
    a.ws.preview_update("installed");
    a.compose();
    click(&mut a, Hit::UpdateClose);
    a.compose();
    assert!(a.hit_rect(Hit::UpdateClose).is_none());
    assert_eq!(page::viewport(&a.ws, a.areas.page), before);
}

#[test]
fn last_times_crash_report_gets_a_bar_under_the_update_bar_and_closing_puts_it_away() {
    let mut a = app(&["linked.md"]);
    let before = page::viewport(&a.ws, a.areas.page);
    let dir = std::env::temp_dir().join(format!("mdview-crash-bar-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let report = dir.join("crash-00000000001.txt");
    std::fs::write(&report, "mdview closed because of an error.\n").unwrap();
    a.ws.preview_update("available");
    a.ws.set_crash_report(mdshared_app::crash::pending(&dir));
    a.compose();
    let with_bars = page::viewport(&a.ws, a.areas.page);
    assert_eq!(with_bars.y, before.y + 2.0 * crate::ui::BANNER_H);
    let (update, open) = (rect(&a, Hit::UpdateGo), rect(&a, Hit::CrashOpen));
    assert!(update.y < open.y && open.y < with_bars.y);
    assert!(a.hit_rect(Hit::CrashCopy).is_some());
    click(&mut a, Hit::CrashClose);
    a.compose();
    assert!(a.hit_rect(Hit::CrashOpen).is_none());
    assert_eq!(
        page::viewport(&a.ws, a.areas.page).y,
        before.y + crate::ui::BANNER_H
    );
    // Seen: kept in the folder, not offered again.
    assert_eq!(mdshared_app::crash::pending(&dir), None);
    assert!(dir.join("crash-00000000001.seen.txt").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_bar_shows_restart_only_when_installed_with_a_file_to_start() {
    let mut a = app(&["linked.md"]);
    for (stage, restart) in [
        ("available", false),
        ("installing", false),
        ("installed-plain", false),
        ("installed", true),
    ] {
        a.ws.preview_update(stage);
        a.compose();
        assert_eq!(a.hit_rect(Hit::UpdateRestart).is_some(), restart, "{stage}");
        assert_eq!(
            a.hit_rect(Hit::UpdateGo).is_some(),
            stage == "available",
            "{stage}"
        );
    }
    // Pressing it closes the window the way Exit does, and remembers what to start.
    assert!(!a.exit);
    click(&mut a, Hit::UpdateRestart);
    assert!(a.exit);
    assert_eq!(a.restart, Some(std::path::PathBuf::from("/preview/app")));
}

#[test]
fn a_bar_without_a_file_to_start_restarts_nothing() {
    let mut a = app(&["linked.md"]);
    a.ws.preview_update("installed-plain");
    a.restart_into_update();
    assert!(!a.exit);
    assert_eq!(a.restart, None);
}

#[test]
fn the_update_bar_shows_over_the_empty_state() {
    let mut a = app(&[]);
    a.ws.preview_update("available");
    a.compose();
    let bar = rect(&a, Hit::UpdateGo);
    let open = rect(&a, Hit::EmptyOpen);
    assert!(bar.bottom() < open.y);
}

#[test]
fn about_holds_the_update_setting() {
    let mut a = app(&["linked.md"]);
    a.command(CommandId::About);
    a.compose();
    assert!(a.ws.checks_updates());
    click(&mut a, Hit::DlgButton(2));
    assert!(!a.ws.checks_updates());
    assert!(a.ui.dialog.is_some(), "the toggle keeps the dialog open");
    a.compose();
    click(&mut a, Hit::DlgButton(2));
    assert!(a.ws.checks_updates());
    // Looking now closes the dialog; with no event loop in a test, nothing is fetched.
    a.compose();
    click(&mut a, Hit::DlgButton(1));
    assert!(a.ui.dialog.is_none());
}

#[test]
fn the_backlinks_list_shows_only_with_backlinks_and_a_click_opens_the_linking_file() {
    // No folder open: nothing is indexed, so the outline has no Backlinks list.
    let mut a = app(&["wiki/Home.md"]);
    assert!(a.hit_rect(Hit::OutlineRow(0)).is_some());
    assert!(a.hit_rect(Hit::BacklinkRow(0)).is_none());

    a.set_folder(fixture("wiki"));
    a.ws.index_notes_now();
    a.compose();
    // Four notes link to Home; the list sits under the outline's two headings.
    let first = rect(&a, Hit::BacklinkRow(0));
    assert!(first.y > rect(&a, Hit::OutlineRow(1)).bottom());
    assert!(a.hit_rect(Hit::BacklinkRow(3)).is_some());
    assert!(a.hit_rect(Hit::BacklinkRow(4)).is_none());

    click(&mut a, Hit::BacklinkRow(2));
    let active = a.ws.active_tab().unwrap().path().to_path_buf();
    assert!(active.ends_with("Recipes.md"), "{active:?}");
    assert!(
        a.ws.has_selection(),
        "the link is selected in the opened file"
    );
    // The list is now that of Recipes: six mentions in Home (the embed counts) and one in Plan.
    assert_eq!(a.ws.backlinks().len(), 7);
    assert!(a.hit_rect(Hit::BacklinkRow(6)).is_some());
    assert!(a.hit_rect(Hit::BacklinkRow(7)).is_none());
}

/// The middle of row `i` of the open main menu.
fn menu_row(a: &App, i: usize) -> (f32, f32) {
    let m = a.ui.popup.as_ref().expect("a menu").menu.rect();
    let y = m.y + overlays::menu_row_offset(&a.ui.popup.as_ref().unwrap().menu.items, i) + 16.0;
    (m.x + 40.0, y)
}

/// Index of the "Open recent" row of the main menu.
fn recent_row(a: &App) -> usize {
    let p = a.ui.popup.as_ref().expect("a menu");
    p.menu.items.iter().position(|i| i.id.is_submenu()).unwrap()
}

#[test]
fn a_submenu_opens_after_the_pointer_rests_and_the_parent_stays() {
    let mut a = app(&["linked.md"]);
    // No menu, no timer: nothing is scheduled for the dwell.
    assert_eq!(a.timers(std::time::Instant::now()), None);
    click(&mut a, Hit::Bar(BarItem::More));
    let r = recent_row(&a);
    let (x, y) = menu_row(&a, r);
    a.pointer_moved(x, y);
    a.compose();
    {
        let p = a.ui.popup.as_ref().unwrap();
        assert!(p.sub.is_none(), "not before the dwell");
        assert!(p.dwell.is_some());
    }
    let now = std::time::Instant::now();
    // The timer asks to wake at the dwell's end, and does nothing before it.
    let next = a.timers(now).expect("a wake for the dwell");
    assert!(next > now && next <= now + crate::ui::SUBMENU_DWELL);
    assert!(a.ui.popup.as_ref().unwrap().sub.is_none());
    // After it the submenu is open beside the item and the main menu is still there.
    a.timers(now + crate::ui::SUBMENU_DWELL + std::time::Duration::from_millis(5));
    a.compose();
    let p = a.ui.popup.as_ref().unwrap();
    assert_eq!(p.kind, PopupKind::Main);
    let sub = p.sub.as_ref().expect("the submenu");
    assert!(p.dwell.is_none());
    assert!(sub.rect().w > 0.0 && sub.rect().right() > p.menu.rect().x);
    assert!(p.menu.rect().w > 0.0, "the parent stays");
    // Nothing is left to wake for.
    assert_eq!(a.timers(now + std::time::Duration::from_secs(1)), None);
    // Closing the menu leaves no timer behind.
    a.pointer_moved(5.0, 790.0);
    a.left_press();
    a.left_release();
    assert!(a.ui.popup.is_none());
    assert_eq!(a.timers(now + std::time::Duration::from_secs(2)), None);
}

#[test]
fn a_click_opens_the_submenu_at_once_and_other_items_close_it() {
    let mut a = app(&["linked.md"]);
    click(&mut a, Hit::Bar(BarItem::More));
    let r = recent_row(&a);
    let (x, y) = menu_row(&a, r);
    a.pointer_moved(x, y);
    a.left_press();
    a.left_release();
    a.compose();
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_some()));
    // The pointer leaving the item sideways crosses the menu's padding, where no row is
    // hovered: the submenu stays and its item stays lit (0.3.0 closed it there, so the pointer
    // never reached the submenu).
    let m = a.ui.popup.as_ref().unwrap().menu.rect();
    let (mut px, mut end) = (x, m.x);
    if a.ui.popup.as_ref().unwrap().sub.as_ref().unwrap().rect().x >= m.right() - 8.0 {
        end = m.right() - 1.0;
    }
    let step = if end < px { -1.0 } else { 1.0 };
    while (end - px) * step >= 0.0 {
        a.pointer_moved(px, y);
        let p = a.ui.popup.as_ref().unwrap();
        assert!(p.sub.is_some(), "closed at x = {px}");
        assert_eq!(p.menu.hover, Some(r), "the item stays lit at x = {px}");
        assert!(p.dwell.is_none());
        px += step;
    }
    // Another item of the parent does not close the submenu at once (the pointer may be on its
    // way to the submenu), only after it has rested there.
    let (x0, y0) = menu_row(&a, 0);
    a.pointer_moved(x0, y0);
    assert!(
        a.ui.popup
            .as_ref()
            .is_some_and(|p| p.sub.is_some() && p.dwell.is_some())
    );
    // Into the submenu before the dwell is up: it stays, and nothing is pending.
    let s = a.ui.popup.as_ref().unwrap().sub.as_ref().unwrap().rect();
    a.pointer_moved(s.x + 40.0, s.y + 16.0);
    assert!(
        a.ui.popup
            .as_ref()
            .is_some_and(|p| p.sub.is_some() && p.dwell.is_none())
    );
    a.timers(std::time::Instant::now() + crate::ui::SUBMENU_DWELL * 2);
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_some()));
    // Resting on the other item closes it.
    a.pointer_moved(x0, y0);
    a.timers(std::time::Instant::now() + crate::ui::SUBMENU_DWELL * 2);
    assert!(
        a.ui.popup
            .as_ref()
            .is_some_and(|p| p.sub.is_none() && p.dwell.is_none())
    );
    // And back on the item: it opens again after the dwell, not before.
    let (x, y) = menu_row(&a, r);
    a.pointer_moved(x, y);
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_none()));
    let late = std::time::Instant::now() + crate::ui::SUBMENU_DWELL * 2;
    a.timers(late);
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_some()));
    // The pointer can travel to the submenu, which keeps the parent's item lit.
    a.compose();
    let s = a.ui.popup.as_ref().unwrap().sub.as_ref().unwrap().rect();
    a.pointer_moved(s.x + 40.0, s.y + 16.0);
    let p = a.ui.popup.as_ref().unwrap();
    assert_eq!(p.menu.hover, Some(r));
    assert!(p.sub.as_ref().unwrap().hover.is_some());
    // A click outside closes both.
    a.pointer_moved(640.0, 600.0);
    a.left_press();
    a.left_release();
    assert!(a.ui.popup.is_none());
}

#[test]
fn escape_and_left_close_only_the_submenu_and_right_opens_it() {
    let mut a = app(&["linked.md"]);
    click(&mut a, Hit::Bar(BarItem::More));
    let r = recent_row(&a);
    for _ in 0..=r {
        key(&mut a, KeyName::Down, None);
    }
    assert_eq!(a.ui.popup.as_ref().unwrap().menu.hover, Some(r));
    key(&mut a, KeyName::Right, None);
    let p = a.ui.popup.as_ref().unwrap();
    assert!(p.sub.as_ref().is_some_and(|s| s.hover.is_some()));
    key(&mut a, KeyName::Left, None);
    let p = a.ui.popup.as_ref().expect("the parent stays");
    assert!(p.sub.is_none());
    key(&mut a, KeyName::Enter, None);
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_some()));
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_none()));
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.popup.is_none());
    // Right on an ordinary item does nothing.
    click(&mut a, Hit::Bar(BarItem::More));
    key(&mut a, KeyName::Down, None);
    key(&mut a, KeyName::Right, None);
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_none()));
}

#[test]
fn choosing_in_the_submenu_acts_and_closes_both() {
    let mut a = app(&["linked.md"]);
    click(&mut a, Hit::Bar(BarItem::More));
    let r = recent_row(&a);
    let (x, y) = menu_row(&a, r);
    a.pointer_moved(x, y);
    a.left_press();
    a.left_release();
    a.compose();
    let s = a.ui.popup.as_ref().unwrap().sub.as_ref().unwrap().rect();
    // The first row is the recent file; opening it is a no-op for the same file but closes both.
    a.pointer_moved(s.x + 40.0, s.y + 5.0 + 16.0);
    a.left_press();
    a.left_release();
    assert!(a.ui.popup.is_none());
}

#[test]
fn menu_row_offsets_follow_the_shared_layout() {
    let a = app(&["linked.md"]);
    let items = overlays::main_menu(&a.ws, &a.ui, 0.0, 0.0).items;
    // Rows are 32 px under 5 px of padding; a separator adds 9 px before its row.
    assert_eq!(overlays::menu_row_offset(&items, 0), 5.0);
    assert_eq!(overlays::menu_row_offset(&items, 2), 5.0 + 64.0);
}

// ---- the command palette and the settings dialog -------------------------------------------------

const CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
    alt: false,
    control: false,
};

/// The targets of the palette's listed entries, in order.
fn listed(a: &App) -> Vec<crate::palette::Target> {
    let p = a.ui.palette.as_ref().expect("the palette is open");
    p.palette
        .listed_ids()
        .into_iter()
        .filter_map(|id| p.target(id).cloned())
        .collect()
}

#[test]
fn nothing_of_either_is_built_until_it_opens() {
    let a = app(&["linked.md"]);
    assert!(a.ui.palette.is_none() && a.ui.settings.is_none());
    assert!(!a.ui.modal());
}

#[test]
fn ctrl_p_opens_the_palette_with_tabs_and_ctrl_p_or_escape_closes_it() {
    use crate::palette::Target;
    let mut a = app(&["linked.md", "math.md"]);
    a.mods = CTRL;
    key(&mut a, KeyName::Char('p'), Some("\u{10}"));
    assert!(a.ui.palette.is_some());
    assert_eq!(listed(&a), [Target::Tab(0), Target::Tab(1)]);
    // The shortcut that opened it closes it; other shortcuts do nothing behind it.
    key(&mut a, KeyName::Char('w'), Some("\u{17}"));
    assert_eq!(a.ws.tab_count(), 2);
    key(&mut a, KeyName::Char('p'), Some("\u{10}"));
    assert!(a.ui.palette.is_none());
    a.mods = Mods::default();
    a.command(C::CommandPalette);
    a.compose();
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.palette.is_none());
}

#[test]
fn choosing_a_tab_switches_to_it() {
    let mut a = app(&["linked.md", "math.md"]);
    assert_eq!(a.ws.active(), Some(1));
    a.command(C::CommandPalette);
    a.compose();
    key(&mut a, KeyName::Enter, None);
    assert!(a.ui.palette.is_none());
    assert_eq!(a.ws.active(), Some(0));
}

#[test]
fn the_greater_than_prefix_lists_commands_and_enter_runs_one() {
    use crate::palette::Target;
    let mut a = app(&["showcase.md"]);
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, ">zoom in");
    assert_eq!(listed(&a).first(), Some(&Target::Cmd(C::ZoomIn)));
    assert_eq!(a.ws.zoom(), 100);
    key(&mut a, KeyName::Enter, None);
    assert!(a.ui.palette.is_none());
    assert_eq!(a.ws.zoom(), 110);
}

#[test]
fn every_listed_command_has_an_icon_and_the_palette_is_not_among_them() {
    use crate::palette::Target;
    let a = app(&["showcase.md"]);
    let state = crate::palette::PaletteState::open(&a.ws, None, ">");
    let ids = state.palette.listed_ids();
    assert!(ids.len() > 20);
    for id in ids {
        let Some(Target::Cmd(c)) = state.target(id) else {
            panic!("a command source lists only commands");
        };
        assert_ne!(*c, C::CommandPalette);
        assert!(!overlays::command_icon(*c).is_empty(), "{c:?}");
    }
}

#[test]
fn the_hash_prefix_lists_headings_and_enter_scrolls_to_one() {
    use crate::palette::Target;
    let mut a = app(&["showcase.md"]);
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, "#");
    let heads = listed(&a);
    assert_eq!(heads.len(), a.ws.outline().len());
    assert!(heads.iter().all(|t| matches!(t, Target::Heading(_))));
    assert_eq!(a.ws.scroll(), 0.0);
    key(&mut a, KeyName::End, None);
    key(&mut a, KeyName::Enter, None);
    assert!(a.ui.palette.is_none());
    assert!(a.ws.scroll() > 0.0);
}

#[test]
fn alt_arrows_and_the_palette_go_back_and_forward() {
    use crate::palette::Target;
    let mut a = app(&["showcase.md"]);
    let last = a.ws.outline().last().expect("headings").block;
    // Nowhere to go yet: both are listed, greyed.
    assert!(!a.ws.can_go_back() && !a.ws.can_go_forward());
    assert!(a.ws.outline_jump(last));
    a.compose();
    let jumped = a.ws.scroll();
    assert!(jumped > 0.0 && a.ws.can_go_back());
    a.mods = Mods {
        alt: true,
        ..Mods::default()
    };
    key(&mut a, KeyName::Left, None);
    assert_eq!(a.ws.scroll(), 0.0);
    assert!(a.ws.can_go_forward());
    key(&mut a, KeyName::Right, None);
    assert_eq!(a.ws.scroll(), jumped);
    a.mods = Mods::default();
    // The palette runs the same steps.
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, ">back");
    let at = listed(&a)
        .iter()
        .position(|t| *t == Target::Cmd(C::Back))
        .expect("Back is listed");
    for _ in 0..at {
        key(&mut a, KeyName::Down, None);
    }
    key(&mut a, KeyName::Enter, None);
    assert!(a.ui.palette.is_none());
    assert_eq!(a.ws.scroll(), 0.0);
}

#[test]
fn the_question_mark_lists_the_prefixes_and_a_row_puts_one_in_the_box() {
    let mut a = app(&["showcase.md"]);
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, "?");
    assert_eq!(listed(&a).len(), 3);
    key(&mut a, KeyName::Down, None);
    key(&mut a, KeyName::Enter, None);
    let p = a.ui.palette.as_ref().expect("still open");
    assert_eq!(p.palette.query(), ">");
    assert!(p.palette.len() > 20);
}

#[test]
fn the_folders_files_join_the_list_once_something_is_typed() {
    use crate::palette::Target;
    let mut a = app(&["linked.md"]);
    a.ws.set_folder(Some(fixture("wiki")));
    a.ws.index_notes_now();
    a.command(C::CommandPalette);
    a.compose();
    assert!(
        !listed(&a).iter().any(|t| matches!(t, Target::File(_))),
        "an empty query lists tabs and recent files only"
    );
    type_text(&mut a, "home");
    let files: Vec<_> = listed(&a)
        .into_iter()
        .filter_map(|t| match t {
            Target::File(p) => Some(p),
            _ => None,
        })
        .collect();
    assert!(files.iter().any(|p| p.ends_with("Home.md")), "{files:?}");
    key(&mut a, KeyName::Enter, None);
    assert!(a.ws.tabs().iter().any(|t| t.path().ends_with("Home.md")));
}

#[test]
fn a_click_outside_the_palette_closes_it_and_the_page_gets_no_click() {
    let mut a = app(&["showcase.md"]);
    a.command(C::CommandPalette);
    a.compose();
    let card = a.ui.palette.as_ref().unwrap().palette.rect();
    // Inside the card: nothing closes.
    a.pointer_moved(card.cx(), card.y + 20.0);
    a.left_press();
    a.left_release();
    assert!(a.ui.palette.is_some());
    a.ui.last_press = None;
    a.pointer_moved(5.0, 700.0);
    a.left_press();
    a.left_release();
    assert!(a.ui.palette.is_none());
}

#[test]
fn the_wheel_goes_to_the_palette_not_the_page() {
    let mut a = app(&["showcase.md"]);
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, ">");
    a.pointer_moved(640.0, 300.0);
    a.wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
    assert_eq!(a.ws.scroll(), 0.0);
    assert!(a.ui.palette.as_ref().unwrap().palette.scroll() > 0.0);
}

#[test]
fn settings_open_with_ctrl_comma_and_the_form_applies_each_change_at_once() {
    use crate::settings as s;
    use mdshared_ui::settings::Value;
    let mut a = app(&["showcase.md"]);
    a.mods = CTRL;
    key(&mut a, KeyName::Char(','), Some(","));
    a.mods = Mods::default();
    assert!(a.ui.settings.is_some());
    a.apply_setting(s::PAGE_WIDTH, Value::Choice(1));
    assert_eq!(a.ws.page_width(), PageWidth::Wide);
    a.apply_setting(s::TEXT_SIZE, Value::Number(130));
    assert_eq!(a.ws.zoom(), 130);
    a.apply_setting(s::THEME, Value::Choice(1));
    assert_eq!(a.ws.theme_choice(), ThemeChoice::Light);
    a.apply_setting(s::REMOTE_IMAGES, Value::Bool(true));
    assert!(a.ws.remote_always());
    a.apply_setting(s::TICK_BOXES, Value::Bool(true));
    assert!(a.ws.edit_always());
    a.apply_setting(s::ANIMATIONS, Value::Bool(false));
    assert!(!a.ws.animations());
    a.apply_setting(s::CHECK_AT_LAUNCH, Value::Bool(false));
    assert!(!a.ws.checks_updates());
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.settings.is_none());
}

#[test]
fn the_settings_form_follows_the_apps_limits() {
    use crate::settings as s;
    use mdshared_ui::settings::Row;
    let a = app(&["showcase.md"]);
    let rows = s::rows_of(&s::pages(&a.ws, true, true));
    let zoom = rows
        .iter()
        .find_map(|r| match r {
            Row::Number {
                id: s::TEXT_SIZE,
                min,
                max,
                step,
                unit,
                ..
            } => Some((*min, *max, *step, unit.clone())),
            _ => None,
        })
        .expect("the text size");
    assert_eq!(zoom, (60, 250, 10, "%".to_string()));
}

/// The pages in the states that make them tallest or change what they say: the default, the
/// brief-tab rule on (its seconds enabled), an update to install and one installing (the button's
/// text), each with every row this copy can offer (corners, "Make mdview the default").
fn settings_states() -> Vec<(&'static str, Vec<mdshared_ui::settings::pages::Page>)> {
    use crate::settings as s;
    let mut out = Vec::new();
    let mut state = |name: &'static str, f: &dyn Fn(&mut App)| {
        let mut a = app(&["showcase.md"]);
        f(&mut a);
        out.push((name, s::pages(&a.ws, true, true)));
    };
    state("the defaults", &|_| {});
    state("the brief-tab rule on", &|a| a.ws.set_skip_brief(true, 600));
    state("an update to install", &|a| {
        a.ws.preview_update("available")
    });
    state("an update installing", &|a| {
        a.ws.preview_update("installing")
    });
    out
}

/// The design's promise, tested as mdedit's is: every page fits its area at the default window
/// and at the smallest, at scale 1, 1.5 and 2, in each state; nothing on it is cut, and the list
/// holds every page. Linux's fonts are wider: mdview's own texts keep at least 15 % spare at the
/// smallest window (measured with Windows' fonts); the tightest ("Shown for less than", the
/// window rows, "Make default") are mdedit's too, whose `ci` runs this check on Linux.
#[test]
fn every_settings_page_fits_at_the_default_and_the_minimum_at_every_scale() {
    use crate::settings as s;
    use mdshared_ui::settings::pages::Form as Pages;
    let states = settings_states();
    // Every setting is on a page in some state.
    let mut seen = std::collections::HashSet::new();
    for (_, pages) in &states {
        seen.extend(s::rows_of(pages).iter().filter_map(|r| r.id()));
    }
    for id in [
        s::PAGE_WIDTH,
        s::TEXT_SIZE,
        s::THEME,
        s::REMOTE_IMAGES,
        s::TICK_BOXES,
        s::ANIMATIONS,
        s::CHECK_AT_LAUNCH,
        s::CHECK_NOW,
        s::DEFAULT_APP,
        s::PAGE_FONT,
        s::CODE_FONT,
        s::WINDOW_CORNERS,
        s::PREVIEWS,
        s::START_ON_PINNED,
        s::OFFER_STALE,
        s::SKIP_BRIEF,
        s::BRIEF_SECS,
    ]
    .into_iter()
    .chain(cfg!(windows).then_some(s::BACKDROP))
    {
        assert!(seen.contains(&id), "setting {id} is on no page");
    }
    for (name, pages) in states {
        let names: Vec<&str> = pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, s::PAGES, "{name}");
        let form = Pages::new("Settings", pages);
        for (w, h) in [(1280.0, 800.0), (520.0, 360.0)] {
            for scale in [1.0f32, 1.5, 2.0] {
                let mut bmp = Bitmap::new((w * scale) as u32, (h * scale) as u32);
                let mut gfx = Gfx::new();
                let mut p = Painter::new(&mut bmp, &mut gfx, scale);
                let fit = form.fit(&mut p, Rect::new(0.0, 0.0, w, h));
                let problems = fit.problems();
                assert!(
                    problems.is_empty(),
                    "{name}, {w}x{h} at {scale}: {problems:#?}"
                );
            }
        }
    }
}

/// Settings is the paged form: the four pages in the list, the card inside the window at the
/// default size and the smallest, and on every page every control inside the page's area.
#[test]
fn the_settings_dialog_has_four_pages_and_every_control_inside_the_card() {
    use crate::settings as s;
    for size in [(1280, 800), (520, 360)] {
        let mut a = app(&["showcase.md"]);
        a.size = size;
        a.command(C::Settings);
        a.compose();
        let win = Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32);
        for (c, name) in s::PAGES.iter().enumerate() {
            let f = a.ui.settings.as_ref().expect("the dialog");
            assert_eq!(f.page_names(), s::PAGES);
            assert!(a.show_settings_section(name));
            a.compose();
            let f = a.ui.settings.as_ref().unwrap();
            let card = f.rect();
            assert!(
                card.x >= 0.0 && card.y >= 0.0 && card.right() <= win.w && card.bottom() <= win.h,
                "{card:?} inside {win:?}"
            );
            assert!(card.w <= 780.0 && card.h <= 420.0, "{card:?}");
            let area = f.page_rect();
            let item = f.list_item_rect(c);
            assert!(item.h > 0.0 && item.bottom() <= card.bottom(), "{item:?}");
            let page = &f.pages()[c];
            for it in &page.items {
                let Some(id) = it.row().id() else { continue };
                let r = f.control_rect(id);
                assert!(r.w > 0.0, "{id} is drawn on {}", page.name);
                assert!(
                    r.x >= area.x && r.right() <= area.right() + 0.5 && r.bottom() <= area.bottom(),
                    "{id}: {r:?} in {area:?} at {size:?}"
                );
            }
        }
    }
}

/// From the list, Tab walks the page's controls in order, then the close button, then back to
/// the list; a disabled row is skipped.
#[test]
fn tab_in_the_settings_dialog_walks_each_page_then_close_then_the_list() {
    let mut a = app(&["showcase.md"]);
    a.command(C::Settings);
    a.compose();
    let pages = a.ui.settings.as_ref().unwrap().pages().len();
    for c in 0..pages {
        let f = a.ui.settings.as_mut().unwrap();
        f.show_page(c);
        assert!(f.focus_in_list());
        let page = f.pages()[c].clone();
        let want: Vec<u32> = page
            .items
            .iter()
            .filter(|it| !it.is_disabled())
            .filter_map(|it| it.row().id())
            .collect();
        let mut seen = Vec::new();
        for _ in 0..want.len() {
            key(&mut a, KeyName::Tab, None);
            seen.extend(a.ui.settings.as_ref().unwrap().focus_id());
        }
        assert_eq!(seen, want, "{}", page.name);
        key(&mut a, KeyName::Tab, None);
        assert!(a.ui.settings.as_ref().unwrap().focus_on_close());
        key(&mut a, KeyName::Tab, None);
        assert!(a.ui.settings.as_ref().unwrap().focus_in_list());
    }
}

/// The seconds are disabled, with the reason, while their switch is off; turning it on from the
/// dialog enables them at once, and both values are kept.
#[test]
fn the_brief_tab_switch_enables_its_seconds_in_the_open_dialog() {
    use crate::settings as s;
    let mut a = app(&["showcase.md"]);
    assert!(!a.ws.skips_brief().0, "off by default");
    a.command(C::Settings);
    a.compose();
    let f = a.ui.settings.as_mut().unwrap();
    assert!(
        !f.focus_row(s::BRIEF_SECS),
        "disabled while the switch is off"
    );
    assert!(f.focus_row(s::SKIP_BRIEF));
    assert_eq!(f.page_names()[f.page()], s::TABS);
    key(&mut a, KeyName::Space, None);
    assert!(a.ws.skips_brief().0);
    let f = a.ui.settings.as_mut().unwrap();
    assert!(f.focus_row(s::BRIEF_SECS), "enabled once the switch is on");
    let secs = a.ws.skips_brief().1;
    key(&mut a, KeyName::Right, None);
    assert_eq!(a.ws.skips_brief(), (true, secs + 1));
}

/// A section names a page, or the group a setting was in before the pages; the dialog opens
/// again on the page it was closed on.
#[test]
fn a_settings_section_shows_its_page_and_the_dialog_reopens_on_it() {
    use crate::settings as s;
    assert_eq!(s::page_for_section("Content"), Some(s::READING));
    assert_eq!(s::page_for_section("page"), Some(s::APPEARANCE));
    assert_eq!(s::page_for_section("System"), Some(s::UPDATES));
    assert_eq!(s::page_for_section("tabs"), Some(s::TABS));
    assert_eq!(s::page_for_section("nonsense"), None);
    let mut a = app(&["showcase.md"]);
    assert!(!a.show_settings_section("Tabs"), "no dialog open");
    a.command(C::Settings);
    let page = |a: &App| {
        let f = a.ui.settings.as_ref().expect("the dialog");
        f.page_names()[f.page()].to_string()
    };
    assert_eq!(page(&a), s::APPEARANCE, "the first page the first time");
    assert!(a.show_settings_section("content"));
    assert_eq!(page(&a), s::READING);
    assert!(!a.show_settings_section("nonsense"));
    assert_eq!(page(&a), s::READING);
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.settings.is_none());
    a.command(C::Settings);
    assert_eq!(page(&a), s::READING);
    assert!(a.ui.settings.as_ref().unwrap().focus_in_list());
}

#[test]
fn settings_keep_the_page_keys_and_the_bar_button_working_after_they_close() {
    let mut a = app(&["showcase.md"]);
    a.command(C::Settings);
    a.compose();
    // A key the form does not use does not reach the page.
    key(&mut a, KeyName::Char('+'), Some("+"));
    a.mods = CTRL;
    key(&mut a, KeyName::Char('='), Some("="));
    a.mods = Mods::default();
    assert_eq!(a.ws.zoom(), 100);
    key(&mut a, KeyName::Escape, None);
    a.compose();
    click(&mut a, Hit::Bar(BarItem::PageWidth));
    assert_eq!(a.ws.page_width(), PageWidth::Wide);
}

#[test]
fn the_main_menu_holds_what_people_use_while_reading_and_fits_the_window() {
    let mut a = app(&["showcase.md"]);
    a.open_popup(PopupKind::Main);
    a.compose();
    let p = a.ui.popup.as_ref().expect("the menu");
    let shown: Vec<String> = p
        .menu
        .items
        .iter()
        .map(|i| mdshared_ui::menu::parse_access(&i.label).text)
        .collect();
    let labels: Vec<&str> = shown.iter().map(String::as_str).collect();
    for gone in [
        "Always load remote images",
        "Tick task boxes without asking",
        "Play animated images",
        "Narrow page",
        "Wide page",
        "Full-width page",
        "Manage recent files…",
        "Make mdview the default for Markdown files",
    ] {
        assert!(!labels.contains(&gone), "{gone} moved to Settings");
    }
    for kept in [
        "Open files…",
        "Reload",
        "Close tab",
        "Zoom in",
        "Files pane",
        "Edit in mdedit",
        "Focus reading mode",
        "Command palette…",
        "Settings…",
        "Keyboard shortcuts",
        "About mdview",
        "Exit",
    ] {
        assert!(labels.contains(&kept), "{kept}");
    }
    assert!(!p.menu.scrolls(), "it fits the default window");
    // The menu's icons are the ones the palette shows for the same commands.
    for item in &p.menu.items {
        if let Act::Cmd(id) = item.id {
            assert_eq!(item.icon, overlays::command_icon(id), "{}", item.label);
        }
    }
}

#[cfg(unix)]
#[test]
fn edit_in_mdedit_starts_the_editor_at_the_top_line() {
    // The "editor" is a script that records what it was started with.
    let dir = std::env::temp_dir().join(format!("mdview-edit-{}", std::process::id()));
    _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (script, record) = (dir.join("mdedit.sh"), dir.join("args.txt"));
    let text = format!(
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\n",
        record.display()
    );
    std::fs::write(&script, text).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // SAFETY: no other test reads or writes this variable.
    unsafe { std::env::set_var("MD_SISTER_MDEDIT", &script) };

    let mut a = app(&["showcase.md"]);
    assert!(a.ws.scroll_to_line(100));
    a.compose();
    let top = a.ws.top_source_line().unwrap();
    assert!((90..=100).contains(&top), "{top}");
    a.mods = CTRL;
    key(&mut a, KeyName::Char('e'), Some("e"));
    for _ in 0..300 {
        if std::fs::read_to_string(&record).is_ok_and(|t| t.ends_with(".md\n")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let args = std::fs::read_to_string(&record).unwrap_or_default();
    let path = a.ws.active_tab().unwrap().path().display().to_string();
    // The column goes with the line only when the top row begins inside a line that wraps.
    let goto = match a.ws.top_source_place().unwrap().column {
        Some(column) => format!("{top}:{column}"),
        None => top.to_string(),
    };
    assert_eq!(args, format!("--goto\n{goto}\n--from\nmdview\n{path}\n"));
    // SAFETY: as above.
    unsafe { std::env::remove_var("MD_SISTER_MDEDIT") };
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hand_over_opens_at_a_line_and_a_follow_only_scrolls() {
    let mut a = app(&["showcase.md", "tables.md"]);
    let (showcase, tables) = (fixture("showcase.md"), fixture("tables.md"));
    let canonical = mdview_core::files::canonical_or_absolute;
    let at = |a: &App| a.ws.active_tab().unwrap().path().to_path_buf();
    assert_eq!(at(&a), canonical(&tables));
    // An opening switches to the file and shows the line.
    a.handed_over(vec![Request {
        line: Some(120),
        ..Request::new(&showcase)
    }]);
    a.compose();
    assert_eq!(at(&a), canonical(&showcase));
    let top = a.ws.top_source_line().unwrap();
    assert!((110..=120).contains(&top), "{top}");
    // A follow for the showing file scrolls it, and for another file does nothing at all.
    let follow = |path: &Path, line| Request {
        follow: true,
        line: Some(line),
        ..Request::new(path)
    };
    a.handed_over(vec![follow(&showcase, 10)]);
    a.compose();
    assert!(a.ws.top_source_line().unwrap() < 20);
    let before = a.ws.scroll();
    a.handed_over(vec![follow(&tables, 40)]);
    a.compose();
    assert_eq!((at(&a), a.ws.scroll()), (canonical(&showcase), before));
    // One that is not open is not opened.
    a.handed_over(vec![follow(&fixture("math.md"), 3)]);
    assert_eq!(a.ws.tab_count(), 2);
    // Of several waiting, the last one counts.
    a.handed_over(vec![follow(&showcase, 150), follow(&showcase, 60)]);
    a.compose();
    let top = a.ws.top_source_line().unwrap();
    assert!((50..=60).contains(&top), "{top}");
}

#[test]
fn a_behind_window_is_not_focused_by_a_hand_over() {
    let mut a = app(&["showcase.md", "tables.md"]);
    let showcase = fixture("showcase.md");
    a.args.behind = true;
    a.handed_over(vec![Request::new(&showcase)]);
    assert_eq!(a.hand_overs_focused, 0, "a --behind run asks for no focus");
    // It still opens what was handed over: only the focus is dropped.
    assert_eq!(
        a.ws.active_tab().map(|t| t.path().to_path_buf()),
        Some(mdview_core::files::canonical_or_absolute(&showcase))
    );
    a.args.behind = false;
    a.handed_over(vec![Request::new(&fixture("tables.md"))]);
    assert_eq!(a.hand_overs_focused, 1, "an ordinary hand-over focuses");
}

/// A scratch folder with one long document, and an app showing it.
pub(super) fn live_app(name: &str) -> (App, PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("mdview-live-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("doc.md");
    let mut text = String::from("# Title\n\n");
    for i in 0..200 {
        text.push_str(&format!("Paragraph number {i} of the file on disk.\n\n"));
    }
    std::fs::write(&file, text).unwrap();
    let args = Args {
        files: vec![file.clone()],
        dark: Some(true),
        no_session: true,
        ..Args::default()
    };
    let mut a = App::new(args, Workspace::new(None), None);
    a.scale = 1.0;
    a.size = (1280, 800);
    a.setup();
    a.compose();
    (a, dir, file)
}

fn live(path: &Path, text: &str, line: Option<u32>) -> Request {
    Request {
        follow: true,
        text: Some(text.to_string()),
        line,
        ..Request::new(path)
    }
}

fn page_text(a: &App) -> String {
    a.ws.active_tab()
        .unwrap()
        .document()
        .unwrap()
        .text
        .to_string()
}

#[test]
fn a_hand_over_with_a_column_shows_the_row_of_a_wrapped_line() {
    let (mut a, dir, file) = live_app("column");
    // One paragraph written as a single line of 3000 words, as the editor has it.
    let words: Vec<String> = (0..3000).map(|n| format!("word{n:04}")).collect();
    let text = format!("# Title\n\n{}\n\nThe end.\n", words.join(" "));
    let place = |line, column| Request {
        follow: true,
        line: Some(line),
        column,
        ..Request::new(&file)
    };
    a.handed_over(vec![Request {
        text: Some(text),
        ..place(3, None)
    }]);
    a.compose();
    let start = a.ws.scroll();
    // The caret far along the line: the page follows into the paragraph, to the row that
    // holds that place, and a caret further along is further down.
    a.handed_over(vec![place(3, Some(9 * 1000 + 1))]);
    a.compose();
    let middle = a.ws.scroll();
    assert!(middle > start + 200.0, "{start} {middle}");
    let at = a.ws.top_source_place().unwrap();
    assert_eq!(at.line, 3);
    assert!(at.column.is_some_and(|c| c < 9001 && c > 8000), "{at:?}");
    a.handed_over(vec![place(3, Some(9 * 2500 + 1))]);
    a.compose();
    assert!(a.ws.scroll() > middle + 200.0);
    // Without a column (an older mdedit) the line is its beginning, as before.
    a.handed_over(vec![place(3, None)]);
    a.compose();
    assert_eq!(a.ws.scroll(), start);
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn live_text_replaces_the_page_keeps_the_place_and_ends_on_live_off() {
    let (mut a, dir, file) = live_app("replace");
    let disk = std::fs::read_to_string(&file).unwrap();
    let disk_page = page_text(&a);
    a.handed_over(vec![Request {
        line: Some(300),
        ..Request::new(&file)
    }]);
    a.compose();
    let top = a.ws.top_source_line().unwrap();
    assert!(top > 200, "{top}");
    let before = a.ws.scroll();
    // The editor's text: the heading changed, one paragraph added below the reader.
    let edited = disk.replace("# Title", "# A new heading") + "\nTail added in the editor.\n";
    a.handed_over(vec![live(&file, &edited, Some(top))]);
    assert_eq!(
        page_text(&a),
        disk_page,
        "nothing is parsed before the frame"
    );
    a.compose();
    assert!(a.ws.active_tab().unwrap().is_live());
    assert!(page_text(&a).contains("A new heading"));
    assert!(page_text(&a).contains("Tail added in the editor"));
    // The line sent is the one whose row is at the top of the page, and its first row may be
    // hidden above the edge (a line that wraps): the page then moves up to show it, by less
    // than one step and never down. A line fully in view keeps the page still.
    let moved_up = before - a.ws.scroll();
    assert!(
        (0.0..48.0 * a.scale).contains(&moved_up) || moved_up.abs() < 2.0,
        "the reader stays where they were: {before} -> {}",
        a.ws.scroll()
    );
    let now = a.ws.top_source_line().unwrap();
    assert!(
        now <= top && top - now < 4,
        "the top line was {top}, is {now}"
    );
    // A follow without text does not end it, and does not change the text.
    a.handed_over(vec![Request {
        follow: true,
        line: Some(top),
        ..Request::new(&file)
    }]);
    a.compose();
    assert!(a.ws.active_tab().unwrap().is_live());
    assert!(page_text(&a).contains("A new heading"));
    // A reload of the file by the watcher path does nothing while nothing changed on disk.
    a.ws.check_changed();
    assert!(a.ws.active_tab().unwrap().is_live());
    // Of several texts arriving together the last is parsed.
    a.handed_over(vec![
        live(&file, "# First\n", None),
        live(&file, "# Second\n", None),
    ]);
    a.compose();
    assert!(page_text(&a).contains("Second") && !page_text(&a).contains("First"));
    // Live off returns to the file, in the same place.
    a.handed_over(vec![Request {
        live_off: true,
        follow: true,
        ..Request::new(&file)
    }]);
    a.compose();
    assert!(!a.ws.active_tab().unwrap().is_live());
    assert_eq!(page_text(&a), disk_page);
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_change_on_disk_ends_the_live_state_and_f5_returns_to_the_file() {
    let (mut a, dir, file) = live_app("disk");
    a.handed_over(vec![live(&file, "# Unsaved\n", None)]);
    a.compose();
    assert!(a.ws.active_tab().unwrap().is_live());
    // The editor saves: the file changed after the last live text.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&file, "# Saved\n\nBody.\n").unwrap();
    a.ws.check_changed();
    a.compose();
    assert!(!a.ws.active_tab().unwrap().is_live());
    assert!(page_text(&a).contains("Saved"));
    // F5 from a live state shows the file.
    a.handed_over(vec![live(&file, "# Unsaved again\n", None)]);
    a.compose();
    assert!(page_text(&a).contains("Unsaved again"));
    a.ws.reload();
    assert!(!a.ws.active_tab().unwrap().is_live());
    assert!(page_text(&a).contains("Saved"));
    // Text for a file that is not open is dropped: nothing is opened.
    let other = dir.join("other.md");
    std::fs::write(&other, "# Other\n").unwrap();
    a.handed_over(vec![live(&other, "# Other live\n", None)]);
    a.compose();
    assert_eq!(a.ws.tab_count(), 1);
    // Live off for a tab that is not live does nothing.
    a.handed_over(vec![Request {
        live_off: true,
        follow: true,
        ..Request::new(&file)
    }]);
    assert!(!a.ws.active_tab().unwrap().is_live());
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_editor_check_runs_only_while_a_tab_is_live() {
    use std::time::{Duration, Instant};
    let (mut a, dir, file) = live_app("alive");
    let now = Instant::now();
    // No live tab: no timer for it, and nothing is remembered.
    assert_eq!(a.timers(now), None);
    assert!(a.sister_due.is_none());
    a.handed_over(vec![live(&file, "# Unsaved\n", None)]);
    a.compose();
    assert!(a.ws.active_tab().unwrap().is_live());
    // A live tab: the first look is 5 s away.
    assert!(a.timers(now).is_some(), "a wake for the editor check");
    assert_eq!(a.sister_due, Some(now + Duration::from_secs(5)));
    // The editor answered: the tab stays live.
    a.woken(Wake::Sister(true));
    assert!(a.ws.active_tab().unwrap().is_live());
    assert!(a.timers(now + Duration::from_secs(1)).is_some());
    // The editor is gone: the tab shows the file, with a toast, and the timer goes.
    a.woken(Wake::Sister(false));
    assert!(!a.ws.active_tab().unwrap().is_live());
    assert!(page_text(&a).contains("of the file on disk"));
    assert_eq!(
        a.ws.status(Instant::now()),
        Some("mdedit closed: showing the saved file")
    );
    a.timers(now + Duration::from_secs(2));
    assert!(a.sister_due.is_none());
    // Gone again with nothing live does nothing.
    a.woken(Wake::Sister(false));
    _ = std::fs::remove_dir_all(&dir);
}

/// The cost of a live update: handing the text over, then the frame that parses and lays it out.
/// `cargo test --release -p mdview live_update_cost -- --ignored --nocapture`
#[test]
#[ignore = "a measurement"]
fn live_update_cost() {
    let measure = |label: &str, a: &mut App, file: &Path, base: &str| {
        let mut times = Vec::new();
        for i in 0..40 {
            // Each update changes the first heading, as typing in it would.
            let text = format!("# Edit {i}\n\n{base}");
            let t = std::time::Instant::now();
            a.handed_over(vec![live(file, &text, Some(1))]);
            a.compose();
            times.push(t.elapsed());
        }
        times.sort();
        eprintln!(
            "{label}: median {:?}, worst {:?} (parse + layout + compose, {} bytes)",
            times[times.len() / 2],
            times[times.len() - 1],
            base.len()
        );
    };
    let (mut a, dir, file) = live_app("cost");
    let showcase = std::fs::read_to_string(fixture("showcase.md")).unwrap();
    measure("showcase", &mut a, &file, &showcase);
    let mut big = String::new();
    while big.len() < 200 * 1024 {
        big.push_str(&showcase);
    }
    big.truncate(200 * 1024);
    measure("200 KB", &mut a, &file, &big);
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_wheel_while_a_menu_is_open_does_nothing_over_a_menu_that_fits_or_outside_it() {
    let mut a = app(&["showcase.md"]);
    // The control: with no menu the wheel scrolls the page.
    let page = a.areas.page;
    a.pointer_moved(page.cx(), page.cy());
    a.wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
    a.compose();
    let scrolled = a.ws.scroll();
    assert!(scrolled > 0.0);
    a.open_popup(PopupKind::Main);
    a.compose();
    let m = a.ui.popup.as_ref().expect("a menu").menu.rect();
    assert!(!a.ui.popup.as_ref().unwrap().menu.scrolls());
    // Over the menu, which fits: nothing happens, the menu stays, the page stays.
    a.pointer_moved(m.cx(), m.cy());
    a.wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
    a.compose();
    assert!(a.ui.popup.is_some());
    assert_eq!(a.ws.scroll(), scrolled);
    // Outside every menu: the same.
    a.pointer_moved(5.0, 700.0);
    a.wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
    a.compose();
    assert!(a.ui.popup.is_some());
    assert_eq!(a.ws.scroll(), scrolled);
    // A menu taller than the window scrolls under the wheel.
    a.ui.popup = None;
    a.size = (1280, 260);
    a.compose();
    a.open_popup(PopupKind::Main);
    a.compose();
    let m = a.ui.popup.as_ref().unwrap().menu.rect();
    assert!(a.ui.popup.as_ref().unwrap().menu.scrolls());
    a.pointer_moved(m.cx(), m.cy());
    a.wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
    assert!(a.ui.popup.as_ref().unwrap().menu.scroll() > 0.0);
    assert_eq!(a.ws.scroll(), scrolled);
}

#[test]
fn the_wheel_beside_the_palette_does_nothing() {
    let mut a = app(&["showcase.md"]);
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, ">");
    a.pointer_moved(5.0, 790.0);
    assert!(!a.ui.palette.as_ref().unwrap().palette.contains(5.0, 790.0));
    a.wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
    assert_eq!(a.ws.scroll(), 0.0);
    assert_eq!(a.ui.palette.as_ref().unwrap().palette.scroll(), 0.0);
}

#[test]
fn the_window_corners_setting_is_applied_kept_and_offered() {
    use crate::settings as s;
    use mdshared_ui::settings::{Row, Value};
    let mut a = app(&["showcase.md"]);
    // Round (8) is the default, listed after Theme and, where it is offered, Window backdrop (the
    // order mdedit has); Square and Small are the other choices.
    assert_eq!(a.ws.corners(), 8);
    let rows = s::rows_of(&s::pages(&a.ws, true, false));
    let at = rows
        .iter()
        .position(|r| matches!(r, Row::Choice { id, .. } if *id == s::THEME))
        .unwrap();
    let after = if cfg!(windows) {
        assert_eq!(rows[at + 1].id(), Some(s::BACKDROP));
        at + 2
    } else {
        at + 1
    };
    match &rows[after] {
        Row::Choice {
            id,
            label,
            options,
            value,
            ..
        } => {
            assert_eq!(*id, s::WINDOW_CORNERS);
            assert_eq!(label, "Window corners");
            assert_eq!(options, &["Square", "Small", "Round"]);
            assert_eq!(*value, 2);
        }
        other => panic!("{other:?}"),
    }
    // Left out where nothing can round the window.
    assert!(
        !s::rows_of(&s::pages(&a.ws, false, false))
            .iter()
            .any(|r| matches!(r, Row::Choice { id, .. } if *id == s::WINDOW_CORNERS))
    );
    a.apply_setting(s::WINDOW_CORNERS, Value::Choice(0));
    assert_eq!(a.ws.corners(), 0);
    a.apply_setting(s::WINDOW_CORNERS, Value::Choice(1));
    assert_eq!(a.ws.corners(), 4);
    a.apply_setting(s::WINDOW_CORNERS, Value::Choice(2));
    assert_eq!(a.ws.corners(), 8);
    // A shot has no window and shows the row; a plain test run on Linux has no shadow route.
    assert_eq!(a.corners_offered(), cfg!(windows));
}

/// `--frame` puts the shadow margin around the composed content and cuts its corners: the corner
/// pixel is the backdrop with a shadow, the middle is the content, and the picture is larger by the
/// margin on each side. Square corners keep the content's corner.
#[test]
fn a_frame_shot_cuts_the_corners() {
    for scale in [1.0f64, 2.0] {
        let (cw, ch) = ((400.0 * scale) as u32, (240.0 * scale) as u32);
        let content = Bitmap {
            width: cw,
            height: ch,
            pixels: [0x60u8, 0x40, 0x20, 0xFF].repeat((cw * ch) as usize),
        };
        let m = (mdshared_ui::shadow::EXTENT * scale).ceil() as u32;
        for dark in [false, true] {
            let round = frame_of(&content, Corners::Round, scale, dark);
            let square = frame_of(&content, Corners::Square, scale, dark);
            assert_eq!((round.width, round.height), (cw + 2 * m, ch + 2 * m));
            let at = |b: &Bitmap, x: u32, y: u32| {
                let o = ((y * b.width + x) * 4) as usize;
                [b.pixels[o + 2], b.pixels[o + 1], b.pixels[o]]
            };
            let own = [0x20, 0x40, 0x60];
            assert_eq!(at(&round, round.width / 2, round.height / 2), own);
            assert_eq!(at(&square, m, m), own);
            assert_ne!(at(&round, m, m), own, "dark {dark} scale {scale}");
            // The straight edge is not touched by the rounding.
            assert_eq!(at(&round, m + 60, m), own);
            assert_eq!(at(&round, m, m + 60), own);
        }
    }
}

#[test]
fn a_burst_of_watcher_wakes_is_one_check_of_the_folder() {
    let dir = std::env::temp_dir().join(format!("mdview-watch-burst-{}", std::process::id()));
    _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.md"), "# a\n").unwrap();
    let mut a = app(&[]);
    a.ui.tree.set_root(Some(dir.clone()));
    assert_eq!(a.ui.tree.rows().len(), 1);
    std::fs::write(dir.join("b.md"), "# b\n").unwrap();
    // The wake only starts the wait; the pane still shows what it showed.
    let now = Instant::now();
    a.woken(Wake::Files);
    a.woken(Wake::Files);
    let due = a.watch_due.expect("a check is due");
    assert_eq!(a.ui.tree.rows().len(), 1);
    assert!(due > now);
    assert!(a.timers(now).is_some(), "the loop wakes for the check");
    // When the wait is over the folder is read once and the new file shows.
    a.timers(due + Duration::from_millis(1));
    assert!(a.watch_due.is_none());
    assert_eq!(a.ui.tree.rows().len(), 2);
    _ = std::fs::remove_dir_all(&dir);
}

/// Whether outline row `i` lies fully inside the outline's list.
fn outline_row_visible(a: &App, i: usize) -> bool {
    let top = i as f32 * mdshared_ui::pane::ROW;
    let s = a.ui.panes.outline_scroll;
    top >= s && top + mdshared_ui::pane::ROW <= s + a.ui.panes.outline_view
}

#[test]
fn the_outline_follows_the_page_and_keeps_the_current_row_in_view() {
    let mut a = app(&["everything.md"]);
    assert!(
        a.ui.panes.outline_content > a.ui.panes.outline_view,
        "the outline needs scrolling for this test"
    );
    // Park the pointer on the page: it is not on the outline.
    a.pointer_moved(900.0, 400.0);
    let total = a.ws.total_height();
    for step in 1..=8 {
        a.ws.scroll_to(total * step as f32 / 9.0);
        a.compose();
        a.compose();
        let Some(i) = a.ws.current_heading() else {
            continue;
        };
        assert!(outline_row_visible(&a, i), "row {i} at step {step}");
    }
    // Back at the top the outline is back at its top.
    a.ws.scroll_to(0.0);
    a.compose();
    a.compose();
    assert_eq!(a.ui.panes.outline_scroll, 0.0);
}

#[test]
fn the_outline_stays_put_under_the_pointer_and_catches_up_when_it_leaves() {
    let mut a = app(&["everything.md"]);
    a.pointer_moved(900.0, 400.0);
    let total = a.ws.total_height();
    let r = rect(&a, Hit::OutlineRow(0));
    a.pointer_moved(r.cx(), r.cy());
    a.compose();
    assert!(matches!(a.ui.hover, Some(Hit::OutlineRow(0))));
    a.ws.scroll_to(total * 0.9);
    a.compose();
    a.compose();
    let i = a.ws.current_heading().expect("a heading at 90%");
    assert!(!outline_row_visible(&a, i), "the outline did not move");
    assert_eq!(a.ui.panes.outline_scroll, 0.0);
    // The pointer leaves for the page: the next frame brings the row into view.
    a.pointer_moved(900.0, 400.0);
    a.compose();
    a.compose();
    assert!(outline_row_visible(&a, i));
}

#[test]
fn a_click_on_an_outline_row_jumps_and_the_highlight_matches() {
    let mut a = app(&["everything.md"]);
    click(&mut a, Hit::OutlineRow(20));
    a.compose();
    a.compose();
    // Painting settles the jump; the heading at the top is the one clicked, or, when the page
    // cannot scroll that far, the page is at its end.
    let now = a.ws.current_heading().expect("a current heading");
    assert!(
        now == 20 || a.ws.scroll() == a.ws.max_scroll(),
        "current {now}"
    );
    assert!(outline_row_visible(&a, 20));
}

#[test]
fn an_outline_hidden_while_the_page_scrolled_catches_up_when_shown() {
    let mut a = app(&["everything.md"]);
    a.pointer_moved(900.0, 400.0);
    a.compose();
    a.command(CommandId::ToggleOutline);
    a.compose();
    assert!(a.areas.outline.is_none());
    a.ws.scroll_to(a.ws.total_height() * 0.9);
    a.compose();
    let i = a.ws.current_heading().expect("a heading at 90%");
    assert_eq!(
        a.ui.panes.outline_scroll, 0.0,
        "a hidden outline is left alone"
    );
    a.command(CommandId::ToggleOutline);
    a.compose();
    a.compose();
    assert!(outline_row_visible(&a, i));
}

/// A page point inside a code box, and one outside any, found by scanning the first screen.
fn code_and_plain_points(a: &mut App) -> ((f32, f32), (f32, f32)) {
    let vr = page::viewport(&a.ws, a.areas.page);
    let (mut code, mut plain) = (None, None);
    let mut y = vr.y + 4.0;
    while y < vr.bottom() - 4.0 {
        let x = vr.x + vr.w * 0.5;
        match a.ws.code_spot((x - vr.x) * a.scale, (y - vr.y) * a.scale) {
            Some(_) => code = code.or(Some((x, y))),
            None => plain = plain.or(Some((x, y))),
        }
        y += 6.0;
    }
    (code.expect("a code block on screen"), plain.unwrap())
}

#[test]
fn a_code_block_shows_a_copy_button_under_the_pointer_only() {
    let mut a = app(&["showcase.md"]);
    a.ui.panes.outline = false;
    a.compose();
    a.ws.scroll_to(300.0);
    a.compose();
    let (code, plain) = code_and_plain_points(&mut a);
    assert!(a.hit_rect(Hit::CopyCode).is_none());
    // On the text of a paragraph: no button.
    a.pointer_moved(plain.0, plain.1);
    a.compose();
    assert!(a.hit_rect(Hit::CopyCode).is_none());
    // In a code block: a button at its top right, inside it.
    a.pointer_moved(code.0, code.1);
    a.compose();
    let vr = page::viewport(&a.ws, a.areas.page);
    let spot =
        a.ws.code_spot((code.0 - vr.x) * a.scale, (code.1 - vr.y) * a.scale)
            .unwrap();
    let button = rect(&a, Hit::CopyCode);
    let (bx, by) = (vr.x + spot.rect.x, vr.y + spot.rect.y);
    assert!(button.x >= bx && button.right() <= bx + spot.rect.w);
    assert!(button.y >= by && button.bottom() <= by + spot.rect.h);
    assert!(button.right() > bx + spot.rect.w - 12.0, "{button:?}");
    // On the button it stays, with its tip.
    a.pointer_moved(button.cx(), button.cy());
    a.compose();
    assert_eq!(a.ui.hover, Some(Hit::CopyCode));
    assert!(a.hit_rect(Hit::CopyCode).is_some());
    assert!(a.ui.tips.deadline().is_some(), "its tip waits");
    a.timers(std::time::Instant::now() + std::time::Duration::from_secs(1));
    assert_eq!(
        a.ui.tips.shown().map(|t| t.text.as_str()),
        Some("Copy code")
    );
    // Back out to the paragraph: gone. A menu over the page hides it as well.
    a.pointer_moved(plain.0, plain.1);
    a.compose();
    assert!(a.hit_rect(Hit::CopyCode).is_none());
    a.pointer_moved(code.0, code.1);
    a.compose();
    assert!(a.hit_rect(Hit::CopyCode).is_some());
    a.ui.mouse = (code.0, code.1);
    a.right_press();
    a.compose();
    assert!(a.hit_rect(Hit::CopyCode).is_none());
}

#[test]
fn the_copy_button_follows_the_page_when_it_scrolls() {
    let mut a = app(&["showcase.md"]);
    a.ui.panes.outline = false;
    a.compose();
    a.ws.scroll_to(300.0);
    a.compose();
    let (code, _) = code_and_plain_points(&mut a);
    a.pointer_moved(code.0, code.1);
    a.compose();
    let before = rect(&a, Hit::CopyCode);
    // The wheel moves the page under a resting pointer; the button is drawn for what is there now.
    a.ws.scroll_to(a.ws.scroll() + 20.0);
    a.compose();
    let after = a.hit_rect(Hit::CopyCode);
    if let Some(after) = after {
        assert!(after.y < before.y, "{before:?} -> {after:?}");
    }
}

#[test]
fn copy_code_block_takes_the_first_block_in_view_without_its_last_newline() {
    let mut a = app(&["showcase.md"]);
    a.ws.scroll_to(300.0);
    a.compose();
    let text = a.ws.code_in_reach().expect("a code block in view");
    assert!(!text.ends_with('\n'));
    assert!(!text.is_empty());
    // The same text from the box itself.
    let (code, _) = code_and_plain_points(&mut a);
    let vr = page::viewport(&a.ws, a.areas.page);
    let spot =
        a.ws.code_spot((code.0 - vr.x) * a.scale, (code.1 - vr.y) * a.scale)
            .unwrap();
    assert_eq!(a.ws.code_text(spot.start).as_deref(), Some(text.as_str()));
}

#[test]
fn copy_code_block_is_a_palette_command() {
    use crate::palette::Target;
    let mut a = app(&["showcase.md"]);
    a.open_palette(">copy code");
    a.compose();
    assert!(listed(&a).contains(&Target::Cmd(CommandId::CopyCodeBlock)));
}

#[test]
fn a_click_on_the_copy_button_copies_the_code_and_shows_a_check_for_a_while() {
    let mut a = app(&["showcase.md"]);
    a.ui.panes.outline = false;
    a.compose();
    a.ws.scroll_to(300.0);
    a.compose();
    let (code, _) = code_and_plain_points(&mut a);
    a.pointer_moved(code.0, code.1);
    a.compose();
    let vr = page::viewport(&a.ws, a.areas.page);
    let spot =
        a.ws.code_spot((code.0 - vr.x) * a.scale, (code.1 - vr.y) * a.scale)
            .unwrap();
    let expected = a.ws.code_text(spot.start).unwrap();
    crate::codebtn::COPIED.with(|c| c.borrow_mut().take());
    click(&mut a, Hit::CopyCode);
    let copied = crate::codebtn::COPIED.with(|c| c.borrow_mut().take());
    assert_eq!(copied.as_deref(), Some(expected.as_str()));
    assert!(expected.starts_with("fn main() {") && expected.ends_with('}'));
    // The press on the button is not the start of a selection on the page.
    assert!(a.ws.selected_text().is_none());
    assert!(a.ui.drag.is_none());
    // The check mark is for this block, and the loop wakes to take it away.
    let (start, until) = a.ui.copied.expect("the check mark shows");
    assert_eq!(start, spot.start);
    let now = Instant::now();
    assert!(a.timers(now).is_some_and(|t| t <= until));
    assert!(a.ui.copied.is_some());
    a.timers(until + std::time::Duration::from_millis(1));
    assert!(a.ui.copied.is_none());
    assert!(a.wants_frame, "the button is drawn again without its check");
}

#[test]
fn copy_code_block_copies_the_selected_block_or_says_there_is_none() {
    let mut a = app(&["showcase.md"]);
    a.ws.scroll_to(300.0);
    a.compose();
    let text = a.ws.code_in_reach().unwrap();
    crate::codebtn::COPIED.with(|c| c.borrow_mut().take());
    a.command(CommandId::CopyCodeBlock);
    assert_eq!(
        crate::codebtn::COPIED.with(|c| c.borrow_mut().take()),
        Some(text)
    );
    assert_eq!(a.ws.status(Instant::now()), Some("Code copied"));
    // A page without code in view: nothing is copied and the status says so.
    a.ws.scroll_to(0.0);
    a.compose();
    if a.ws.code_in_reach().is_none() {
        a.command(CommandId::CopyCodeBlock);
        assert!(crate::codebtn::COPIED.with(|c| c.borrow().is_none()));
        assert_eq!(a.ws.status(Instant::now()), Some("No code block in view"));
    }
}

// ---- hover cards -----------------------------------------------------------------------------

/// `previews.md` in a 1000x600 window with no panes, as the screenshots have it.
fn previews_app() -> App {
    let args = Args {
        files: vec![fixture("previews.md")],
        dark: Some(false),
        files_pane: Some(false),
        outline_pane: Some(false),
        no_session: true,
        ..Args::default()
    };
    let mut app = App::new(args, Workspace::new(None), None);
    app.scale = 1.0;
    app.size = (1000, 600);
    app.setup();
    app.compose();
    app
}

/// The first point of the page, row by row, whose hover is what `want` asks for. The tests find
/// their marks and links this way, so they hold whatever fonts the system lays the page out with.
fn hover_point(a: &mut App, want: impl Fn(&crate::workspace::Hover) -> bool) -> (f32, f32) {
    let vr = page::viewport(&a.ws, a.areas.page);
    let mut y = vr.y + 2.0;
    while y < vr.bottom() - 2.0 {
        let mut x = vr.x + 2.0;
        while x < vr.right() - 2.0 {
            let (_, px, py) = a.page_point(x, y);
            if want(&a.ws.hover(px, py)) {
                // A little inside the mark, not on its first pixel.
                return (x + 2.0, y + 2.0);
            }
            x += 3.0;
        }
        y += 3.0;
    }
    panic!("nothing on the page matches");
}

fn footnote_mark(a: &mut App) -> (f32, f32) {
    use crate::preview::Target;
    hover_point(a, |h| h.preview == Some(Target::Footnote(0)))
}

fn heading_link(a: &mut App) -> (f32, f32) {
    use crate::preview::Target;
    hover_point(a, |h| matches!(h.preview, Some(Target::Heading(_))))
}

fn file_link(a: &mut App) -> (f32, f32) {
    use crate::preview::Target;
    hover_point(a, |h| matches!(h.preview, Some(Target::File { .. })))
}

/// Rests the pointer at `(x, y)` past the tooltips' delay.
fn rest(a: &mut App, (x, y): (f32, f32)) {
    a.pointer_moved(x, y);
    a.timers(std::time::Instant::now() + std::time::Duration::from_secs(1));
    a.compose();
}

#[test]
fn resting_on_a_footnote_shows_its_text_and_moving_away_hides_it() {
    let mut a = previews_app();
    let mark = footnote_mark(&mut a);
    a.pointer_moved(mark.0, mark.1);
    let c = a.ui.card.as_ref().expect("a card waits");
    assert!(!c.shown, "not before the pointer has rested");
    let due = a
        .timers(std::time::Instant::now())
        .expect("a wake for the card");
    assert!(due > std::time::Instant::now());
    rest(&mut a, mark);
    let c = a.ui.card.as_ref().expect("the card");
    assert!(c.shown);
    assert_eq!(c.content.as_ref().unwrap().title, "Footnote 1");
    assert!(c.content.as_ref().unwrap().body.starts_with("A short note"));
    assert!(a.card_layout(1000.0, 600.0).is_some());
    // Small moves on the mark keep it; leaving the mark drops it.
    a.pointer_moved(mark.0 + 1.0, mark.1 + 1.0);
    assert!(a.ui.card.as_ref().is_some_and(|c| c.shown));
    a.pointer_moved(600.0, 450.0);
    assert!(a.ui.card.is_none());
}

#[test]
fn a_link_to_a_heading_of_the_page_shows_the_heading_and_its_lines() {
    let mut a = previews_app();
    let link = heading_link(&mut a);
    rest(&mut a, link);
    let p = a.ui.card.as_ref().and_then(|c| c.content.clone()).unwrap();
    assert_eq!(p.title, "Notes");
    assert!(p.body.starts_with("The notes are a heading"));
}

#[test]
fn a_local_markdown_link_is_read_on_a_thread_and_then_shown() {
    let mut a = previews_app();
    let link = file_link(&mut a);
    a.pointer_moved(link.0, link.1);
    a.timers(std::time::Instant::now() + std::time::Duration::from_secs(1));
    // The answer is being read: the card waits and does not ask again every tick.
    assert!(a.ui.card.as_ref().is_some_and(|c| c.loading && !c.shown));
    assert_eq!(a.timers(std::time::Instant::now()), None);
    let mut tries = 0;
    while !a.ui.previews.collect() {
        tries += 1;
        assert!(tries < 500, "the thread never answered");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    a.resolve_card(std::time::Instant::now() + std::time::Duration::from_secs(1));
    let c = a.ui.card.as_ref().expect("the card");
    assert!(c.shown);
    assert_eq!(c.content.as_ref().unwrap().title, "The linked file");
    // Resting on it again is answered from what was kept, with no thread.
    a.pointer_moved(600.0, 450.0);
    rest(&mut a, link);
    assert!(a.ui.card.as_ref().is_some_and(|c| c.shown));
}

#[test]
fn a_web_address_and_the_page_text_have_no_card() {
    let mut a = previews_app();
    let web = hover_point(&mut a, |h| h.link.is_some() && h.preview.is_none());
    rest(&mut a, web);
    assert!(a.ui.card.is_none());
    let text = hover_point(&mut a, |h| {
        h.text && h.link.is_none() && h.preview.is_none()
    });
    rest(&mut a, text);
    assert!(a.ui.card.is_none());
}

#[test]
fn a_key_a_press_and_the_wheel_dismiss_the_card() {
    use winit::dpi::PhysicalPosition;
    let mut a = previews_app();
    let mark = footnote_mark(&mut a);
    rest(&mut a, mark);
    assert!(a.ui.card.is_some());
    key(&mut a, KeyName::Char('x'), Some("x"));
    assert!(a.ui.card.is_none());
    rest(&mut a, mark);
    a.wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        0.0, -40.0,
    )));
    // The page moved under the pointer: the card is gone, and whatever is under the pointer now
    // has at most a card that waits for it to rest again.
    assert!(!a.ui.card.as_ref().is_some_and(|c| c.shown));
    a.ws.scroll_to(0.0);
    a.compose();
    rest(&mut a, mark);
    assert!(a.ui.card.as_ref().is_some_and(|c| c.shown));
    a.left_press();
    assert!(a.ui.card.is_none());
}

#[test]
fn previews_can_be_turned_off_in_settings() {
    use crate::settings as s;
    use mdshared_ui::settings::Value;
    let mut a = previews_app();
    let mark = footnote_mark(&mut a);
    rest(&mut a, mark);
    assert!(a.ui.card.is_some());
    a.apply_setting(s::PREVIEWS, Value::Bool(false));
    assert!(!a.ws.previews());
    assert!(a.ui.card.is_none());
    rest(&mut a, mark);
    assert!(a.ui.card.is_none(), "no card while they are off");
    a.apply_setting(s::PREVIEWS, Value::Bool(true));
    rest(&mut a, mark);
    assert!(a.ui.card.is_some());
}

// ---- find in folder ----------------------------------------------------------------------------

/// A folder of its own with a few Markdown files and a note that is not one, removed when dropped.
struct Folder(PathBuf);

impl Folder {
    fn new(name: &str) -> Folder {
        let dir = std::env::temp_dir().join(format!("mdview-fs-{name}-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.md"), "# Alpha\n\nplain\n\nthe Needle here\n").unwrap();
        std::fs::write(dir.join("sub/b.md"), "# Beta\n\nneedle one\n\nneedle two\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "needle in a text file\n").unwrap();
        Folder(mdview_core::files::canonical_or_absolute(&dir))
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Waits for the search of the open Find in folder to end, taking what it finds as the window does.
fn settle(a: &mut App) {
    let p = a.ui.palette.as_mut().expect("Find in folder is open");
    for _ in 0..4000 {
        p.poll_folder();
        if !p.folder_running() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the search did not end");
}

/// The rows listed: `(id, label)`.
fn rows(a: &App) -> Vec<(u32, String)> {
    let p = &a.ui.palette.as_ref().unwrap().palette;
    let ids = p.listed_ids();
    ids.into_iter()
        .map(|id| (id, p.label_of(id).unwrap_or_default().to_string()))
        .collect()
}

fn folder_app(f: &Folder) -> App {
    let mut a = app(&["linked.md"]);
    a.set_folder(f.0.clone());
    a.compose();
    a
}

#[test]
fn find_in_folder_without_a_folder_says_so_and_opens_nothing() {
    let mut a = app(&["linked.md"]);
    a.command(C::FindInFolder);
    assert!(a.ui.palette.is_none());
    assert_eq!(
        a.ws.status(Instant::now()),
        Some("Open a folder to search it")
    );
}

#[test]
fn find_in_folder_lists_lines_by_file_and_enter_opens_the_page_at_the_match() {
    let f = Folder::new("open");
    let mut a = folder_app(&f);
    a.command(C::FindInFolder);
    a.compose();
    assert!(a.ui.modal());
    type_text(&mut a, "needle");
    settle(&mut a);
    a.compose();
    let labels: Vec<String> = rows(&a).into_iter().map(|r| r.1).collect();
    // Folders first, as the Files pane lists them; only the Markdown files; the matching lines.
    assert_eq!(
        labels,
        [
            "sub/b.md",
            "needle one",
            "needle two",
            "a.md",
            "the Needle here"
        ]
    );
    let p = a.ui.palette.as_ref().unwrap();
    // The first row that can be chosen is selected, not a file's name.
    assert_eq!(p.palette.selected_id(), Some(1));
    assert_eq!(p.folder_hit(1).map(|h| h.1), Some(3));
    // Down, Down: into the next file.
    key(&mut a, KeyName::Down, None);
    key(&mut a, KeyName::Down, None);
    key(&mut a, KeyName::Enter, None);
    assert!(a.ui.palette.is_none());
    assert_eq!(
        a.ws.active_tab().unwrap().path().file_name().unwrap(),
        "a.md"
    );
    // The find box and the page's find hold the query, on the match that was chosen.
    assert_eq!(a.ui.find.text(), "needle");
    assert_eq!(a.ws.find_query(), "needle");
    assert_eq!(
        (a.ws.find_counts().current, a.ws.find_counts().total),
        (1, 1)
    );
}

#[test]
fn find_in_folder_clicking_a_result_opens_it_and_a_second_in_the_same_file_selects_its_match() {
    let f = Folder::new("click");
    let mut a = folder_app(&f);
    a.command(C::FindInFolder);
    type_text(&mut a, "needle");
    settle(&mut a);
    a.compose();
    let p = a.ui.palette.as_ref().unwrap();
    // The second line of sub/b.md (id 2).
    let r = p.palette.row_rect(2);
    a.pointer_moved(r.cx(), r.cy());
    a.left_press();
    a.left_release();
    a.compose();
    assert!(a.ui.palette.is_none());
    assert_eq!(
        a.ws.active_tab().unwrap().path().file_name().unwrap(),
        "b.md"
    );
    assert_eq!(
        (a.ws.find_counts().current, a.ws.find_counts().total),
        (2, 2)
    );
}

#[test]
fn find_in_folder_a_new_query_replaces_the_results_and_nothing_found_says_so() {
    let f = Folder::new("query");
    let mut a = folder_app(&f);
    a.command(C::FindInFolder);
    type_text(&mut a, "needle");
    settle(&mut a);
    assert_eq!(rows(&a).len(), 5);
    type_text(&mut a, "x");
    settle(&mut a);
    a.compose();
    assert!(rows(&a).is_empty());
    let p = a.ui.palette.as_ref().unwrap();
    assert_eq!(p.palette.selected_id(), None);
    key(&mut a, KeyName::Backspace, None);
    settle(&mut a);
    assert_eq!(rows(&a).len(), 5, "the first query again");
    // Enter with nothing selected and the card empty does nothing but stay.
    for _ in 0..6 {
        key(&mut a, KeyName::Backspace, None);
    }
    settle(&mut a);
    assert!(rows(&a).is_empty());
    key(&mut a, KeyName::Enter, None);
    assert!(a.ui.palette.is_some());
}

#[test]
fn find_in_folder_escape_closes_it_and_ends_the_search() {
    let f = Folder::new("escape");
    let mut a = folder_app(&f);
    a.command(C::FindInFolder);
    type_text(&mut a, "needle");
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.palette.is_none());
    assert!(!a.ui.modal());
    // The page is as it was.
    assert_eq!(
        a.ws.active_tab().unwrap().path().file_name().unwrap(),
        "linked.md"
    );
}

#[test]
fn find_in_folder_starts_with_the_find_boxes_text_and_is_in_the_menu_and_the_palette() {
    use crate::palette::Target;
    let f = Folder::new("prefill");
    let mut a = folder_app(&f);
    a.ui.find.set_text("alpha");
    a.command(C::FindInFolder);
    settle(&mut a);
    assert_eq!(a.ui.palette.as_ref().unwrap().palette.query(), "alpha");
    assert_eq!(rows(&a).len(), 2);
    key(&mut a, KeyName::Escape, None);
    // The command palette lists it, enabled with a folder, with no key of its own.
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, ">find in f");
    assert_eq!(listed(&a).first(), Some(&Target::Cmd(C::FindInFolder)));
    assert_eq!(crate::keys::hint(C::FindInFolder), "");
    key(&mut a, KeyName::Enter, None);
    assert!(
        a.ui.palette
            .as_ref()
            .is_some_and(|p| p.folder_running() || p.palette.query() == "alpha")
    );
}

// ---- export and print -------------------------------------------------------------------------

#[test]
fn export_and_print_are_in_the_tab_menu_and_the_palette_and_need_a_document() {
    use crate::palette::Target;
    let a = app(&["showcase.md", "tables.md"]);
    // Not in the main menu: it has to fit the default window without scrolling.
    let main = overlays::main_menu(&a.ws, &a.ui, 0.0, 0.0).items;
    let items = overlays::tab_menu(&a.ws, 0, false, 0.0, 0.0).items;
    for (id, label) in [
        (C::ExportHtml, "Export as HTML…"),
        (C::Print, "Print (opens in browser)"),
    ] {
        assert!(!main.iter().any(|i| i.id == Act::Cmd(id)));
        let item = items
            .iter()
            .find(|i| i.id == Act::TabCmd(id, 0))
            .unwrap_or_else(|| panic!("{id:?} is not in the tab menu"));
        assert_eq!(mdshared_ui::menu::parse_access(&item.label).text, label);
        assert!(item.enabled);
        assert!(!overlays::command_icon(id).is_empty());
    }
    // Chosen for a tab that is not the active one, it is that tab's page: the tab comes forward.
    let mut b = app(&["showcase.md", "tables.md"]);
    let other = 1 - b.ws.active().unwrap();
    b.exporting = true;
    b.tab_command(C::Print, other);
    assert_eq!(b.ws.active(), Some(other));
    assert_eq!(
        b.ws.status(Instant::now()),
        Some("Another export is still running")
    );
    // Without a document there is nothing to export.
    assert!(app(&[]).export_job().is_none());
    let mut a = a;
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, ">export");
    assert!(listed(&a).contains(&Target::Cmd(C::ExportHtml)));
    key(&mut a, KeyName::Escape, None);
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, ">print");
    assert!(listed(&a).contains(&Target::Cmd(C::Print)));
}

#[test]
fn the_exported_page_is_the_active_document_in_the_readers_page_look() {
    let mut a = app(&["showcase.md"]);
    a.ws.set_page_width(PageWidth::Wide);
    let job = a.export_job().expect("a document is open");
    let html = job.page(false);
    assert!(html.starts_with("<!DOCTYPE html>"));
    // A wiki-link is its words, as on the page, not its brackets.
    assert!(html.contains("the second page"));
    assert!(!html.contains("[[linked|"), "a wiki-link is written raw");
    assert!(
        html.contains("<h1 id="),
        "the page has the document's headings"
    );
    assert!(html.contains("--width:1080px"), "the chosen page width");
    assert!(
        html.contains("--bg:#fbfbfb"),
        "the light theme, whatever the window shows"
    );
    assert!(!html.contains("window.print"));
    assert!(job.page(true).contains("window.print"));
    // Nothing without a document.
    assert!(app(&[]).export_job().is_none());
}

#[test]
fn a_second_export_waits_and_the_answers_clear_and_report() {
    use export::Progress;
    let mut a = app(&["showcase.md"]);
    // No event loop in this test, so there is no helper thread to start: nothing runs.
    a.command(C::ExportHtml);
    a.command(C::Print);
    assert!(!a.exporting);
    a.exporting = true;
    a.command(C::ExportHtml);
    assert_eq!(
        a.ws.status(Instant::now()),
        Some("Another export is still running")
    );
    a.export_progress(Progress::Working);
    assert_eq!(a.ws.status(Instant::now()), Some("Exporting…"));
    assert!(a.exporting);
    a.export_progress(Progress::Saved(PathBuf::from("/x/read me.html")));
    assert!(!a.exporting);
    assert_eq!(
        a.ws.status(Instant::now()),
        Some("Exported to read me.html")
    );
    for (progress, text) in [
        (Progress::Printing, Some("Opened in the browser to print")),
        (
            Progress::Failed("Could not save the HTML file"),
            Some("Could not save the HTML file"),
        ),
        (Progress::Cancelled, None),
    ] {
        a.exporting = true;
        a.ws.clear_status();
        a.export_progress(progress);
        assert!(!a.exporting);
        assert_eq!(a.ws.status(Instant::now()), text);
    }
}

// ---- presentation ------------------------------------------------------------------------------

fn slide_index(a: &App) -> usize {
    a.ui.present.as_ref().expect("presenting").index()
}

#[test]
fn present_shows_the_slides_and_escape_returns_to_the_same_place() {
    let mut a = app(&["slides.md"]);
    a.ws.scroll_to(120.0);
    a.compose();
    let place = a.ws.scroll();
    assert!(!a.presenting());
    a.command(CommandId::Present);
    a.compose();
    assert!(a.presenting());
    let show = a.ui.present.as_ref().unwrap();
    assert_eq!(show.len(), 4);
    // Front matter is no slide: the first one is the title.
    assert!(
        show.slide(0).starts_with("# Quarterly review"),
        "{}",
        show.slide(0)
    );
    assert_eq!(slide_index(&a), 0);
    key(&mut a, KeyName::Right, None);
    assert_eq!(slide_index(&a), 1);
    key(&mut a, KeyName::Space, Some(" "));
    assert_eq!(slide_index(&a), 2);
    key(&mut a, KeyName::Left, None);
    assert_eq!(slide_index(&a), 1);
    key(&mut a, KeyName::PageDown, None);
    key(&mut a, KeyName::PageUp, None);
    assert_eq!(slide_index(&a), 1);
    key(&mut a, KeyName::End, None);
    assert_eq!(slide_index(&a), 3);
    // Past the last slide nothing moves.
    key(&mut a, KeyName::Right, None);
    assert_eq!(slide_index(&a), 3);
    key(&mut a, KeyName::Home, None);
    assert_eq!(slide_index(&a), 0);
    // A click goes forward.
    a.pointer_moved(600.0, 400.0);
    a.left_press();
    a.left_release();
    a.compose();
    assert_eq!(slide_index(&a), 1);
    key(&mut a, KeyName::Escape, None);
    assert!(!a.presenting());
    a.compose();
    assert_eq!(a.ws.scroll(), place, "the page is where the reader left it");
    // The chrome is drawn again.
    assert!(a.hit_rect(Hit::Bar(BarItem::More)).is_some());
}

#[test]
fn a_presentation_takes_every_other_input() {
    let mut a = app(&["slides.md"]);
    a.command(CommandId::Present);
    a.compose();
    // No menu, no palette, no find box, no history travel while the slides show.
    a.pointer_moved(600.0, 400.0);
    a.right_press();
    assert!(a.ui.popup.is_none());
    a.mods = Mods {
        ctrl: true,
        ..Mods::default()
    };
    key(&mut a, KeyName::Char('p'), None);
    assert!(a.ui.palette.is_none());
    key(&mut a, KeyName::Char('f'), None);
    assert!(!a.ui.find_focused);
    a.mods = Mods::default();
    a.travel(true);
    assert!(a.presenting());
    assert_eq!(slide_index(&a), 0);
    // Present again (its key or the command) leaves, as Escape does.
    a.mods = Mods {
        shift: true,
        ..Mods::default()
    };
    key(&mut a, KeyName::F(5), None);
    assert!(!a.presenting());
}

#[test]
fn present_has_its_key_and_reload_keeps_f5() {
    use crate::keys::command_for;
    let shift = Mods {
        shift: true,
        ..Mods::default()
    };
    assert_eq!(command_for(&KeyName::F(5), shift), Some(CommandId::Present));
    assert_eq!(
        command_for(&KeyName::F(5), Mods::default()),
        Some(CommandId::Refresh)
    );
    // Without a document there is nothing to present, and the status says so.
    let mut a = app(&[]);
    a.command(CommandId::Present);
    assert!(!a.presenting());
    assert_eq!(a.ws.status(Instant::now()), Some("Nothing to present"));
}

#[test]
fn a_changed_text_keeps_the_slide_and_a_text_without_slides_ends_the_show() {
    let mut a = app(&["slides.md"]);
    let path = a.ws.active_tab().unwrap().path().to_path_buf();
    a.command(CommandId::Present);
    a.compose();
    key(&mut a, KeyName::Right, None);
    key(&mut a, KeyName::Right, None);
    assert_eq!(slide_index(&a), 2);
    // The editor sends new text (live preview): the slides are cut again, the number stays.
    assert!(a.ws.live_text(
        &path,
        "# One\n\n---\n\n# Two\n\n---\n\n# Three, new\n\n---\n\n# Four\n\n---\n\n# Five\n".into(),
        None
    ));
    a.compose();
    let show = a.ui.present.as_ref().expect("still presenting");
    assert_eq!((show.len(), show.index()), (5, 2));
    assert!(show.slide(2).contains("Three, new"));
    // Fewer slides than the number: the last one.
    assert!(a.ws.live_text(&path, "# Only\n\ntext\n".into(), None));
    a.compose();
    let show = a.ui.present.as_ref().expect("still presenting");
    assert_eq!((show.len(), show.index()), (1, 0));
    // Nothing left to show: the presentation ends.
    assert!(a.ws.live_text(&path, "\n".into(), None));
    a.compose();
    assert!(!a.presenting());
}

// ---- a document mdedit has not saved yet ---------------------------------------------------

pub(super) const DOC: &str = "mdedit-77-3";

/// "Preview in mdview" on a document that has no file: its text, its name, and `open`.
pub(super) fn shown(doc: &str, text: &str, line: Option<u32>) -> Request {
    Request {
        open: true,
        text: Some(text.to_string()),
        line,
        from: Some("mdedit".into()),
        ..Request::document(doc, "Untitled 1")
    }
}

/// A later update of it: the text, or the caret's line alone.
fn sent(doc: &str, text: Option<&str>, line: Option<u32>) -> Request {
    Request {
        follow: true,
        text: text.map(str::to_string),
        line,
        ..Request::document(doc, "Untitled 1")
    }
}

#[test]
fn a_document_without_a_file_gets_a_tab_from_its_text_and_follows_the_editor() {
    let (mut a, dir, _file) = live_app("untitled");
    // A text that does not ask to be shown opens nothing.
    a.handed_over(vec![sent(DOC, Some("# Stray\n"), None)]);
    a.compose();
    assert_eq!(a.ws.tab_count(), 1);
    // Shown: a tab named like the editor's, active, live, with the text.
    a.handed_over(vec![shown(DOC, "# Never saved\n\nBody.\n", Some(1))]);
    a.compose();
    assert_eq!((a.ws.tab_count(), a.ws.active()), (2, Some(1)));
    assert_eq!(a.ws.labels()[1], "Untitled 1");
    let tab = a.ws.active_tab().unwrap();
    assert!(tab.is_untitled() && tab.is_live() && !tab.is_missing() && tab.is_loaded());
    assert!(tab.describe().starts_with("Untitled 1 ("));
    assert!(a.ws.title().contains("Untitled 1"));
    assert!(page_text(&a).contains("Never saved"));
    // The next texts replace it; the tab is not opened twice, and nothing else is raised.
    let mut long = String::from("# Never saved\n\n");
    for i in 0..200 {
        long.push_str(&format!(
            "Paragraph {i} that exists only in the editor.\n\n"
        ));
    }
    a.handed_over(vec![sent(DOC, Some(&long), Some(300))]);
    a.compose();
    assert_eq!(a.ws.tab_count(), 2);
    assert!(page_text(&a).contains("Paragraph 199"));
    let top = a.ws.top_source_line().unwrap();
    assert!((280..=300).contains(&top), "followed to the caret: {top}");
    // The caret alone moves the page.
    a.handed_over(vec![sent(DOC, None, Some(3))]);
    a.compose();
    assert!(a.ws.top_source_line().unwrap() < 20);
    // An image and a link given relative to the document have no folder to be found in: the
    // page is drawn, the link is refused, and nothing is opened.
    let text = "# Links\n\n![pic](pic.png)\n\n[other](other.md) [abs](https://example.com)\n";
    a.handed_over(vec![sent(DOC, Some(text), Some(1))]);
    a.compose();
    assert!(page_text(&a).contains("other"));
    let refused = a.ws.open_link("other.md");
    assert!(matches!(refused, crate::workspace::LinkOutcome::Refused(_)));
    assert_eq!(
        a.ws.status(std::time::Instant::now()),
        Some("not a file: other.md")
    );
    assert_eq!(a.ws.tab_count(), 2);
    // There is no file: F5 says so and changes nothing, and the watcher has nothing to see.
    assert!(!a.ws.reload());
    a.ws.check_changed();
    a.compose();
    assert!(a.ws.active_tab().unwrap().is_live() && page_text(&a).contains("Links"));
    assert!(a.ws.watched_paths().all(|p| p.is_file()));
    // Left alone, the tab keeps its text (there is nothing to read it from again).
    a.ws.activate(0);
    let later = std::time::Instant::now() + std::time::Duration::from_secs(600);
    a.ws.drop_idle(later, std::time::Duration::from_secs(60));
    assert!(a.ws.tabs()[1].is_loaded());
    a.ws.activate(1);
    a.compose();
    // The editor ends the preview: the text stays, the mark goes.
    a.handed_over(vec![Request {
        live_off: true,
        ..Request::document(DOC, "Untitled 1")
    }]);
    a.compose();
    let tab = a.ws.active_tab().unwrap();
    assert!(!tab.is_live() && tab.is_untitled());
    assert!(page_text(&a).contains("Links"));
    // Closed here, it does not come back with the editor's next keystroke.
    a.ws.close(1);
    a.handed_over(vec![sent(DOC, Some("# Typed on\n"), Some(1))]);
    a.compose();
    assert_eq!(a.ws.tab_count(), 1);
    // Until the editor shows it again.
    a.handed_over(vec![shown(DOC, "# Typed on\n", Some(1))]);
    a.compose();
    assert_eq!(a.ws.tab_count(), 2);
    assert!(page_text(&a).contains("Typed on"));
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_saved_document_becomes_its_files_tab_without_a_duplicate() {
    let (mut a, dir, file) = live_app("untitled-saved");
    a.handed_over(vec![shown(DOC, "# Draft\n\nBody.\n", Some(1))]);
    a.compose();
    // The editor saved it: the same tab is the file's now, read from disk, no longer live.
    let saved = dir.join("notes.md");
    std::fs::write(&saved, "# Draft\n\nBody, saved.\n").unwrap();
    let became = |path: &Path| Request {
        follow: true,
        doc: Some(DOC.into()),
        line: Some(1),
        ..Request::new(path)
    };
    a.handed_over(vec![became(&saved)]);
    a.compose();
    assert_eq!((a.ws.tab_count(), a.ws.active()), (2, Some(1)));
    let tab = a.ws.active_tab().unwrap();
    assert!(!tab.is_untitled() && !tab.is_live() && !tab.is_missing());
    assert!(crate::workspace::same_file(
        tab.path(),
        &mdshared_app::paths::canonical_or_absolute(&saved)
    ));
    assert_eq!(a.ws.labels()[1], "notes.md");
    assert!(page_text(&a).contains("Body, saved."));
    // It is an ordinary file tab from here: the editor's text for the file is taken as live.
    a.handed_over(vec![live(&saved, "# Draft\n\nEdited again.\n", None)]);
    a.compose();
    assert!(a.ws.active_tab().unwrap().is_live() && page_text(&a).contains("Edited again"));
    // The old name means nothing any more.
    a.handed_over(vec![sent(DOC, Some("# Late\n"), None)]);
    a.compose();
    assert!(!page_text(&a).contains("Late"));
    // A second document saved over a file that already has a tab: that tab is shown, and the
    // document's own tab goes. The file is never open twice.
    let other = "mdedit-77-4";
    a.handed_over(vec![shown(other, "# Second\n", Some(1))]);
    a.compose();
    assert_eq!((a.ws.tab_count(), a.ws.active()), (3, Some(2)));
    a.handed_over(vec![Request {
        doc: Some(other.into()),
        ..became(&file)
    }]);
    a.compose();
    assert_eq!((a.ws.tab_count(), a.ws.active()), (2, Some(0)));
    assert!(page_text(&a).contains("of the file on disk"));
    assert!(a.ws.tabs().iter().all(|t| !t.is_untitled()));
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_editor_that_is_gone_leaves_the_unsaved_document_as_it_was_last_sent() {
    let (mut a, dir, _file) = live_app("untitled-gone");
    a.handed_over(vec![shown(DOC, "# Draft\n", Some(1))]);
    a.compose();
    // Live, so the editor is asked after like for any live tab.
    assert!(a.timers(std::time::Instant::now()).is_some());
    a.woken(Wake::Sister(false));
    a.compose();
    let tab = a.ws.active_tab().unwrap();
    assert!(!tab.is_live() && tab.is_untitled());
    assert!(page_text(&a).contains("Draft"));
    assert_eq!(
        a.ws.status(std::time::Instant::now()),
        Some("mdedit closed: this document was not saved")
    );
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_tab_menu_of_an_unsaved_document_asks_the_editor_by_name() {
    let (mut a, dir, _file) = live_app("untitled-menu");
    a.handed_over(vec![shown(DOC, "# Draft\n\nBody.\n", Some(3))]);
    a.compose();
    // No path to copy and nowhere to show; Edit in mdedit stays.
    let menu = overlays::tab_menu(&a.ws, 1, false, 0.0, 0.0);
    let enabled = |id: CommandId| {
        let item = menu.items.iter().find(|it| it.id == Act::TabCmd(id, 1));
        item.is_some_and(|it| it.enabled)
    };
    assert!(enabled(CommandId::EditInMdedit));
    assert!(!enabled(CommandId::CopyFilePath) && !enabled(CommandId::ShowInFileManager));
    // The editor is asked for the document by the name it gave, at the line being read.
    let _one = EDITOR_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let settings = dir.join("editor").join("settings.ini");
    let rx = mdshared_app::single::listen_requests_waking("mdedit", &settings, || {}).unwrap();
    // SAFETY: the tests that write this variable take `EDITOR_ENV` first; no other reads it.
    unsafe { std::env::set_var("MD_SISTER_MDEDIT_SETTINGS", &settings) };
    a.tab_command(CommandId::EditInMdedit, 1);
    let r = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert_eq!(r.doc.as_deref(), Some(DOC));
    assert!(r.path.as_os_str().is_empty() && !r.follow && r.line.is_some());
    assert_eq!(r.from.as_deref(), Some("mdview"));
    // With no editor running it says so; a path that is not one is never copied.
    unsafe { std::env::set_var("MD_SISTER_MDEDIT_SETTINGS", dir.join("none.ini")) };
    a.tab_command(CommandId::EditInMdedit, 1);
    assert_eq!(
        a.ws.status(std::time::Instant::now()),
        Some("mdedit is not running")
    );
    a.tab_command(CommandId::CopyFilePath, 1);
    assert_eq!(
        a.ws.status(std::time::Instant::now()),
        Some("This document has no file yet: it is shown as mdedit has it.")
    );
    unsafe { std::env::remove_var("MD_SISTER_MDEDIT_SETTINGS") };
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_editor_giving_up_side_by_side_opens_and_changes_nothing() {
    let mut a = app(&["showcase.md"]);
    let tabs = a.ws.tab_count();
    // What the editor sends: a live update with no file and no line.
    let message = super::commands::unplace_request().encode("mdview");
    let lines = message.lines().skip(1).take_while(|l| !l.is_empty());
    let requests = Request::parse(lines);
    assert!(requests.len() == 1 && requests[0].unplace && requests[0].follow);
    a.placed_for = Some((std::path::PathBuf::from("x.md"), None));
    a.handed_over(requests);
    assert_eq!(a.ws.tab_count(), tabs, "nothing was opened or closed");
    assert!(a.placed_for.is_none(), "the arrangement is over");
}

#[test]
fn a_tab_is_found_by_its_file_or_by_the_editors_name_for_it() {
    let a = app(&["showcase.md"]);
    let showcase = fixture("showcase.md");
    assert!(a.ws.shows(&showcase, None));
    assert!(!a.ws.shows(std::path::Path::new("no-such.md"), None));
    assert!(!a.ws.shows(std::path::Path::new(""), Some("mdedit-1-2")));
}

// ---- copy as markdown ------------------------------------------------------------------------

#[test]
fn copy_as_markdown_is_a_palette_command_that_waits_for_a_selection() {
    use crate::palette::Target;
    let mut a = app(&["showcase.md"]);
    a.open_palette(">copy as");
    a.compose();
    assert!(listed(&a).contains(&Target::Cmd(CommandId::CopyMarkdown)));
    let cmd = mdview_core::commands::command(CommandId::CopyMarkdown);
    assert!(cmd.shortcuts.is_empty(), "no default key");
    // Without a selection the command only says so; nothing is copied.
    crate::codebtn::COPIED.with(|c| c.borrow_mut().take());
    a.command(CommandId::CopyMarkdown);
    assert!(crate::codebtn::COPIED.with(|c| c.borrow().is_none()));
    assert_ne!(a.ws.status(Instant::now()), Some("Copied"));
}

#[test]
fn copy_as_markdown_copies_the_source_and_says_copied() {
    let mut a = app(&["showcase.md"]);
    a.compose();
    assert!(a.ws.select_all());
    crate::codebtn::COPIED.with(|c| c.borrow_mut().take());
    a.command(CommandId::CopyMarkdown);
    let copied = crate::codebtn::COPIED
        .with(|c| c.borrow_mut().take())
        .expect("copied");
    let file = std::fs::read_to_string(fixture("showcase.md")).unwrap();
    assert!(
        file.replace("\r\n", "\n")
            .contains(&copied.replace("\r\n", "\n"))
    );
    assert!(copied.len() > 500);
    assert_eq!(a.ws.status(Instant::now()), Some("Copied"));
}

#[test]
fn the_page_menu_offers_copy_as_markdown_after_copy_and_greyed_without_a_selection() {
    let ctx = |has_selection| crate::workspace::PageContext {
        has_selection,
        ..Default::default()
    };
    for has in [false, true] {
        let menu = crate::overlays::page_menu(&ctx(has), false, 10.0, 10.0);
        let items = &menu.items;
        let at = items
            .iter()
            .position(|i| matches!(i.id, crate::ui::Act::Cmd(CommandId::CopyMarkdown)))
            .expect("the item");
        assert_eq!(items[at].label, "Copy as &Markdown");
        assert_eq!(items[at].enabled, has);
        assert!(matches!(items[at - 1].id, crate::ui::Act::Copy));
    }
}

#[test]
fn the_outline_pane_is_walked_with_the_keyboard_only_once_focused() {
    let mut a = app(&["everything.md"]);
    // Not focused: Down is the page's.
    key(&mut a, KeyName::Down, None);
    assert_eq!(a.ui.outline_focus, None);
    assert!(a.ws.scroll() > 0.0, "the page took the key");

    a.command(CommandId::FocusOutline);
    a.compose();
    let total = a.ws.outline().len();
    assert!(total > 20);
    let first = a.ui.outline_focus.expect("a focused row");
    let page = a.ws.scroll();

    key(&mut a, KeyName::Down, None);
    assert_eq!(a.ui.outline_focus, Some(first + 1));
    assert_eq!(a.ws.scroll(), page, "the page did not move");
    key(&mut a, KeyName::Up, None);
    key(&mut a, KeyName::Up, None);
    assert_eq!(a.ui.outline_focus, Some(first.saturating_sub(1)));
    key(&mut a, KeyName::End, None);
    assert_eq!(a.ui.outline_focus, Some(total - 1));
    assert!(
        outline_row_visible(&a, total - 1),
        "End reveals the last row"
    );
    key(&mut a, KeyName::Down, None);
    assert_eq!(a.ui.outline_focus, Some(total - 1), "no wrapping");
    key(&mut a, KeyName::Home, None);
    assert_eq!(a.ui.outline_focus, Some(0));
    assert!(outline_row_visible(&a, 0));
    key(&mut a, KeyName::PageDown, None);
    let n = a.ui.outline_focus.unwrap();
    assert!(n > 1 && n < total, "a page of rows: {n}");
    assert!(outline_row_visible(&a, n));
    key(&mut a, KeyName::PageUp, None);
    assert_eq!(a.ui.outline_focus, Some(0));

    // Enter goes to the heading and keeps the keyboard in the pane.
    key(&mut a, KeyName::End, None);
    key(&mut a, KeyName::Up, None);
    let target = a.ui.outline_focus.unwrap();
    key(&mut a, KeyName::Enter, None);
    a.compose();
    a.compose();
    let now = a.ws.current_heading().expect("a current heading");
    assert!(
        now == target || a.ws.scroll() == a.ws.max_scroll(),
        "went to {now}, wanted {target}"
    );
    assert_eq!(a.ui.outline_focus, Some(target));

    // Escape gives the keyboard back to the page.
    key(&mut a, KeyName::Escape, None);
    assert_eq!(a.ui.outline_focus, None);
    a.ws.scroll_to(0.0);
    key(&mut a, KeyName::Down, None);
    assert!(a.ws.scroll() > 0.0, "Down is the page's again");
}

#[test]
fn the_outline_ring_is_drawn_again_at_once_on_every_key() {
    let mut a = app(&["everything.md"]);
    a.command(CommandId::FocusOutline);
    a.compose();
    let outline = crate::app::Region::Outline.index();
    // Each key that moves the row changes what the pane looks like, so the pane's cached
    // bitmap must be dropped and drawn again: its signature has to change with the focus.
    let mut seen = a.sigs[outline];
    for name in [
        KeyName::Down,
        KeyName::Down,
        KeyName::Up,
        KeyName::End,
        KeyName::Home,
        KeyName::PageDown,
    ] {
        let before = a.ui.outline_focus;
        key(&mut a, name.clone(), None);
        assert_ne!(a.ui.outline_focus, before, "{name:?} moved the row");
        assert_ne!(a.sigs[outline], seen, "{name:?} repaints the outline pane");
        seen = a.sigs[outline];
    }
    // Escape takes the ring away at once.
    key(&mut a, KeyName::Escape, None);
    assert_eq!(a.ui.outline_focus, None);
    assert_ne!(a.sigs[outline], seen, "Escape repaints the outline pane");
    // So does a click on the page.
    a.command(CommandId::FocusOutline);
    a.compose();
    seen = a.sigs[outline];
    a.ui.outline_focus = None;
    a.compose();
    assert_ne!(a.sigs[outline], seen, "losing the focus repaints the pane");
}

#[test]
fn the_outline_focus_follows_clicks_and_other_panes() {
    let mut a = app(&["everything.md"]);
    click(&mut a, Hit::OutlineRow(2));
    assert_eq!(a.ui.outline_focus, Some(2));
    // A click on the page, or the find box, takes the keyboard back.
    a.pointer_moved(400.0, 400.0);
    a.left_press();
    a.left_release();
    a.compose();
    assert_eq!(a.ui.outline_focus, None);
    click(&mut a, Hit::OutlineRow(2));
    a.command(CommandId::Find);
    key(&mut a, KeyName::Down, None);
    assert_eq!(a.ui.outline_focus, None);
    // Shortcuts still work with the pane focused.
    click(&mut a, Hit::OutlineRow(1));
    a.mods.ctrl = true;
    key(&mut a, KeyName::Down, None);
    a.mods.ctrl = false;
    assert_eq!(a.ui.outline_focus, Some(1));
    // Closing the pane drops the focus; the command opens it again.
    a.command(CommandId::ToggleOutline);
    assert_eq!(a.ui.outline_focus, None);
    assert!(!a.ui.panes.outline);
    a.command(CommandId::FocusOutline);
    assert!(a.ui.panes.outline);
    assert!(a.ui.outline_focus.is_some());
}

#[test]
fn the_keyboard_walks_on_from_the_headings_into_the_backlinks_and_enter_opens_one() {
    let mut a = app(&["wiki/Home.md"]);
    a.set_folder(fixture("wiki"));
    a.ws.index_notes_now();
    a.compose();
    let headings = a.ws.outline().len();
    let links = a.ws.backlinks().len();
    assert!(links >= 4);
    a.command(CommandId::FocusOutline);
    key(&mut a, KeyName::End, None);
    assert_eq!(a.ui.outline_focus, Some(headings + links - 1));
    assert!(a.hit_rect(Hit::BacklinkRow(links - 1)).is_some());
    key(&mut a, KeyName::Up, None);
    key(&mut a, KeyName::Up, None);
    assert_eq!(a.ui.outline_focus, Some(headings + links - 3));
    let before = a.ws.active_tab().unwrap().path().to_path_buf();
    key(&mut a, KeyName::Enter, None);
    assert_ne!(a.ws.active_tab().unwrap().path(), before);
    assert_eq!(
        a.ui.outline_focus, None,
        "the keyboard went back to the page"
    );
}

#[test]
fn a_covered_window_arms_no_animation_timer_and_wakes_when_seen_again() {
    let mut a = app(&["formats.md"]);
    a.first = false; // the first frame is drawn by the event loop, which a test has not got
    a.ws.set_animation_wake(|| {});
    let mut now = Instant::now();
    // Images arrive from helper threads: compose until the animated one is on the page.
    let mut armed = None;
    for _ in 0..200 {
        a.compose();
        armed = a.timers(now);
        if armed.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        now = Instant::now();
    }
    assert!(a.shows_page(), "no window, nothing hides it");
    assert!(armed.is_some(), "an animated GIF in view asks for a wake");

    // Covered: the same page, nothing asked for. Idle is idle.
    a.occluded = true;
    assert!(!a.shows_page());
    assert_eq!(a.timers(Instant::now()), None, "no timer while covered");
    // Seen again: the timer is armed again.
    a.occluded = false;
    assert!(a.timers(Instant::now()).is_some());
}

#[test]
fn find_in_folder_searches_again_when_files_change_and_keeps_the_selected_row() {
    let f = Folder::new("changes");
    let mut a = folder_app(&f);
    a.command(C::FindInFolder);
    type_text(&mut a, "needle");
    settle(&mut a);
    a.compose();
    // Select "needle two" of sub/b.md (the second row that can be chosen).
    key(&mut a, KeyName::Down, None);
    let p = a.ui.palette.as_ref().unwrap();
    assert_eq!(p.folder_hit(p.palette.selected_id().unwrap()).unwrap().1, 5);
    assert_eq!(rows(&a).len(), 5);

    // A watcher wake with nothing changed leaves the list as it was.
    a.watch_check();
    refreshed(&mut a);
    assert_eq!(rows(&a).len(), 5);

    // Two files change: a line added to a.md, a new file (listed before a.md, so ids move). The list is
    // replaced when the new search is done, and the selection is on the same line of b.md.
    std::fs::write(
        f.0.join("a.md"),
        "# Alpha\n\nneedle top\n\nthe Needle here\n",
    )
    .unwrap();
    std::fs::write(f.0.join("0.md"), "needle zero\n").unwrap();
    a.watch_check();
    // Not replaced yet: the old rows stay while the search reads.
    assert_eq!(rows(&a).len(), 5);
    assert!(a.ui.palette.as_ref().unwrap().folder_refreshing());
    refreshed(&mut a);
    a.compose();
    let labels: Vec<String> = rows(&a).into_iter().map(|r| r.1).collect();
    assert_eq!(
        labels,
        [
            "sub/b.md",
            "needle one",
            "needle two",
            "0.md",
            "needle zero",
            "a.md",
            "needle top",
            "the Needle here"
        ]
    );
    let p = a.ui.palette.as_ref().unwrap();
    let (path, line, query) = p.folder_hit(p.palette.selected_id().unwrap()).unwrap();
    assert_eq!((path.ends_with("sub/b.md"), line), (true, 5));
    assert_eq!(query, "needle");

    // Without a query, or with the card closed, a wake searches nothing.
    key(&mut a, KeyName::Escape, None);
    a.watch_check();
    assert!(a.ui.palette.is_none());
}

/// Waits for a search started by changed files to end, taking what it finds as the window does.
fn refreshed(a: &mut App) {
    let p = a.ui.palette.as_mut().expect("Find in folder is open");
    for _ in 0..4000 {
        p.poll_folder();
        if !p.folder_refreshing() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the search did not end");
}

// ---- presentation: links, selection and the slide overview --------------------------------------

/// A deck and a neighbour file in a fresh temp folder; returns the deck's path.
fn deck(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mdview-deck-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("other.md"), "# Other file\n\nHello.\n").unwrap();
    let path = dir.join("deck.md");
    std::fs::write(
        &path,
        "# First\n\nGo to [the end](#last-slide), [the web](https://example.com/x) \
         or [the file](other.md).\n\n---\n\n## Middle\n\nSome middle words to select here, \
         and a few more of them.\n\n---\n\n## Last slide\n\ndone\n",
    )
    .unwrap();
    path
}

fn deck_app(path: &Path) -> App {
    app(&[path.to_str().unwrap()])
}

/// The first point (device px) of the slide where `want` is true, scanning the window.
fn find_on_slide(
    a: &mut App,
    mut want: impl FnMut(&mut crate::present::Show, f32, f32) -> bool,
) -> Option<(f32, f32)> {
    let show = a.ui.present.as_mut()?;
    for y in (0..800).step_by(6) {
        for x in (0..1280).step_by(8) {
            if want(show, x as f32, y as f32) {
                return Some((x as f32, y as f32));
            }
        }
    }
    None
}

/// What a click at the point would follow, when it is a link.
fn link_is(show: &mut crate::present::Show, x: f32, y: f32, dest: &str) -> bool {
    show.link_at(x, y) && {
        show.press_at(x, y);
        show.release() == crate::present::Release::Link(dest.into())
    }
}

#[test]
fn a_click_on_a_link_on_a_slide_goes_to_the_slide_that_holds_the_target() {
    let path = deck("fragment");
    let mut a = deck_app(&path);
    a.ws.scroll_to(0.0);
    a.compose();
    let place = a.ws.scroll();
    a.command(CommandId::Present);
    a.compose();
    // The three links of slide 1 are all there to be found.
    for dest in ["#last-slide", "https://example.com/x", "other.md"] {
        assert!(
            find_on_slide(&mut a, |s, x, y| link_is(s, x, y, dest)).is_some(),
            "{dest}"
        );
    }
    // An in-document link: the slide with the heading shows, and the page behind stays put.
    let (x, y) = find_on_slide(&mut a, |s, x, y| link_is(s, x, y, "#last-slide")).unwrap();
    a.pointer_moved(x, y);
    a.left_press();
    a.left_release();
    a.compose();
    assert_eq!(slide_index(&a), 2);
    assert_eq!(a.ws.scroll(), place);
    assert!(a.presenting());
    // A name no slide has is not found; one that a slide has is.
    key(&mut a, KeyName::Home, None);
    let show = a.ui.present.as_mut().unwrap();
    assert!(!show.go_to_fragment("no-such-heading"));
    assert!(show.go_to_fragment("middle"));
    assert_eq!(slide_index(&a), 1);
}

#[test]
fn a_click_on_a_file_link_on_a_slide_opens_it_and_ends_the_show() {
    let path = deck("file");
    let mut a = deck_app(&path);
    a.command(CommandId::Present);
    a.compose();
    let (x, y) = find_on_slide(&mut a, |s, x, y| link_is(s, x, y, "other.md"))
        .expect("the link to the file");
    a.pointer_moved(x, y);
    a.left_press();
    a.left_release();
    a.compose();
    assert_eq!(a.ws.tab_count(), 2, "the file opened in a tab");
    assert!(!a.presenting(), "the show was of the other tab");
}

#[test]
fn a_link_to_the_web_on_a_slide_is_handed_over_as_the_page_hands_it() {
    // Not clicked for real (that would start the browser): the click gives the destination as
    // written, which the shell hands to the workspace, which answers as it does for the page.
    let path = deck("web");
    let mut a = deck_app(&path);
    a.command(CommandId::Present);
    a.compose();
    assert!(find_on_slide(&mut a, |s, x, y| link_is(s, x, y, "https://example.com/x")).is_some());
    assert_eq!(
        a.ws.open_link("https://example.com/x"),
        LinkOutcome::OpenUrl("https://example.com/x".into())
    );
    // And a scheme the page never follows is not followed from a slide either.
    assert_eq!(a.ws.open_link("javascript:alert(1)"), LinkOutcome::Ignored);
}

#[test]
fn dragging_on_a_slide_selects_text_and_ctrl_c_copies_it_and_a_click_still_goes_on() {
    let path = deck("select");
    let mut a = deck_app(&path);
    a.command(CommandId::Present);
    a.compose();
    key(&mut a, KeyName::Right, None);
    assert_eq!(slide_index(&a), 1);
    let without = a.screen.pixels.clone();
    let (x, y) = find_on_slide(&mut a, |s, x, y| s.hit(x, y).is_some_and(|h| h.on_text))
        .expect("text on the slide");
    // A drag along the line selects, and does not go on.
    a.pointer_moved(x + 2.0, y);
    a.left_press();
    a.pointer_moved(x + 300.0, y);
    a.left_release();
    a.compose();
    assert_eq!(slide_index(&a), 1);
    let (from, to) =
        a.ui.present
            .as_ref()
            .unwrap()
            .selection()
            .expect("selected");
    assert!(to > from);
    assert_ne!(without, a.screen.pixels, "the selection tints the slide");
    // Ctrl+C puts the text on the clipboard (the tests' stand-in) and says Copied.
    crate::codebtn::COPIED.with(|c| c.borrow_mut().take());
    a.mods = Mods {
        ctrl: true,
        ..Mods::default()
    };
    key(&mut a, KeyName::Char('c'), None);
    a.mods = Mods::default();
    let copied = crate::codebtn::COPIED
        .with(|c| c.borrow_mut().take())
        .expect("copied");
    assert!(!copied.is_empty());
    let slide = a.ui.present.as_ref().unwrap().slide(1).to_string();
    assert!(slide.contains(copied.trim()), "{copied:?}");
    assert_eq!(a.ws.status(Instant::now()), Some("Copied"));
    // A plain click drops the selection and goes on.
    a.pointer_moved(x, y);
    a.left_press();
    a.left_release();
    a.compose();
    assert_eq!(slide_index(&a), 2);
    assert_eq!(a.ui.present.as_ref().unwrap().selection(), None);
    // Ctrl+A takes the whole text of the slide.
    a.mods = Mods {
        ctrl: true,
        ..Mods::default()
    };
    key(&mut a, KeyName::Char('a'), None);
    a.mods = Mods::default();
    assert!(a.ui.present.as_ref().unwrap().selection().is_some());
}

#[test]
fn the_overview_lists_the_slides_and_enter_goes_to_one() {
    let mut a = app(&["slides.md"]);
    a.command(CommandId::Present);
    a.compose();
    key(&mut a, KeyName::Right, None);
    key(&mut a, KeyName::Right, None);
    assert_eq!(slide_index(&a), 2);
    // G opens the list, on the slide showing, with a row per slide: number and title.
    key(&mut a, KeyName::Char('g'), Some("g"));
    let show = a.ui.present.as_ref().unwrap();
    assert!(show.overview_open());
    assert_eq!(show.overview_selected(), Some(2));
    assert_eq!(
        show.titles(),
        ["Quarterly review", "What changed", "Numbers", "Thank you"]
    );
    // Keys go to the list, not to the slides.
    key(&mut a, KeyName::Down, None);
    assert_eq!(a.ui.present.as_ref().unwrap().overview_selected(), Some(3));
    assert_eq!(slide_index(&a), 2);
    // Enter goes there and closes the list.
    key(&mut a, KeyName::Enter, None);
    assert_eq!(slide_index(&a), 3);
    assert!(!a.ui.present.as_ref().unwrap().overview_open());
    // Escape closes only the list; the show stays.
    key(&mut a, KeyName::Char('g'), Some("g"));
    key(&mut a, KeyName::Escape, None);
    assert!(a.presenting());
    assert!(!a.ui.present.as_ref().unwrap().overview_open());
    assert_eq!(slide_index(&a), 3);
    // Typing narrows the list: a word of the title finds its slide.
    key(&mut a, KeyName::Char('g'), Some("g"));
    type_text(&mut a, "numb");
    key(&mut a, KeyName::Enter, None);
    assert_eq!(slide_index(&a), 2);
    // Present still leaves from inside the list.
    key(&mut a, KeyName::Char('g'), Some("g"));
    a.mods = Mods {
        shift: true,
        ..Mods::default()
    };
    key(&mut a, KeyName::F(5), None);
    a.mods = Mods::default();
    assert!(!a.presenting());
}

#[test]
fn the_overview_is_drawn_over_the_slide_and_a_click_on_a_row_goes_there() {
    let mut a = app(&["slides.md"]);
    a.command(CommandId::Present);
    a.compose();
    let plain = a.screen.pixels.clone();
    key(&mut a, KeyName::Char('g'), Some("g"));
    assert_ne!(plain, a.screen.pixels, "the list is on screen");
    // The pointer over the fourth row selects it, a click chooses it.
    let row = {
        let show = a.ui.present.as_mut().unwrap();
        (100..700)
            .step_by(4)
            .find_map(|y| {
                show.overview_move(640.0, y as f32);
                (show.overview_selected() == Some(3)).then_some(y as f32)
            })
            .expect("a row for slide 4")
    };
    a.pointer_moved(640.0, row);
    a.left_press();
    a.left_release();
    a.compose();
    assert_eq!(slide_index(&a), 3);
    assert!(!a.ui.present.as_ref().unwrap().overview_open());
}

#[test]
fn a_window_the_system_does_not_report_covered_but_gdi_says_is_asked_about_once_a_second() {
    let mut a = app(&["formats.md"]);
    a.first = false;
    a.ws.set_animation_wake(|| {});
    let mut now = Instant::now();
    let mut armed = None;
    for _ in 0..200 {
        a.compose();
        armed = a.timers(now);
        if armed.is_some() && a.anim_seen {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        now = Instant::now();
    }
    assert!(armed.is_some() && a.anim_seen, "an animated GIF is playing");

    // Covered (the answer of the system call, kept for a second): no animation work, no frame,
    // only a slow look again.
    let now = Instant::now();
    a.covered = Some((now, true));
    a.anim_rows = None;
    a.wants_frame = false;
    assert_eq!(a.timers(now), Some(now + COVER_CHECK));
    assert!(
        a.anim_rows.is_none() && !a.wants_frame,
        "no frame while covered"
    );
    // Once the answer is a second old it is asked again (no window here: not covered) and the
    // animation goes on as before.
    let later = now + COVER_CHECK + Duration::from_millis(1);
    let _ = a.timers(later);
    assert_eq!(a.covered, Some((later, false)));
    // Without an animated image on the page nothing is asked at all.
    let mut b = app(&["showcase.md"]);
    b.first = false;
    b.compose();
    assert_eq!(b.timers(Instant::now()), None, "idle is idle");
    assert_eq!(b.covered, None, "and the system is never asked");
}

#[test]
fn wiki_labels_with_markup_and_embeds_resolve_beside_the_document() {
    let a = app(&["wiki-embeds/Embeds.md"]);
    let doc = a.ws.active_document().expect("the document is open");
    // Four labels, the note embed and its labelled twin: each one link to Target.
    let wiki: Vec<_> = doc.links.iter().filter(|l| l.wiki).collect();
    assert_eq!(wiki.len(), 6);
    assert!(wiki.iter().all(|l| !l.missing));
    assert!(doc.text.contains("a b c has emphasis inside one link"));
    // The picture that is there is an image; the two that are not stay as written.
    assert_eq!(doc.images.len(), 1);
    assert_eq!(doc.str(doc.images[0].src), "../assets/badge.png");
    assert!(doc.text.contains("![[missing.png]] and ![[Nowhere]]"));
    // The embeds left as text are remembered, so the document is parsed again when they appear.
    let kept: Vec<&str> = doc.wiki_text.iter().map(|&s| doc.str(s)).collect();
    assert_eq!(kept, ["!missing.png", "!Nowhere"]);
}

#[test]
fn resting_on_an_abbreviation_shows_its_title_and_text_beside_it_shows_none() {
    use crate::preview::Target;
    let args = Args {
        files: vec![fixture("render-more.md")],
        dark: Some(false),
        files_pane: Some(false),
        outline_pane: Some(false),
        no_session: true,
        ..Args::default()
    };
    let mut a = App::new(args, Workspace::new(None), None);
    a.scale = 1.0;
    a.size = (1000, 600);
    a.setup();
    a.compose();
    let mut a = a;
    let mark = hover_point(&mut a, |h| matches!(h.preview, Some(Target::Abbr(_))));
    rest(&mut a, mark);
    let p = a.ui.card.as_ref().and_then(|c| c.content.clone()).unwrap();
    assert_eq!(p.title, "");
    assert!(
        p.body == "World Health Organization" || p.body == "Hypertext Markup Language",
        "{p:?}"
    );
    // Away from it, no card.
    a.pointer_moved(900.0, 500.0);
    assert!(a.ui.card.is_none());
}

// ---- access keys in menus ------------------------------------------------------------------------

/// The access key of every item of a menu, each item having one, no two the same.
fn assert_distinct_access_keys(items: &[crate::menu::Item<Act>], what: &str) {
    let mut seen: Vec<char> = Vec::new();
    for item in items {
        let key = mdshared_ui::menu::access_key_of(&item.label)
            .unwrap_or_else(|| panic!("{what}: {:?} has no access key", item.label));
        let lower: String = key.to_lowercase().collect();
        let key = lower.chars().next().unwrap();
        assert!(!seen.contains(&key), "{what}: {key} is used twice");
        seen.push(key);
        let plain = mdshared_ui::menu::parse_access(&item.label).text;
        assert!(
            plain.to_lowercase().contains(key),
            "{what}: {:?} does not contain its key",
            item.label
        );
    }
}

#[test]
fn every_menu_with_fixed_labels_has_one_access_key_per_item() {
    use crate::workspace::PageContext;
    let a = app(&["showcase.md", "tables.md"]);
    let main = overlays::main_menu(&a.ws, &a.ui, 0.0, 0.0);
    assert_distinct_access_keys(&main.items, "main menu");
    assert_distinct_access_keys(
        &overlays::tab_menu(&a.ws, 0, false, 0.0, 0.0).items,
        "tab menu",
    );
    assert_distinct_access_keys(
        &overlays::find_menu(true, true, 0.0, 0.0).items,
        "find menu",
    );
    // Every combination of what the page menu offers.
    for link in [None, Some("https://example.com".to_string())] {
        for source in [
            None,
            Some((mdview_render::SourceKind::Formula, "x".to_string())),
            Some((mdview_render::SourceKind::Diagram, "a-->b".to_string())),
        ] {
            let ctx = PageContext {
                link: link.clone(),
                source,
                has_selection: true,
                editable: true,
            };
            let menu = overlays::page_menu(&ctx, false, 0.0, 0.0);
            assert_distinct_access_keys(&menu.items, "page menu");
        }
    }
}

#[test]
fn the_recent_menu_stays_out_of_access_keys_because_its_labels_are_file_names() {
    let mut a = app(&["showcase.md"]);
    a.open_popup(PopupKind::Recent);
    a.compose();
    let p = a.ui.popup.as_ref().expect("the menu");
    assert!(
        p.menu.items.iter().all(|i| !i.label.contains('&')),
        "no marks were added to it"
    );
    // A letter does nothing in it.
    key(&mut a, KeyName::Char('m'), Some("m"));
    assert!(a.ui.popup.is_some());
}

#[test]
fn a_letter_picks_the_marked_item_of_the_open_menu_like_enter() {
    let mut a = app(&["showcase.md", "tables.md"]);
    click(&mut a, Hit::Bar(BarItem::More));
    assert_eq!(a.ui.popup_kind(), Some(PopupKind::Main));
    let tabs = a.ws.tab_count();
    // "Close &all tabs", typed in capitals too.
    key(&mut a, KeyName::Char('a'), Some("A"));
    assert!(a.ui.popup.is_none(), "the menu closed");
    assert_eq!(a.ws.tab_count(), 0, "{tabs} tabs were closed");

    // A letter nothing is marked with changes nothing.
    let mut a = app(&["showcase.md"]);
    click(&mut a, Hit::Bar(BarItem::More));
    key(&mut a, KeyName::Char('j'), Some("j"));
    assert_eq!(a.ui.popup_kind(), Some(PopupKind::Main));

    // With Ctrl held it is a shortcut, not an access key: the menu stays and nothing is closed.
    a.mods = Mods {
        ctrl: true,
        shift: false,
        alt: false,
        control: false,
    };
    key(&mut a, KeyName::Char('a'), Some("a"));
    a.mods = Mods::default();
    assert_eq!(a.ui.popup_kind(), Some(PopupKind::Main));
    assert_eq!(a.ws.tab_count(), 1);
}

#[test]
fn an_access_key_of_a_submenu_item_opens_the_submenu_with_the_item_highlighted() {
    let mut a = app(&["showcase.md"]);
    click(&mut a, Hit::Bar(BarItem::More));
    key(&mut a, KeyName::Char('r'), Some("r"));
    let p = a.ui.popup.as_ref().expect("still open");
    assert!(p.sub.is_some(), "Open recent opened beside the menu");
}

#[test]
fn a_disabled_item_is_not_picked_by_its_letter() {
    let mut a = app(&["showcase.md"]);
    // No folder is open: "Find in folder" is disabled.
    click(&mut a, Hit::Bar(BarItem::More));
    key(&mut a, KeyName::Char('f'), Some("f"));
    assert_eq!(a.ui.popup_kind(), Some(PopupKind::Main));
    assert!(a.ui.palette.is_none());
}

#[test]
fn underlines_follow_how_the_menu_was_opened_and_alt_shows_them() {
    let mut a = app(&["showcase.md"]);
    // By the pointer: hidden.
    click(&mut a, Hit::Bar(BarItem::More));
    assert!(!a.ui.popup.as_ref().unwrap().access);
    a.ui.popup = None;
    // Opened while Alt is held: shown.
    a.mods = Mods {
        ctrl: false,
        shift: false,
        alt: true,
        control: false,
    };
    a.open_popup(PopupKind::Main);
    assert!(a.ui.popup.as_ref().unwrap().access);
    a.mods = Mods::default();
    // A menu from the pointer shows them once Alt is pressed.
    a.ui.popup = None;
    a.open_popup(PopupKind::Main);
    assert!(!a.ui.popup.as_ref().unwrap().access);
    a.ui.popup.as_mut().unwrap().show_access();
    assert!(a.ui.popup.as_ref().unwrap().access);
}

/// In a restored window the vertical scrollbars at the right edge (the page with the Outline pane
/// closed, the Outline pane's list) can be grabbed: the resize band over their track is only the
/// outermost pixels, which the thumb does not cover, and the rest of the right edge, the corners
/// and the other edges still resize.
#[test]
fn the_scrollbar_at_the_right_edge_is_not_covered_by_the_resize_band() {
    use winit::window::ResizeDirection as D;
    let mut a = app(&["everything.md"]);
    let (w, h) = a.logical();
    // The Outline pane at the window's edge.
    let track = rect(&a, Hit::ScrollBar(Scroller::Pane(Side::Outline)));
    assert!((track.right() - w).abs() < 0.5, "the bar is at the edge");
    let y = track.cy();
    for dx in [4.0, 6.0, 7.5, 11.0] {
        assert_eq!(a.resize_band(w - dx, y), None, "outline, {dx} px in");
        assert_eq!(
            a.hit_at(w - dx, y),
            Some(Hit::ScrollBar(Scroller::Pane(Side::Outline)))
        );
    }
    assert_eq!(a.resize_band(w - 1.0, y), Some(D::East));
    assert_eq!(a.resize_band(w - 6.0, h - 4.0), Some(D::SouthEast));
    assert_eq!(a.resize_band(w - 6.0, 2.0), Some(D::NorthEast));
    assert_eq!(a.resize_band(2.0, y), Some(D::West));
    assert_eq!(a.resize_band(w / 2.0, h - 2.0), Some(D::South));
    assert_eq!(a.resize_band(w / 2.0, 2.0), Some(D::North));
    // The page with the Outline pane closed.
    click(&mut a, Hit::Bar(BarItem::Outline));
    let track = rect(&a, Hit::ScrollBar(Scroller::Page));
    assert!((track.right() - w).abs() < 0.5, "the bar is at the edge");
    let y = track.cy();
    for dx in [4.0, 6.0, 7.5, 11.0] {
        assert_eq!(a.resize_band(w - dx, y), None, "page, {dx} px in");
        assert_eq!(a.hit_at(w - dx, y), Some(Hit::ScrollBar(Scroller::Page)));
    }
    assert_eq!(a.resize_band(w - 1.0, y), Some(D::East));
    // With no scrollbar beside the point the old band stands.
    assert_eq!(a.resize_band(w - 6.0, 24.0), Some(D::East));
}

/// Text sent as `KEYEVENTF_UNICODE` arrives as `Key::Unidentified` with the characters only in the
/// event's text: it still gets a key, so it reaches the find box and the palette.
#[test]
fn text_without_a_key_name_still_gets_a_key_so_that_it_is_typed() {
    use super::input::event_key;
    use winit::keyboard::{Key as WKey, NativeKey};
    let none = Mods::default();
    let packet = WKey::Unidentified(NativeKey::Unidentified);
    assert_eq!(
        event_key(&packet, &packet, Some("\u{e9}"), none),
        Some(KeyName::Char('\u{e9}'))
    );
    assert_eq!(
        event_key(&packet, &packet, Some("\u{11f}\u{11f}"), none),
        Some(KeyName::Char('\u{11f}'))
    );
    assert_eq!(event_key(&packet, &packet, Some("\r"), none), None);
    assert_eq!(event_key(&packet, &packet, None, none), None);
    let ctrl = Mods { ctrl: true, ..none };
    assert_eq!(event_key(&packet, &packet, Some("\u{e9}"), ctrl), None);

    // And the text lands in the find box and the palette.
    let mut a = app(&["linked.md"]);
    a.command(CommandId::Find);
    a.compose();
    key(&mut a, KeyName::Char('\u{e9}'), Some("\u{e9}"));
    assert_eq!(a.ui.find.text(), "\u{e9}");
}

#[test]
fn a_composition_shows_in_the_find_box_and_is_not_typed() {
    let mut a = app(&["showcase.md"]);
    click(&mut a, Hit::Find);
    a.caret_reset();
    a.compose();
    let before = a.screen.pixels.clone();
    a.ime_preedit("かな".to_string(), Some((6, 6)));
    a.caret_reset();
    a.compose();
    assert!(a.screen.pixels != before, "the composition is drawn");
    assert_eq!(a.ui.find.text(), "", "a composition is not text yet");
    a.ime_preedit(String::new(), None);
    a.ime_commit("家");
    a.caret_reset();
    a.compose();
    assert_eq!(a.ui.find.text(), "家");
    assert_eq!(a.ui.find.preedit(), None);
}

/// The `closed` requests waiting for the editor, as (file, document) pairs, and that each says
/// who it is from and is a live update, which an older editor does nothing with.
fn told_closed(a: &mut App) -> Vec<(PathBuf, Option<String>)> {
    let closed = a.editor_closed();
    for r in &closed {
        assert!(r.closed && r.from.as_deref() == Some("mdview"));
        let message = r.encode("mdedit");
        let lines = message.lines().skip(1).take_while(|l| !l.is_empty());
        assert!(Request::parse(lines).iter().all(|p| p.follow && p.closed));
    }
    closed.into_iter().map(|r| (r.path, r.doc)).collect()
}

#[test]
fn closing_a_tab_the_editor_asked_for_tells_the_editor_once() {
    let mut a = app(&["tables.md"]);
    let showcase = fixture("showcase.md");
    let math = fixture("math.md");
    // The editor's "Preview in mdview" opens a file; another launch opens one too.
    a.handed_over(vec![Request {
        line: Some(3),
        from: Some("mdedit".into()),
        ..Request::new(&showcase)
    }]);
    a.handed_over(vec![Request {
        line: Some(3),
        ..Request::new(&math)
    }]);
    a.compose();
    assert_eq!(a.ws.tab_count(), 3);
    assert!(told_closed(&mut a).is_empty(), "every tab still shows");
    // Closing tabs the editor did not ask for tells it nothing.
    let index = |a: &App, name: &str| {
        let labels = a.ws.labels();
        labels.iter().position(|l| l.starts_with(name)).unwrap()
    };
    a.ws.close(index(&a, "tables"));
    a.ws.close(index(&a, "math"));
    assert!(told_closed(&mut a).is_empty());
    // Closing the editor's: it is told once, naming the file as it gave it.
    a.ws.close(index(&a, "showcase"));
    assert_eq!(told_closed(&mut a), [(showcase.clone(), None)]);
    assert!(told_closed(&mut a).is_empty());
    // A window the editor's launch started counts its file the same way.
    let mut b = app(&["showcase.md"]);
    b.args.from = Some("mdedit".into());
    b.started_by_editor();
    b.ws.close(0);
    assert_eq!(told_closed(&mut b), [(showcase, None)]);
}

#[test]
fn closing_the_tab_of_the_editors_document_without_a_file_tells_it_by_name() {
    let (mut a, dir, _file) = live_app("closed-doc");
    a.handed_over(vec![shown(DOC, "# Never saved\n", Some(1))]);
    a.compose();
    assert!(told_closed(&mut a).is_empty());
    a.ws.close(a.ws.active().unwrap());
    assert_eq!(told_closed(&mut a), [(PathBuf::new(), Some(DOC.into()))]);
    // Saved before it closes: the tab is the file's, and the editor is told the file.
    a.handed_over(vec![shown(DOC, "# Draft\n", Some(1))]);
    a.compose();
    let saved = dir.join("notes.md");
    std::fs::write(&saved, "# Draft\n").unwrap();
    a.handed_over(vec![Request {
        follow: true,
        doc: Some(DOC.into()),
        from: Some("mdedit".into()),
        ..Request::new(&saved)
    }]);
    a.compose();
    assert!(told_closed(&mut a).is_empty(), "the tab shows the file now");
    a.ws.close(a.ws.active().unwrap());
    assert_eq!(told_closed(&mut a), [(saved, None)]);
    _ = std::fs::remove_dir_all(&dir);
}

/// Audit 4, E1: on every turn of the loop the window looks for the tabs the editor asked for and
/// for the one it was placed for. That asks the disk nothing, however many there are: each path
/// was resolved once, when it arrived. (It was a `canonicalize` per tab per turn: 131 µs a turn
/// with three such tabs.)
#[test]
fn looking_for_the_editors_tabs_on_each_turn_asks_the_disk_nothing() {
    use crate::workspace::{RESOLVES, resolve};
    let mut a = app(&["tables.md"]);
    for f in ["showcase.md", "math.md", "linked.md"] {
        a.handed_over(vec![Request {
            from: Some("mdedit".into()),
            ..Request::new(fixture(f))
        }]);
    }
    a.placed_for = Some((resolve(&fixture("linked.md")), None));
    a.compose();
    let before = RESOLVES.with(|n| n.get());
    for _ in 0..100 {
        assert!(told_closed(&mut a).is_empty());
        assert!(!a.placed_tab_gone());
    }
    assert_eq!(RESOLVES.with(|n| n.get()), before, "no path was resolved");
    // They are still found by their files: closing one tells the editor, by the name it gave.
    let math = a.ws.labels().iter().position(|l| l.starts_with("math"));
    a.ws.close(math.unwrap());
    assert_eq!(told_closed(&mut a), [(fixture("math.md"), None)]);
    let linked = a.ws.labels().iter().position(|l| l.starts_with("linked"));
    a.ws.close(linked.unwrap());
    assert!(a.placed_tab_gone(), "the tab placed for is gone");
}

/// Audit 4, E11: a file the editor asked for that cannot be opened has no tab, so the editor is
/// told at once, as for a tab closed, and its preview ends instead of waiting for one.
#[test]
fn a_file_the_editor_asked_for_that_cannot_be_opened_is_told_closed() {
    let mut a = app(&["tables.md"]);
    let gone = fixture("no-such-file.md");
    a.handed_over(vec![Request {
        from: Some("mdedit".into()),
        ..Request::new(&gone)
    }]);
    assert_eq!(a.ws.tab_count(), 1, "nothing opened");
    assert_eq!(told_closed(&mut a), [(gone, None)]);
    assert!(told_closed(&mut a).is_empty(), "told once");
    // A file another launch asked for, and could not be opened, tells the editor nothing.
    a.handed_over(vec![Request::new(fixture("no-such-either.md"))]);
    assert!(told_closed(&mut a).is_empty());
}

/// The tests that point `MD_SISTER_MDEDIT_SETTINGS` somewhere take this first.
static EDITOR_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn the_editor_is_sent_closed_for_its_tab_when_it_closes() {
    let _one = EDITOR_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("mdview-closed-send-{}", std::process::id()));
    let settings = dir.join("editor").join("settings.ini");
    let rx = mdshared_app::single::listen_requests_waking("mdedit", &settings, || {}).unwrap();
    // SAFETY: as in the test above.
    unsafe { std::env::set_var("MD_SISTER_MDEDIT_SETTINGS", &settings) };
    let mut a = app(&["tables.md"]);
    let showcase = fixture("showcase.md");
    a.handed_over(vec![Request {
        from: Some("mdedit".into()),
        ..Request::new(&showcase)
    }]);
    a.tell_editor_closed();
    let quiet = std::time::Duration::from_millis(300);
    assert!(rx.recv_timeout(quiet).is_err(), "nothing while it shows");
    let shown = a.ws.labels().iter().position(|l| l.starts_with("showcase"));
    a.ws.close(shown.unwrap());
    a.tell_editor_closed();
    let r = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    // The path may arrive spelled another way; the editor makes it canonical as it does any.
    let canonical = mdview_core::files::canonical_or_absolute;
    assert!(r.closed && r.follow && canonical(&r.path) == canonical(&showcase));
    assert_eq!(r.from.as_deref(), Some("mdview"));
    a.tell_editor_closed();
    assert!(rx.recv_timeout(quiet).is_err(), "told once");
    unsafe { std::env::remove_var("MD_SISTER_MDEDIT_SETTINGS") };
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closing_the_window_tells_the_editor_of_every_tab_it_asked_for() {
    let _one = EDITOR_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("mdview-closing-send-{}", std::process::id()));
    let settings = dir.join("editor").join("settings.ini");
    let rx = mdshared_app::single::listen_requests_waking("mdedit", &settings, || {}).unwrap();
    // SAFETY: as in the tests above.
    unsafe { std::env::set_var("MD_SISTER_MDEDIT_SETTINGS", &settings) };
    let mut a = app(&["tables.md"]);
    let showcase = fixture("showcase.md");
    a.handed_over(vec![Request {
        from: Some("mdedit".into()),
        ..Request::new(&showcase)
    }]);
    a.compose();
    // The window closes with the tab still open: the editor is told before the process ends,
    // the window having waited for it.
    a.closing();
    let r = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    let canonical = mdview_core::files::canonical_or_absolute;
    assert!(r.closed && r.follow && canonical(&r.path) == canonical(&showcase));
    // Nothing about tables.md, which it did not ask for, and nothing more however the window
    // ends after.
    a.tell_editor_closed();
    assert!(
        a.tell_editor_all_closed(std::time::Duration::ZERO),
        "nothing left to tell"
    );
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(300))
            .is_err()
    );
    unsafe { std::env::remove_var("MD_SISTER_MDEDIT_SETTINGS") };
    _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_diagram_opens_at_reading_size_and_can_be_panned_zoomed_and_closed() {
    let mut a = app(&["linked.md"]);
    let raster = Rc::new(mdview_render::DecodedImage {
        width: 2400,
        height: 1200,
        baseline: 1200,
        rgba: vec![200; 2400 * 1200 * 4],
    });
    a.show_zoom(raster);
    a.compose();
    let area = a.zoom_area();
    let view = a.ui.zoom.as_ref().expect("the zoom view opened");
    assert_eq!(
        view.zoom, 1.0,
        "reading size is one device px per raster px, twice the page's size"
    );
    assert_eq!(
        view.image_rect(area),
        Rect::new(0.0, 0.0, 2400.0, 1200.0),
        "a diagram wider than the window starts at its left edge"
    );
    // The close button is drawn over the window's top right.
    let close = rect(&a, Hit::ZoomClose);
    assert!(close.x > area.right() - 60.0 && close.y < 60.0);

    // A wheel notch over the middle zooms in; the point under the pointer stays put.
    a.ui.mouse = (area.cx(), area.cy());
    a.wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
    let zoomed = a.ui.zoom.as_ref().unwrap().zoom;
    assert!(zoomed > 1.0);
    assert!(zoomed <= crate::zoom::MAX_ZOOM);

    // A drag moves the diagram with the pointer, by as much as the pointer moved. (The wheel
    // above has already moved the pan off zero on both axes, so the drag is measured from where
    // it was: audit 4, E3, an assertion that held with the drag taken out.)
    a.pointer_moved(area.cx(), area.cy());
    let before = a.ui.zoom.as_ref().unwrap().pan;
    a.left_press();
    a.pointer_moved(area.cx() - 100.0, area.cy() + 30.0);
    a.left_release();
    let view = a.ui.zoom.as_ref().expect("still open after a pan");
    assert_eq!(view.pan, (before.0 - 100.0, before.1 + 30.0));

    // Escape closes it.
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.zoom.is_none());

    // So does its close button.
    let raster = Rc::new(mdview_render::DecodedImage {
        width: 300,
        height: 200,
        baseline: 200,
        rgba: vec![200; 300 * 200 * 4],
    });
    a.show_zoom(raster);
    a.compose();
    click(&mut a, Hit::ZoomClose);
    assert!(a.ui.zoom.is_none());
}

/// Audit 4, E2: the view opens at twice the size the page drew the diagram at every device
/// scale, each raster pixel on one device pixel. (At 150 % it opened at 3x, at 200 % at 4x.)
#[test]
fn the_zoom_view_opens_at_twice_the_pages_size_at_every_device_scale() {
    for scale in [1.0f32, 1.5, 2.0] {
        let mut a = app(&["linked.md"]);
        a.scale = scale;
        a.size = ((1280.0 * scale) as u32, (800.0 * scale) as u32);
        a.compose();
        // The raster the page makes, at twice the device scale, of a diagram it draws 500
        // logical px wide.
        let w = (500.0 * 2.0 * scale) as u32;
        let raster = Rc::new(mdview_render::DecodedImage {
            width: w,
            height: 100,
            baseline: 100,
            rgba: vec![200; w as usize * 100 * 4],
        });
        a.show_zoom(raster);
        a.compose();
        let r = a.ui.zoom.as_ref().unwrap().image_rect(a.zoom_area());
        assert_eq!(r.w, 1000.0, "twice the page's 500 logical px at {scale}");
        assert_eq!(
            r.w * scale,
            w as f32,
            "one device px a raster px at {scale}"
        );
    }
}

/// Audit 4, E8: the view is a picture of the tab it was opened on. When the editor opens another
/// file, the tab's text changes, or the theme does, it closes rather than show a diagram the page
/// no longer has; a frame that changes none of them leaves it open.
#[test]
fn the_zoom_view_closes_when_the_page_under_it_changes() {
    let raster = || {
        Rc::new(mdview_render::DecodedImage {
            width: 300,
            height: 200,
            baseline: 200,
            rgba: vec![200; 300 * 200 * 4],
        })
    };
    // Another file, from the editor.
    let mut a = app(&["linked.md"]);
    a.show_zoom(raster());
    a.compose();
    a.compose();
    assert!(a.ui.zoom.is_some(), "frames alone leave it open");
    a.handed_over(vec![Request {
        from: Some("mdedit".into()),
        ..Request::new(fixture("math.md"))
    }]);
    a.compose();
    assert!(a.ui.zoom.is_none(), "closed over another tab");
    // A new text of the same tab, as the editor sends it while the reader types.
    let (mut b, _dir, file) = live_app("zoom-live");
    b.show_zoom(raster());
    b.compose();
    assert!(b.ui.zoom.is_some());
    b.handed_over(vec![live(
        &file,
        "# Changed

New text.
",
        None,
    )]);
    b.compose();
    assert!(b.ui.zoom.is_none(), "closed over a new text");
    // Another theme.
    let mut c = app(&["linked.md"]);
    c.show_zoom(raster());
    c.compose();
    c.ws.set_theme_choice(ThemeChoice::Light);
    c.compose();
    assert!(c.ui.zoom.is_none(), "closed over another theme");
}

/// Audit 4, E9: a trackpad pinch over the view zooms the diagram, about the pointer, and the
/// smart-zoom tap takes it back to reading size; neither changes the page's zoom under it. With
/// no view, a pinch zooms the page as before.
#[test]
fn a_pinch_over_the_zoom_view_zooms_the_diagram_not_the_page() {
    let mut a = app(&["linked.md"]);
    let page = a.ws.zoom();
    a.show_zoom(Rc::new(mdview_render::DecodedImage {
        width: 600,
        height: 400,
        baseline: 400,
        rgba: vec![200; 600 * 400 * 4],
    }));
    a.compose();
    let area = a.zoom_area();
    a.ui.mouse = (area.cx(), area.cy());
    for _ in 0..5 {
        a.pinch(0.1, false);
    }
    a.pinch(0.0, true);
    let zoomed = a.ui.zoom.as_ref().unwrap().zoom;
    assert!(zoomed > 1.5, "{zoomed}");
    assert_eq!(a.ws.zoom(), page, "the page is as it was");
    a.double_tap();
    assert_eq!(a.ui.zoom.as_ref().unwrap().zoom, 1.0);
    assert_eq!(a.ws.zoom(), page);
    a.close_zoom();
    a.pinch(0.1, false);
    a.pinch(0.0, true);
    assert!(a.ws.zoom() > page, "without the view the page zooms");
}

fn engine_available() -> bool {
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
fn a_diagram_click_opens_the_zoom_view_with_the_pages_own_raster() {
    if !engine_available() {
        eprintln!("skipping: no mdview-engine in target/release");
        return;
    }
    let mut a = app(&["diagrams.md"]);
    let source = a.ws.diagram_source(0).expect("a diagram on the page");
    let raster = a.ws.diagram_raster(&source).expect("the diagram is drawn");
    a.handle_click(Click::Diagram(source));
    a.compose();
    let view = a.ui.zoom.as_ref().expect("the zoom view opened");
    assert_eq!(
        (view.image.width, view.image.height),
        (raster.width, raster.height)
    );
    assert_eq!(view.zoom, 1.0);
}
