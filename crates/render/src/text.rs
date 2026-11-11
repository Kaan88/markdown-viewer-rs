//! CPU text: parley shapes and lays out, swash rasterizes, this module blends. Ported from the
//! Phase 0 text spike (`spikes/text`), which the user accepted after the subpixel fix. The glyph
//! cache is byte-capped, and text uses RGB subpixel masks: `Format::CustomSubpixel([0.3, 0.0,
//! -0.3])` is deliberate, because zeno's `Format::Subpixel` swaps red and blue for RGB stripes.

use crate::bitmap::Bitmap;
use parley::{FontData, Layout, PositionedLayoutItem};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
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
    /// The engine's tick when this glyph was last drawn, for least-recently-used eviction.
    last_used: u64,
}

/// Map slot and box overhead per cached glyph, so the cap bounds real memory.
const ENTRY_OVERHEAD: usize = 96;

/// Multiply-rotate hashing (the FxHash scheme) for the glyph key, which is a handful of integers
/// built by this module: SipHash's flood resistance buys nothing here and cost a visible share of
/// every cached glyph draw.
#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.write_u64(n as u64);
    }
    #[inline]
    fn write_u16(&mut self, n: u16) {
        self.write_u64(n as u64);
    }
    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.write_u64(n as u64);
    }
    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

type GlyphMap = HashMap<Key, Entry, BuildHasherDefault<KeyHasher>>;

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
    map: GlyphMap,
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
            map: GlyphMap::default(),
            tick: 0,
            cap,
            stats: CacheStats::default(),
            lut: Lut::new(1.4, 0.5),
        }
    }

    fn raster(
        &mut self,
        key: Key,
        font: &FontData,
        coords: impl Iterator<Item = i16>,
        size: f32,
    ) -> Entry {
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
            .normalized_coords(coords)
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
                last_used: tick,
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
            last_used: tick,
        }
    }

    /// Called when the cache is over `cap`: drops the least recently drawn glyphs until it is at
    /// three quarters of the cap. Eviction is rare (a warm document fits in a fraction of the cap),
    /// so this sorts the entries by last use instead of keeping an LRU order on every draw, and the
    /// quarter of headroom keeps a cache at its cap from paying the sort on every miss.
    fn evict(&mut self) {
        if self.stats.bytes <= self.cap {
            return;
        }
        let target = self.cap - self.cap / 4;
        let mut order: Vec<(u64, Key)> = self.map.iter().map(|(k, e)| (e.last_used, *k)).collect();
        order.sort_unstable_by_key(|&(tick, _)| tick);
        for (_, key) in order {
            if self.stats.bytes <= target {
                break;
            }
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
                // Borrowed, not cloned: the font is only needed (and cloned once into `fonts`)
                // when a glyph misses the cache.
                let font = run.font();
                let size = run.font_size();
                let coords = run.normalized_coords();
                let coord_hash = coords.iter().fold(0u64, |h, c| {
                    (h ^ (c.to_bits() as u16 as u64)).wrapping_mul(0x0000_0100_0000_01b3)
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
                        entry.last_used = tick;
                        self.stats.hits += 1;
                        blit(fb, clip, entry, floor_x as i32, y as i32, brush, &self.lut);
                        continue;
                    }
                    self.stats.misses += 1;
                    let entry = self.raster(key, font, coords.iter().map(|c| c.to_bits()), size);
                    self.stats.bytes += entry.data.len() + ENTRY_OVERHEAD;
                    self.stats.entries += 1;
                    self.stats.peak_bytes = self.stats.peak_bytes.max(self.stats.bytes);
                    self.map.insert(key, entry);
                    self.evict();
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

    #[test]
    fn a_small_cache_stays_under_its_cap_and_draws_the_same() {
        let layouts: Vec<_> = [14.0, 18.0, 23.0, 31.0]
            .iter()
            .map(|&size| paragraph(&SAMPLE.repeat(2), size, 600.0, [0, 0, 0, 255]))
            .collect();
        let h = layouts
            .iter()
            .map(|l| l.height().ceil() as u32)
            .sum::<u32>()
            + 40;
        let draw_all = |engine: &mut TextEngine, check_cap: bool| {
            let mut fb = Bitmap::new(640, h);
            fb.clear([255, 255, 255]);
            let mut y = 5.0;
            for layout in &layouts {
                engine.draw_layout(&mut fb, layout, 3.3, y);
                y += layout.height().ceil() + 8.0;
                if check_cap {
                    assert!(engine.stats.bytes <= engine.cap, "{:?}", engine.stats);
                }
                assert_eq!(engine.stats.entries, engine.map.len());
            }
            fb
        };
        let mut roomy = TextEngine::new(16 << 20);
        let expected = draw_all(&mut roomy, true);
        assert_eq!(roomy.stats.evictions, 0);
        let total = roomy.stats.bytes;

        let cap = total * 2 / 3;
        let mut tight = TextEngine::new(cap);
        for _ in 0..3 {
            let got = draw_all(&mut tight, true);
            assert!(got.pixels == expected.pixels, "eviction changed the output");
        }
        assert!(tight.stats.evictions > 0);
        assert!(
            tight.stats.peak_bytes <= cap + 64 * 1024,
            "{:?}",
            tight.stats
        );

        // The glyphs drawn last are the ones kept: redrawing only the last layout hits.
        let mut fb = Bitmap::new(640, h);
        let misses = tight.stats.misses;
        tight.draw_layout(&mut fb, layouts.last().unwrap(), 3.3, 5.0);
        assert_eq!(tight.stats.misses, misses, "{:?}", tight.stats);
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
