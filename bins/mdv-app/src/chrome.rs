//! The title row (logo, tabs, window buttons) and the command bar under it. Both are drawn from
//! scratch each time their region is dirty, and record what the pointer can hit.

use crate::gfx::{Face, Painter, Rect, Rgb, mix};
use crate::keys;
use crate::textfield::FieldColors;
use crate::theme::Palette;
use crate::ui::{BarItem, Hit, PopupKind, Ui};
use crate::workspace::{ThemeChoice, Workspace};
use mdshared_ui::chrome::{
    BAR_H, DRAG_MIN, NEW_TAB_W, TABS_X, TITLE_H, TabState, WIN_BTN_W, WinButton, button_fill,
    draw_new_tab_button, draw_tab, draw_win_button, tab_natural_width, tab_widths, win_button_rect,
};
pub use mdshared_ui::glyph;
use mdview_core::commands::{CommandId, strings};
use mdview_core::session::PageWidth;

const FIND_W: f32 = 260.0;

/// Shared by every drawing function: the painter, colours, and where hits are recorded.
pub struct Ctx<'a, 'b> {
    pub p: &'a mut Painter<'b>,
    pub pal: &'a Palette,
    pub hits: &'a mut Vec<(Hit, Rect)>,
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
}

impl Ctx<'_, '_> {
    pub fn hit(&mut self, hit: Hit, r: Rect) {
        self.hits.push((hit, r));
    }

    /// The fill behind a button: pressed, hovered, toggled on, or none.
    pub fn fill_for(&self, hit: Hit, on: bool, open: bool) -> Option<Rgb> {
        button_fill(
            self.pal,
            self.hover == Some(hit),
            self.pressed == Some(hit),
            on,
            open,
        )
    }
}

/// The mark's shapes on the icon's 256-unit canvas, as in `assets/make_icon.py`: the Markdown
/// "M" and a down arrow standing a little clear of it.
const MARK_M: [(f32, f32); 12] = [
    (44.0, 176.0),
    (44.0, 80.0),
    (72.0, 80.0),
    (100.0, 116.0),
    (128.0, 80.0),
    (156.0, 80.0),
    (156.0, 176.0),
    (128.0, 176.0),
    (128.0, 122.0),
    (100.0, 158.0),
    (72.0, 122.0),
    (72.0, 176.0),
];
const MARK_ARROW: [(f32, f32); 7] = [
    (188.0, 128.0),
    (164.0, 128.0),
    (202.0, 172.0),
    (240.0, 128.0),
    (216.0, 128.0),
    (216.0, 80.0),
    (188.0, 80.0),
];

/// The app logo, the same drawing as the icon file: a blue tile with a lighter top half, and
/// the white mark scaled and centred on it. `r` is the tile.
pub fn logo(p: &mut Painter, r: Rect) {
    const TILE: Rgb = [0x2b, 0x5f, 0xab];
    // The icon's tile is 240 of its 256 units wide, with corners of 52.
    let radius = r.w * 52.0 / 240.0;
    p.rbox(r, radius, TILE);
    p.rbox_a(
        Rect::new(r.x, r.y, r.w, r.h / 2.0),
        radius,
        [255, 255, 255],
        0.08,
    );
    let scale = 0.96;
    let all = MARK_M.iter().chain(&MARK_ARROW);
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &(x, y) in all {
        (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
    }
    let (dx, dy) = (
        128.0 - (x0 + x1) / 2.0 * scale,
        128.0 - (y0 + y1) / 2.0 * scale,
    );
    let place = |&(x, y): &(f32, f32)| {
        (
            r.x + (x * scale + dx - 8.0) / 240.0 * r.w,
            r.y + (y * scale + dy - 8.0) / 240.0 * r.h,
        )
    };
    let white = [255, 255, 255];
    p.polygon(&MARK_M.iter().map(place).collect::<Vec<_>>(), white);
    p.polygon(&MARK_ARROW.iter().map(place).collect::<Vec<_>>(), white);
}

/// What the command bar shows at a given width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPlan {
    pub labels: bool,
    pub find_w: f32,
    pub zoom: bool,
    pub panes: bool,
    pub recent: bool,
    pub count: bool,
}

