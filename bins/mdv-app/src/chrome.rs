//! The title row (logo, tabs, window buttons) and the command bar under it. Both are drawn from
//! scratch each time their region is dirty, and record what the pointer can hit.

use crate::gfx::{Face, Painter, Rect, Rgb, mix};
use crate::keys;
use crate::textfield::FieldColors;
use crate::theme::Palette;
use crate::ui::{BAR_H, BarItem, Hit, PopupKind, TITLE_H, Ui};
use crate::workspace::{ThemeChoice, Workspace};
use mdview_core::commands::{CommandId, strings};

pub const WIN_BTN_W: f32 = 46.0;
pub const TAB_MIN: f32 = 90.0;
pub const TAB_MAX: f32 = 220.0;
/// Tabs squeeze below `TAB_MIN` only when nothing else fits; under this they would be unusable.
pub const TAB_FLOOR: f32 = 36.0;
/// Where the tabs start, after the logo.
pub const TABS_X: f32 = 50.0;
/// The new-tab button after the last tab, and the drag area kept free beside it.
const NEW_TAB_W: f32 = 34.0;
const DRAG_MIN: f32 = 56.0;
const FIND_W: f32 = 260.0;

/// Segoe Fluent Icons code points used across the chrome.
pub mod glyph {
    pub const DOC: &str = "\u{E8A5}";
    pub const FOLDER: &str = "\u{E8B7}";
    pub const FOLDER_OPEN: &str = "\u{E838}";
    pub const OPEN: &str = "\u{E8E5}";
    pub const RELOAD: &str = "\u{E72C}";
    pub const RECENT: &str = "\u{E81C}";
    pub const OUTLINE: &str = "\u{E8FD}";
    pub const MINUS: &str = "\u{E738}";
    pub const PLUS: &str = "\u{E710}";
    pub const UP: &str = "\u{E70E}";
    pub const DOWN: &str = "\u{E70D}";
    pub const RIGHT: &str = "\u{E76C}";
    pub const SEARCH: &str = "\u{E721}";
    pub const CLOSE: &str = "\u{E711}";
    pub const MORE: &str = "\u{E712}";
    pub const SUN: &str = "\u{E706}";
    pub const MOON: &str = "\u{E708}";
    pub const SYSTEM: &str = "\u{E7F4}";
    pub const WARN: &str = "\u{E7BA}";
    pub const IMAGE: &str = "\u{E8B9}";
    pub const COPY: &str = "\u{E8C8}";
    pub const LINK: &str = "\u{E71B}";
    pub const SELECT_ALL: &str = "\u{E8B3}";
    pub const FULLSCREEN: &str = "\u{E740}";
    pub const BACK_TO_WINDOW: &str = "\u{E73F}";
    pub const KEYBOARD: &str = "\u{E765}";
    pub const INFO: &str = "\u{E946}";
    pub const POWER: &str = "\u{E7E8}";
    pub const ZOOM: &str = "\u{E71E}";
    pub const DELETE: &str = "\u{E74D}";
    pub const OPEN_WITH: &str = "\u{E8A7}";
    pub const WIN_MIN: &str = "\u{E921}";
    pub const WIN_MAX: &str = "\u{E922}";
    pub const WIN_RESTORE: &str = "\u{E923}";
    pub const WIN_CLOSE: &str = "\u{E8BB}";
}

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
        if open || (self.pressed == Some(hit) && self.hover == Some(hit)) {
            Some(self.pal.press)
        } else if self.hover == Some(hit) {
            Some(if on {
                mix(self.pal.on, self.pal.text, 0.06)
            } else {
                self.pal.hover
            })
        } else if on {
            Some(self.pal.on)
        } else {
            None
        }
    }
}

/// The app logo: a rounded accent square with a white "M" and a small down arrow, the Markdown mark.
pub fn logo(p: &mut Painter, r: Rect, pal: &Palette) {
    let top = mix(
        pal.accent,
        [255, 255, 255],
        if pal.dark { 0.10 } else { 0.22 },
    );
    let bottom = mix(pal.accent, [0, 0, 0], if pal.dark { 0.25 } else { 0.10 });
    p.rbox_gradient(r, r.w * 0.26, top, bottom);
    let s = r.w / 22.0;
    let white = [255, 255, 255];
    let t = 1.9 * s;
    // "M" on the left two thirds, an arrow on the right third.
    let (x0, x1, y0, y1) = (r.x + 4.2 * s, r.x + 12.6 * s, r.y + 7.0 * s, r.y + 15.2 * s);
    let xm = (x0 + x1) / 2.0;
    p.line((x0, y1), (x0, y0), t, white);
    p.line((x0, y0), (xm, y0 + 4.6 * s), t, white);
    p.line((xm, y0 + 4.6 * s), (x1, y0), t, white);
    p.line((x1, y0), (x1, y1), t, white);
    let ax = r.x + 16.8 * s;
    p.line((ax, y0), (ax, y1 - 0.4 * s), t, white);
    p.line((ax - 2.8 * s, y1 - 3.0 * s), (ax, y1), t, white);
    p.line((ax + 2.8 * s, y1 - 3.0 * s), (ax, y1), t, white);
}

