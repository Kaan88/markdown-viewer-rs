//! Estimated block heights, for a layout that is ready before anything is shaped.
//!
//! A first layout of a large document used to shape every paragraph before the first frame. Now
//! each top-level block gets a height from its kind, its text length and the layout constants the
//! [`Pager`] uses for that kind; the blocks a frame shows are measured exactly, and refinement
//! corrects the rest in the background (`refine.rs`). An estimate only steers the scroll bar and
//! where an off-screen block lands until it is measured, so the aim is a total that is close and
//! cheap, not a per-block truth.
//!
//! Text wraps at about `width / advance` characters per line, where the advance is the average
//! glyph width of the theme's fonts, taken once per scale by shaping a sample sentence
//! ([`Metrics`]). Images use their stored size when the store has them (a relayout after a resize
//! or zoom finds every image it drew), their declared size, or a typical box. Nothing here
//! decodes, runs an engine job or asks for a remote image.

use super::containers::{DEFINITION_GAP, DEFINITION_INDENT};
use super::inline::{Inline, Style};
use super::media::{GUESS_DIAGRAM, GUESS_IMAGE, GUESS_MATH};
use super::pager::{Pager, Txt};
use super::store::Found;
use mdview_core::model::{BlockKind, Footnote, NONE, SpanKind};

/// Lines are rarely filled to the edge: a word that does not fit moves down whole.
const FILL: f32 = 0.92;

/// A sentence with the letter mix of running English text, for the average advance.
const SAMPLE: &str = "The quick brown fox jumps over the lazy dog, and then the reader keeps \
                      going through a paragraph of ordinary words with numbers like 2024.";
const SAMPLE_MONO: &str = "fn main() { let x = 42; }";

/// Average glyph advances, in device px per px of font size at the layout's scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Metrics {
    sans: f32,
    mono: f32,
}

impl Metrics {
    /// Shapes the two samples once; the renderer keeps the result per scale and theme.
    pub(super) fn measure(pager: &mut Pager) -> Metrics {
        let mut advance = |sample: &str, mono: bool| {
            let mut inline = Inline::default();
            inline.push(sample, Style::default());
            let size = pager.theme.body_size;
            let txt = Txt {
                size,
                line_height: 1.0,
                weight: 400.0,
                mono,
                color: pager.theme.text,
                max: None,
            };
            let width = pager.text_layout(&inline, &txt).width();
            width / (sample.chars().count() as f32 * size * pager.scale)
        };
        Metrics {
            sans: advance(SAMPLE, false),
            mono: advance(SAMPLE_MONO, true),
        }
    }
}

impl<'a, 'b> Pager<'a, 'b> {
    /// The estimated height of block `b` laid out at `width`, as [`block`](Self::block) would
    /// advance `y`.
    pub(super) fn estimate(&mut self, m: &Metrics, b: u32, width: f32) -> f32 {
        let block = self.doc.blocks[b as usize];
        match block.kind {
            BlockKind::Paragraph { .. } => self.estimate_text(m, b, width, 16.0),
            BlockKind::Plain => self.estimate_text(m, b, width, 4.0),
            BlockKind::Heading { level, .. } => {
                let (size, top, bottom) = match level {
                    1 => (34.0, 0.0, 14.0),
                    2 => (24.0, 22.0, 12.0),
                    3 => (18.0, 12.0, 8.0),
                    4 => (17.0, 10.0, 8.0),
                    5 => (16.0, 8.0, 6.0),
                    _ => (15.0, 8.0, 6.0),
                };
                let text = self.doc.text_of(block.text);
                if text.trim().is_empty() {
                    return 0.0;
                }
                // Semibold runs a little wider than the sample.
                let lines = self.lines(text, m.sans * 1.05, size, width);
                self.px(top) + (lines * size * 1.25 * self.scale).round() + self.px(bottom)
            }
            BlockKind::BlockQuote | BlockKind::Alert(_) => {
                let alert = matches!(block.kind, BlockKind::Alert(_));
                let pad = self.px(if alert { 12.0 } else { 2.0 });
                let inner = width - self.px(18.0) - self.px(14.0);
                let mut height = pad;
                if alert {
                    height += (self.theme.body_size * 1.3 * self.scale).round() + self.px(4.0);
                }
                height += self.estimate_children(m, b, inner);
                height.max(self.px(8.0)) + pad + self.px(16.0)
            }
            BlockKind::List { .. } => {
                let items: Vec<u32> = self.doc.children(b).collect();
                let indent = self.px(26.0);
                let count = items.len();
                let mut height = 0.0;
                for (index, item) in items.into_iter().enumerate() {
                    height += self.estimate_children(m, item, width - indent);
                    height += self.px(if index + 1 < count { 4.0 } else { 16.0 });
                }
                height
            }
            BlockKind::ListItem { .. }
            | BlockKind::Div { .. }
            | BlockKind::DefTerm
            | BlockKind::FigCaption
            | BlockKind::TableRow { .. }
            | BlockKind::TableCell { .. } => self.estimate_children(m, b, width),
            BlockKind::DefDesc => {
                let indent = self.px(DEFINITION_INDENT);
                let gap = if self.ends_in_plain(b) {
                    self.px(DEFINITION_GAP)
                } else {
                    0.0
                };
                self.estimate_children(m, b, width - indent) + gap
            }
            BlockKind::Figure { .. } => self.estimate_children(m, b, width) + self.px(12.0),
            BlockKind::CodeBlock { .. } => self.estimate_code(m, b, width),
            BlockKind::Diagram { oversize, .. } => self
                .estimate_diagram(b, width, oversize.is_some())
                .unwrap_or_else(|| self.estimate_code(m, b, width)),
            BlockKind::Math { oversize } => self
                .estimate_math(b, width, oversize.is_some())
                .unwrap_or_else(|| self.estimate_code(m, b, width)),
            BlockKind::ThematicBreak => self.px(8.0) + self.hair() + self.px(16.0),
            BlockKind::Table => self.estimate_table(m, b, width),
            BlockKind::Details { .. } => {
                let mut height = 0.0;
                let children: Vec<u32> = self.doc.children(b).collect();
                for &child in &children {
                    if matches!(self.doc.blocks[child as usize].kind, BlockKind::Summary) {
                        let text = self.doc.text_of(self.doc.blocks[child as usize].text);
                        let lines = self.lines(text, m.sans * 1.05, self.theme.body_size, width);
                        height += self.line_px(lines, self.theme.body_size) + self.px(4.0);
                    }
                }
                if self.details_open(b) {
                    for child in children {
                        if !matches!(self.doc.blocks[child as usize].kind, BlockKind::Summary) {
                            height += self.estimate(m, child, width);
                        }
                    }
                    height += self.px(12.0);
                }
                height
            }
            BlockKind::Summary | BlockKind::FootnoteDefinition { .. } => 0.0,
        }
    }