/// Label widths that decide the bar's fit, in logical px.
#[derive(Clone, Copy, Debug, Default)]
pub struct BarText {
    pub open: f32,
    pub recent: f32,
    pub files: f32,
    pub outline: f32,
    pub theme: f32,
    /// The match counter, 0 when empty.
    pub count: f32,
}

const ICON_BTN: f32 = 36.0;
const GAP: f32 = 2.0;
const SEP: f32 = 13.0;
const ZOOM_LABEL: f32 = 52.0;
const MARGIN: f32 = 8.0;

fn labelled(label: f32, on: bool) -> f32 {
    if on { 44.0 + label } else { ICON_BTN }
}

/// The width a plan needs.
fn plan_width(plan: &BarPlan, t: &BarText) -> f32 {
    let l = plan.labels;
    let mut w = 2.0 * MARGIN;
    w += labelled(t.open, l) + GAP;
    if plan.recent {
        w += ICON_BTN + GAP + labelled(t.recent, l) + GAP;
    }
    if plan.panes {
        w += SEP + labelled(t.files, l) + GAP + labelled(t.outline, l) + GAP;
    }
    if plan.zoom {
        // Zoom out, the percentage, zoom in, page width.
        w += SEP + ICON_BTN + GAP + ZOOM_LABEL + GAP + 2.0 * (ICON_BTN + GAP);
    }
    // Right group: more, theme, separator, next, previous, find box, counter.
    w += ICON_BTN + GAP + labelled(t.theme, l) + SEP + 2.0 * (ICON_BTN + GAP) + plan.find_w;
    if plan.count && t.count > 0.0 {
        w += t.count + 12.0;
    }
    // Keep a little air between the groups.
    w + 16.0
}

/// Drops labels first, then narrows the find box, then hides zoom, the pane toggles and
/// Reload/Recent (all of which stay in the main menu and on the keyboard), so nothing overlaps.
pub fn bar_plan(avail: f32, t: &BarText) -> BarPlan {
    let full = BarPlan {
        labels: true,
        find_w: FIND_W,
        zoom: true,
        panes: true,
        recent: true,
        count: true,
    };
    let steps = [
        full,
        BarPlan {
            labels: false,
            ..full
        },
        BarPlan {
            labels: false,
            find_w: 200.0,
            ..full
        },
        BarPlan {
            labels: false,
            find_w: 200.0,
            zoom: false,
            ..full
        },
        BarPlan {
            labels: false,
            find_w: 180.0,
            zoom: false,
            panes: false,
            ..full
        },
        BarPlan {
            labels: false,
            find_w: 160.0,
            zoom: false,
            panes: false,
            recent: false,
            ..full
        },
    ];
    for plan in steps {
        if plan_width(&plan, t) <= avail {
            return plan;
        }
    }
    let mut last = BarPlan {
        labels: false,
        find_w: 0.0,
        zoom: false,
        panes: false,
        recent: false,
        count: false,
    };
    last.find_w = (avail - plan_width(&last, t)).clamp(96.0, 160.0);
    last
}

fn theme_glyph(choice: ThemeChoice) -> &'static str {
    match choice {
        ThemeChoice::System => glyph::SYSTEM,
        ThemeChoice::Light => glyph::SUN,
        ThemeChoice::Dark => glyph::MOON,
    }
}

pub fn theme_label(choice: ThemeChoice) -> &'static str {
    match choice {
        ThemeChoice::System => "System",
        ThemeChoice::Light => "Light",
        ThemeChoice::Dark => "Dark",
    }
}

