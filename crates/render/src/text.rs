//! CPU text: parley shapes and lays out, swash rasterizes, this module blends. Ported from the
//! Phase 0 text spike (`spikes/text`), which the user accepted after the subpixel fix. The glyph
//! cache is byte-capped, and text uses RGB subpixel masks: `Format::CustomSubpixel([0.3, 0.0,
//! -0.3])` is deliberate, because zeno's `Format::Subpixel` swaps red and blue for RGB stripes.

use crate::bitmap::Bitmap;
use parley::{FontData, Layout, PositionedLayoutItem};
use std::collections::{BTreeMap, HashMap};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::{CacheKey, FontRef};

pub type Brush = [u8; 4];

/// Per-channel preblend tables, so glyph coverage becomes colour without a separate gamma pass.
struct Lut(Box<[[u8; 256]; 8]>);

impl Lut {
    /// The gamma and contrast the text spike settled on.
    fn new(gamma: f32, contrast: f32) -> Self {
        let mut table = Box::new([[0u8; 256]; 8]);
        for (bucket, tab) in table.iter_mut().enumerate() {
            let src = bucket as f32 / 7.0;
            let dst = 1.0 - src;
            let (ls, ld) = (src.powf(gamma), dst.powf(gamma));
            let adj = contrast * ld;
            for (i, v) in tab.iter_mut().enumerate() {
                let raw = i as f32 / 255.0;
                let a = raw + (1.0 - raw) * adj * raw;
                let r = if (dst - src).abs() < 1.0 / 256.0 {
                    a
                } else {
                    let out = (ls * a + ld * (1.0 - a)).powf(1.0 / gamma);
                    (out - dst) / (src - dst)
                };
                *v = (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        Lut(table)
    }

    #[inline]
    fn channel(&self, c: u8) -> &[u8; 256] {
        &self.0[(c >> 5) as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    font: u64,
    index: u32,
    glyph: u16,
    size: u32,
    sub: u8,
    coords: u64,
}

struct Entry {
    left: i32,
    top: i32,
    w: u32,
    h: u32,
    color: bool,
    data: Box<[u8]>,
    tick: u64,
}

/// Map, LRU node and box overhead per cached glyph, so the cap bounds real memory.
const ENTRY_OVERHEAD: usize = 96;

#[derive(Default, Clone, Copy, Debug)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub bytes: usize,
    pub peak_bytes: usize,
    pub entries: usize,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f32 / total as f32
        }
    }
}

pub struct TextEngine {
    scaler: ScaleContext,
    fonts: HashMap<(u64, u32), (FontData, u32, CacheKey)>,
    map: HashMap<Key, Entry>,
    lru: BTreeMap<u64, Key>,
    tick: u64,
    cap: usize,
    pub stats: CacheStats,
    lut: Lut,
}

impl TextEngine {
    /// `cap` bounds the raster cache in bytes. The plan's hard cap is 4 MB.
    pub fn new(cap: usize) -> Self {
        TextEngine {
            scaler: ScaleContext::new(),
            fonts: HashMap::new(),
            map: HashMap::new(),
            lru: BTreeMap::new(),
            tick: 0,
            cap,
            stats: CacheStats::default(),
            lut: Lut::new(1.4, 0.5),
        }
    }

    fn raster(&mut self, key: Key, font: &FontData, coords: &[i16], size: f32) -> Entry {
        let cache_key = (font.data.id(), font.index);
        let (data, offset, font_key) = self.fonts.entry(cache_key).or_insert_with(|| {
            let r = FontRef::from_index(font.data.as_ref(), font.index as usize).expect("font");
            (font.clone(), r.offset, r.key)
        });
        let font_ref = FontRef {
            data: data.data.as_ref(),
            offset: *offset,
            key: *font_key,
        };
        let mut scaler = self
            .scaler
            .builder(font_ref)
            .size(size)
            .hint(false)
            .normalized_coords(coords.iter().copied())
            .build();
        let image = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ])
        // RGB order: zeno's `Format::Subpixel` was measured to swap red and blue against Qt.
        .format(Format::CustomSubpixel([0.3, 0.0, -0.3]))
        .offset(Vector::new(key.sub as f32 / 4.0, 0.0))
        .render(&mut scaler, key.glyph);
        let tick = self.tick;
        let Some(image) = image else {
            return Entry {
                left: 0,
                top: 0,
                w: 0,
                h: 0,
                color: false,
                data: Box::new([]),
                tick,
            };
        };
        let (w, h) = (image.placement.width, image.placement.height);
        let (color, data): (bool, Box<[u8]>) = match image.content {
            Content::Color => (true, image.data.into_boxed_slice()),
            Content::SubpixelMask => {
                let mut data = Vec::with_capacity((w * h * 3) as usize);
                for pixel in image.data.chunks_exact(4) {
                    data.extend_from_slice(&pixel[..3]);
                }
                (false, data.into_boxed_slice())
            }
            Content::Mask => {
                let mut data = Vec::with_capacity((w * h * 3) as usize);
                for &a in &image.data {
                    data.extend_from_slice(&[a, a, a]);
                }
                (false, data.into_boxed_slice())
            }
        };
        Entry {
            left: image.placement.left,
            top: image.placement.top,
            w,
            h,
            color,
            data,
            tick,
        }
    }

