//! The copy button of a code block: shown at the top right of the block under the pointer, drawn
//! over the page, never part of its layout.

use crate::chrome::{Ctx, glyph};
use crate::gfx::{Rect, mix};
use crate::ui::{Hit, Ui};
use crate::workspace::{CodeSpot, Workspace};
use std::time::Duration;

/// The button's side and its distance from the box's top and right edges, in logical px.
const SIDE: f32 = 24.0;
const INSET: f32 = 6.0;

/// How long the check mark shows after a copy.
pub const COPIED_FOR: Duration = Duration::from_millis(1500);

/// The code box under the pointer, when the button shows: the pointer is on the page (or on the
/// button itself) with nothing open over it and no drag going. `vr` is the page viewport in
/// logical px. Costs nothing unless the pointer is over the page.
pub fn spot(ws: &mut Workspace, ui: &Ui, vr: Rect, scale: f32) -> Option<CodeSpot> {
    if !matches!(ui.hover, Some(Hit::Page | Hit::CopyCode))
        || ui.drag.is_some()
        || ui.popup.is_some()
        || ui.dialog.is_some()
        || ui.modal()
    {
        return None;
    }
    let (x, y) = ui.mouse;
    if !vr.contains(x, y) {
        return None;
    }
    ws.code_spot((x - vr.x) * scale, (y - vr.y) * scale)
}

fn code_rect(code: &CodeSpot) -> Rect {
    let r = code.rect;
    Rect::new(r.x, r.y, r.w, r.h)
}

/// Where the button sits for the code box `code` (viewport device px, as in [`CodeSpot`]): at its
/// top right, or, when the top has scrolled out of view, at the top of the viewport so a tall
/// block keeps a button in reach. `None` when the box is too small to hold it without covering
/// most of its first line.
pub fn button_rect(code: Rect, vr: Rect, scale: f32) -> Option<Rect> {
    let (x, y) = (vr.x + code.x / scale, vr.y + code.y / scale);
    let (w, h) = (code.w / scale, code.h / scale);
    if h < SIDE + 2.0 * INSET || w < 3.0 * SIDE {
        return None;
    }
    let top = (y + INSET)
        .max(vr.y + INSET)
        .min(y + h - INSET - SIDE)
        .round();
    let left = (x + w - INSET - SIDE).round();
    let r = Rect::new(left, top, SIDE, SIDE);
    (r.bottom() > vr.y && r.y < vr.bottom()).then_some(r)
}

/// Draws the button over the code box under the pointer, if any, and records its hit.
pub fn draw(c: &mut Ctx, ws: &mut Workspace, ui: &Ui, vr: Rect) {
    let Some(code) = spot(ws, ui, vr, c.p.scale) else {
        return;
    };
    let Some(r) = button_rect(code_rect(&code), vr, c.p.scale) else {
        return;
    };
    let pal = *c.pal;
    let theme = ws.theme();
    let (hovered, pressed) = (
        c.hover == Some(Hit::CopyCode),
        c.pressed == Some(Hit::CopyCode),
    );
    let copied = ui.copied.is_some_and(|(start, _)| start == code.start);
    let base = theme.code_bg;
    let fill = match (pressed, hovered) {
        (true, _) => mix(base, pal.text, 0.20),
        (_, true) => mix(base, pal.text, 0.12),
        _ => base,
    };
    // The language label sits where the button does: it gives way while the button shows.
    if code.tag > 0.0 {
        let right = vr.x + (code.rect.x + code.rect.w) / c.p.scale;
        let from = (right - code.tag / c.p.scale - 6.0).min(r.x);
        c.p.fill(
            Rect::new(from, r.y, r.x - from + r.w / 2.0, r.h),
            theme.code_bg,
        );
    }
    // A hairline edge keeps it apart from the text under it, as the block's own edge does.
    c.p.rbox(r, 5.0, theme.border);
    let px = c.p.px();
    c.p.rbox(
        Rect::new(r.x + px, r.y + px, r.w - 2.0 * px, r.h - 2.0 * px),
        5.0,
        fill,
    );
    let (icon, color) = if copied {
        (glyph::CHECK, pal.accent)
    } else if hovered {
        (glyph::COPY, pal.text)
    } else {
        (glyph::COPY, pal.muted)
    };
    c.p.icon(icon, 14.0, r.cx(), r.cy(), color);
    c.hit(Hit::CopyCode, r);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vr() -> Rect {
        Rect::new(100.0, 50.0, 800.0, 600.0)
    }

    #[test]
    fn the_button_sits_at_the_top_right_of_the_box() {
        // A box 400 x 100 logical at (20, 40) in the viewport, at scale 2.
        let code = Rect::new(40.0, 80.0, 800.0, 200.0);
        let r = button_rect(code, vr(), 2.0).unwrap();
        assert_eq!(
            r,
            Rect::new(
                100.0 + 20.0 + 400.0 - 6.0 - 24.0,
                50.0 + 40.0 + 6.0,
                24.0,
                24.0
            )
        );
    }

    #[test]
    fn a_box_too_small_gets_none() {
        assert!(button_rect(Rect::new(0.0, 0.0, 400.0, 30.0), vr(), 1.0).is_none());
        assert!(button_rect(Rect::new(0.0, 0.0, 60.0, 100.0), vr(), 1.0).is_none());
    }

    #[test]
    fn a_box_scrolled_up_keeps_the_button_at_the_top_of_the_view() {
        let code = Rect::new(0.0, -300.0, 500.0, 600.0);
        let r = button_rect(code, vr(), 1.0).unwrap();
        assert_eq!(r.y, 50.0 + INSET);
        // Near the end of the box it stays inside it.
        let code = Rect::new(0.0, -590.0, 500.0, 600.0);
        let r = button_rect(code, vr(), 1.0).unwrap();
        assert_eq!(r.bottom(), 50.0 - 590.0 + 600.0 - INSET);
        assert!(r.bottom() > vr().y);
        // A box wholly above the view has none.
        assert!(button_rect(Rect::new(0.0, -700.0, 500.0, 100.0), vr(), 1.0).is_none());
    }
}