/// The tooltip for a chrome hit, if it has one.
pub fn tooltip(hit: Hit, ws: &Workspace, maximized: bool) -> Option<String> {
    let with = |text: &str, id: CommandId| {
        let k = keys::hint(id);
        if k.is_empty() {
            text.to_string()
        } else {
            format!("{text} ({k})")
        }
    };
    Some(match hit {
        Hit::Tab(i) => ws.tab(i)?.path().display().to_string(),
        Hit::TabClose(_) => with("Close tab", CommandId::CloseTab),
        Hit::NewTab => with("Open files", CommandId::OpenFiles),
        Hit::LeaveFocus => "Leave focus mode (Escape)".into(),
        Hit::WinMin => "Minimize".into(),
        Hit::WinMax if maximized => "Restore down".into(),
        Hit::WinMax => "Maximize".into(),
        Hit::WinClose => "Close".into(),
        Hit::Bar(b) => match b {
            BarItem::Open => with("Open files", CommandId::OpenFiles),
            BarItem::Reload => with("Reload", CommandId::Refresh),
            BarItem::Recent => "Recent files".into(),
            BarItem::Files => with("Files pane", CommandId::ToggleFiles),
            BarItem::Outline => with("Outline pane", CommandId::ToggleOutline),
            BarItem::ZoomOut => with("Zoom out", CommandId::ZoomOut),
            BarItem::Zoom => with("Reset zoom", CommandId::ZoomReset),
            BarItem::ZoomIn => with("Zoom in", CommandId::ZoomIn),
            BarItem::PageWidth => {
                format!("Page width: {} (click to change)", ws.page_width().name())
            }
            BarItem::FindPrev => with("Previous match", CommandId::FindPrevious),
            BarItem::FindNext => with("Next match", CommandId::FindNext),
            BarItem::Theme => format!(
                "Theme: {} (click to change)",
                theme_label(ws.theme_choice())
            ),
            BarItem::More => "More".into(),
        },
        Hit::FindClear => "Clear search (Escape)".into(),
        Hit::FilesFolder => with("Open folder", CommandId::OpenFolder),
        _ => return None,
    })
}

// ---- title row -------------------------------------------------------------------------------

/// Draws the title row into `r` (the full window width, `TITLE_H` tall).
pub fn draw_title(c: &mut Ctx, ws: &Workspace, ui: &Ui, r: Rect, maximized: bool) {
    let pal = *c.pal;
    c.p.fill(r, pal.title);
    c.hit(Hit::TitleDrag, r);
    logo(c.p, Rect::new(14.0, 9.0, 22.0, 22.0));

    // Window buttons, right to left.
    let mut right = r.right();
    let buttons = [
        (Hit::WinClose, WinButton::Close),
        (Hit::WinMax, WinButton::Max),
        (Hit::WinMin, WinButton::Min),
    ];
    for (hit, kind) in buttons {
        let b = win_button_rect(right, 0);
        right -= WIN_BTN_W;
        draw_win_button(
            c.p,
            &pal,
            kind,
            b,
            maximized,
            c.hover == Some(hit),
            c.pressed == Some(hit),
        );
        c.hit(hit, b);
    }
    if ui.focus_mode {
        right -= WIN_BTN_W;
        let b = Rect::new(right, 0.0, WIN_BTN_W, TITLE_H);
        if let Some(fill) = c.fill_for(Hit::LeaveFocus, false, false) {
            c.p.fill(b, fill);
        }
        c.p.icon(glyph::BACK_TO_WINDOW, 13.0, b.cx(), b.cy(), pal.text);
        c.hit(Hit::LeaveFocus, b);
    }

    // Tabs.
    let labels = ws.labels();
    let avail = (right - DRAG_MIN - NEW_TAB_W - TABS_X).max(0.0);
    let natural: Vec<f32> = labels.iter().map(|l| tab_natural_width(c.p, l)).collect();
    let widths = tab_widths(&natural, avail);
    let active = ws.active();
    let mut x = TABS_X;
    // Tabs that still do not fit at the floor width are cut off rather than drawn over the buttons.
    let clip = c.p.clip(Rect::new(0.0, 0.0, TABS_X + avail + 8.0, TITLE_H));
    let is_hot = |hover: Option<Hit>, i: usize| matches!(hover, Some(Hit::Tab(j) | Hit::TabClose(j)) if j == i);
    for (i, label) in labels.iter().enumerate() {
        let w = widths[i];
        let state = TabState {
            active: active == Some(i),
            hovered: is_hot(c.hover, i),
            dragging: matches!(
                ui.drag,
                Some(crate::ui::Drag::Tab { index, moving: true, .. }) if index == i
            ),
            missing: ws.tab(i).is_some_and(|t| t.is_missing()),
            next_quiet: i + 1 < labels.len() && active != Some(i + 1) && !is_hot(c.hover, i + 1),
            close_hovered: c.hover == Some(Hit::TabClose(i)),
            close_pressed: c.pressed == Some(Hit::TabClose(i)),
        };
        let drawn = draw_tab(c.p, &pal, x, w, r.w, label, &state);
        c.hit(Hit::Tab(i), drawn.tab);
        if let Some(b) = drawn.close {
            c.hit(Hit::TabClose(i), b);
        }
        x += w;
    }
    c.p.restore(clip);
    let b = draw_new_tab_button(
        c.p,
        &pal,
        x,
        avail,
        c.hover == Some(Hit::NewTab),
        c.pressed == Some(Hit::NewTab),
    );
    c.hit(Hit::NewTab, b);
}

