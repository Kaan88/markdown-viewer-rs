//! Throwaway CPU text renderer: parley layout, swash raster, LCD masks, 1/4 px
//! horizontal positioning, gamma/contrast LUT, byte-capped LRU glyph cache.
use parley::{FontData, Layout, PositionedLayoutItem};
use std::collections::{BTreeMap, HashMap};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::{CacheKey, FontRef};

pub mod spec {
    include!("specimen_data.rs");
}

pub type Brush = [u8; 4];

/// Skia-style preblend: alpha -> alpha, per text channel luminance bucket.
pub struct Lut(Box<[[u8; 256]; 8]>);
impl Lut {
    pub fn new(gamma: f32, contrast: f32) -> Self {
        let mut t = Box::new([[0u8; 256]; 8]);
        for (b, tab) in t.iter_mut().enumerate() {
            let src = b as f32 / 7.0;
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
        Lut(t)
    }
    #[inline]
    fn tab(&self, c: u8) -> &[u8; 256] {
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
const ENTRY_OVERHEAD: usize = 96; // estimated map + LRU node + Box overhead

#[derive(Default, Clone, Copy, Debug)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub bytes: usize,
    pub peak_bytes: usize,
    pub entries: usize,
}

pub struct Renderer {
    sc: ScaleContext,
    fonts: HashMap<(u64, u32), (FontData, u32, CacheKey)>,
    map: HashMap<Key, Entry>,
    lru: BTreeMap<u64, Key>,
    tick: u64,
    cap: usize,
    pub stats: CacheStats,
    lut: Lut,
}

pub struct Fb {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>, // BGRA
}
impl Fb {
    pub fn new(w: usize, h: usize) -> Self {
        Fb { w, h, px: vec![0; w * h * 4] }
    }
    pub fn clear(&mut self, rgb: [u8; 3]) {
        let p = [rgb[2], rgb[1], rgb[0], 255];
        for c in self.px.chunks_exact_mut(4) {
            c.copy_from_slice(&p);
        }
    }
    pub fn write_png(&self, path: &str) {
        let f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut e = png::Encoder::new(f, self.w as u32, self.h as u32);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        let mut rgb = Vec::with_capacity(self.w * self.h * 3);
        for c in self.px.chunks_exact(4) {
            rgb.extend_from_slice(&[c[2], c[1], c[0]]);
        }
        w.write_image_data(&rgb).unwrap();
    }
}

impl Renderer {
    pub fn new(cap: usize) -> Self {
        Renderer {
            sc: ScaleContext::new(),
            fonts: HashMap::new(),
            map: HashMap::new(),
            lru: BTreeMap::new(),
            tick: 0,
            cap,
            stats: CacheStats::default(),
            lut: match std::env::var("LUT").as_deref() {
                Ok("off") => Lut::new(1.0, 0.0),
                Ok("soft") => Lut::new(1.2, 0.2),
                _ => Lut::new(1.4, 0.5),
            },
        }
    }

    fn raster(&mut self, k: Key, font: &FontData, coords: &[i16], size: f32) -> Entry {
        let fk = (font.data.id(), font.index);
        let (fd, off, ck) = self.fonts.entry(fk).or_insert_with(|| {
            let r = FontRef::from_index(font.data.as_ref(), font.index as usize).expect("font");
            (font.clone(), r.offset, r.key)
        });
        let fr = FontRef { data: fd.data.as_ref(), offset: *off, key: *ck };
        let mut scaler = self
            .sc
            .builder(fr)
            .size(size)
            .hint(std::env::var("HINT").is_ok())
            .normalized_coords(coords.iter().copied())
            .build();
        let img = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ])
        // zeno's Format::Subpixel shifts the outline the wrong way for RGB stripes:
        // measured against Qt, it swaps red and blue. This offset order is RGB.
        .format(if std::env::var("OLD_ORDER").is_ok() { Format::Subpixel } else { Format::CustomSubpixel([0.3, 0.0, -0.3]) })
        .offset(Vector::new(k.sub as f32 / 4.0, 0.0))
        .render(&mut scaler, k.glyph);
        let tick = self.tick;
        let Some(img) = img else {
            return Entry { left: 0, top: 0, w: 0, h: 0, color: false, data: Box::new([]), tick };
        };
        let (w, h) = (img.placement.width, img.placement.height);
        let (color, data): (bool, Box<[u8]>) = match img.content {
            Content::Color => (true, img.data.into_boxed_slice()),
            Content::SubpixelMask => {
                // Keep 3 bytes per pixel; the 4th is unused.
                let mut d = Vec::with_capacity((w * h * 3) as usize);
                for c in img.data.chunks_exact(4) {
                    d.extend_from_slice(&c[..3]);
                }
                (false, d.into_boxed_slice())
            }
            Content::Mask => {
                let mut d = Vec::with_capacity((w * h * 3) as usize);
                for &a in &img.data {
                    d.extend_from_slice(&[a, a, a]);
                }
                (false, d.into_boxed_slice())
            }
        };
        Entry { left: img.placement.left, top: img.placement.top, w, h, color, data, tick }
    }

