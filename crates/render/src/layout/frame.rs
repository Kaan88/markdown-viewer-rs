//! The frame kept between scroll calls: [`DocumentRenderer::view_frame`] shifts the rows that stay
//! on screen and paints only the band that came into view, reporting `band`/`shift` to the shell.

use super::pager::Pager;
use super::{DocumentRenderer, Frame, Options, PageLayout};
use crate::bitmap::Bitmap;
use mdview_core::model::Document;

impl DocumentRenderer {
    /// Draws the viewport into a frame kept between calls. When the new view still overlaps the last
    /// one, those rows are shifted in place and only the band that came into view is painted.
    pub fn view_frame(
        &mut self,
        doc: &Document,
        options: &Options,
        page: &PageLayout,
        top: f32,
        height: f32,
    ) -> &Frame {
        debug_assert!(page.matches(options), "page layout for different options");
        let height = height.max(1.0);
        let top = top
            .clamp(0.0, (page.total_height - height).max(0.0))
            .round();
        let key = (options.width, options.scale, options.theme.name);
        if self.cache_key != Some(key) {
            self.cache.clear();
            self.cache_key = Some(key);
            self.frame = None;
        }
        let background = options.theme.bg;
        self.store.begin_frame();
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            Some(&mut self.cache),
            &mut self.store,
            &self.details,
        );
        let rows = height.ceil().max(1.0) as u32;
        let width = pager.width.round().max(1.0) as u32;
        if let Some(frame) = self.frame.as_ref()
            && frame.bitmap.width == width
            && frame.bitmap.height == rows
            && frame.top == top
        {
            // Nothing changed, so there is nothing for the caller to send again.
            let frame = self.frame.as_mut().expect("frame");
            frame.band = Some((0, 0));
            frame.shift = 0;
            return self.frame.as_ref().expect("frame");
        }
        let mut band = (top, rows as f32);
        let mut shift = 0i32;
        let mut whole = true;
        let same_size = self
            .frame
            .as_ref()
            .is_some_and(|frame| frame.bitmap.width == width && frame.bitmap.height == rows);
        if same_size {
            let frame = self.frame.as_mut().expect("frame");
            let dy = frame.top as i64 - top as i64;
            if dy.unsigned_abs() < rows as u64 {
                shift_rows(&mut frame.bitmap, dy as i32);
                // `dy` is old top minus new top: positive when the view scrolled up and the newly
                // exposed rows are at the top, negative when it scrolled down and they are at the
                // bottom.
                band = if dy > 0 {
                    (top, dy as f32)
                } else {
                    (top + rows as f32 + dy as f32, (-dy) as f32)
                };
                // Row `r` of the new frame was row `r - dy` of the old one.
                shift = -(dy as i32);
                whole = false;
            } else {
                frame.bitmap.clear(background);
            }
            frame.top = top;
        } else {
            let mut bitmap = Bitmap::new(width, rows);
            bitmap.clear(background);
            self.frame = Some(Frame {
                bitmap,
                top,
                stats: crate::text::CacheStats::default(),
                band: None,
                shift: 0,
            });
            band = (top, rows as f32);
        }
        let frame_top = self.frame.as_ref().expect("frame").top;
        // Blocks can reach past the band, so draw the band on its own bitmap and copy it in;
        // otherwise the overlap would be blended twice.
        let stats = if band.1 + 0.5 >= rows as f32 && (band.0 - frame_top).abs() < 0.5 {
            pager.slice_page(page, band.0, band.1);
            let frame = self.frame.as_mut().expect("frame");
            pager.draw(&mut frame.bitmap, frame_top, &mut self.engine)
        } else {
            let mut band_bitmap = Bitmap::new(width, band.1.ceil().max(1.0) as u32);
            band_bitmap.clear(background);
            pager.slice_page(page, band.0, band.1);
            let stats = pager.draw(&mut band_bitmap, band.0, &mut self.engine);
            let frame = self.frame.as_mut().expect("frame");
            copy_rows(
                &mut frame.bitmap,
                &band_bitmap,
                (band.0 - frame_top) as usize,
            );
            stats
        };
        let first = (band.0 - frame_top).round().max(0.0) as u32;
        let band_rows = (first, band.1.ceil().max(0.0) as u32);
        let frame = self.frame.as_mut().expect("frame");
        frame.stats = stats;
        frame.band = (!whole).then_some(band_rows);
        frame.shift = shift;
        self.frame.as_ref().expect("frame")
    }
}

/// Copies `source` into `target` starting at row `row`.
fn copy_rows(target: &mut Bitmap, source: &Bitmap, row: usize) {
    let stride = target.width as usize * 4;
    let rows = source.height as usize;
    let start = row * stride;
    target.pixels[start..start + rows * stride].copy_from_slice(&source.pixels[..rows * stride]);
}

/// Moves the rows of `bitmap` down by `dy` rows when `dy` is positive, up when negative.
fn shift_rows(bitmap: &mut Bitmap, dy: i32) {
    let stride = bitmap.width as usize * 4;
    let rows = bitmap.height as usize;
    let shift = dy.unsigned_abs() as usize;
    if shift == 0 || shift >= rows {
        return;
    }
    if dy > 0 {
        // The view scrolled up: the content moves down.
        bitmap
            .pixels
            .copy_within(0..(rows - shift) * stride, shift * stride);
    } else {
        // The view scrolled down: the content moves up.
        bitmap.pixels.copy_within(shift * stride..rows * stride, 0);
    }
}