// ---- command bar -----------------------------------------------------------------------------

/// The measured text widths the bar's fit depends on.
pub fn bar_text(p: &mut Painter, ws: &Workspace) -> BarText {
    let count = ws.find_label();
    BarText {
        open: p.measure(Face::Ui, 13.0, "Open"),
        recent: p.measure(Face::Ui, 13.0, "Recent"),
        files: p.measure(Face::Ui, 13.0, "Files"),
        outline: p.measure(Face::Ui, 13.0, "Outline"),
        theme: p.measure(Face::Ui, 13.0, theme_label(ws.theme_choice())),
        count: if count.is_empty() {
            0.0
        } else {
            p.measure(Face::Ui, 12.5, &count)
        },
    }
}

/// Where the find box's text goes inside its frame, and the clear button if the field has text.
pub fn find_parts(frame: Rect, has_text: bool) -> (Rect, Option<Rect>) {
    let clear =
        has_text.then(|| Rect::new(frame.right() - 30.0, frame.y + 4.0, 24.0, frame.h - 8.0));
    let right = if has_text {
        frame.right() - 32.0
    } else {
        frame.right() - 10.0
    };
    (
        Rect::new(
            frame.x + 32.0,
            frame.y + 1.0,
            (right - frame.x - 32.0).max(0.0),
            frame.h - 2.0,
        ),
        clear,
    )
}

