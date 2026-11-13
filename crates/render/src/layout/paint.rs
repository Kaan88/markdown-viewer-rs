//! The paint list a layout pass produces, the per-block cache entry that keeps it between frames,
//! and the raster step that draws a paint list into a bitmap.

use super::hit::{FloatBox, LineGeom, SourceBox, TaskBox};
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
    /// The formulas and diagrams drawn in the block.
    pub(super) sources: Vec<SourceBox>,
    /// The task-list boxes drawn in the block.
    pub(super) tasks: Vec<TaskBox>,
    /// The floated images the block draws.
    pub(super) floats: Vec<FloatBox>,
    pub(super) phase: f32,
}

impl CachedBlock {
    /// Whether the paints fit a block whose top is at `y`.
    pub(super) fn fits(&self, y: f32) -> bool {
        (y - y.floor()).to_bits() == self.phase.to_bits()
    }
}

/// Copies an opaque image the size of its box to `(left, top)`: an animation frame the worker
/// already scaled, so putting it up is a row copy and not a filter.
fn copy_opaque(frame: &mut Bitmap, image: &crate::image::DecodedImage, left: i32, top: i32) {
    let (width, height) = (image.width as i32, image.height as i32);
    let (c0, c1) = ((-left).max(0), (frame.width as i32 - left).min(width));
    let (r0, r1) = ((-top).max(0), (frame.height as i32 - top).min(height));
    if c0 >= c1 || r0 >= r1 || image.rgba.len() < (width * height * 4) as usize {
        return;
    }
    let count = (c1 - c0) as usize * 4;
    for row in r0..r1 {
        let from = ((row * width + c0) * 4) as usize;
        let to = (((top + row) * frame.width as i32 + left + c0) * 4) as usize;
        let source = &image.rgba[from..from + count];
        let target = &mut frame.pixels[to..to + count];
        // RGBA to the bitmap's BGRA.
        for (out, pixel) in target.chunks_exact_mut(4).zip(source.chunks_exact(4)) {
            out[0] = pixel[2];
            out[1] = pixel[1];
            out[2] = pixel[0];
            out[3] = 255;
        }
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
                    if let Some((image, fitted)) = self.store.pixels_at(index, w, h) {
                        // Snapped to a page row before the shift: the bitmap rounds half away
                        // from zero, so an image whose top sits on a half row above a band's
                        // first row would otherwise land a row off from the whole-page draw.
                        let y = y.round() - shift;
                        if fitted {
                            copy_opaque(frame, &image, x.round() as i32, y.round() as i32);
                            continue;
                        }
                        frame.image(&image.rgba, image.width, image.height, x, y, w, h);
                    }
                }
            }
        }
        engine.stats
    }
}
