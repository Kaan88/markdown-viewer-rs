//! The settings dialog's rows, from the options the application has today. The form, its
//! controls and its layout are `mdshared_ui::settings`; each change comes back as an outcome and
//! is applied (and saved, where the option is saved) by the same `Workspace` setter the menus
//! used, so there is no second copy of any setting. It is one page: three small headings, no
//! category list, and it scrolls only in a window too short for it.

use crate::platform;
use crate::updater;
use crate::workspace::{ThemeChoice, Workspace};
use mdshared_ui::corners::{self, Corners};
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
pub const WINDOW_CORNERS: u32 = 12;
pub const BACKDROP: u32 = 13;
pub const PREVIEWS: u32 = 30;
pub const START_ON_PINNED: u32 = 31;
pub const OFFER_STALE: u32 = 32;

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

/// The rows. `corners` is whether the window can be rounded here, so the setting is offered: where
/// the system gives it no way to (X11 without the margin) the row is left out.
pub fn rows(ws: &Workspace, corners: bool) -> Vec<Row> {
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
        Row::text(PAGE_FONT, "Page font", ws.page_font()).placeholder("Default"),
        Row::text(CODE_FONT, "Code font", ws.code_font()).placeholder("Default"),
        // One group for content and tabs, and help lines only on the Windows rows: the form is one
        // page at the default window size only so (Windows adds the backdrop and corners rows).
        Row::header("Content and tabs"),
        Row::toggle(
            REMOTE_IMAGES,
            "Always load remote images",
            ws.remote_always(),
        ),
        Row::toggle(
            TICK_BOXES,
            "Tick task boxes without asking",
            ws.edit_always(),
        ),
        Row::toggle(ANIMATIONS, "Play animated images", ws.animations()),
        Row::toggle(
            PREVIEWS,
            "Preview footnotes and links on hover",
            ws.previews(),
        ),
        Row::toggle(
            OFFER_STALE,
            "Offer to close tabs not opened in a week",
            ws.offers_stale(),
        ),
        Row::toggle(
            START_ON_PINNED,
            "Start on the last pinned tab",
            ws.start_on_pinned(),
        ),
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
    // The backdrop is Windows 11's: the row is offered, under the theme, where it can do something.
    #[cfg(windows)]
    if let Some(at) = rows.iter().position(|r| r.id() == Some(THEME)) {
        use mdshared_app::backdrop::Backdrop;
        rows.insert(
            at + 1,
            Row::choice(
                BACKDROP,
                Backdrop::TITLE,
                &Backdrop::LABELS,
                ws.backdrop().index(),
            )
            .help(Backdrop::HELP),
        );
    }
    if corners {
        // After Theme, in the Page group.
        let at = rows
            .iter()
            .position(|r| matches!(r, Row::Choice { id, .. } if *id == THEME))
            .map_or(rows.len(), |i| i + 1);
        rows.insert(
            at,
            Row::choice(
                WINDOW_CORNERS,
                Corners::SETTING_LABEL,
                &Corners::LABELS,
                Corners::from_setting(ws.corners()).index(),
            )
            .help(corners::setting_help()),
        );
    }
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

pub fn form(ws: &Workspace, corners: bool) -> Form {
    Form::new("Settings", rows(ws, corners)).single_page()
}
