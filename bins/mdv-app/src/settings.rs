//! The settings dialog's pages, from the options the application has today. The form, its
//! controls and its layout are `mdshared_ui::settings::pages` (shared with mdedit): a list of
//! pages at the left, one page at a time at the right, nothing that scrolls. Each change comes
//! back as an outcome and is applied (and saved, where the option is saved) by the same
//! `Workspace` setter the menus used, so there is no second copy of any setting. The pages are
//! made again after every change, so a switch turned off disables the row under it.

use crate::platform;
use crate::updater;
use crate::workspace::{ThemeChoice, Workspace};
use mdshared_ui::corners::{self, Corners};
use mdshared_ui::settings::Row;
use mdshared_ui::settings::pages::{Form, Item, Page};
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
pub const SKIP_BRIEF: u32 = 33;
pub const BRIEF_SECS: u32 = 34;

pub const WIDTHS: [&str; 3] = ["Narrow", "Wide", "Full"];
pub const THEMES: [&str; 3] = ["System", "Light", "Dark"];

// The pages, in the list's order. Appearance, Tabs and Updates are mdedit's names, in mdedit's
// order; Reading is mdview's own.
pub const APPEARANCE: &str = "Appearance";
pub const READING: &str = "Reading";
pub const TABS: &str = "Tabs";
pub const UPDATES: &str = "Updates";
pub const PAGES: [&str; 4] = [APPEARANCE, READING, TABS, UPDATES];

/// The reason a row under a switch gives while the switch is off (mdedit's words).
pub const NEEDS_SWITCH: &str = "Needs the switch above.";

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

/// The page a name refers to (`--settings-section`): a page's name in any case, with hyphens for
/// spaces or not, or the group a setting was in before the pages (`Page`, `Content`, `System`).
pub fn page_for_section(name: &str) -> Option<&'static str> {
    let key = name.trim().to_ascii_lowercase().replace('-', " ");
    if let Some(p) = PAGES.iter().find(|p| p.to_ascii_lowercase() == key) {
        return Some(p);
    }
    Some(match key.as_str() {
        "page" | "view" => APPEARANCE,
        "content" | "content and tabs" => READING,
        "session" => TABS,
        "system" => UPDATES,
        _ => return None,
    })
}

/// The pages. `corners` is whether the window can be rounded here, so the setting is offered:
/// where the system gives it no way to (X11 without the margin) the row is left out.
/// The seconds' label under "Don't reopen tabs barely looked at". Shorter than the shared
/// `brief::SECONDS` ("Looked at for less than"), which needs 145 px of the row's 139 with Linux's
/// fonts at the smallest window.
const BRIEF_SECS_LABEL: &str = "Shown for less than";