/// Draws the command bar into `r`.
pub fn draw_bar(c: &mut Ctx, ws: &Workspace, ui: &mut Ui, r: Rect) {
    let pal = *c.pal;
    c.p.fill(r, pal.bar);
    let t = bar_text(c.p, ws);
    let plan = bar_plan(r.w, &t);
    let y = r.y + (BAR_H - 32.0) / 2.0;
    let cy = y + 16.0;
    let open = ui.popup_kind();

    let has_tab = ws.active().is_some();
    let button =
        |c: &mut Ctx, x: f32, item: BarItem, g: &str, label: Option<&str>, on: bool| -> f32 {
            // Reload and match stepping need a document; without one they show greyed and do nothing.
            let enabled = has_tab
                || !matches!(
                    item,
                    BarItem::Reload | BarItem::FindPrev | BarItem::FindNext
                );
            let w = match label {
                Some(l) => 44.0 + c.p.measure(Face::Ui, 13.0, l),
                None => ICON_BTN,
            };
            let b = Rect::new(x, y, w, 32.0);
            let hit = Hit::Bar(item);
            let menu_open = matches!(
                (item, open),
                (BarItem::Recent, Some(PopupKind::Recent)) | (BarItem::More, Some(PopupKind::Main))
            );
            if let Some(fill) = c.fill_for(hit, on, menu_open).filter(|_| enabled) {
                c.p.rbox(b, 6.0, fill);
            }
            let gx = if label.is_some() { x + 18.0 } else { b.cx() };
            let color = if !enabled {
                pal.faint
            } else if on {
                pal.accent
            } else {
                pal.text
            };
            c.p.icon(g, 15.0, gx, cy, color);
            if let Some(l) = label {
                c.p.text(Face::Ui, 13.0, x + 34.0, cy, l, pal.text, f32::INFINITY);
            }
            if enabled {
                c.hit(hit, b);
            }
            w
        };
    let sep = |c: &mut Ctx, x: f32| {
        c.p.fill(
            Rect::new((x + SEP / 2.0).round(), y + 6.0, c.p.px(), 20.0),
            pal.line,
        );
        SEP
    };
    let lab = |s: &'static str| plan.labels.then_some(s);

    // Left group.
    let mut x = r.x + MARGIN;
    x += button(c, x, BarItem::Open, glyph::OPEN, lab("Open"), false) + GAP;
    if plan.recent {
        x += button(c, x, BarItem::Reload, glyph::RELOAD, None, false) + GAP;
        x += button(c, x, BarItem::Recent, glyph::RECENT, lab("Recent"), false) + GAP;
    }
    if plan.panes {
        x += sep(c, x);
        x += button(
            c,
            x,
            BarItem::Files,
            glyph::FOLDER,
            lab("Files"),
            ui.panes.files && !ui.focus_mode,
        ) + GAP;
        x += button(
            c,
            x,
            BarItem::Outline,
            glyph::OUTLINE,
            lab("Outline"),
            ui.panes.outline && !ui.focus_mode,
        ) + GAP;
    }
    if plan.zoom {
        x += sep(c, x);
        x += button(c, x, BarItem::ZoomOut, glyph::MINUS, None, false) + GAP;
        let b = Rect::new(x, y, ZOOM_LABEL, 32.0);
        let hit = Hit::Bar(BarItem::Zoom);
        if let Some(fill) = c.fill_for(hit, false, false) {
            c.p.rbox(b, 6.0, fill);
        }
        c.p.text_center(Face::Ui, 13.0, b, &format!("{}%", ws.zoom()), pal.text);
        c.hit(hit, b);
        x += ZOOM_LABEL + GAP;
        x += button(c, x, BarItem::ZoomIn, glyph::PLUS, None, false) + GAP;
        page_width_button(c, ws, Rect::new(x, y, ICON_BTN, 32.0));
    }

    // Right group, from the right edge.
    let mut rx = r.right() - MARGIN - ICON_BTN;
    button(c, rx, BarItem::More, glyph::MORE, None, false);
    let choice = ws.theme_choice();
    let tw = labelled(t.theme, plan.labels);
    rx -= GAP + tw;
    button(
        c,
        rx,
        BarItem::Theme,
        theme_glyph(choice),
        lab(theme_label(choice)),
        false,
    );
    rx -= SEP;
    sep(c, rx);
    rx -= ICON_BTN + GAP;
    button(c, rx, BarItem::FindNext, glyph::DOWN, None, false);
    rx -= ICON_BTN + GAP;
    button(c, rx, BarItem::FindPrev, glyph::UP, None, false);
    rx -= plan.find_w + 4.0;
    let frame = Rect::new(rx, y, plan.find_w, 32.0);
    draw_find(c, ui, frame);
    if plan.count && t.count > 0.0 {
        let label = ws.find_label();
        let none = ws.find_counts().total == 0;
        c.p.text_right(
            Face::Ui,
            12.5,
            frame.x - 10.0,
            cy,
            &label,
            if none { pal.warn } else { pal.muted },
        );
    }
}

