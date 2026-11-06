//! Drives the UI with Slint WindowEvents. These are synthetic: they enter Slint's own event
//! routing but do not come from a Wayland seat.
use crate::{App, Doc};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, SharedString};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

fn mv(a: &App, x: f32, y: f32) {
    a.window().dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(x, y) });
}
fn press(a: &App, x: f32, y: f32) {
    mv(a, x, y);
    a.window().dispatch_event(WindowEvent::PointerPressed { position: LogicalPosition::new(x, y), button: PointerEventButton::Left });
}
fn release(a: &App, x: f32, y: f32) {
    a.window().dispatch_event(WindowEvent::PointerReleased { position: LogicalPosition::new(x, y), button: PointerEventButton::Left });
}
fn click(a: &App, x: f32, y: f32) { press(a, x, y); release(a, x, y); }
fn key(a: &App, k: impl Into<SharedString>) {
    let t: SharedString = k.into();
    a.window().dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
    a.window().dispatch_event(WindowEvent::KeyReleased { text: t });
}
fn ctrl(a: &App, s: &str) {
    a.window().dispatch_event(WindowEvent::KeyPressed { text: Key::Control.into() });
    key(a, s);
    a.window().dispatch_event(WindowEvent::KeyReleased { text: Key::Control.into() });
}