    /// The estimated height of the footnote list, as [`footnotes`](Self::footnotes) lays it out.
    pub(super) fn estimate_footnotes(&mut self, m: &Metrics) -> f32 {
        let notes: Vec<Footnote> = self
            .doc
            .footnotes
            .iter()
            .filter(|f| f.number > 0 && f.defined)
            .copied()
            .collect();
        if notes.is_empty() {
            return 0.0;
        }
        let width = self.content_width();
        let mut height = self.px(20.0) + self.hair() + self.px(10.0);
        for note in notes {
            let body = if note.block == NONE {
                ""
            } else {
                self.doc.text_of(self.doc.blocks[note.block as usize].text)
            };
            // The number and a space lead the entry; paragraphs are joined on one line.
            let chars = body.chars().filter(|c| *c != '\n').count() + 4;
            let lines = self.wrap(chars, m.sans, 14.0, width).max(1.0);
            height += (lines * 14.0 * 1.6 * self.scale).round() + self.px(6.0);
        }
        height
    }

    fn estimate_children(&mut self, m: &Metrics, b: u32, width: f32) -> f32 {
        let children: Vec<u32> = self.doc.children(b).collect();
        children
            .into_iter()
            .map(|child| self.estimate(m, child, width))
            .sum()
    }

    /// A paragraph or tight list text: wrapped lines of body text, its images, then `gap`.
    fn estimate_text(&mut self, m: &Metrics, b: u32, width: f32, gap: f32) -> f32 {
        let block = self.doc.blocks[b as usize];
        let text = self.doc.text_of(block.text);
        let mut task = false;
        let mut images = 0.0f32;
        for span in &self.doc.spans[block.spans.usize()] {
            match span.kind {
                SpanKind::Image(index) => images += self.estimate_image(index),
                SpanKind::Checkbox { .. } => task = true,
                _ => {}
            }
        }
        if text.trim().is_empty() && images == 0.0 && !task {
            return 0.0;
        }
        let indent = if task { self.px(22.0) } else { 0.0 };
        let size = self.theme.body_size;
        let lines = self.lines(text, m.sans, size, width - indent);
        let text_height = if text.trim().is_empty() {
            0.0
        } else {
            lines * size * self.theme.body_line_height * self.scale
        };
        (text_height + images).round() + self.px(gap)
    }

    /// The height an inline image adds: its stored size, its declared size, a typical box for a
    /// local file not loaded yet, or nothing when it shows as alt text.
    fn estimate_image(&mut self, index: u32) -> f32 {
        let info = self.doc.images[index as usize];
        let (_, found) = self.image(index);
        match found {
            Found::Ready(image, density) => {
                let natural = (
                    image.width as f32 / density * self.scale,
                    image.height as f32 / density * self.scale,
                );
                self.sized_image(natural, info.width, info.height).1
            }
            Found::Pending | Found::Blocked | Found::Unknown => match (info.width, info.height) {
                (Some(w), Some(h @ mdview_core::model::Length::Px(_))) => {
                    self.sized_image((1.0, 1.0), Some(w), Some(h)).1
                }
                _ if self.unloaded_local(index, &found) => self.px(GUESS_IMAGE.1),
                _ => 0.0,
            },
            Found::Failed => 0.0,
        }
    }

