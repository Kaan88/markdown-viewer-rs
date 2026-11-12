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
