//! The paint list a layout pass produces, the per-block cache entry that keeps it between frames,
//! and the raster step that draws a paint list into a bitmap.

use super::hit::LineGeom;
use super::pager::Pager;
use super::{Rendered, Slice};
use crate::bitmap::Bitmap;
use crate::text::{Brush, TextEngine};
use crate::theme::Color;
use parley::Layout;
use std::rc::Rc;

#[derive(Clone)]
pub(super) enum Paint {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
    },
    Line {
        a: (f32, f32),
        b: (f32, f32),
        thickness: f32,
        color: Color,
    },
    Text {
        layout: Rc<Layout<Brush>>,
        x: f32,
        y: f32,
    },
    Image {
        index: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

impl Paint {
    pub(super) fn shift(&mut self, dy: f32) {
        match self {
            Paint::Rect { y, .. } => *y += dy,
            Paint::Line { a, b, .. } => {
                a.1 += dy;
                b.1 += dy;
            }
            Paint::Text { y, .. } => *y += dy,
            Paint::Image { y, .. } => *y += dy,
        }
    }
}

/// Everything a top-level block keeps for later frames: what to paint and where its text is,
/// relative to the whole row above the block's top.
///
/// Layout rounds rules, chips and underlines to whole rows of the page, so paints made for a top
/// at one fraction of a row are only right at that fraction: shifted by a fractional amount they
/// would straddle two rows and blur. `phase` is that fraction; the paints are reused while the
/// block's top keeps it, and the block is measured again when a correction above changes it.
#[derive(Clone, Default)]
pub(super) struct CachedBlock {
    pub(super) paints: Vec<Paint>,
    pub(super) lines: Vec<LineGeom>,
    pub(super) phase: f32,
}

impl CachedBlock {
    /// Whether the paints fit a block whose top is at `y`.
    pub(super) fn fits(&self, y: f32) -> bool {
        (y - y.floor()).to_bits() == self.phase.to_bits()
    }
}

impl<'a, 'b> Pager<'a, 'b> {
    pub(super) fn finish(mut self, engine: &mut TextEngine) -> Rendered {
        let content_height = self.y + self.px(24.0);
        let height = content_height.ceil().max(1.0) as u32;
        let width = self.width.round().max(1.0) as u32;
        let mut frame = Bitmap::new(width, height);
        frame.clear(self.theme.bg);
        let stats = self.draw(&mut frame, 0.0, engine);
        Rendered {
            bitmap: frame,
            content_height,
            blocks: self.blocks,
            stats,
        }
    }

    pub(super) fn finish_slice(
        mut self,
        top: f32,
        height: f32,
        total_height: f32,
        engine: &mut TextEngine,
    ) -> Slice {
        let rows = height.ceil().max(1.0) as u32;
        let width = self.width.round().max(1.0) as u32;
        let mut frame = Bitmap::new(width, rows);
        frame.clear(self.theme.bg);
        let stats = self.draw(&mut frame, top, engine);
        Slice {
            bitmap: frame,
            top,
            total_height,
            stats,
        }
    }

    pub(super) fn draw(
        &mut self,
        frame: &mut Bitmap,
        shift: f32,
        engine: &mut TextEngine,
    ) -> crate::text::CacheStats {
        for paint in self.paints.drain(..) {
            match paint {
                Paint::Rect {
                    x,
                    y,
                    w,
                    h,
                    radius,
                    color,
                } => frame.rrect(x, y - shift, w, h, radius, color),
                Paint::Line {
                    a,
                    b,
                    thickness,
                    color,
                } => frame.line((a.0, a.1 - shift), (b.0, b.1 - shift), thickness, color),
                Paint::Text { layout, x, y } => engine.draw_layout(frame, &layout, x, y - shift),
                Paint::Image { index, x, y, w, h } => {
                    if let Some(image) = self.store.pixels(index) {
                        frame.image(&image.rgba, image.width, image.height, x, y - shift, w, h);
                    }
                }
            }
        }
        engine.stats
    }
}