    fn evict(&mut self) {
        while self.stats.bytes > self.cap {
            let Some((&tick, &key)) = self.lru.iter().next() else {
                break;
            };
            self.lru.remove(&tick);
            if let Some(entry) = self.map.remove(&key) {
                self.stats.bytes -= entry.data.len() + ENTRY_OVERHEAD;
                self.stats.evictions += 1;
                self.stats.entries -= 1;
            }
        }
    }

    /// Draws a laid-out block at `(ox, oy)`, in device px. Only glyphs are drawn; the caller draws
    /// chips, underlines and rules.
    pub fn draw_layout(&mut self, fb: &mut Bitmap, layout: &Layout<Brush>, ox: f32, oy: f32) {
        let clip = (0, 0, fb.width as i32, fb.height as i32);
        self.draw_layout_clipped(fb, layout, ox, oy, clip);
    }

    /// [`draw_layout`](Self::draw_layout) that touches only the pixels inside `clip`, given in
    /// device px as `(x0, y0, x1, y1)` (end-exclusive, intersected with the bitmap). The pixels it
    /// writes are exactly the ones an unclipped draw writes inside `clip`.
    pub fn draw_layout_clipped(
        &mut self,
        fb: &mut Bitmap,
        layout: &Layout<Brush>,
        ox: f32,
        oy: f32,
        clip: (i32, i32, i32, i32),
    ) {
        let clip = (
            clip.0.max(0),
            clip.1.max(0),
            clip.2.min(fb.width as i32),
            clip.3.min(fb.height as i32),
        );
        if clip.0 >= clip.2 || clip.1 >= clip.3 {
            return;
        }
        for line in layout.lines() {
            let metrics = line.metrics();
            if oy + metrics.baseline + metrics.line_height < clip.1 as f32
                || oy + metrics.baseline - metrics.line_height > clip.3 as f32
            {
                continue;
            }
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let font = run.font().clone();
                let size = run.font_size();
                let coords: Vec<i16> = run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect();
                let coord_hash = coords.iter().fold(0u64, |h, c| {
                    (h ^ (*c as u16 as u64)).wrapping_mul(0x0000_0100_0000_01b3)
                });
                let brush = glyph_run.style().brush;
                for glyph in glyph_run.positioned_glyphs() {
                    let (x, y) = (ox + glyph.x, (oy + glyph.y).round());
                    let floor_x = x.floor();
                    let sub = (((x - floor_x) * 4.0) as u8).min(3);
                    let key = Key {
                        font: font.data.id(),
                        index: font.index,
                        glyph: glyph.id as u16,
                        size: (size * 64.0) as u32,
                        sub,
                        coords: coord_hash,
                    };
                    self.tick += 1;
                    let tick = self.tick;
                    if let Some(entry) = self.map.get_mut(&key) {
                        self.lru.remove(&entry.tick);
                        entry.tick = tick;
                        self.lru.insert(tick, key);
                        self.stats.hits += 1;
                    } else {
                        self.stats.misses += 1;
                        let entry = self.raster(key, &font, &coords, size);
                        self.stats.bytes += entry.data.len() + ENTRY_OVERHEAD;
                        self.stats.entries += 1;
                        self.stats.peak_bytes = self.stats.peak_bytes.max(self.stats.bytes);
                        self.map.insert(key, entry);
                        self.lru.insert(tick, key);
                        self.evict();
                    }
                    let Some(entry) = self.map.get(&key) else {
                        continue;
                    };
                    blit(fb, clip, entry, floor_x as i32, y as i32, brush, &self.lut);
                }
            }
        }
    }
}

