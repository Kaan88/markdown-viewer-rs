//! The diagram zoom view: a rendered Mermaid diagram over the whole window, at a size its
//! labels can be read, panned with the pointer and zoomed with the wheel. `Esc`, the close
//! button and a click on the backdrop's button close it.
//!
//! The raster is the page's own (twice the device scale); one raster pixel per logical pixel
//! shows it at twice the size the page shrank it to, which is where its text is readable.

use crate::chrome::{Ctx, glyph};
use crate::gfx::{Face, Rect};
use mdview_render::DecodedImage;
use std::rc::Rc;

/// The smallest and largest drawing size, in logical px per raster px.
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 4.0;
/// How much one wheel notch changes the size.
pub const WHEEL_STEP: f32 = 1.1;

/// The line under the diagram.
pub const HINT: &str = "Scroll to zoom · drag to pan · Esc to close";

pub struct ZoomView {
    pub image: Rc<DecodedImage>,
    /// Logical px per raster px, never outside [`MIN_ZOOM`]..[`MAX_ZOOM`].
    pub zoom: f32,
    /// The image's top left, logical px from the area's top left. Clamped so no part of it can
    /// be moved past an edge of the area, and centred while it is smaller than the area.
    pub pan: (f32, f32),
}

impl ZoomView {
    /// Opens a diagram at reading size, centred in `area`.
    pub fn open(image: Rc<DecodedImage>, area: Rect) -> ZoomView {
        let mut view = ZoomView {
            image,
            zoom: 1.0,
            pan: (0.0, 0.0),
        };
        view.clamp(area);
        view
    }

    /// The drawn size, logical px.
    pub fn image_size(&self) -> (f32, f32) {
        (
            self.image.width as f32 * self.zoom,
            self.image.height as f32 * self.zoom,
        )
    }

    /// Where the image is drawn inside `area`.
    pub fn image_rect(&self, area: Rect) -> Rect {
        let (w, h) = self.image_size();
        Rect::new(area.x + self.pan.0, area.y + self.pan.1, w, h)
    }

    /// Keeps the image where it can be seen: centred on an axis where it is smaller than the
    /// area, and never moved so far that a gap opens at an edge where it is larger.
    pub fn clamp(&mut self, area: Rect) {
        let (w, h) = self.image_size();
        self.pan.0 = if w <= area.w {
            ((area.w - w) / 2.0).round()
        } else {
            self.pan.0.clamp(area.w - w, 0.0)
        };
        self.pan.1 = if h <= area.h {
            ((area.h - h) / 2.0).round()
        } else {
            self.pan.1.clamp(area.h - h, 0.0)
        };
    }

    /// Zooms to `zoom`, keeping the raster point under `focus` where it is.
    pub fn zoom_to(&mut self, area: Rect, zoom: f32, focus: (f32, f32)) {
        let before = self.zoom;
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let at = (
            (focus.0 - area.x - self.pan.0) / before,
            (focus.1 - area.y - self.pan.1) / before,
        );
        self.pan = (
            focus.0 - area.x - at.0 * self.zoom,
            focus.1 - area.y - at.1 * self.zoom,
        );
        self.clamp(area);
    }

    /// One wheel step: in with a positive `notches`, out with a negative one.
    pub fn zoom_by(&mut self, area: Rect, notches: f32, focus: (f32, f32)) {
        if notches == 0.0 || !notches.is_finite() {
            return;
        }
        self.zoom_to(area, self.zoom * WHEEL_STEP.powf(notches), focus);
    }

    /// Shows the whole diagram, centred.
    pub fn fit(&mut self, area: Rect) {
        let (w, h) = (self.image.width as f32, self.image.height as f32);
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        self.zoom = (area.w / w).min(area.h / h).clamp(MIN_ZOOM, MAX_ZOOM);
        self.clamp(area);
    }

    /// Moves the image by a drag, in logical px.
    pub fn pan_by(&mut self, area: Rect, dx: f32, dy: f32) {
        self.pan.0 += dx;
        self.pan.1 += dy;
        self.clamp(area);
    }
}

