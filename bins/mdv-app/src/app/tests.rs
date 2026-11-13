//! Headless interaction tests: a real `App` without a window, composed at 1280x800 and driven
//! through the same pointer and key handlers the event loop calls.

use super::*;
use mdview_core::commands::CommandId;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

fn app(files: &[&str]) -> App {
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

fn type_text(app: &mut App, text: &str) {
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
    // Moving to another item of the parent closes the submenu.
    let (x, y) = menu_row(&a, 0);
    a.pointer_moved(x, y);
    assert!(a.ui.popup.as_ref().is_some_and(|p| p.sub.is_none()));
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
