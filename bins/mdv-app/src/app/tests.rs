//! Headless interaction tests: a real `App` without a window, composed at 1280x800 and driven
//! through the same pointer and key handlers the event loop calls.

use super::*;
use mdview_core::commands::CommandId;
use mdview_core::session::PageWidth;

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

// ---- the command palette and the settings dialog -------------------------------------------------

const CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
    alt: false,
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
    let state = crate::palette::PaletteState::open(&a.ws, ">");
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
    // Focus mode hides the panes, and the rows say so.
    a.apply_setting(s::FOCUS_MODE, Value::Bool(true));
    assert!(a.ui.focus_mode);
    let form = a.ui.settings.as_ref().unwrap();
    assert_eq!(form.value(s::FOCUS_MODE), Some(Value::Bool(true)));
    assert_eq!(form.value(s::FILES_PANE), Some(Value::Bool(false)));
    // Turning a pane on leaves focus mode.
    a.apply_setting(s::FILES_PANE, Value::Bool(true));
    assert!(!a.ui.focus_mode && a.ui.panes.files);
    let form = a.ui.settings.as_ref().unwrap();
    assert_eq!(form.value(s::FOCUS_MODE), Some(Value::Bool(false)));
    a.apply_setting(s::OUTLINE_PANE, Value::Bool(false));
    assert!(!a.ui.panes.outline);
    key(&mut a, KeyName::Escape, None);
    assert!(a.ui.settings.is_none());
}

#[test]
fn the_settings_form_follows_the_apps_limits() {
    use crate::settings as s;
    use mdshared_ui::settings::Row;
    let a = app(&["showcase.md"]);
    let rows = s::rows(&a.ws, &a.ui);
    let zoom = rows
        .iter()
        .find_map(|r| match r {
            Row::Number {
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
    let labels: Vec<_> = p.menu.items.iter().map(|i| i.label.as_str()).collect();
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