/// `offer_default` adds "Make mdview the default" to Updates (an AppImage that is not yet).
pub fn pages(ws: &Workspace, corners: bool, offer_default: bool) -> Vec<Page> {
    let mut appearance = vec![
        Item::new(Row::choice(
            PAGE_WIDTH,
            "Page width",
            &WIDTHS,
            width_index(ws.page_width()),
        )),
        Item::new(
            Row::number(
                TEXT_SIZE,
                "Text size",
                ws.zoom() as i32,
                ZOOM_MIN as i32,
                ZOOM_MAX as i32,
            )
            .step(ZOOM_STEP as i32)
            .unit("%"),
        ),
        Item::new(Row::text(PAGE_FONT, "Page font", ws.page_font()).placeholder("Default")),
        Item::new(Row::text(CODE_FONT, "Code font", ws.code_font()).placeholder("Default")),
        // The window rows in mdedit's order: Theme, Window backdrop, Window corners.
        Item::new(Row::choice(
            THEME,
            "Theme",
            &THEMES,
            theme_index(ws.theme_choice()),
        )),
    ];
    // The backdrop is Windows 11's: the row is offered where it can do something.
    #[cfg(windows)]
    {
        use mdshared_app::backdrop::Backdrop;
        appearance.push(Item::new(
            Row::choice(
                BACKDROP,
                Backdrop::TITLE,
                &Backdrop::LABELS,
                ws.backdrop().index(),
            )
            .help(Backdrop::HELP),
        ));
    }
    if corners {
        appearance.push(Item::new(
            Row::choice(
                WINDOW_CORNERS,
                Corners::SETTING_LABEL,
                &Corners::LABELS,
                Corners::from_setting(ws.corners()).index(),
            )
            .help(corners::setting_help()),
        ));
    }

    let reading = vec![
        Item::new(Row::toggle(
            REMOTE_IMAGES,
            "Always load remote images",
            ws.remote_always(),
        )),
        Item::new(Row::toggle(
            TICK_BOXES,
            "Tick task boxes without asking",
            ws.edit_always(),
        )),
        Item::new(Row::toggle(
            ANIMATIONS,
            "Play animated images",
            ws.animations(),
        )),
        Item::new(Row::toggle(
            PREVIEWS,
            "Preview footnotes and links on hover",
            ws.previews(),
        )),
    ];

    // The seconds are a child of their switch, disabled with the reason while it is off. The
    // help lines are the design's shorter wording, one line each at the smallest window.
    let (skip, secs) = ws.skips_brief();
    let seconds = Item::new(
        Row::number(
            BRIEF_SECS,
            BRIEF_SECS_LABEL,
            secs as i32,
            mdshared_app::brief::SECS_MIN as i32,
            mdshared_app::brief::SECS_MAX as i32,
        )
        .unit("sec")
        .help("Counted only while in front."),
    )
    .child();
    let tabs = vec![
        Item::new(Row::toggle(
            START_ON_PINNED,
            "Start on the last pinned tab",
            ws.start_on_pinned(),
        )),
        Item::new(Row::toggle(
            OFFER_STALE,
            "Offer to close tabs not opened in a week",
            ws.offers_stale(),
        )),
        Item::new(
            Row::toggle(SKIP_BRIEF, mdshared_app::brief::SWITCH, skip)
                .help("Pinned tabs and the tab you left on always come back."),
        ),
        if skip {
            seconds
        } else {
            seconds.disabled(NEEDS_SWITCH)
        },
    ];

    // The About dialog offers both unconditionally (a copy that cannot update itself says so
    // when asked), so this does too.
    let mut updates = vec![
        Item::new(Row::toggle(
            CHECK_AT_LAUNCH,
            "Check for updates at launch",
            ws.checks_updates(),
        )),
        Item::new(Row::action(
            CHECK_NOW,
            "Look for a newer version",
            check_now_label(ws),
        )),
    ];
    if offer_default {
        // The main menu's old label does not fit beside the button at the smallest window.
        updates.push(Item::new(
            Row::action(DEFAULT_APP, "Make mdview the default", "Make default")
                .help("For Markdown files."),
        ));
    }

    vec![
        Page::new(APPEARANCE, appearance),
        Page::new(READING, reading),
        Page::new(TABS, tabs),
        Page::new(UPDATES, updates),
    ]
}

/// Every setting and action row of `pages`, in order: what a test reads.
#[cfg(test)]
pub fn rows_of(pages: &[Page]) -> Vec<Row> {
    pages
        .iter()
        .flat_map(|p| p.items.iter())
        .map(|it| it.row().clone())
        .filter(|r| r.id().is_some())
        .collect()
}

/// The button's text: "Check now", "Update" when there is something to install (the About box's
/// "Update to 1.4.0" in fewer words, beside a label that says the rest: the button is 96 px), or
/// what the About box says while it installs.
fn check_now_label(ws: &Workspace) -> String {
    match ws.update_stage() {
        updater::Stage::Available(_) => "Update".to_string(),
        updater::Stage::Installing(_) => ws.update_label(),
        _ => "Check now".to_string(),
    }
}

/// The pages as this copy offers them.
pub fn pages_here(ws: &Workspace, corners: bool) -> Vec<Page> {
    pages(ws, corners, platform::can_offer_default_app())
}

/// The dialog on page `page` (the one last shown this run), the keyboard in the list.
pub fn form(ws: &Workspace, corners: bool, page: usize) -> Form {
    let mut f = Form::new("Settings", pages_here(ws, corners));
    f.show_page(page);
    f
}