type Step = Box<dyn Fn(&App, &Rc<RefCell<Doc>>) -> Option<(&'static str, bool, String)>>;

fn check(name: &'static str, ok: bool, v: String) -> Option<(&'static str, bool, String)> { Some((name, ok, v)) }

fn steps() -> Vec<Step> {
    let mut s: Vec<Step> = vec![];
    macro_rules! act { ($f:expr) => { s.push(Box::new(move |a, d| { let f: fn(&App, &Rc<RefCell<Doc>>) = $f; f(a, d); None })); } }
    macro_rules! chk { ($n:expr, $f:expr) => { s.push(Box::new(move |a, d| { let f: fn(&App, &Rc<RefCell<Doc>>) -> (bool, String) = $f; let (o, v) = f(a, d); check($n, o, v) })); } }
    let w = |a: &App| a.window().size().to_logical(a.window().scale_factor());

    // Find bar.
    act!(|a, _| ctrl(a, "f"));
    chk!("ctrl-f-opens-find", |a, _| (a.get_find_open(), format!("{}", a.get_find_open())));
    act!(|a, _| { key(a, "x"); key(a, "y"); });
    chk!("typing-replaces-selection", |a, _| (a.get_find_text() == "xy", a.get_find_text().to_string()));
    act!(|a, _| { key(a, Key::Backspace); key(a, "ç"); });
    chk!("backspace-and-turkish", |a, _| (a.get_find_text() == "xç", a.get_find_text().to_string()));
    act!(|a, _| key(a, Key::Return));
    chk!("enter-next-match", |a, _| (a.get_find_index() == 3, format!("{}", a.get_find_index())));
    act!(|a, _| key(a, Key::Escape));
    chk!("esc-closes-find", |a, _| (!a.get_find_open(), format!("{}", a.get_find_open())));
    act!(|a, _| ctrl(a, "f"));
    chk!("ctrl-f-after-close", |a, _| (a.get_find_open(), format!("{}", a.get_find_open())));
    act!(|a, _| ctrl(a, "f"));
    chk!("ctrl-f-toggles-from-field", |a, _| (!a.get_find_open(), format!("{}", a.get_find_open())));

    // App menu by mouse: open, hover View (row 3), click "Files sidebar".
    act!(|a, _| click(a, 20.0, 20.0));
    chk!("menu-opens", |a, _| (a.get_menu_open(), format!("{}", a.get_menu_open())));
    act!(|a, _| mv(a, 80.0, 40.0 + 4.0 + 2.0 * 32.0 + 16.0));
    act!(|a, _| mv(a, 300.0, 40.0 + 2.0 * 32.0 + 4.0 + 16.0));
    act!(|a, _| click(a, 300.0, 40.0 + 2.0 * 32.0 + 4.0 + 16.0));
    chk!("menu-view-toggles-sidebar", |a, _| (!a.get_left_open() && !a.get_menu_open(), format!("left={} cmd={}", a.get_left_open(), a.get_last_command())));
    act!(|a, _| a.set_left_open(true));

    // App menu by keyboard: Alt+V, Right, Down, Enter = Outline sidebar.
    act!(|a, _| {
        a.window().dispatch_event(WindowEvent::KeyPressed { text: Key::Alt.into() });
        key(a, "v");
        a.window().dispatch_event(WindowEvent::KeyReleased { text: Key::Alt.into() });
    });
    chk!("alt-v-opens", |a, _| (a.get_menu_open(), format!("{}", a.get_menu_open())));
    act!(|a, _| { key(a, Key::RightArrow); key(a, Key::DownArrow); key(a, Key::Return); });
    chk!("menu-keyboard-activate", |a, _| (!a.get_right_open() && !a.get_menu_open(), format!("right={} cmd={}", a.get_right_open(), a.get_last_command())));
    act!(|a, _| a.set_right_open(true));

    // Dismissal.
    act!(|a, _| click(a, 20.0, 20.0));
    act!(|a, _| key(a, Key::Escape));
    chk!("menu-esc-closes", |a, _| (!a.get_menu_open(), format!("{}", a.get_menu_open())));
    act!(|a, _| click(a, 20.0, 20.0));
    act!(|a, _| click(a, 900.0, 700.0));
    chk!("menu-outside-click-closes", |a, _| (!a.get_menu_open(), format!("{}", a.get_menu_open())));
    act!(|a, _| ctrl(a, "f"));
    chk!("focus-returns-after-menu", |a, _| (a.get_find_open(), format!("{}", a.get_find_open())));
    act!(|a, _| key(a, Key::Escape));

    // Tabs: select the second, close it.
    act!(|a, _| click(a, 170.0, 26.0));
    chk!("tab-select", |a, _| (a.get_current_tab() == 1, format!("{}", a.get_current_tab())));
    act!(|a, _| ctrl(a, "w"));
    chk!("ctrl-w-closes-tab", |a, _| (a.get_tabs().row_count() == 2, format!("{}", a.get_tabs().row_count())));

    // Splitter drag.
    act!(|a, _| { press(a, 229.0, 300.0); mv(a, 259.0, 300.0); });
    chk!("splitter-drag", |a, _| ((a.get_left_width() - 260.0).abs() < 1.5, format!("{}", a.get_left_width())));
    act!(|a, _| { release(a, 259.0, 300.0); a.set_left_width(230.0); });

    // Tree: collapse "fixtures" (row 3 of the visible list).
    act!(|a, _| click(a, 100.0, 40.0 + 36.0 + 2.0 * 28.0 + 14.0));
    chk!("tree-collapse", |a, _| (a.get_tree_rows().row_count() < 8, format!("rows={}", a.get_tree_rows().row_count())));
    act!(|a, _| { key(a, Key::DownArrow); key(a, Key::UpArrow); key(a, Key::RightArrow); });
    chk!("tree-keyboard-expand", |a, _| (a.get_tree_rows().row_count() >= 8, format!("rows={}", a.get_tree_rows().row_count())));
    act!(|a, _| { let _ = a; });
    // Wheel on the document, then wait for the animation.
    s.push(Box::new(move |a, _| {
        a.window().dispatch_event(WindowEvent::PointerScrolled { position: LogicalPosition::new(600.0, 400.0), delta_x: 0.0, delta_y: -240.0 });
        None
    }));
    for _ in 0..12 { act!(|_, _| {}); }
    chk!("wheel-scrolls", |a, _| (a.get_scroll() > 100, format!("{}", a.get_scroll())));

    // Right click opens the context menu; Find... runs from it.
    s.push(Box::new(move |a, _| {
        let p = LogicalPosition::new(600.0, 300.0);
        a.window().dispatch_event(WindowEvent::PointerMoved { position: p });
        a.window().dispatch_event(WindowEvent::PointerPressed { position: p, button: PointerEventButton::Right });
        a.window().dispatch_event(WindowEvent::PointerReleased { position: p, button: PointerEventButton::Right });
        None
    }));
    act!(|a, _| click(a, 600.0 + 40.0, 300.0 + 4.0 + 2.0 * 32.0 + 16.0));
    chk!("context-menu-find", |a, _| (a.get_find_open(), format!("find={} cmd={}", a.get_find_open(), a.get_last_command())));
    let _ = w;
    act!(|a, _| key(a, "q"));
    chk!("find-field-focused-after-context-find", |a, _| (a.get_find_text().ends_with('q'), a.get_find_text().to_string()));
    act!(|a, _| key(a, Key::Escape));
    chk!("esc-after-context-find", |a, _| (!a.get_find_open(), format!("{}", a.get_find_open())));
    // The context menu by keyboard: Down x3 lands on Find..., Return runs it.
    s.push(Box::new(move |a, _| {
        let p = LogicalPosition::new(610.0, 310.0);
        a.window().dispatch_event(WindowEvent::PointerMoved { position: p });
        a.window().dispatch_event(WindowEvent::PointerPressed { position: p, button: PointerEventButton::Right });
        a.window().dispatch_event(WindowEvent::PointerReleased { position: p, button: PointerEventButton::Right });
        None
    }));
    act!(|a, _| { key(a, Key::DownArrow); key(a, Key::DownArrow); key(a, Key::DownArrow); key(a, Key::Return); });
    chk!("context-menu-keyboard", |a, _| (a.get_find_open(), format!("find={} last={}", a.get_find_open(), a.get_last_command())));
    s
}

pub fn start(app: &App, d: Rc<RefCell<Doc>>) {
    let s = steps();
    let i = Rc::new(Cell::new(0usize));
    let fails = Rc::new(Cell::new(0u32));
    let w = app.as_weak();
    let t = Box::leak(Box::new(slint::Timer::default()));
    t.start(slint::TimerMode::Repeated, Duration::from_millis(60), move || {
        let Some(a) = w.upgrade() else { return };
        if a.window().size().width == 0 { return; }
        let n = i.get();
        if n >= s.len() {
            println!("SELFTEST done fails={}", fails.get());
            std::process::exit(if fails.get() == 0 { 0 } else { 1 });
        }
        i.set(n + 1);
        if let Some((name, ok, v)) = s[n](&a, &d) {
            println!("SELFTEST {} {name} value={v}", if ok { "PASS" } else { "FAIL" });
            if !ok { fails.set(fails.get() + 1); }
        }
    });
}
