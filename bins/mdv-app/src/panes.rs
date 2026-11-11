//! The Files pane on the left, the Outline pane on the right, and the overlay scrollbar they share
//! with the page.

use crate::chrome::{Ctx, glyph};
use crate::gfx::{Face, Painter, Rect};
use crate::theme::Palette;
use crate::ui::{Hit, Scroller, Side, Ui};
use crate::workspace::Workspace;
use mdview_core::commands::strings;

/// Row pitch and height in both panes.
pub const ROW: f32 = 30.0;
const ROW_H: f32 = 28.0;
/// The pane title row.
const HEAD: f32 = 40.0;

/// The track and thumb of an overlay scrollbar for `area`, or `None` when everything fits.
/// `content`, `view` and `offset` may be in any one unit.
pub fn scrollbar(area: Rect, content: f32, view: f32, offset: f32) -> Option<(Rect, Rect)> {
    if content <= view + 0.5 || area.h < 24.0 {
        return None;
    }
    let track = Rect::new(area.right() - 12.0, area.y + 2.0, 12.0, area.h - 4.0);
    let len = (track.h * view / content).clamp(28.0_f32.min(track.h), track.h);
    let t = (offset / (content - view)).clamp(0.0, 1.0);
    let thumb = Rect::new(track.x, track.y + t * (track.h - len), track.w, len);
    Some((track, thumb))
}

/// The scroll offset that puts the thumb's top at `thumb_y`.
pub fn offset_for_thumb(track: Rect, thumb_h: f32, thumb_y: f32, content: f32, view: f32) -> f32 {
    let span = (track.h - thumb_h).max(1.0);
    ((thumb_y - track.y) / span).clamp(0.0, 1.0) * (content - view).max(0.0)
}

/// Thin at rest, wider while hovered or dragged.
pub fn draw_scrollbar(p: &mut Painter, pal: &Palette, track: Rect, thumb: Rect, active: bool) {
    let w = if active { 8.0 } else { 3.0 };
    if active {
        p.rbox_a(
            Rect::new(track.right() - w - 3.0, track.y, w, track.h),
            w / 2.0,
            pal.thumb,
            0.10,
        );
    }
    let r = Rect::new(track.right() - w - 3.0, thumb.y, w, thumb.h);
    p.rbox_a(r, w / 2.0, pal.thumb, if active { 0.85 } else { 0.55 });
}

fn pane_head(c: &mut Ctx, r: Rect, title: &str) {
    let pal = *c.pal;
    c.p.fill(r, pal.pane);
    c.p.text(
        Face::Semibold,
        11.0,
        r.x + 16.0,
        r.y + HEAD / 2.0,
        title,
        pal.muted,
        r.w - 60.0,
    );
}

fn row_fill(c: &mut Ctx, row: Rect, hit: Hit, selected: bool) {
    let pal = *c.pal;
    if selected {
        c.p.rbox(row, 5.0, pal.on);
        c.p.rbox(
            Rect::new(row.x, row.y + 7.0, 3.0, row.h - 14.0),
            1.5,
            pal.accent,
        );
    } else if c.hover == Some(hit) {
        c.p.rbox(
            row,
            5.0,
            if c.pressed == Some(hit) {
                pal.press
            } else {
                pal.hover
            },
        );
    }
}

/// The splitter on a pane's inner edge, and the hairline that marks it.
fn edge(c: &mut Ctx, r: Rect, side: Side, dragging: bool) {
    let pal = *c.pal;
    let px = c.p.px();
    let x = match side {
        Side::Files => r.right() - px,
        Side::Outline => r.x,
    };
    let hot = dragging || c.hover == Some(Hit::Splitter(side));
    if hot {
        c.p.fill(Rect::new(x - px, r.y, 3.0 * px, r.h), pal.accent);
    } else {
        c.p.fill(Rect::new(x, r.y, px, r.h), pal.line);
    }
    c.hit(Hit::Splitter(side), Rect::new(x - 3.0, r.y, 6.0, r.h));
}

