//! The layout walk: [`Pager`] carries the cursor, the paint list and the caret geometry through
//! the block tree, dispatches each block kind, builds parley layouts from [`Inline`] runs and
//! emits their glyphs, chips, decorations and inline images.

use super::hit::LineGeom;
use super::inline::{ImageBox, Inline};
use super::paint::{CachedBlock, Paint};
use super::store::{Found, ImageStore};
use super::{BlockBox, Options};
use crate::text::Brush;
use crate::theme::{Color, Theme};
use mdview_core::model::{Align, BlockKind, Document, NONE};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, InlineBox,
    InlineBoxKind, Layout, LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty,
    VerticalAlign,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub(super) struct Txt {
    pub(super) size: f32,
    pub(super) line_height: f32,
    pub(super) weight: f32,
    pub(super) mono: bool,
    pub(super) color: Color,
    pub(super) max: Option<f32>,
}

pub(super) struct Pager<'a, 'b> {
    pub(super) doc: &'a Document,
    pub(super) theme: Theme,
    pub(super) scale: f32,
    pub(super) width: f32,
    pub(super) left: f32,
    pub(super) max_y: f32,
    pub(super) y: f32,
    pub(super) paints: Vec<Paint>,
    pub(super) blocks: Vec<BlockBox>,
    /// Caret geometry for the block being laid out, in page coordinates until it moves into the
    /// cache, where it is stored relative to that block's top like the paints.
    pub(super) hits: Vec<LineGeom>,
    /// Whether to keep hit geometry; the height pass does not need it.
    record: bool,
    /// The measured y of each footnote entry, built by the height pass.
    pub(super) notes: Vec<(u32, f32)>,
    pub(super) fonts: &'b mut FontContext,
    pub(super) layouts: &'b mut LayoutContext<Brush>,
    pub(super) cache: Option<&'b mut HashMap<u32, CachedBlock>>,
    pub(super) base_dir: Option<PathBuf>,
    /// Every raster the document draws, owned by the `DocumentRenderer` so it outlives layouts.
    pub(super) store: &'b mut ImageStore,
    /// Whether this pass may decode images and run engine jobs for what the store lacks.
    pub(super) allow: bool,
    /// Set when something this pass needed was not loaded (because `allow` was off), so what was
    /// laid out is a stand-in.
    pub(super) provisional: bool,
    /// Store ids the pass looked up, so an arrival can find the blocks that use it.
    pub(super) used: Vec<u32>,
    /// `<details>` open overrides from the shell; empty means use the document.
    pub(super) details: &'b HashMap<u32, bool>,
}

/// Marks an inline box that reserves space for an image without pixels.
const RESERVED: u64 = 1 << 32;

fn brush(c: Color) -> Brush {
    [c[0], c[1], c[2], 255]
}

impl<'a, 'b> Pager<'a, 'b> {
    pub(super) fn new(
        doc: &'a Document,
        options: &Options,
        fonts: &'b mut FontContext,
        layouts: &'b mut LayoutContext<Brush>,
        cache: Option<&'b mut HashMap<u32, CachedBlock>>,
        store: &'b mut ImageStore,
        details: &'b HashMap<u32, bool>,
    ) -> Self {
        let scale = options.scale.max(0.1);
        let logical_width = options.width.max(120.0);
        let content = (logical_width - 2.0 * options.theme.margin)
            .max(80.0)
            .min(options.theme.reading_width);
        let left = (logical_width - content) / 2.0 * scale;
        let max_y = options
            .max_height
            .map_or(f32::MAX, |h| (h * scale).max(1.0));
        let record = cache.is_some();
        Pager {
            doc,
            theme: options.theme,
            scale,
            width: (logical_width * scale).round(),
            left: left.round(),
            max_y,
            y: 0.0,
            paints: Vec::new(),
            blocks: Vec::new(),
            hits: Vec::new(),
            record,
            notes: Vec::new(),
            fonts,
            layouts,
            cache,
            base_dir: options.base_dir.clone(),
            store,
            allow: true,
            provisional: false,
            used: Vec::new(),
            details,
        }
    }

