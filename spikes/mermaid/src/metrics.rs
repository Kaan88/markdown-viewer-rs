//! Real text metrics for merman's layout: the same font file and the same shaper (harfrust) that
//! resvg paints with, so a measured label box matches the painted text.
//!
//! merman's default measurer is a deterministic estimate for a font we never paint with.

use merman::svg::{DeterministicTextMeasurer, TextMeasurer, TextMetrics, TextStyle, WrapMode};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Lines longer than this are estimated. merman asks about every wrap candidate of a label, so
/// shaping a hostile megabyte-long label on each ask would be quadratic.
const MAX_SHAPED_LINE_BYTES: usize = 4096;
/// Average advance for the estimate, in em. Only hostile input ever reaches it.
const ESTIMATE_ADVANCE_EM: f64 = 0.6;
const MAX_CACHE_ENTRIES: usize = 8192;
const MAX_CACHED_LINE_BYTES: usize = 512;
/// CSS line height of merman's HTML labels.
const HTML_LINE_HEIGHT: f64 = 1.5;

pub struct Face {
    pub data: Arc<Vec<u8>>,
    index: u32,
    /// (ascender - descender) / units per em.
    height_em: f64,
}

impl Face {
    pub fn from_data(data: Arc<Vec<u8>>) -> Result<Self, String> {
        let f = ttf_parser::Face::parse(&data, 0).map_err(|e| e.to_string())?;
        let upem = f64::from(f.units_per_em());
        let height_em = (f64::from(f.ascender()) - f64::from(f.descender())) / upem;
        drop(f);
        Ok(Self { data, index: 0, height_em })
    }
}

/// The four faces of one family.
pub struct Faces {
    pub regular: Face,
    pub bold: Face,
    pub italic: Face,
    pub bold_italic: Face,
}

pub struct FontMeasurer {
    faces: Faces,
    /// Tried in order for characters the primary face lacks, as resvg's fallback does.
    fallback: Vec<Face>,
    /// Width in em per (is_bold, is_italic, line).
    cache: Mutex<HashMap<(bool, bool, String), f64>>,
}

impl FontMeasurer {
    pub fn new(faces: Faces, fallback: Vec<Face>) -> Self {
        Self { faces, fallback, cache: Mutex::new(HashMap::new()) }
    }

    fn face(&self, bold: bool, italic: bool) -> &Face {
        match (bold, italic) {
            (false, false) => &self.faces.regular,
            (true, false) => &self.faces.bold,
            (false, true) => &self.faces.italic,
            (true, true) => &self.faces.bold_italic,
        }
    }

    fn is_italic(style: &TextStyle) -> bool {
        matches!(style.font_style.as_deref().map(str::trim), Some("italic" | "oblique"))
    }

    fn is_bold(style: &TextStyle) -> bool {
        match style.font_weight.as_deref().map(str::trim) {
            Some("bold" | "bolder") => true,
            Some(w) => w.parse::<u32>().is_ok_and(|n| n >= 600),
            None => false,
        }
    }

    /// Width of one line in px, kerning and ligatures included.
    pub fn line_width(&self, line: &str, style: &TextStyle) -> f64 {
        let size = style.font_size.max(1.0);
        if line.is_empty() {
            return 0.0;
        }
        if line.len() > MAX_SHAPED_LINE_BYTES {
            return line.chars().count() as f64 * ESTIMATE_ADVANCE_EM * size;
        }
        let (bold, italic) = (Self::is_bold(style), Self::is_italic(style));
        let cacheable = line.len() <= MAX_CACHED_LINE_BYTES;
        if cacheable {
            if let Some(w) = self.cache.lock().unwrap().get(&(bold, italic, line.to_owned())) {
                return w * size;
            }
        }
        let em = self.shape_em(line, self.face(bold, italic));
        if cacheable {
            let mut c = self.cache.lock().unwrap();
            if c.len() >= MAX_CACHE_ENTRIES {
                c.clear();
            }
            c.insert((bold, italic, line.to_owned()), em);
        }
        em * size
    }

    fn shape_em(&self, line: &str, primary: &Face) -> f64 {
        let faces: Vec<&Face> = std::iter::once(primary).chain(self.fallback.iter()).collect();
        let parsed: Vec<_> = faces.iter().map(|f| ttf_parser::Face::parse(&f.data, f.index).ok()).collect();
        let face_of = |c: char| {
            // Controls and spaces stay with whichever run they are in.
            if c.is_control() {
                return None;
            }
            parsed.iter().position(|p| p.as_ref().is_some_and(|p| p.glyph_index(c).is_some()))
        };
        let mut total = 0.0;
        let mut start = 0;
        let mut cur: Option<usize> = None;
        for (i, c) in line.char_indices() {
            let f = face_of(c).or(cur);
            if f != cur && cur.is_some() {
                total += shape_run(&line[start..i], faces[cur.unwrap()]);
                start = i;
            }
            cur = f.or(cur);
        }
        total + shape_run(&line[start..], faces[cur.unwrap_or(0)])
    }