pub fn draw_files(c: &mut Ctx, ws: &Workspace, ui: &mut Ui, r: Rect) {
    let pal = *c.pal;
    pane_head(c, r, strings::FILES_DOCK);
    let b = Rect::new(r.right() - 38.0, r.y + 6.0, 28.0, 28.0);
    if let Some(fill) = c.fill_for(Hit::FilesFolder, false, false) {
        c.p.rbox(b, 5.0, fill);
    }
    c.p.icon(glyph::FOLDER_OPEN, 14.0, b.cx(), b.cy(), pal.muted);
    c.hit(Hit::FilesFolder, b);

    let Some(root) = ui.tree.root().map(|p| p.to_path_buf()) else {
        let cy = r.y + HEAD + 40.0;
        c.p.text_center(
            Face::Ui,
            13.0,
            Rect::new(r.x, cy - 10.0, r.w, 20.0),
            "No folder open",
            pal.muted,
        );
        let label = "Open folder…";
        let w = c.p.measure(Face::Ui, 13.0, label) + 28.0;
        let b = Rect::new((r.cx() - w / 2.0).round(), cy + 18.0, w, 30.0);
        let fill = c
            .fill_for(Hit::FilesOpenFolder, false, false)
            .unwrap_or(pal.bar);
        c.p.frame(b, 6.0, fill, pal.line);
        c.p.text_center(Face::Ui, 13.0, b, label, pal.text);
        c.hit(Hit::FilesOpenFolder, b);
        ui.panes.files_content = 0.0;
        edge(c, r, Side::Files, false);
        return;
    };
    // The folder itself, as the root row.
    let name = root.file_name().map_or_else(
        || root.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let ry = r.y + HEAD;
    c.p.icon(
        glyph::FOLDER_OPEN,
        14.0,
        r.x + 22.0,
        ry + ROW_H / 2.0,
        pal.folder,
    );
    c.p.text(
        Face::Semibold,
        12.5,
        r.x + 36.0,
        ry + ROW_H / 2.0,
        &name,
        pal.text,
        r.w - 52.0,
    );

    let list = Rect::new(r.x, ry + ROW, r.w, (r.bottom() - ry - ROW).max(0.0));
    let rows = ui.tree.rows();
    ui.panes.files_content = rows.len() as f32 * ROW + 8.0;
    ui.panes.files_view = list.h;
    ui.panes.files_scroll = ui
        .panes
        .files_scroll
        .clamp(0.0, (ui.panes.files_content - list.h).max(0.0));
    let scroll = ui.panes.files_scroll;
    let active = ws.active_tab().map(|t| t.path().to_path_buf());
    let old = c.p.clip(list);
    let first = (scroll / ROW).floor() as usize;
    for (i, row) in rows.iter().enumerate().skip(first) {
        let y = list.y + i as f32 * ROW - scroll;
        if y > list.bottom() {
            break;
        }
        let rr = Rect::new(r.x + 6.0, y, r.w - 12.0, ROW_H);
        let hit = Hit::FilesRow(i);
        let selected = !row.is_dir
            && active
                .as_deref()
                .is_some_and(|a| crate::workspace::same_file(a, &row.path));
        row_fill(c, rr, hit, selected);
        let ix = r.x + 14.0 + (row.depth as f32 + 1.0) * 14.0;
        let cy = rr.cy();
        if row.is_dir {
            c.p.icon(
                if row.expanded {
                    glyph::DOWN
                } else {
                    glyph::RIGHT
                },
                8.0,
                ix - 8.0,
                cy,
                pal.faint,
            );
            c.p.icon(
                if row.expanded {
                    glyph::FOLDER_OPEN
                } else {
                    glyph::FOLDER
                },
                14.0,
                ix + 9.0,
                cy,
                pal.folder,
            );
        } else {
            c.p.icon(
                glyph::DOC,
                13.0,
                ix + 9.0,
                cy,
                if selected { pal.accent } else { pal.muted },
            );
        }
        let color = if selected { pal.text } else { mixed_text(&pal) };
        c.p.text(
            Face::Ui,
            13.0,
            ix + 23.0,
            cy,
            &row.name,
            color,
            rr.right() - ix - 30.0,
        );
        c.hit(hit, rr);
    }
    c.p.restore(old);
    let thumb_hit = Hit::ScrollBar(Scroller::Pane(Side::Files));
    if let Some((track, thumb)) = scrollbar(list, ui.panes.files_content, list.h, scroll) {
        let active = c.hover == Some(thumb_hit)
            || matches!(
                ui.drag,
                Some(crate::ui::Drag::Thumb {
                    target: Scroller::Pane(Side::Files),
                    ..
                })
            );
        draw_scrollbar(c.p, &pal, track, thumb, active);
        c.hit(thumb_hit, track);
    }
    let dragging = matches!(
        ui.drag,
        Some(crate::ui::Drag::Splitter {
            side: Side::Files,
            ..
        })
    );
    edge(c, r, Side::Files, dragging);
}

/// Row text: a step quieter than headings and the selected row.
fn mixed_text(pal: &Palette) -> crate::gfx::Rgb {
    crate::gfx::mix(pal.text, pal.muted, 0.35)
}

pub fn draw_outline(c: &mut Ctx, ws: &Workspace, ui: &mut Ui, r: Rect) {
    let pal = *c.pal;
    pane_head(c, r, strings::OUTLINE_DOCK);
    let list = Rect::new(r.x, r.y + HEAD, r.w, (r.h - HEAD).max(0.0));
    let entries = ws.outline();
    ui.panes.outline_content = entries.len() as f32 * ROW + 8.0;
    ui.panes.outline_view = list.h;
    ui.panes.outline_scroll = ui
        .panes
        .outline_scroll
        .clamp(0.0, (ui.panes.outline_content - list.h).max(0.0));
    let scroll = ui.panes.outline_scroll;
    if entries.is_empty() {
        let text = if ws.active().is_some() {
            "No headings"
        } else {
            "No document"
        };
        c.p.text(
            Face::Ui,
            13.0,
            r.x + 16.0,
            list.y + 14.0,
            text,
            pal.faint,
            r.w - 32.0,
        );
    }
    let base = entries.iter().map(|e| e.level).min().unwrap_or(1);
    let current = ws.current_heading();
    let old = c.p.clip(list);
    let first = (scroll / ROW).floor() as usize;
    for (i, e) in entries.iter().enumerate().skip(first) {
        let y = list.y + i as f32 * ROW - scroll;
        if y > list.bottom() {
            break;
        }
        let rr = Rect::new(r.x + 6.0, y, r.w - 12.0, ROW_H);
        let hit = Hit::OutlineRow(i);
        let selected = current == Some(i);
        row_fill(c, rr, hit, selected);
        let x = r.x + 18.0 + (e.level.saturating_sub(base).min(5)) as f32 * 14.0;
        let top = e.level == base;
        let color = if selected || top {
            pal.text
        } else {
            mixed_text(&pal)
        };
        let face = if top { Face::Semibold } else { Face::Ui };
        let title = if e.title.trim().is_empty() {
            "(untitled)"
        } else {
            e.title
        };
        c.p.text(face, 13.0, x, rr.cy(), title, color, rr.right() - x - 8.0);
        c.hit(hit, rr);
    }
    c.p.restore(old);
    let thumb_hit = Hit::ScrollBar(Scroller::Pane(Side::Outline));
    if let Some((track, thumb)) = scrollbar(list, ui.panes.outline_content, list.h, scroll) {
        let active = c.hover == Some(thumb_hit)
            || matches!(
                ui.drag,
                Some(crate::ui::Drag::Thumb {
                    target: Scroller::Pane(Side::Outline),
                    ..
                })
            );
        draw_scrollbar(c.p, &pal, track, thumb, active);
        c.hit(thumb_hit, track);
    }
    let dragging = matches!(
        ui.drag,
        Some(crate::ui::Drag::Splitter {
            side: Side::Outline,
            ..
        })
    );
    edge(c, r, Side::Outline, dragging);
}

/// Scrolls the outline pane so row `index` is visible.
pub fn reveal_outline_row(ui: &mut Ui, index: usize) {
    let top = index as f32 * ROW;
    let view = ui.panes.outline_view;
    if view <= 0.0 {
        return;
    }
    let s = &mut ui.panes.outline_scroll;
    if top < *s {
        *s = (top - ROW).max(0.0);
    } else if top + ROW > *s + view {
        *s = top + 2.0 * ROW - view;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_geometry() {
        let area = Rect::new(0.0, 0.0, 100.0, 404.0);
        assert!(scrollbar(area, 300.0, 404.0, 0.0).is_none());
        let (track, thumb) = scrollbar(area, 800.0, 400.0, 0.0).unwrap();
        assert_eq!(track.h, 400.0);
        assert_eq!(thumb.h, 200.0);
        assert_eq!(thumb.y, track.y);
        let (_, end) = scrollbar(area, 800.0, 400.0, 400.0).unwrap();
        assert_eq!(end.bottom(), track.bottom());
        let back = offset_for_thumb(track, end.h, end.y, 800.0, 400.0);
        assert!((back - 400.0).abs() < 0.01);
        // A huge document keeps a grabbable thumb.
        let (_, tiny) = scrollbar(area, 1e7, 400.0, 0.0).unwrap();
        assert_eq!(tiny.h, 28.0);
    }
}
