//! The toolkit's own screens on their pointer routes (toolkit `84c0fc8`): the command palette's
//! query box and the list of slides' box take the whole of a text field's mouse (the run of
//! clicks, the drag past an end, the edit menu), and a press the window lost acts on none of
//! them, nor on Settings.

use super::tests::{app, type_text};
use super::*;
use mdview::commands::CommandId as C;

fn key(a: &mut App, name: KeyName) {
    a.key_named(name, None);
    a.compose();
}

fn shift_f10(a: &mut App) {
    a.mods.shift = true;
    key(a, KeyName::F(10));
    a.mods.shift = false;
}

fn click_at(a: &mut App, x: f32, y: f32) {
    a.pointer_moved(x, y);
    a.left_press();
    a.left_release();
}

/// The palette open, with `query` typed in its box, laid out.
fn palette_with(query: &str) -> App {
    let mut a = app(&["linked.md", "math.md"]);
    a.command(C::CommandPalette);
    a.compose();
    type_text(&mut a, query);
    a
}

fn query(a: &App) -> String {
    a.ui.palette
        .as_ref()
        .expect("the palette is open")
        .palette
        .query()
        .to_string()
}

fn field(a: &App) -> Rect {
    a.ui.palette
        .as_ref()
        .expect("the palette is open")
        .palette
        .field_rect()
}

fn palette_menu_open(a: &App) -> bool {
    a.ui.palette
        .as_ref()
        .is_some_and(|p| p.palette.edit_menu_open())
}

/// A triple click in the query box takes all of it, so what is typed next replaces the whole
/// query; on the old route the third click took a word, as the second does.
#[test]
fn a_triple_click_in_the_palettes_box_takes_all_of_it() {
    let mut a = palette_with("alpha beta");
    let f = field(&a);
    for _ in 0..3 {
        click_at(&mut a, f.cx(), f.cy());
    }
    type_text(&mut a, "x");
    assert_eq!(query(&a), "x");
}

/// A right press on the query box opens its edit menu, Escape closes the menu and leaves the
/// palette open, and Shift+F10 opens it from the keys.
#[test]
fn the_palettes_box_has_an_edit_menu_by_the_right_press_and_shift_f10() {
    let mut a = palette_with("alpha");
    let f = field(&a);
    a.pointer_moved(f.cx(), f.cy());
    a.right_press();
    assert!(palette_menu_open(&a), "the right press opens the menu");
    // While it is open everything is under the palette: the menu can reach past the card.
    a.compose();
    assert!(
        a.overlay_bounds
            .iter()
            .any(|b| b.w >= a.size.0 as f32 / a.scale - 1.0),
        "{:?}",
        a.overlay_bounds
    );
    key(&mut a, KeyName::Escape);
    assert!(!palette_menu_open(&a), "Escape closes the menu");
    assert!(a.ui.palette.is_some(), "and only the menu");
    shift_f10(&mut a);
    assert!(palette_menu_open(&a), "Shift+F10 opens it at the caret");
}

/// A selection dragged past the query box's start goes on with the text each time the loop's
/// timer comes, until all of it is selected. (Typing leaves the box scrolled to its end.)
#[test]
fn a_drag_past_the_palettes_box_start_scrolls_on_at_rest() {
    let long = "word ".repeat(60);
    let long = long.trim_end();
    let mut a = palette_with(long);
    let f = field(&a);
    a.pointer_moved(f.cx(), f.cy());
    a.left_press();
    a.pointer_moved(f.x - 30.0, f.cy());
    let first = a
        .palette_text_deadline()
        .expect("the scroll waits for its timer");
    let before = first - Duration::from_millis(1);
    assert!(
        a.timers(before).is_some_and(|t| t <= first),
        "the loop is woken for it"
    );
    let mut now = before;
    let mut steps = 0;
    while let Some(due) = a.palette_text_deadline() {
        now = now.max(due);
        a.timers(now);
        steps += 1;
        assert!(steps < 4000, "the scroll ends");
    }
    assert!(steps > 0, "the timer scrolled the text");
    a.left_release();
    type_text(&mut a, "x");
    let q = query(&a);
    assert!(
        q.starts_with('x') && q.len() < long.len() / 2,
        "the start was selected: {q:?}"
    );
}

/// A press on a row, then the window loses the focus (no release comes on Windows): the
/// release after coming back chooses nothing.
#[test]
fn a_palette_row_pressed_when_the_focus_goes_is_not_chosen() {
    let mut a = app(&["linked.md", "math.md"]);
    a.command(C::CommandPalette);
    a.compose();
    let active = a.ws.active();
    let r = a.ui.palette.as_ref().unwrap().palette.row_rect(0);
    a.pointer_moved(r.cx(), r.cy());
    a.left_press();
    a.end_held();
    a.left_release();
    assert!(a.ui.palette.is_some(), "nothing was chosen");
    assert_eq!(a.ws.active(), active);
    // Without the lost focus the same click chooses the row.
    click_at(&mut a, r.cx(), r.cy());
    assert!(a.ui.palette.is_none(), "the click itself chooses");
}

/// A category of Settings pressed when the window loses the focus is not opened by the
/// release after it.
#[test]
fn a_settings_category_pressed_when_the_focus_goes_is_not_opened() {
    let mut a = app(&["linked.md"]);
    a.open_settings();
    a.compose();
    let r = a.ui.settings.as_ref().unwrap().list_item_rect(1);
    a.pointer_moved(r.cx(), r.cy());
    a.left_press();
    a.end_held();
    a.left_release();
    assert_eq!(a.ui.settings.as_ref().unwrap().page(), 0);
    a.ui.last_press = None;
    click_at(&mut a, r.cx(), r.cy());
    assert_eq!(
        a.ui.settings.as_ref().unwrap().page(),
        1,
        "the click itself works"
    );
}

/// The list of slides open over a presentation.
fn overview() -> App {
    let mut a = app(&["slides.md"]);
    a.command(C::Present);
    a.compose();
    key(&mut a, KeyName::Char('g'));
    assert!(a.ui.present.as_ref().unwrap().overview_open());
    a
}

/// The list of slides' box has the edit menu by the right press, and a row pressed when the
/// focus goes is not chosen.
#[test]
fn the_list_of_slides_has_the_edit_menu_and_drops_a_lost_press() {
    let mut a = overview();
    // The rows and the box, found through the pointer: the box is above the rows.
    let row = {
        let (show, gfx) = (a.ui.present.as_mut().unwrap(), &mut a.gfx);
        (100..700)
            .step_by(4)
            .find_map(|y| {
                show.overview_move(640.0, y as f32, gfx);
                (show.overview_selected() == Some(2)).then_some(y as f32)
            })
            .expect("a row for slide 3")
    };
    a.pointer_moved(640.0, row);
    a.left_press();
    a.end_held();
    a.left_release();
    let show = a.ui.present.as_ref().unwrap();
    assert!(show.overview_open(), "the lost press chose nothing");
    // Shift+F10 opens the box's menu, Escape closes it and leaves the list open.
    shift_f10(&mut a);
    assert!(a.ui.present.as_ref().unwrap().overview_menu_open());
    key(&mut a, KeyName::Escape);
    let show = a.ui.present.as_ref().unwrap();
    assert!(!show.overview_menu_open() && show.overview_open());
    // A right press on the box opens it too.
    let f = show.overview_field().expect("the box");
    a.pointer_moved(f.cx(), f.cy());
    a.right_press();
    assert!(a.ui.present.as_ref().unwrap().overview_menu_open());
}