    fn wrap(&self, line: &str, style: &TextStyle, max: f64, break_long_words: bool) -> (usize, f64) {
        let (mut lines, mut widest, mut cur) = (1usize, 0.0f64, String::new());
        let flush = |cur: &mut String, lines: &mut usize, widest: &mut f64| {
            *widest = widest.max(self.line_width(cur.trim_end(), style));
            *lines += 1;
            cur.clear();
        };
        for word in line.split(' ') {
            let candidate = if cur.is_empty() { word.to_owned() } else { format!("{cur} {word}") };
            if self.line_width(&candidate, style) <= max || cur.is_empty() && !break_long_words {
                cur = candidate;
                continue;
            }
            if !cur.is_empty() {
                flush(&mut cur, &mut lines, &mut widest);
            }
            if break_long_words && self.line_width(word, style) > max {
                for c in word.chars() {
                    cur.push(c);
                    if self.line_width(&cur, style) > max && cur.chars().count() > 1 {
                        cur.pop();
                        flush(&mut cur, &mut lines, &mut widest);
                        cur.push(c);
                    }
                }
            } else {
                cur = word.to_owned();
            }
        }
        widest = widest.max(self.line_width(cur.trim_end(), style));
        (lines, widest)
    }

    fn wrapped(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        mode: WrapMode,
    ) -> (TextMetrics, f64) {
        let size = style.font_size.max(1.0);
        let max = max_width.filter(|w| w.is_finite() && *w > 0.0);
        let (mut line_count, mut width, mut raw) = (0usize, 0.0f64, 0.0f64);
        for line in DeterministicTextMeasurer::normalized_text_lines(text) {
            let line = line.trim();
            let w = self.line_width(line, style);
            raw = raw.max(w);
            match max {
                // Hostile long lines skip the wrap loop; the estimate is enough to bound layout.
                Some(m) if w > m && line.len() <= MAX_SHAPED_LINE_BYTES => {
                    let (n, widest) = self.wrap(line, style, m, mode != WrapMode::HtmlLike);
                    line_count += n;
                    width = width.max(widest);
                }
                Some(m) if w > m => {
                    line_count += (w / m).ceil() as usize;
                    width = width.max(m);
                }
                _ => {
                    line_count += 1;
                    width = width.max(w);
                }
            }
        }
        let face = self.face(Self::is_bold(style), Self::is_italic(style));
        let line_height = match mode {
            WrapMode::HtmlLike => HTML_LINE_HEIGHT,
            // A browser's getBBox on SVG text reports ascent + descent.
            WrapMode::SvgLike | WrapMode::SvgLikeSingleRun => face.height_em,
        };
        let height = line_count as f64 * size * line_height;
        (TextMetrics { width, height, line_count }, raw)
    }
}

fn shape_run(text: &str, face: &Face) -> f64 {
    use harfrust::{FontRef, ShaperData, UnicodeBuffer};
    if text.is_empty() {
        return 0.0;
    }
    let Ok(font) = FontRef::from_index(&face.data, face.index) else {
        return text.chars().count() as f64 * ESTIMATE_ADVANCE_EM;
    };
    let data = ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let mut buf = UnicodeBuffer::new();
    buf.push_str(text);
    buf.guess_segment_properties();
    let out = shaper.shape(buf, harfrust::ShapeOptions::new());
    let advance: i64 = out.glyph_positions().iter().map(|p| i64::from(p.x_advance)).sum();
    advance as f64 / f64::from(shaper.units_per_em())
}

impl TextMeasurer for FontMeasurer {
    fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
        self.wrapped(text, style, None, WrapMode::SvgLike).0
    }

    fn measure_wrapped(&self, text: &str, style: &TextStyle, max_width: Option<f64>, mode: WrapMode) -> TextMetrics {
        self.wrapped(text, style, max_width, mode).0
    }

    fn measure_wrapped_with_raw_width(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        mode: WrapMode,
    ) -> (TextMetrics, Option<f64>) {
        let (m, raw) = self.wrapped(text, style, max_width, mode);
        (m, Some(raw))
    }

    fn measure_html_line_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.line_width(text, style)
    }

    fn measure_html_line_height_px(&self, _text: &str, style: &TextStyle) -> f64 {
        style.font_size.max(1.0) * HTML_LINE_HEIGHT
    }

    fn measure_canvas_text_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.line_width(&text.replace(['\t', '\r', '\n', '\x0c'], " "), style)
    }

    fn measure_svg_raw_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.line_width(text, style)
    }

    fn measure_svg_preserved_tspan_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.line_width(text, style)
    }
}
