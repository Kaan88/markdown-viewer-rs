//! The page region: the workspace's frame blitted in, find marks and the selection blended over it,
//! the overlay scrollbar, the missing-file banner, and the empty state when no tab is open.

use crate::chrome::{Ctx, glyph, logo};
use crate::gfx::{Face, Rect, mix};
use crate::panes::{draw_scrollbar, scrollbar};
use crate::ui::{BANNER_H, Drag, Hit, Scroller, Ui};
use crate::updater::Stage;
use crate::workspace::Workspace;
use mdview_core::commands::strings;
use std::time::{Duration, Instant};

/// The page viewport inside the region: below the banner when the file is missing, and below
/// the bar that offers to load remote images when the document holds some back.
pub fn viewport(ws: &Workspace, r: Rect) -> Rect {
    let mut top = 0.0;
    if ws.active_tab().is_some_and(|t| t.is_missing()) {
        top += BANNER_H;
    }
    if ws.remote_blocked() > 0 {
        top += BANNER_H;
    }
    let r = below_update_bar(ws, r);
    Rect::new(r.x, r.y + top, r.w, (r.h - top).max(0.0))
}

/// The region without the update bar, which sits at its top over a document or the empty state.
fn below_update_bar(ws: &Workspace, r: Rect) -> Rect {
    if ws.update_bar().is_some() {
        Rect::new(r.x, r.y + BANNER_H, r.w, (r.h - BANNER_H).max(0.0))
    } else {
        r
    }
}

/// The Segoe "Download" glyph.
const GLYPH_DOWNLOAD: &str = "\u{E896}";

/// The bar that offers a newer version, shows its download, and says when it is in place.
fn draw_update_bar(c: &mut Ctx, bar: Rect, stage: &Stage) {
    let pal = *c.pal;
    c.p.fill(bar, pal.bar);
    c.p.fill(
        Rect::new(bar.x, bar.bottom() - c.p.px(), bar.w, c.p.px()),
        pal.line,
    );
    let mut right = bar.right() - 10.0;
    if !matches!(stage, Stage::Installing(_)) {
        let close = Rect::new(bar.right() - 34.0, bar.cy() - 12.0, 24.0, 24.0);
        if c.hover == Some(Hit::UpdateClose) {
            c.p.rbox(close, 5.0, mix(pal.bar, pal.text, 0.10));
        }
        c.p.icon(glyph::CLOSE, 10.0, close.cx(), close.cy(), pal.muted);
        c.hit(Hit::UpdateClose, close);
        right = close.x - 10.0;
    }
    let label = match stage {
        Stage::None => return,
        Stage::Available(offer) => {
            let go = strings::UPDATE_GO;
            let w = (c.p.measure(Face::Ui, 12.5, go) + 22.0).round();
            right -= w;
            bar_button(c, Hit::UpdateGo, right, bar.cy(), go, true);
            right -= 12.0;
            let text = strings::UPDATE_AVAILABLE.replace("{}", &offer.version.to_string());
            match &offer.notes {
                Some(notes) => format!("{text} {notes}"),
                None => text,
            }
        }
        Stage::Installing(offer) => {
            strings::UPDATE_INSTALLING.replace("{}", &offer.version.to_string())
        }
        Stage::Installed(version) => strings::UPDATE_INSTALLED.replace("{}", &version.to_string()),
    };
    c.p.icon(GLYPH_DOWNLOAD, 13.0, bar.x + 22.0, bar.cy(), pal.muted);
    c.p.text(
        Face::Ui,
        12.5,
        bar.x + 38.0,
        bar.cy(),
        &label,
        pal.text,
        (right - (bar.x + 38.0)).max(0.0),
    );
}

/// A text button on a bar: quiet until hovered, or filled with the accent when `primary`.
fn bar_button(c: &mut Ctx, hit: Hit, x: f32, cy: f32, label: &str, primary: bool) -> f32 {
    let pal = *c.pal;
    let w = (c.p.measure(Face::Ui, 12.5, label) + 22.0).round();
    let b = Rect::new(x, cy - 12.0, w, 24.0);
    let (hover, pressed) = (c.hover == Some(hit), c.pressed == Some(hit));
    let (fill, text) = if primary {
        let fill = match (hover, pressed) {
            (true, true) => mix(pal.accent, pal.page, 0.2),
            (true, false) => mix(pal.accent, pal.page, 0.1),
            _ => pal.accent,
        };
        (fill, pal.on_accent)
    } else {
        let lift = match (hover, pressed) {
            (true, true) => 0.16,
            (true, false) => 0.10,
            _ => 0.05,
        };
        (mix(pal.bar, pal.text, lift), pal.text)
    };
    c.p.rbox(b, 5.0, fill);
    c.p.text_center(Face::Ui, 12.5, b, label, text);
    c.hit(hit, b);
    w
}