/// Draws the view over the whole window: a dimmed page, the diagram, a close button and the
/// hint. Records the close button for the pointer.
pub fn draw(c: &mut Ctx, view: &ZoomView, area: Rect) {
    let pal = *c.pal;
    c.p.fill_a(area, [0, 0, 0], if pal.dark { 0.70 } else { 0.52 });
    let r = view.image_rect(area);
    c.p.shadow(r, 8.0, pal.dark);
    let scale = c.p.scale;
    c.p.bmp.image(
        &view.image.rgba,
        view.image.width,
        view.image.height,
        r.x * scale,
        r.y * scale,
        r.w * scale,
        r.h * scale,
    );
    // A hairline edge, so a diagram with a pale background still reads as a sheet.
    let px = c.p.px();
    let edge = pal.popup_line;
    c.p.fill(Rect::new(r.x, r.y, r.w, px), edge);
    c.p.fill(Rect::new(r.x, r.bottom() - px, r.w, px), edge);
    c.p.fill(Rect::new(r.x, r.y, px, r.h), edge);
    c.p.fill(Rect::new(r.right() - px, r.y, px, r.h), edge);

    // The close button at the window's top right, over the dimmed page.
    let close = Rect::new(area.right() - 40.0, area.y + 12.0, 28.0, 28.0);
    let hovered = c.hover == Some(crate::ui::Hit::ZoomClose);
    c.p.rbox(close, 8.0, if hovered { pal.hover } else { pal.popup });
    c.p.icon(glyph::CLOSE, 9.5, close.cx(), close.cy(), pal.text);
    c.hit(crate::ui::Hit::ZoomClose, close);

    let tw = c.p.measure(Face::Ui, 12.5, HINT);
    let pill = Rect::new(
        (area.cx() - tw / 2.0 - 14.0).round(),
        area.bottom() - 46.0,
        tw + 28.0,
        29.0,
    );
    c.p.rbox(pill, 14.5, pal.popup);
    c.p.text_center(Face::Ui, 12.5, pill, HINT, pal.text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32) -> Rc<DecodedImage> {
        Rc::new(DecodedImage {
            width: w,
            height: h,
            baseline: h,
            rgba: vec![255; w as usize * h as usize * 4],
        })
    }

    fn area() -> Rect {
        Rect::new(0.0, 0.0, 1000.0, 800.0)
    }

    #[test]
    fn a_diagram_opens_at_reading_size_and_is_centred() {
        let view = ZoomView::open(image(300, 200), area());
        assert_eq!(view.zoom, 1.0);
        let r = view.image_rect(area());
        assert_eq!((r.x, r.y, r.w, r.h), (350.0, 300.0, 300.0, 200.0));
    }

    #[test]
    fn a_diagram_wider_than_the_window_starts_at_reading_size_and_pans_within_it() {
        let mut view = ZoomView::open(image(3000, 1500), area());
        // Wider than the area: its left edge is at the area's left, not centred off screen.
        assert_eq!(view.pan.0, 0.0);
        view.pan_by(area(), -500.0, -500.0);
        assert_eq!(view.pan, (-500.0, -500.0));
        // Its right and bottom edges stop at the area's edges.
        view.pan_by(area(), -5000.0, -5000.0);
        assert_eq!(view.pan, (1000.0 - 3000.0, 800.0 - 1500.0));
        view.pan_by(area(), 5000.0, 5000.0);
        assert_eq!(view.pan, (0.0, 0.0));
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer_and_stays_in_range() {
        let mut view = ZoomView::open(image(1200, 1000), area());
        let focus = (600.0, 450.0);
        let before = (
            (focus.0 - view.image_rect(area()).x) / view.zoom,
            (focus.1 - view.image_rect(area()).y) / view.zoom,
        );
        view.zoom_to(area(), 1.5, focus);
        assert_eq!(view.zoom, 1.5);
        let r = view.image_rect(area());
        assert!(((focus.0 - r.x) / view.zoom - before.0).abs() < 0.01);
        assert!(((focus.1 - r.y) / view.zoom - before.1).abs() < 0.01);
        view.zoom_to(area(), 100.0, focus);
        assert_eq!(view.zoom, MAX_ZOOM);
        view.zoom_to(area(), 0.0, focus);
        assert_eq!(view.zoom, MIN_ZOOM);
    }

    #[test]
    fn fitting_shows_the_whole_diagram_and_centres_it() {
        let mut view = ZoomView::open(image(3000, 1500), area());
        view.fit(area());
        let r = view.image_rect(area());
        assert!(r.w <= area().w + 0.01 && r.h <= area().h + 0.01);
        assert_eq!((r.cx(), r.cy()), (area().cx(), area().cy()));
    }
}
