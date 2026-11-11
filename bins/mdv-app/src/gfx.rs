//! Drawing for the chrome: clipped, antialiased boxes and the page's own text path (parley lays
//! out, `mdview_render::TextEngine` rasterizes with subpixel masks), so UI text and document text
//! look the same. Callers work in logical px; the painter multiplies by the display scale and
//! snaps box edges to device pixels.

use mdview_render::{Bitmap, TextEngine, shared_fonts};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, Layout, LayoutContext,
    LineHeight, StyleProperty,
};
use std::collections::HashMap;
use std::rc::Rc;

pub type Rgb = [u8; 3];
type Brush = [u8; 4];

pub const fn rgb(hex: u32) -> Rgb {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

/// `a` moved `t` of the way towards `b`.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let ch = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    [ch(0), ch(1), ch(2)]
}

/// A rectangle in logical px.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }
    pub fn cx(self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn cy(self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
    /// Shrinks every side by `d` (grows when negative).
    pub fn inset(self, d: f32) -> Rect {
        Rect::new(
            self.x + d,
            self.y + d,
            (self.w - 2.0 * d).max(0.0),
            (self.h - 2.0 * d).max(0.0),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Face {
    Ui,
    Semibold,
    Icon,
}

const UI_FAMILY: &str =
    "Segoe UI Variable Text, Segoe UI, Inter, Noto Sans, Cantarell, DejaVu Sans, sans-serif";
const ICON_FAMILY: &str = "Segoe Fluent Icons, Segoe MDL2 Assets";

/// Font contexts, the glyph cache and a cache of laid-out labels. One per window.
pub struct Gfx {
    fonts: FontContext,
    layouts: LayoutContext<Brush>,
    engine: TextEngine,
    cache: HashMap<u64, Rc<Layout<Brush>>>,
}

fn key(face: Face, size: f32, color: Rgb, wrap: f32, text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (face, size.to_bits(), color, wrap.to_bits(), text).hash(&mut h);
    h.finish()
}

impl Gfx {
    pub fn new() -> Gfx {
        Gfx {
            fonts: shared_fonts(),
            layouts: LayoutContext::new(),
            engine: TextEngine::new(2 << 20),
            cache: HashMap::new(),
        }
    }

    /// A laid-out label. `size` is in device px; `wrap` is a device-px line width, or 0 for one
    /// line. Layouts are cached, since the chrome draws the same labels every frame.
    fn layout(
        &mut self,
        face: Face,
        size: f32,
        color: Rgb,
        wrap: f32,
        text: &str,
    ) -> Rc<Layout<Brush>> {
        let k = key(face, size, color, wrap, text);
        if let Some(l) = self.cache.get(&k) {
            return l.clone();
        }
        if self.cache.len() > 4000 {
            self.cache.clear();
        }
        let mut b = self
            .layouts
            .ranged_builder(&mut self.fonts, text, 1.0, true);
        let family = match face {
            Face::Ui | Face::Semibold => UI_FAMILY,
            Face::Icon => ICON_FAMILY,
        };
        b.push_default(StyleProperty::FontFamily(FontFamily::from(family)));
        b.push_default(StyleProperty::FontSize(size));
        let weight = if face == Face::Semibold { 600.0 } else { 400.0 };
        b.push_default(StyleProperty::FontWeight(FontWeight::new(weight)));
        if wrap > 0.0 {
            b.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
                1.45,
            )));
        }
        b.push_default(StyleProperty::Brush([color[0], color[1], color[2], 255]));
        let mut layout = b.build(text);
        layout.break_all_lines(if wrap > 0.0 { Some(wrap) } else { None });
        layout.align(Alignment::Start, AlignmentOptions::default());
        let layout = Rc::new(layout);
        self.cache.insert(k, layout.clone());
        layout
    }
}

/// Draws into one bitmap with a clip rectangle. Coordinates are logical px.
pub struct Painter<'a> {
    pub bmp: &'a mut Bitmap,
    pub gfx: &'a mut Gfx,
    pub scale: f32,
    /// Device px: x0, y0, x1, y1.
    clip: (i32, i32, i32, i32),
}