    /// Whether a `<details>` shows its body: the shell's toggle if there is one, else the source.
    pub(super) fn details_open(&self, block: u32) -> bool {
        self.details
            .get(&block)
            .copied()
            .unwrap_or_else(|| self.doc.details_open_as_written(block))
    }

    pub(super) fn px(&self, logical: f32) -> f32 {
        (logical * self.scale).round()
    }

    pub(super) fn hair(&self) -> f32 {
        self.scale.round().max(1.0)
    }

    pub(super) fn content_width(&self) -> f32 {
        self.width - 2.0 * self.left
    }

    pub(super) fn page(&mut self) {
        self.y = self.px(36.0);
        let content = self.content_width();
        self.blocks(NONE, self.left, content, self.theme.text, 0);
        self.footnotes();
    }

    pub(super) fn blocks(
        &mut self,
        parent: u32,
        left: f32,
        width: f32,
        color: Color,
        depth: usize,
    ) {
        let children: Vec<u32> = self.doc.children(parent).collect();
        for b in children {
            if self.y > self.max_y {
                return;
            }
            let y0 = self.y;
            self.block(b, left, width, color, depth);
            if depth == 0 {
                self.blocks.push(BlockBox {
                    block: b,
                    y: y0,
                    height: self.y - y0,
                });
            }
        }
    }

    pub(super) fn block(&mut self, b: u32, left: f32, width: f32, color: Color, depth: usize) {
        let kind = self.doc.blocks[b as usize].kind;
        match kind {
            BlockKind::Paragraph { align } => self.paragraph(b, left, width, color, align),
            BlockKind::Plain => self.plain(b, left, width, color),
            BlockKind::Heading { level, align, .. } => {
                self.heading(b, left, width, level, align);
            }
            BlockKind::BlockQuote => self.quote(b, left, width, None, color, depth),
            BlockKind::Alert(alert) => self.quote(b, left, width, Some(alert), color, depth),
            BlockKind::List { start } => self.list(b, left, width, color, depth, start),
            BlockKind::ListItem { .. } => self.blocks(b, left, width, color, depth + 1),
            BlockKind::CodeBlock { lang } => {
                let label = if self.doc.str(lang).is_empty() {
                    None
                } else {
                    Some(self.doc.str(lang).to_string())
                };
                self.code_block(b, left, width, label);
            }
            BlockKind::Diagram { lang, .. } => {
                let label = if self.doc.str(lang).is_empty() {
                    "diagram".to_string()
                } else {
                    self.doc.str(lang).to_string()
                };
                if !self.diagram_block(b, left, width) {
                    self.code_block(b, left, width, Some(label));
                }
            }
            BlockKind::Math { .. } => {
                if !self.math_block(b, left, width) {
                    self.code_block(b, left, width, Some("math".to_string()));
                }
            }
            BlockKind::ThematicBreak => {
                self.y += self.px(8.0);
                let h = self.hair();
                self.paints.push(Paint::Rect {
                    x: left,
                    y: self.y,
                    w: width,
                    h,
                    radius: 0.0,
                    color: self.theme.border,
                });
                self.y += h + self.px(16.0);
            }
            BlockKind::Table => self.table(b, left, width),
            BlockKind::TableRow { .. } | BlockKind::TableCell { .. } => {
                // Only reached outside a table, which the model's shape promise rules out.
                self.blocks(b, left, width, color, depth + 1);
            }
            BlockKind::Details { .. } => self.details(b, left, width, color, depth),
            BlockKind::Summary => {}
            BlockKind::FootnoteDefinition { .. } => {
                // Moved to the end list; never drawn in flow.
            }
            BlockKind::Div { .. } => self.blocks(b, left, width, color, depth + 1),
        }
    }