/// Widths for tabs that would like `natural` px each, fitting in `avail`: each between
/// `TAB_MIN` and `TAB_MAX`; when they do not fit, the widest are capped first (so short labels
/// keep their size), then all shrink evenly down to `TAB_FLOOR`.
pub fn tab_widths(natural: &[f32], avail: f32) -> Vec<f32> {
    let wants: Vec<f32> = natural
        .iter()
        .map(|w| w.clamp(TAB_MIN, TAB_MAX).round())
        .collect();
    let sum: f32 = wants.iter().sum();
    if sum <= avail || wants.is_empty() {
        return wants;
    }
    let fits = |cap: f32| wants.iter().map(|w| w.min(cap)).sum::<f32>() <= avail;
    let (mut lo, mut hi) = (TAB_FLOOR, TAB_MAX);
    if !fits(lo) {
        return vec![TAB_FLOOR; wants.len()];
    }
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let cap = lo.floor();
    wants.iter().map(|w| w.min(cap)).collect()
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
        w += SEP + ICON_BTN + GAP + ZOOM_LABEL + GAP + ICON_BTN + GAP;
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
    logo(c.p, Rect::new(14.0, 9.0, 22.0, 22.0), &pal);

    // Window buttons, right to left.
    let mut right = r.right();
    let buttons = [
        (Hit::WinClose, glyph::WIN_CLOSE),
        (
            Hit::WinMax,
            if maximized {
                glyph::WIN_RESTORE
            } else {
                glyph::WIN_MAX
            },
        ),
        (Hit::WinMin, glyph::WIN_MIN),
    ];
    for (hit, g) in buttons {
        right -= WIN_BTN_W;
        let b = Rect::new(right, 0.0, WIN_BTN_W, TITLE_H);
        let mut color = pal.text;
        let (hovered, pressed) = (
            c.hover == Some(hit),
            c.pressed == Some(hit) && c.hover == Some(hit),
        );
        if hit == Hit::WinClose && hovered {
            c.p.fill(
                b,
                if pressed {
                    mix(pal.danger, [0, 0, 0], 0.15)
                } else {
                    pal.danger
                },
            );
            color = [255, 255, 255];
        } else if hovered {
            c.p.fill(b, if pressed { pal.press } else { pal.hover });
        }
        c.p.icon(g, 10.0, b.cx(), b.cy(), color);
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
    let natural: Vec<f32> = labels
        .iter()
        .map(|l| c.p.measure(Face::Semibold, 12.5, l) + 74.0)
        .collect();
    let widths = tab_widths(&natural, avail);
    let active = ws.active();
    let mut x = TABS_X;
    // Tabs that still do not fit at the floor width are cut off rather than drawn over the buttons.
    let clip = c.p.clip(Rect::new(0.0, 0.0, TABS_X + avail + 8.0, TITLE_H));
    for (i, label) in labels.iter().enumerate() {
        let w = widths[i];
        let tab = Rect::new(x, 6.0, w, TITLE_H - 6.0);
        let is_active = active == Some(i);
        let hovered = matches!(c.hover, Some(Hit::Tab(j) | Hit::TabClose(j)) if j == i);
        let dragging =
            matches!(ui.drag, Some(crate::ui::Drag::Tab { index, moving: true, .. }) if index == i);
        if is_active {
            // The active tab joins the bar: square bottom, outward curves where it meets it.
            let old = c.p.clip(Rect::new(0.0, 0.0, r.w, TITLE_H));
            c.p.rbox(Rect::new(x, 6.0, w, TITLE_H), 8.0, pal.bar);
            for (cx, sq) in [(x - 8.0, x - 8.0), (x + w + 8.0, x + w)] {
                let corner = Rect::new(sq, TITLE_H - 8.0, 8.0, 8.0);
                let o2 = c.p.clip(corner);
                c.p.fill(corner, pal.bar);
                c.p.rbox(
                    Rect::new(cx - 8.0, TITLE_H - 16.0, 16.0, 16.0),
                    8.0,
                    pal.title,
                );
                c.p.restore(o2);
            }
            c.p.restore(old);
        } else if hovered || dragging {
            c.p.rbox(Rect::new(x + 2.0, 9.0, w - 4.0, 28.0), 6.0, pal.hover);
        } else {
            // A hairline between two quiet tabs.
            let next_quiet = i + 1 < labels.len()
                && active != Some(i + 1)
                && !matches!(c.hover, Some(Hit::Tab(j) | Hit::TabClose(j)) if j == i + 1);
            if next_quiet {
                c.p.fill(Rect::new(x + w - c.p.px(), 15.0, c.p.px(), 16.0), pal.line);
            }
        }
        let cy = 23.0;
        let missing = ws.tab(i).is_some_and(|t| t.is_missing());
        let show_close = (is_active || hovered) && w >= 64.0;
        let tiny = w < 64.0;
        let icon_x = if tiny { x + w / 2.0 } else { x + 19.0 };
        let (g, gc) = if missing {
            (glyph::WARN, pal.warn)
        } else if is_active {
            (glyph::DOC, pal.accent)
        } else {
            (glyph::DOC, pal.muted)
        };
        c.p.icon(g, 13.0, icon_x, cy, gc);
        if !tiny {
            let tx = x + 34.0;
            let max = w - 34.0 - if show_close { 30.0 } else { 12.0 };
            let (face, color) = if is_active {
                (Face::Semibold, pal.text)
            } else if hovered {
                (Face::Ui, pal.text)
            } else {
                (Face::Ui, pal.muted)
            };
            let color = if missing { pal.faint } else { color };
            let drawn = c.p.text(face, 12.5, tx, cy, label, color, max);
            if missing {
                c.p.fill(Rect::new(tx, cy + 0.5, drawn, c.p.px().max(1.0)), pal.faint);
            }
        }
        c.hit(Hit::Tab(i), tab);
        if show_close {
            let b = Rect::new(x + w - 29.0, cy - 10.0, 20.0, 20.0);
            if let Some(fill) = c.fill_for(Hit::TabClose(i), false, false) {
                c.p.rbox(b, 4.0, fill);
            }
            c.p.icon(
                glyph::CLOSE,
                8.5,
                b.cx(),
                b.cy(),
                if c.hover == Some(Hit::TabClose(i)) {
                    pal.text
                } else {
                    pal.muted
                },
            );
            c.hit(Hit::TabClose(i), b);
        }
        x += w;
    }
    c.p.restore(clip);
    let b = Rect::new((x + 4.0).min(TABS_X + avail + 4.0), 8.0, 30.0, 30.0);
    if let Some(fill) = c.fill_for(Hit::NewTab, false, false) {
        c.p.rbox(b, 6.0, fill);
    }
    c.p.icon(glyph::PLUS, 12.0, b.cx(), b.cy() - 0.5, pal.text);
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
        button(c, x, BarItem::ZoomIn, glyph::PLUS, None, false);
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

    #[test]
    fn tabs_keep_natural_width_when_room() {
        assert_eq!(tab_widths(&[100.0, 150.0], 1000.0), vec![100.0, 150.0]);
        assert_eq!(tab_widths(&[40.0, 400.0], 1000.0), vec![TAB_MIN, TAB_MAX]);
        assert!(tab_widths(&[], 10.0).is_empty());
    }

    #[test]
    fn crowded_tabs_cap_the_widest_first() {
        let w = tab_widths(&[100.0, 220.0, 220.0], 440.0);
        assert_eq!(w[0], 100.0);
        assert!(w[1] == w[2] && w[1] <= 170.0 && w[1] >= 160.0);
        assert!(w.iter().sum::<f32>() <= 440.0);
    }

    #[test]
    fn many_tabs_squeeze_to_the_floor() {
        let w = tab_widths(&[200.0; 8], 500.0);
        assert!(w.iter().all(|&x| (TAB_FLOOR..TAB_MIN).contains(&x)));
        assert!(w.iter().sum::<f32>() <= 500.0);
        assert_eq!(tab_widths(&[200.0; 20], 100.0), vec![TAB_FLOOR; 20]);
    }

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