    fn evict(&mut self) {
        while self.stats.bytes > self.cap {
            let Some((&t, &k)) = self.lru.iter().next() else { break };
            self.lru.remove(&t);
            if let Some(e) = self.map.remove(&k) {
                self.stats.bytes -= e.data.len() + ENTRY_OVERHEAD;
                self.stats.evictions += 1;
                self.stats.entries -= 1;
            }
        }
    }

    pub fn draw_layout(&mut self, fb: &mut Fb, layout: &Layout<Brush>, ox: f32, oy: f32) {
        for line in layout.lines() {
            let m = line.metrics();
            if oy + m.baseline + m.line_height < 0.0 || oy + m.baseline - m.line_height > fb.h as f32 {
                continue;
            }
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(gr) = item else { continue };
                let run = gr.run();
                let font = run.font().clone();
                let size = run.font_size();
                let coords: Vec<i16> =
                    run.normalized_coords().iter().map(|c| c.to_bits()).collect();
                let chash = coords.iter().fold(0u64, |h, c| {
                    (h ^ (*c as u16 as u64)).wrapping_mul(0x100000001b3)
                });
                let b = gr.style().brush;
                for g in gr.positioned_glyphs() {
                    let (x, y) = (ox + g.x, (oy + g.y).round());
                    let fx = x.floor();
                    let sub = (((x - fx) * 4.0) as u8).min(3);
                    let k = Key {
                        font: font.data.id(),
                        index: font.index,
                        glyph: g.id as u16,
                        size: (size * 64.0) as u32,
                        sub,
                        coords: chash,
                    };
                    self.tick += 1;
                    let tick = self.tick;
                    if let Some(e) = self.map.get_mut(&k) {
                        self.lru.remove(&e.tick);
                        e.tick = tick;
                        self.lru.insert(tick, k);
                        self.stats.hits += 1;
                    } else {
                        self.stats.misses += 1;
                        let e = self.raster(k, &font, &coords, size);
                        self.stats.bytes += e.data.len() + ENTRY_OVERHEAD;
                        self.stats.entries += 1;
                        self.stats.peak_bytes = self.stats.peak_bytes.max(self.stats.bytes);
                        self.map.insert(k, e);
                        self.lru.insert(tick, k);
                        self.evict();
                    }
                    let Some(e) = self.map.get(&k) else { continue };
                    blit(fb, e, fx as i32, y as i32, b, &self.lut);
                }
            }
        }
    }
}

#[inline]
fn blit(fb: &mut Fb, e: &Entry, gx: i32, gy: i32, b: Brush, lut: &Lut) {
    if e.w == 0 || e.h == 0 {
        return;
    }
    let (x0, y0) = (gx + e.left, gy - e.top);
    let (w, h) = (e.w as i32, e.h as i32);
    let (fw, fh) = (fb.w as i32, fb.h as i32);
    let (lr, lg, lb) = (lut.tab(b[0]), lut.tab(b[1]), lut.tab(b[2]));
    let bpp = if e.color { 4 } else { 3 };
    for row in y0.max(0)..(y0 + h).min(fh) {
        let cs = x0.max(0);
        let ce = (x0 + w).min(fw);
        if ce <= cs {
            return;
        }
        let so = ((row - y0) * w + (cs - x0)) as usize * bpp;
        let d0 = (row as usize * fb.w + cs as usize) * 4;
        let n = (ce - cs) as usize;
        let src = &e.data[so..so + n * bpp];
        let dst = &mut fb.px[d0..d0 + n * 4];
        if e.color {
            for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                let a = s[3] as u32;
                if a == 0 {
                    continue;
                }
                let ia = 255 - a;
                // Assumes swash emits straight alpha RGBA.
                d[0] = ((s[2] as u32 * a + d[0] as u32 * ia + 127) / 255) as u8;
                d[1] = ((s[1] as u32 * a + d[1] as u32 * ia + 127) / 255) as u8;
                d[2] = ((s[0] as u32 * a + d[2] as u32 * ia + 127) / 255) as u8;
            }
        } else {
            for (s, d) in src.chunks_exact(3).zip(dst.chunks_exact_mut(4)) {
                if s[0] | s[1] | s[2] == 0 {
                    continue;
                }
                let (ar, ag, ab) =
                    (lr[s[0] as usize] as u32, lg[s[1] as usize] as u32, lb[s[2] as usize] as u32);
                d[0] = ((b[2] as u32 * ab + d[0] as u32 * (255 - ab) + 127) / 255) as u8;
                d[1] = ((b[1] as u32 * ag + d[1] as u32 * (255 - ag) + 127) / 255) as u8;
                d[2] = ((b[0] as u32 * ar + d[2] as u32 * (255 - ar) + 127) / 255) as u8;
            }
        }
    }
}