    pub(super) fn text_layout(&mut self, inline: &Inline, txt: &Txt) -> Layout<Brush> {
        let boxes: Vec<(u32, f32, f32, Option<f32>)> = inline
            .boxes
            .iter()
            .map(|image| self.box_size(image, txt))
            .collect();
        let mut builder =
            self.layouts
                .ranged_builder(&mut self.fonts, &inline.text, self.scale, true);
        let family = if txt.mono {
            self.theme.mono
        } else {
            self.theme.sans
        };
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(family)));
        builder.push_default(StyleProperty::FontSize(txt.size));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            txt.line_height,
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(txt.weight)));
        builder.push_default(StyleProperty::Brush(brush(txt.color)));
        for (range, style) in &inline.runs {
            let range = range.clone();
            if style.bold {
                builder.push(StyleProperty::FontWeight(FontWeight::BOLD), range.clone());
            }
            if style.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), range.clone());
            }
            if style.strike {
                builder.push(StyleProperty::Strikethrough(true), range.clone());
            }
            if style.underline || style.link {
                builder.push(StyleProperty::Underline(true), range.clone());
            }
            if style.link || style.accent {
                builder.push(
                    StyleProperty::Brush(brush(self.theme.accent)),
                    range.clone(),
                );
            }
            if style.muted {
                builder.push(StyleProperty::Brush(brush(self.theme.muted)), range.clone());
            }
            if let Some(color) = style.color {
                builder.push(StyleProperty::Brush(brush(color)), range.clone());
            }
            if style.code || style.kbd || style.math {
                builder.push(
                    StyleProperty::FontFamily(FontFamily::from(self.theme.mono)),
                    range.clone(),
                );
                builder.push(
                    StyleProperty::FontSize(txt.size * style.size_factor()),
                    range.clone(),
                );
            } else if style.small || style.sub || style.sup {
                builder.push(
                    StyleProperty::FontSize(txt.size * style.size_factor()),
                    range.clone(),
                );
            }
        }
        for (image, &(id, mut width, mut height, mut baseline)) in inline.boxes.iter().zip(&boxes) {
            if let Some(max) = txt.max
                && width > max
            {
                let shrink = max / width;
                height *= shrink;
                baseline = baseline.map(|b| b * shrink);
                width = max;
            }
            builder.push_inline_box(InlineBox {
                id: if image.reserved {
                    RESERVED | u64::from(id)
                } else {
                    u64::from(id)
                },
                kind: InlineBoxKind::InFlow,
                index: image.at,
                width,
                height,
                baseline,
                vertical_align: VerticalAlign::BASELINE,
            });
        }
        let mut layout = builder.build(&inline.text);
        layout.break_all_lines(txt.max);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }

    /// An inline box's store id and size. A math box resolves the raster for this text's size
    /// and colour here.
    fn box_size(&mut self, image: &ImageBox, txt: &Txt) -> (u32, f32, f32, Option<f32>) {
        let (mut width, mut height, mut baseline) = (image.width, image.height, image.baseline);
        let mut id = image.id;
        if let Some((start, end)) = image.math {
            // The probe in `push_atom` is body size in the text colour. A heading, table cell,
            // footnote or muted quote asks for another size or colour, which is another key:
            // the store renders it once and every later layout finds it.
            let target = (txt.size * 2.0 * self.scale).clamp(8.0, 512.0);
            let source = self
                .doc
                .text
                .get(start as usize..end as usize)
                .unwrap_or("");
            let (key, found) = self
                .store
                .math(source, target, txt.color, false, self.allow);
            self.note(key, &found, true);
            id = key;
            match found {
                Found::Ready(math, _) => {
                    width = math.width as f32 / 2.0;
                    height = math.height as f32 / 2.0;
                    baseline = Some(math.baseline as f32 / 2.0);
                }
                // Not rendered yet in this pass: the probe, scaled, stands in.
                Found::Unknown => {
                    let probe = self.theme.body_size * 2.0 * self.scale;
                    let ratio = target / probe;
                    width *= ratio;
                    height *= ratio;
                    baseline = baseline.map(|b| b * ratio);
                }
                _ => {
                    width = 0.0;
                    height = 0.0;
                    baseline = None;
                }
            }
        }
        (id, width, height, baseline)
    }

    pub(super) fn aligned_x(
        &self,
        left: f32,
        width: f32,
        layout: &Layout<Brush>,
        align: Align,
    ) -> f32 {
        match align {
            Align::Center => left + ((width - layout.width()) / 2.0).round(),
            Align::Right => left + (width - layout.width()).round(),
            _ => left,
        }
    }

    /// Pushes the chips, the glyphs and the decorations for one laid-out inline block, in that order.
    pub(super) fn emit(
        &mut self,
        inline: &Inline,
        layout: Layout<Brush>,
        x: f32,
        y: f32,
        base_size: f32,
    ) {
        let layout = Rc::new(layout);
        // Only document text is hittable; a list marker or a language label is drawn but has no
        // document offsets to select.
        let track = self.record && !inline.origins.is_empty();
        let source = Rc::new(inline.clone());
        let mut images = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::InlineBox(inline_box) = item {
                    if inline_box.id & RESERVED != 0 {
                        // A declared size with no pixels yet: the reserved box, tinted.
                        images.push(Paint::Rect {
                            x: x + inline_box.x,
                            y: y + inline_box.y,
                            w: inline_box.width,
                            h: inline_box.height,
                            radius: self.px(3.0),
                            color: self.theme.code_bg,
                        });
                        continue;
                    }
                    images.push(Paint::Image {
                        index: inline_box.id as u32,
                        x: x + inline_box.x,
                        y: y + inline_box.y,
                        w: inline_box.width,
                        h: inline_box.height,
                    });
                }
            }
        }
        let mut chips = Vec::new();
        let mut decos = Vec::new();
        let mut lines = Vec::new();
        for line in layout.lines() {
            let metrics = line.metrics();
            let line_height = metrics.line_height.max(1.0);
            let baseline = y + metrics.baseline;
            let top = baseline - line_height * 0.82;
            let bottom = baseline + line_height * 0.18;
            if track && let Some(doc_start) = inline.doc_at(line.text_range().start) {
                lines.push(LineGeom {
                    top,
                    bottom,
                    layout: layout.clone(),
                    source: source.clone(),
                    x,
                    y,
                    end: line.text_range().end,
                    x0: x + metrics.offset,
                    x1: x + metrics.offset + metrics.advance,
                    doc_start,
                    doc_end: inline.doc_at(line.text_range().end).unwrap_or(doc_start),
                });
            }
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                // A line-scoped run reports the whole line's range from 	ext_range, so the run's
                // own start is its first cluster's text range.
                let run_start = run.clusters().next().map_or(0, |c| c.text_range().start);
                let style = inline.style_at(run_start);
                let dev = base_size * style.size_factor() * self.scale;
                let (gx, baseline, advance) = (
                    x + glyph_run.offset(),
                    y + glyph_run.baseline(),
                    glyph_run.advance(),
                );
                if style.code || style.kbd || style.math {
                    let pad = self.px(3.0);
                    chips.push(Paint::Rect {
                        x: gx - pad,
                        y: (baseline - dev * 0.92).round(),
                        w: advance + 2.0 * pad,
                        h: (dev * 1.24).round().max(1.0),
                        radius: self.px(3.0),
                        color: self.theme.code_bg,
                    });
                }
                let color = glyph_run.style().brush;
                let color = [color[0], color[1], color[2]];
                let thickness = (dev * 0.065).round().max(1.0);
                if style.underline || style.link {
                    decos.push(Paint::Rect {
                        x: gx,
                        y: (baseline + dev * 0.13).round(),
                        w: advance,
                        h: thickness,
                        radius: 0.0,
                        color,
                    });
                }
                if style.strike {
                    decos.push(Paint::Rect {
                        x: gx,
                        y: (baseline - dev * 0.3).round(),
                        w: advance,
                        h: thickness,
                        radius: 0.0,
                        color,
                    });
                }
            }
        }
        if track {
            self.hits.extend(lines);
        }
        self.paints.extend(images);
        self.paints.extend(chips);
        self.paints.push(Paint::Text { layout, x, y });
        self.paints.extend(decos);
    }
}