/// The page-width button: a small window outline holding a column as wide as the choice, so
/// the icon shows the current setting. A click steps to the next one.
fn page_width_button(c: &mut Ctx, ws: &Workspace, b: Rect) {
    let pal = *c.pal;
    let hit = Hit::Bar(BarItem::PageWidth);
    let fill = c.fill_for(hit, false, false);
    if let Some(fill) = fill {
        c.p.rbox(b, 6.0, fill);
    }
    let window = Rect::new(b.cx() - 9.0, b.cy() - 7.0, 18.0, 14.0);
    c.p.frame(window, 3.0, fill.unwrap_or(pal.bar), pal.text);
    let column = match ws.page_width() {
        PageWidth::Narrow => 4.0,
        PageWidth::Wide => 8.0,
        PageWidth::Full => 12.0,
    };
    c.p.rbox(
        Rect::new(
            b.cx() - column / 2.0,
            window.y + 3.0,
            column,
            window.h - 6.0,
        ),
        1.0,
        pal.text,
    );
    c.hit(hit, b);
}

fn draw_find(c: &mut Ctx, ui: &mut Ui, frame: Rect) {
    let pal = *c.pal;
    let focused = ui.find_focused;
    let hovered = c.hover == Some(Hit::Find) || c.hover == Some(Hit::FindClear);
    let edge = if focused {
        pal.line
    } else if hovered {
        mix(pal.line, pal.text, 0.18)
    } else {
        pal.line
    };
    let fill = if focused || !hovered {
        pal.field
    } else {
        mix(pal.field, pal.text, 0.03)
    };
    c.p.frame(frame, 6.0, fill, edge);
    if focused {
        // The Windows 11 text box: an accent underline that follows the rounded corners.
        let old =
            c.p.clip(Rect::new(frame.x, frame.bottom() - 2.0, frame.w, 2.0));
        c.p.rbox(frame, 6.0, pal.accent);
        c.p.restore(old);
    }
    c.p.icon(
        glyph::SEARCH,
        12.0,
        frame.x + 17.0,
        frame.cy(),
        if focused { pal.text } else { pal.muted },
    );
    let (area, clear) = find_parts(frame, !ui.find.is_empty());
    let colors = FieldColors {
        text: pal.text,
        placeholder: pal.faint,
        selection: pal.selection,
        caret: pal.text,
    };
    ui.find.draw(
        c.p,
        area,
        Face::Ui,
        13.0,
        &colors,
        strings::FIND_PLACEHOLDER,
        focused,
        ui.caret_on,
    );
    c.hit(Hit::Find, frame);
    if let Some(b) = clear {
        if let Some(f) = c.fill_for(Hit::FindClear, false, false) {
            c.p.rbox(b, 4.0, f);
        }
        c.p.icon(glyph::CLOSE, 8.5, b.cx(), b.cy(), pal.muted);
        c.hit(Hit::FindClear, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text() -> BarText {
        BarText {
            open: 32.0,
            recent: 42.0,
            files: 28.0,
            outline: 46.0,
            theme: 44.0,
            count: 50.0,
        }
    }

    #[test]
    fn bar_degrades_in_order() {
        let t = text();
        let wide = bar_plan(1400.0, &t);
        assert!(wide.labels && wide.zoom && wide.panes && wide.recent && wide.find_w == FIND_W);
        let mut last_width = f32::INFINITY;
        let mut seen_no_labels = false;
        for avail in (300..1400).rev().step_by(10) {
            let plan = bar_plan(avail as f32, &t);
            if !plan.labels {
                seen_no_labels = true;
            }
            // Items hide only after labels are gone.
            if !plan.zoom || !plan.panes || !plan.recent {
                assert!(seen_no_labels && !plan.labels);
            }
            if plan_width(&plan, &t) <= avail as f32 {
                assert!(plan.find_w <= last_width || plan.find_w == FIND_W);
            }
            last_width = last_width.min(plan.find_w);
        }
        let narrow = bar_plan(520.0, &t);
        assert!(!narrow.zoom && !narrow.panes);
        assert!(plan_width(&narrow, &t) <= 520.0);
    }

    #[test]
    fn bar_fits_at_the_window_minimum() {
        let plan = bar_plan(520.0, &text());
        assert!(plan_width(&plan, &text()) <= 520.0 + 1.0);
        let tiny = bar_plan(300.0, &text());
        assert!(tiny.find_w >= 96.0);
    }
}