impl<'a> Painter<'a> {
    pub fn new(bmp: &'a mut Bitmap, gfx: &'a mut Gfx, scale: f32) -> Painter<'a> {
        let clip = (0, 0, bmp.width as i32, bmp.height as i32);
        Painter {
            bmp,
            gfx,
            scale,
            clip,
        }
    }

    /// One device pixel, in logical px: the width of a hairline.
    pub fn px(&self) -> f32 {
        1.0 / self.scale
    }

    fn dev(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    /// Restricts drawing to `r` (intersected with the bitmap) and returns the previous clip, to be
    /// given back to [`restore`](Self::restore).
    pub fn clip(&mut self, r: Rect) -> (i32, i32, i32, i32) {
        let old = self.clip;
        self.clip = (
            self.dev(r.x).max(old.0),
            self.dev(r.y).max(old.1),
            self.dev(r.right()).min(old.2),
            self.dev(r.bottom()).min(old.3),
        );
        old
    }

    pub fn restore(&mut self, clip: (i32, i32, i32, i32)) {
        self.clip = clip;
    }

    #[inline]
    fn span(&mut self, y: i32, x0: i32, x1: i32, c: Rgb, a: f32) {
        let (x0, x1) = (x0.max(self.clip.0), x1.min(self.clip.2));
        if y < self.clip.1 || y >= self.clip.3 || x0 >= x1 || a <= 0.0 {
            return;
        }
        let o = (y as usize * self.bmp.width as usize + x0 as usize) * 4;
        let row = &mut self.bmp.pixels[o..o + (x1 - x0) as usize * 4];
        if a >= 1.0 {
            let px = [c[2], c[1], c[0], 255];
            for d in row.as_chunks_mut::<4>().0 {
                d.copy_from_slice(&px);
            }
        } else {
            let a = (a * 256.0) as u32;
            let ia = 256 - a;
            for d in row.as_chunks_mut::<4>().0 {
                d[0] = ((c[2] as u32 * a + d[0] as u32 * ia) >> 8) as u8;
                d[1] = ((c[1] as u32 * a + d[1] as u32 * ia) >> 8) as u8;
                d[2] = ((c[0] as u32 * a + d[2] as u32 * ia) >> 8) as u8;
            }
        }
    }

    pub fn fill(&mut self, r: Rect, c: Rgb) {
        self.rbox_a(r, 0.0, c, 1.0);
    }

    pub fn fill_a(&mut self, r: Rect, c: Rgb, a: f32) {
        self.rbox_a(r, 0.0, c, a);
    }

    pub fn rbox(&mut self, r: Rect, radius: f32, c: Rgb) {
        self.rbox_a(r, radius, c, 1.0);
    }

    /// A rounded rectangle with antialiased corners, blended at `alpha`.
    pub fn rbox_a(&mut self, r: Rect, radius: f32, c: Rgb, alpha: f32) {
        let (x0, y0, x1, y1) = (
            self.dev(r.x),
            self.dev(r.y),
            self.dev(r.right()),
            self.dev(r.bottom()),
        );
        self.rbox_dev(x0, y0, x1, y1, radius * self.scale, c, alpha);
    }

    fn rbox_dev(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, rad: f32, c: Rgb, alpha: f32) {
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let rad = rad
            .min((x1 - x0) as f32 / 2.0)
            .min((y1 - y0) as f32 / 2.0)
            .max(0.0);
        let band = rad.ceil() as i32;
        for y in y0.max(self.clip.1)..y1.min(self.clip.3) {
            let fy = y as f32 + 0.5;
            let qy = if fy < y0 as f32 + rad {
                y0 as f32 + rad - fy
            } else if fy > y1 as f32 - rad {
                fy - (y1 as f32 - rad)
            } else {
                0.0
            };
            if qy <= 0.0 || band == 0 {
                self.span(y, x0, x1, c, alpha);
                continue;
            }
            self.span(y, x0 + band, x1 - band, c, alpha);
            for i in 0..band {
                for x in [x0 + i, x1 - 1 - i] {
                    let fx = x as f32 + 0.5;
                    let qx = if fx < x0 as f32 + rad {
                        x0 as f32 + rad - fx
                    } else if fx > x1 as f32 - rad {
                        fx - (x1 as f32 - rad)
                    } else {
                        0.0
                    };
                    let cover = (rad - qx.hypot(qy) + 0.5).clamp(0.0, 1.0);
                    self.span(y, x, x + 1, c, alpha * cover);
                    if x0 + i == x1 - 1 - i {
                        break;
                    }
                }
            }
        }
    }

    /// A filled box with a one-device-pixel outline.
    pub fn frame(&mut self, r: Rect, radius: f32, fill: Rgb, stroke: Rgb) {
        let (x0, y0, x1, y1) = (
            self.dev(r.x),
            self.dev(r.y),
            self.dev(r.right()),
            self.dev(r.bottom()),
        );
        let rad = radius * self.scale;
        self.rbox_dev(x0, y0, x1, y1, rad, stroke, 1.0);
        self.rbox_dev(
            x0 + 1,
            y0 + 1,
            x1 - 1,
            y1 - 1,
            (rad - 1.0).max(0.0),
            fill,
            1.0,
        );
    }

    /// `r` in device px, as the painter rounds it: x0, y0, x1, y1.
    pub fn dev_rect(&self, r: Rect) -> (i32, i32, i32, i32) {
        (
            self.dev(r.x),
            self.dev(r.y),
            self.dev(r.right()),
            self.dev(r.bottom()),
        )
    }

    /// A plain box in device px, blended at `alpha`, for marks the workspace reports in device px.
    pub fn fill_dev(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb, alpha: f32) {
        self.rbox_dev(x0, y0, x1, y1, 0.0, c, alpha);
    }

    /// A rounded box shaded from `top` to `bottom`, for the logo.
    pub fn rbox_gradient(&mut self, r: Rect, radius: f32, top: Rgb, bottom: Rgb) {
        let (x0, y0, x1, y1) = self.dev_rect(r);
        let old = self.clip;
        for y in y0..y1 {
            let t = (y - y0) as f32 / (y1 - y0).max(1) as f32;
            self.clip = (old.0, y.max(old.1), old.2, (y + 1).min(old.3));
            self.rbox_dev(
                x0,
                y0,
                x1,
                y1,
                radius * self.scale,
                mix(top, bottom, t),
                1.0,
            );
        }
        self.clip = old;
    }

    /// A soft drop shadow around `r`, for popups: a few expanding translucent boxes.
    pub fn shadow(&mut self, r: Rect, radius: f32, dark: bool) {
        let steps = 10;
        let strength = if dark { 0.055 } else { 0.022 };
        for i in (1..=steps).rev() {
            let d = i as f32 * 1.6;
            let rr = Rect::new(r.x - d, r.y - d + 4.0, r.w + 2.0 * d, r.h + 2.0 * d);
            self.rbox_a(rr, radius + d, [0, 0, 0], strength);
        }
    }

    /// An antialiased line with round ends, for the glyphs drawn by hand.
    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), thickness: f32, c: Rgb) {
        let s = self.scale;
        let (ax, ay, bx, by, t) = (a.0 * s, a.1 * s, b.0 * s, b.1 * s, thickness * s);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = (dx * dx + dy * dy).max(0.0001);
        let y0 = (ay.min(by) - t).floor() as i32;
        let y1 = (ay.max(by) + t).ceil() as i32;
        let x0 = (ax.min(bx) - t).floor() as i32;
        let x1 = (ax.max(bx) + t).ceil() as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                let (qx, qy) = (x as f32 + 0.5 - ax, y as f32 + 0.5 - ay);
                let u = ((qx * dx + qy * dy) / len2).clamp(0.0, 1.0);
                let d = (qx - u * dx).hypot(qy - u * dy) - t / 2.0;
                self.span(y, x, x + 1, c, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Copies `src` rows `sy..sy + h` to device position `(dx, dy)`, clipped.
    pub fn blit(&mut self, src: &Bitmap, sy: i32, h: i32, dx: i32, dy: i32) {
        let x0 = dx.max(self.clip.0);
        let x1 = (dx + src.width as i32).min(self.clip.2);
        if x0 >= x1 {
            return;
        }
        let n = (x1 - x0) as usize * 4;
        for row in 0..h.min(src.height as i32 - sy) {
            let y = dy + row;
            if y < self.clip.1 || y >= self.clip.3 || sy + row < 0 {
                continue;
            }
            let s = ((sy + row) as usize * src.width as usize + (x0 - dx) as usize) * 4;
            let d = (y as usize * self.bmp.width as usize + x0 as usize) * 4;
            self.bmp.pixels[d..d + n].copy_from_slice(&src.pixels[s..s + n]);
        }
    }

    /// The width of one line of text, in logical px.
    pub fn measure(&mut self, face: Face, size: f32, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        self.gfx
            .layout(face, size * self.scale, [0, 0, 0], 0.0, text)
            .width()
            / self.scale
    }

    /// `text` shortened with an ellipsis so it fits `max` logical px.
    pub fn elide(&mut self, face: Face, size: f32, text: &str, max: f32) -> String {
        if self.measure(face, size, text) <= max {
            return text.to_string();
        }
        // Trailing spaces and dots are dropped before the ellipsis, so "notes.md" never reads
        // "notes.…".
        let cut = |end: usize| {
            format!(
                "{}…",
                text[..end].trim_end_matches(|c: char| c.is_whitespace() || c == '.')
            )
        };
        let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let (mut lo, mut hi) = (0, ends.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if mid < ends.len() && self.measure(face, size, &cut(ends[mid])) <= max {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        cut(ends[lo])
    }

    fn draw_layout(&mut self, layout: &Layout<Brush>, x: f32, y: f32) {
        let (w, h) = (
            layout.width().ceil() as i32 + 2,
            layout.height().ceil() as i32 + 2,
        );
        let (xi, yi) = (x.floor() as i32, y.floor() as i32);
        let c = self.clip;
        if xi >= c.2 || yi >= c.3 || xi + w <= c.0 || yi + h <= c.1 {
            return;
        }
        self.gfx
            .engine
            .draw_layout_clipped(self.bmp, layout, x, y, self.clip);
    }

    /// One line of text starting at `x`, vertically centred on `cy`, elided to `max` logical px.
    /// Returns the width drawn.
    pub fn text(
        &mut self,
        face: Face,
        size: f32,
        x: f32,
        cy: f32,
        text: &str,
        c: Rgb,
        max: f32,
    ) -> f32 {
        if text.is_empty() || max <= 0.0 {
            return 0.0;
        }
        let shown;
        let mut text = text;
        if max.is_finite() && self.measure(face, size, text) > max {
            shown = self.elide(face, size, text, max);
            text = &shown;
        }
        let layout = self.gfx.layout(face, size * self.scale, c, 0.0, text);
        let y = (cy * self.scale - layout.height() / 2.0).round();
        self.draw_layout(&layout, (x * self.scale).round(), y);
        layout.width() / self.scale
    }

    /// One line centred in `r`.
    pub fn text_center(&mut self, face: Face, size: f32, r: Rect, text: &str, c: Rgb) {
        let w = self.measure(face, size, text).min(r.w);
        self.text(face, size, r.x + (r.w - w) / 2.0, r.cy(), text, c, r.w);
    }

    /// One line ending at `right`.
    pub fn text_right(
        &mut self,
        face: Face,
        size: f32,
        right: f32,
        cy: f32,
        text: &str,
        c: Rgb,
    ) -> f32 {
        let w = self.measure(face, size, text);
        self.text(face, size, right - w, cy, text, c, f32::INFINITY);
        w
    }

    /// An icon-font glyph centred on a point.
    pub fn icon(&mut self, glyph: &str, size: f32, cx: f32, cy: f32, c: Rgb) {
        let layout = self
            .gfx
            .layout(Face::Icon, size * self.scale, c, 0.0, glyph);
        let x = (cx * self.scale - layout.width() / 2.0).round();
        let y = (cy * self.scale - layout.height() / 2.0).round();
        self.draw_layout(&layout, x, y);
    }

    /// Wrapped text in a column `w` wide starting at `(x, y)`. Returns the height used.
    pub fn paragraph(
        &mut self,
        face: Face,
        size: f32,
        x: f32,
        y: f32,
        w: f32,
        text: &str,
        c: Rgb,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let layout = self
            .gfx
            .layout(face, size * self.scale, c, (w * self.scale).max(1.0), text);
        self.draw_layout(&layout, (x * self.scale).round(), (y * self.scale).round());
        layout.height() / self.scale
    }

    /// The x offset of each character boundary of `text` (one entry per `char`, plus the end), in
    /// logical px, for carets and selections in a text field.
    pub fn caret_stops(&mut self, face: Face, size: f32, text: &str) -> Vec<f32> {
        let mut stops = Vec::with_capacity(text.len() + 1);
        for (i, _) in text.char_indices() {
            stops.push(self.measure(face, size, &text[..i]));
        }
        stops.push(self.measure(face, size, text));
        stops
    }
}
