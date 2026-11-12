//! The Files pane on the left, the Outline pane on the right, and the overlay scrollbar they share
//! with the page.

use crate::chrome::{Ctx, glyph};
use crate::gfx::{Face, Rect};
use crate::theme::Palette;
use crate::ui::{Hit, Scroller, Side, Ui};
use crate::workspace::Workspace;
pub use mdshared_ui::pane::ROW;
use mdshared_ui::pane::{HEAD, ROW_H, draw_head, draw_row_fill, draw_splitter};
pub use mdshared_ui::scroll::{draw_scrollbar, offset_for_thumb, scrollbar};
use mdview_core::commands::strings;

fn pane_head(c: &mut Ctx, r: Rect, title: &str) {
    draw_head(c.p, c.pal, r, title);
}

fn row_fill(c: &mut Ctx, row: Rect, hit: Hit, selected: bool) {
    let hovered = c.hover == Some(hit);
    draw_row_fill(c.p, c.pal, row, hovered, c.pressed == Some(hit), selected);
}

/// The splitter on a pane's inner edge, and the hairline that marks it.
fn edge(c: &mut Ctx, r: Rect, side: Side, dragging: bool) {
    let hot = dragging || c.hover == Some(Hit::Splitter(side));
    let hit = draw_splitter(c.p, c.pal, r, side == Side::Files, hot);
    c.hit(Hit::Splitter(side), hit);
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