/// The bar over a document whose remote images are held back.
fn draw_remote_bar(c: &mut Ctx, bar: Rect, count: usize) {
    let pal = *c.pal;
    c.p.fill(bar, pal.bar);
    c.p.fill(
        Rect::new(bar.x, bar.bottom() - c.p.px(), bar.w, c.p.px()),
        pal.line,
    );
    // Right to left: close, then the two choices; the text takes what is left.
    let close = Rect::new(bar.right() - 34.0, bar.cy() - 12.0, 24.0, 24.0);
    if c.hover == Some(Hit::RemoteClose) {
        c.p.rbox(close, 5.0, mix(pal.bar, pal.text, 0.10));
    }
    c.p.icon(glyph::CLOSE, 10.0, close.cx(), close.cy(), pal.muted);
    c.hit(Hit::RemoteClose, close);
    let always = strings::REMOTE_ALWAYS;
    let load = strings::REMOTE_LOAD;
    let wa = (c.p.measure(Face::Ui, 12.5, always) + 22.0).round();
    let wl = (c.p.measure(Face::Ui, 12.5, load) + 22.0).round();
    let xa = close.x - 10.0 - wa;
    let xl = xa - 8.0 - wl;
    bar_button(c, Hit::RemoteAlways, xa, bar.cy(), always, false);
    bar_button(c, Hit::RemoteLoad, xl, bar.cy(), load, true);
    let label = if count == 1 {
        strings::REMOTE_ONE.to_string()
    } else {
        strings::REMOTE_MANY.replace("{}", &count.to_string())
    };
    c.p.icon(glyph::IMAGE, 13.0, bar.x + 22.0, bar.cy(), pal.muted);
    c.p.text(
        Face::Ui,
        12.5,
        bar.x + 38.0,
        bar.cy(),
        &label,
        pal.text,
        (xl - 12.0 - (bar.x + 38.0)).max(0.0),
    );
}

/// How many Recent entries the empty state lists.
const EMPTY_RECENTS: usize = 6;

