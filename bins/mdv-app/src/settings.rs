//! The settings dialog's rows, from the options the application has today. The form, its
//! controls and its layout are `mdshared_ui::settings`; each change comes back as an outcome and
//! is applied (and saved, where the option is saved) by the same `Workspace` setter the menus
//! used, so there is no second copy of any setting.

use crate::platform;
use crate::updater;
use crate::workspace::{ThemeChoice, Workspace};
use mdshared_ui::settings::{Form, Row};
use mdview_core::session::{PageWidth, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP};

pub const PAGE_WIDTH: u32 = 1;
pub const TEXT_SIZE: u32 = 2;
pub const THEME: u32 = 3;
pub const REMOTE_IMAGES: u32 = 4;
pub const TICK_BOXES: u32 = 5;
pub const ANIMATIONS: u32 = 6;
pub const CHECK_AT_LAUNCH: u32 = 7;
pub const CHECK_NOW: u32 = 8;
pub const DEFAULT_APP: u32 = 9;
pub const PAGE_FONT: u32 = 10;
pub const CODE_FONT: u32 = 11;

pub const WIDTHS: [&str; 3] = ["Narrow", "Wide", "Full"];
pub const THEMES: [&str; 3] = ["System", "Light", "Dark"];

pub fn width_index(w: PageWidth) -> usize {
    PageWidth::ALL.iter().position(|&x| x == w).unwrap_or(0)
}

pub fn theme_index(c: ThemeChoice) -> usize {
    match c {
        ThemeChoice::System => 0,
        ThemeChoice::Light => 1,
        ThemeChoice::Dark => 2,
    }
}

pub fn theme_of(i: usize) -> ThemeChoice {
    match i {
        1 => ThemeChoice::Light,
        2 => ThemeChoice::Dark,
        _ => ThemeChoice::System,
    }
}

pub fn rows(ws: &Workspace) -> Vec<Row> {
    let mut rows = vec![
        Row::header("Page"),
        Row::choice(
            PAGE_WIDTH,
            "Page width",
            &WIDTHS,
            width_index(ws.page_width()),
        ),
        Row::number(
            TEXT_SIZE,
            "Text size",
            ws.zoom() as i32,
            ZOOM_MIN as i32,
            ZOOM_MAX as i32,
        )
        .step(ZOOM_STEP as i32)
        .unit("%"),
        Row::choice(THEME, "Theme", &THEMES, theme_index(ws.theme_choice())),
        Row::text(PAGE_FONT, "Page font", ws.page_font())
            .placeholder("Default")
            .help("Any installed family. Unknown ones are ignored."),
        Row::text(CODE_FONT, "Code font", ws.code_font()).placeholder("Default"),
        Row::header("Content"),
        Row::toggle(
            REMOTE_IMAGES,
            "Always load remote images",
            ws.remote_always(),
        )
        .help("Otherwise each document asks first."),
        Row::toggle(
            TICK_BOXES,
            "Tick task boxes without asking",
            ws.edit_always(),
        )
        .help("Otherwise the first tick in a file asks."),
        Row::toggle(ANIMATIONS, "Play animated images", ws.animations())
            .help("Off shows their first frame."),
        // The About dialog offers both unconditionally (a copy that cannot update itself says so
        // when asked), so this does too.
        Row::header("Updates"),
        Row::toggle(
            CHECK_AT_LAUNCH,
            "Check for updates at launch",
            ws.checks_updates(),
        ),
        Row::action(CHECK_NOW, "Look for a newer version", check_now_label(ws)),
    ];
    if platform::can_offer_default_app() {
        rows.push(Row::header("System"));
        rows.push(Row::action(
            DEFAULT_APP,
            "Make mdview the default for Markdown files",
            "Make default",
        ));
    }
    rows
}

/// "Check now", or what the About dialog's button would say when there is something to install.
fn check_now_label(ws: &Workspace) -> String {
    match ws.update_stage() {
        updater::Stage::Available(_) | updater::Stage::Installing(_) => ws.update_label(),
        _ => "Check now".to_string(),
    }
}

pub fn form(ws: &Workspace) -> Form {
    Form::new("Settings", rows(ws))
}