    fn estimate_code(&mut self, m: &Metrics, b: u32, width: f32) -> f32 {
        let source = self.doc.block_text(b).trim_end_matches('\n');
        let pad_x = self.px(16.0);
        let pad_y = self.px(13.0);
        let lines = self.lines(source, m.mono, 14.0, (width - 2.0 * pad_x).max(1.0));
        let text = (lines * 14.0 * 1.5 * self.scale)
            .round()
            .max(14.0 * self.scale);
        text + 2.0 * pad_y + self.px(16.0)
    }

    /// A diagram's height from the store, or a typical one. `None` when it shows as source.
    fn estimate_diagram(&mut self, b: u32, width: f32, oversize: bool) -> Option<f32> {
        let source = self.doc.block_text(b).trim();
        if oversize || source.is_empty() {
            return None;
        }
        let dark = self.theme.name == "dark";
        let (_, found) = self.store.mermaid(source, 2.0 * self.scale, dark, false);
        let h = match found {
            Found::Ready(image, _) => self.shrunk(&image, width),
            Found::Unknown => self.px(GUESS_DIAGRAM),
            _ => return None,
        };
        Some(self.px(8.0) + h + self.px(16.0))
    }

    fn estimate_math(&mut self, b: u32, width: f32, oversize: bool) -> Option<f32> {
        let source = self.doc.block_text(b).trim();
        if oversize || source.is_empty() {
            return None;
        }
        let size = self.theme.body_size * 1.15 * 2.0 * self.scale;
        let color = self.theme.text;
        let (_, found) = self.store.math(source, size, color, true, false);
        let h = match found {
            Found::Ready(image, _) => self.shrunk(&image, width),
            Found::Unknown => self.px(GUESS_MATH),
            _ => return None,
        };
        Some(self.px(10.0) + h + self.px(18.0))
    }

    /// The height of a block image rendered at twice the device scale, shrunk to fit.
    fn shrunk(&self, image: &crate::image::DecodedImage, width: f32) -> f32 {
        let (w, h) = (image.width as f32 / 2.0, image.height as f32 / 2.0);
        let avail = (width - 2.0 * self.px(16.0)).max(1.0);
        h * (avail / w.max(1.0)).min(1.0)
    }

    /// Rows of the cells' wrapped text, with columns sized by their longest cell as the table
    /// layout does, scaled down together when they do not fit.
    fn estimate_table(&mut self, m: &Metrics, b: u32, width: f32) -> f32 {
        let pad_x = self.px(13.0);
        let pad_y = self.px(8.0);
        let hr = self.hair();
        let advance = m.sans * 15.0 * self.scale;
        let rows: Vec<Vec<usize>> = self
            .doc
            .children(b)
            .map(|row| {
                self.doc
                    .children(row)
                    .map(|cell| {
                        let text = self.doc.text_of(self.doc.blocks[cell as usize].text);
                        text.chars().count()
                    })
                    .collect()
            })
            .collect();
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0).min(200);
        if columns == 0 {
            return 0.0;
        }
        let mut natural = vec![0f32; columns];
        for row in &rows {
            for (c, &chars) in row.iter().enumerate().take(columns) {
                natural[c] = natural[c].max(chars as f32 * advance);
            }
        }
        let available = width - 2.0 * hr;
        let total: f32 = natural.iter().sum::<f32>() + 2.0 * pad_x * columns as f32;
        let fit = if total > available {
            available / total
        } else {
            1.0
        };
        let text_width: Vec<f32> = natural
            .iter()
            .map(|w| {
                (((w + 2.0 * pad_x) * fit).max(2.0 * pad_x + self.px(8.0)) - 2.0 * pad_x).max(1.0)
            })
            .collect();
        let line = (15.0 * 1.45 * self.scale).round();
        let mut height = hr;
        for row in &rows {
            let lines = row
                .iter()
                .enumerate()
                .take(columns)
                .map(|(c, &chars)| {
                    (chars as f32 * advance / (text_width[c] * FILL))
                        .ceil()
                        .max(1.0)
                })
                .fold(1.0f32, f32::max);
            height += (lines * line).max(line) + 2.0 * pad_y + hr;
        }
        height + self.px(16.0)
    }

    /// Wrapped lines of `text`, hard breaks included, at `size` px font.
    fn lines(&self, text: &str, advance: f32, size: f32, width: f32) -> f32 {
        text.split('\n')
            .map(|line| {
                self.wrap(line.chars().count(), advance, size, width)
                    .max(1.0)
            })
            .sum()
    }

    fn wrap(&self, chars: usize, advance: f32, size: f32, width: f32) -> f32 {
        (chars as f32 * advance * size * self.scale / (width.max(1.0) * FILL)).ceil()
    }

    fn line_px(&self, lines: f32, size: f32) -> f32 {
        (lines * size * self.theme.body_line_height * self.scale).round()
    }
}