#[inline]
fn blit(
    fb: &mut Bitmap,
    clip: (i32, i32, i32, i32),
    entry: &Entry,
    gx: i32,
    gy: i32,
    brush: Brush,
    lut: &Lut,
) {
    if entry.w == 0 || entry.h == 0 {
        return;
    }
    let (x0, y0) = (gx + entry.left, gy - entry.top);
    let (w, h) = (entry.w as i32, entry.h as i32);
    let (lr, lg, lb) = (
        lut.channel(brush[0]),
        lut.channel(brush[1]),
        lut.channel(brush[2]),
    );
    let bpp = if entry.color { 4 } else { 3 };
    let cs = x0.max(clip.0);
    let ce = (x0 + w).min(clip.2);
    for row in y0.max(clip.1)..(y0 + h).min(clip.3) {
        if ce <= cs {
            continue;
        }
        let so = ((row - y0) * w + (cs - x0)) as usize * bpp;
        let d0 = (row as usize * fb.width as usize + cs as usize) * 4;
        let n = (ce - cs) as usize;
        let src = &entry.data[so..so + n * bpp];
        let dst = &mut fb.pixels[d0..d0 + n * 4];
        if entry.color {
            for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                let a = s[3] as u32;
                if a == 0 {
                    continue;
                }
                let ia = 255 - a;
                // swash emits straight-alpha RGBA; the framebuffer is BGRA.
                d[0] = ((s[2] as u32 * a + d[0] as u32 * ia + 127) / 255) as u8;
                d[1] = ((s[1] as u32 * a + d[1] as u32 * ia + 127) / 255) as u8;
                d[2] = ((s[0] as u32 * a + d[2] as u32 * ia + 127) / 255) as u8;
            }
        } else {
            for (s, d) in src.chunks_exact(3).zip(dst.chunks_exact_mut(4)) {
                if s[0] | s[1] | s[2] == 0 {
                    continue;
                }
                let (ar, ag, ab) = (
                    lr[s[0] as usize] as u32,
                    lg[s[1] as usize] as u32,
                    lb[s[2] as usize] as u32,
                );
                d[0] = ((brush[2] as u32 * ab + d[0] as u32 * (255 - ab) + 127) / 255) as u8;
                d[1] = ((brush[1] as u32 * ag + d[1] as u32 * (255 - ag) + 127) / 255) as u8;
                d[2] = ((brush[0] as u32 * ar + d[2] as u32 * (255 - ar) + 127) / 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parley::{Alignment, AlignmentOptions, FontFamily, LayoutContext, StyleProperty};

    /// A wrapped paragraph in the UI's sans face, `size` device px, `width` device px wide.
    pub(super) fn paragraph(text: &str, size: f32, width: f32, brush: Brush) -> Layout<Brush> {
        let mut fonts = crate::layout::shared_fonts();
        let mut layouts = LayoutContext::<Brush>::new();
        let mut b = layouts.ranged_builder(&mut fonts, text, 1.0, true);
        b.push_default(StyleProperty::FontFamily(FontFamily::from(
            "Segoe UI, Noto Sans, DejaVu Sans, sans-serif",
        )));
        b.push_default(StyleProperty::FontSize(size));
        b.push_default(StyleProperty::Brush(brush));
        let mut layout = b.build(text);
        layout.break_all_lines(Some(width));
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }

    const SAMPLE: &str = "The quick brown fox jumps over the lazy dog. Pack my box with five dozen \
        liquor jugs! 0123456789 (sphinx of black quartz, judge my vow) — Ünïcödé façade, 日本語. ";

    #[test]
    fn a_clipped_draw_is_the_unclipped_draw_cropped() {
        let layout = paragraph(&SAMPLE.repeat(3), 17.0, 300.0, [20, 90, 200, 255]);
        let (w, h) = (360u32, layout.height().ceil() as u32 + 30);
        let background = |bm: &mut Bitmap| {
            // A non-uniform background, so a write outside the clip cannot hide.
            for (i, p) in bm.pixels.chunks_exact_mut(4).enumerate() {
                p.copy_from_slice(&[(i * 7) as u8, (i / 3) as u8, 200, 255]);
            }
        };
        let mut engine = TextEngine::new(4 << 20);
        let mut full = Bitmap::new(w, h);
        background(&mut full);
        engine.draw_layout(&mut full, &layout, 7.6, 11.0);
        let clips = [
            (0, 0, w as i32, h as i32),
            (13, 9, 141, 47),
            (-50, -50, 90, 4000),
            (200, 30, 9999, 60),
            (50, 50, 50, 80),
            (100, 100, 40, 40),
        ];
        for clip in clips {
            let mut clipped = Bitmap::new(w, h);
            background(&mut clipped);
            let before = clipped.pixels.clone();
            engine.draw_layout_clipped(&mut clipped, &layout, 7.6, 11.0, clip);
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let o = (y as usize * w as usize + x as usize) * 4;
                    let inside = x >= clip.0 && x < clip.2 && y >= clip.1 && y < clip.3;
                    let want = if inside { &full.pixels } else { &before };
                    assert_eq!(
                        clipped.pixels[o..o + 4],
                        want[o..o + 4],
                        "clip {clip:?} at {x},{y}"
                    );
                }
            }
        }
    }

    /// Not a correctness test: `cargo test --release -p mdview-render text_draw_speed -- --ignored
    /// --nocapture` prints the time per glyph of a warm-cache draw.
    #[test]
    #[ignore]
    fn text_draw_speed() {
        let text = SAMPLE.repeat(40);
        let layout = paragraph(&text, 16.0, 1100.0, [30, 30, 30, 255]);
        let mut fb = Bitmap::new(1200, layout.height().ceil() as u32 + 20);
        let mut engine = TextEngine::new(4 << 20);
        engine.draw_layout(&mut fb, &layout, 10.3, 5.0);
        let glyphs: usize = layout
            .lines()
            .flat_map(|l| l.items())
            .map(|i| match i {
                PositionedLayoutItem::GlyphRun(r) => r.positioned_glyphs().count(),
                _ => 0,
            })
            .sum();
        for _ in 0..3 {
            let n = 300;
            let t = std::time::Instant::now();
            for _ in 0..n {
                fb.clear([255, 255, 255]);
                engine.draw_layout(&mut fb, &layout, 10.3, 5.0);
            }
            let total = t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            for _ in 0..n {
                fb.clear([255, 255, 255]);
            }
            let clear = t.elapsed().as_secs_f64();
            println!(
                "{glyphs} glyphs: {:.1} ns/glyph (draw only, clear subtracted)",
                (total - clear) / (n * glyphs) as f64 * 1e9
            );
        }
    }
}