/// Draws the page region. Returns the time the workspace spent laying out and painting.
pub fn draw_page(c: &mut Ctx, ws: &mut Workspace, ui: &Ui, r: Rect) -> Duration {
    let pal = *c.pal;
    let whole = r;
    let r = below_update_bar(ws, whole);
    if let Some(stage) = ws.update_bar() {
        draw_update_bar(c, Rect::new(whole.x, whole.y, whole.w, BANNER_H), stage);
    }
    if ws.active().is_none() {
        c.p.fill(r, pal.page);
        draw_empty(c, ws, r);
        return Duration::ZERO;
    }
    let vr = viewport(ws, whole);
    let blocked = ws.remote_blocked();
    if blocked > 0 {
        draw_remote_bar(c, Rect::new(r.x, vr.y - BANNER_H, r.w, BANNER_H), blocked);
    }
    if ws.active_tab().is_some_and(|t| t.is_missing()) {
        let banner = Rect::new(r.x, r.y, r.w, BANNER_H);
        c.p.fill(banner, pal.warn_bg);
        c.p.fill(
            Rect::new(r.x, banner.bottom() - c.p.px(), r.w, c.p.px()),
            mix(pal.warn_bg, pal.warn, 0.3),
        );
        c.p.icon(glyph::WARN, 13.0, r.x + 22.0, banner.cy(), pal.warn);
        c.p.text(
            Face::Ui,
            12.5,
            r.x + 38.0,
            banner.cy(),
            strings::FILE_DELETED,
            pal.text,
            r.w - 54.0,
        );
    }
    let scale = c.p.scale;
    let (x0, y0, x1, y1) = c.p.dev_rect(vr);
    let (w, h) = ((x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32);
    let started = Instant::now();
    let view = ws.paint(w, h, scale);
    let spent = started.elapsed();
    let painted = view.map(|view| {
        let bmp = &view.frame.bitmap;
        let bx = x0 + view.x;
        c.p.blit(bmp, 0, bmp.height as i32, bx, y0);
        let bg = view.background;
        // Whatever the frame does not cover (rounding at the edges).
        if bx > x0 {
            c.p.fill_dev(x0, y0, bx, y1, bg, 1.0);
        }
        if bx + (bmp.width as i32) < x1 {
            c.p.fill_dev(bx + bmp.width as i32, y0, x1, y1, bg, 1.0);
        }
        if y0 + (bmp.height as i32) < y1 {
            c.p.fill_dev(x0, y0 + bmp.height as i32, x1, y1, bg, 1.0);
        }
        (view.scroll, view.total_height)
    });
    let Some((scroll, total)) = painted else {
        c.p.fill(vr, pal.page);
        return spent;
    };

    // Find marks and the selection, blended over the page; the frame bitmap itself stays clean.
    let marks = ws.find_marks();
    let (others_a, current_a) = if pal.dark { (0.38, 0.62) } else { (0.45, 0.62) };
    for m in &marks.others {
        mark(c, x0, y0, m, pal.find_mark, others_a);
    }
    for m in &marks.current {
        mark(c, x0, y0, m, pal.find_current, current_a);
    }
    for m in ws.selection_rects() {
        mark(
            c,
            x0,
            y0,
            &m,
            pal.selection,
            if pal.dark { 0.42 } else { 0.30 },
        );
    }
    c.hit(Hit::Page, vr);
    if let Some((track, thumb)) = scrollbar(vr, total, h as f32, scroll) {
        let hit = Hit::ScrollBar(Scroller::Page);
        let active = c.hover == Some(hit)
            || matches!(
                ui.drag,
                Some(Drag::Thumb {
                    target: Scroller::Page,
                    ..
                })
            );
        draw_scrollbar(c.p, &pal, track, thumb, active);
        c.hit(hit, track);
    }
    spent
}

fn mark(
    c: &mut Ctx,
    x0: i32,
    y0: i32,
    m: &mdview_render::Rect,
    color: crate::gfx::Rgb,
    alpha: f32,
) {
    let (a, b) = (x0 + m.x.round() as i32, y0 + m.y.round() as i32);
    let (e, f) = (
        x0 + (m.x + m.w).round() as i32,
        y0 + (m.y + m.h).round() as i32,
    );
    c.p.fill_dev(a, b, e, f, color, alpha);
}

/// The empty state: the logo, a hint, a primary button and the recent files.
fn draw_empty(c: &mut Ctx, ws: &Workspace, r: Rect) {
    let pal = *c.pal;
    let recents: Vec<&std::path::PathBuf> = ws.recents().iter().take(EMPTY_RECENTS).collect();
    let col_w = (r.w - 48.0).clamp(200.0, 460.0);
    let list_h = if recents.is_empty() {
        0.0
    } else {
        44.0 + recents.len() as f32 * 40.0
    };
    let block_h = 64.0 + 24.0 + 30.0 + 26.0 + 28.0 + 36.0 + list_h;
    let mut y = (r.y + (r.h - block_h) * 0.42).max(r.y + 24.0).round();
    let cx = r.cx();
    logo(c.p, Rect::new((cx - 32.0).round(), y, 64.0, 64.0));
    y += 64.0 + 24.0;
    c.p.text_center(
        Face::Semibold,
        20.0,
        Rect::new(r.x, y, r.w, 30.0),
        strings::APP_NAME,
        pal.text,
    );
    y += 30.0 + 6.0;
    c.p.text_center(
        Face::Ui,
        13.5,
        Rect::new(r.x + 16.0, y, r.w - 32.0, 20.0),
        strings::EMPTY_HINT,
        pal.muted,
    );
    y += 20.0 + 28.0;
    let label = strings::EMPTY_BUTTON;
    let bw = c.p.measure(Face::Semibold, 13.5, label) + 56.0;
    let b = Rect::new((cx - bw / 2.0).round(), y, bw.round(), 36.0);
    let fill = match (
        c.hover == Some(Hit::EmptyOpen),
        c.pressed == Some(Hit::EmptyOpen),
    ) {
        (true, true) => mix(pal.accent, pal.page, 0.2),
        (true, false) => mix(pal.accent, pal.page, 0.1),
        _ => pal.accent,
    };
    c.p.rbox(b, 6.0, fill);
    c.p.icon(glyph::OPEN, 14.0, b.x + 24.0, b.cy(), pal.on_accent);
    c.p.text(
        Face::Semibold,
        13.5,
        b.x + 38.0,
        b.cy(),
        label,
        pal.on_accent,
        f32::INFINITY,
    );
    c.hit(Hit::EmptyOpen, b);
    y += 36.0;
    if recents.is_empty() {
        return;
    }
    y += 28.0;
    let x = (cx - col_w / 2.0).round();
    c.p.text(
        Face::Semibold,
        11.0,
        x + 10.0,
        y + 8.0,
        "RECENT",
        pal.muted,
        col_w,
    );
    c.p.fill(
        Rect::new(x + 70.0, y + 8.0, col_w - 80.0, c.p.px()),
        pal.line,
    );
    y += 20.0;
    for (i, path) in recents.iter().enumerate() {
        let row = Rect::new(x, y, col_w, 36.0);
        let hit = Hit::EmptyRecent(i);
        if c.hover == Some(hit) {
            c.p.rbox(
                row,
                6.0,
                if c.pressed == Some(hit) {
                    pal.press
                } else {
                    pal.hover
                },
            );
        }
        let missing = !path.exists();
        c.p.icon(
            if missing { glyph::WARN } else { glyph::DOC },
            14.0,
            x + 20.0,
            row.cy(),
            if missing { pal.warn } else { pal.accent },
        );
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let nw = c.p.text(
            Face::Ui,
            13.5,
            x + 38.0,
            row.cy(),
            &name,
            if missing { pal.faint } else { pal.text },
            col_w - 60.0,
        );
        if let Some(dir) = path.parent() {
            let left = x + 38.0 + nw + 12.0;
            c.p.text(
                Face::Ui,
                12.0,
                left,
                row.cy() + 0.5,
                &dir.display().to_string(),
                pal.faint,
                row.right() - left - 12.0,
            );
        }
        c.hit(hit, row);
        y += 40.0;
    }
}
