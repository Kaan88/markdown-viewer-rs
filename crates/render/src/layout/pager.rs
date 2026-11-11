//! The layout walk: [`Pager`] carries the cursor, the paint list and the caret geometry through
//! the block tree, dispatches each block kind, builds parley layouts from [`Inline`] runs and
//! emits their glyphs, chips, decorations and inline images.

use super::hit::{LineGeom, SourceBox};
use super::inline::{ImageBox, Inline, RubyNote, Style};
use super::paint::Paint;
use super::store::{Found, ImageStore};
use super::{BlockBox, Options, Source, SourceKind};
use crate::text::Brush;
use crate::theme::{Color, Theme};
use mdview_core::model::{Align, BlockKind, Document, Length, NONE};
use parley::{
    Alignment, AlignmentOptions, Cluster, FontContext, FontFamily, FontStyle, FontWeight,
    InlineBox, InlineBoxKind, Layout, LayoutContext, LineHeight, PositionedLayoutItem,
    StyleProperty, VerticalAlign,
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
    /// The formulas and diagrams of the block being laid out, kept like `hits`.
    pub(super) sources: Vec<SourceBox>,
    /// Whether to keep hit geometry; a measuring pass does not need it.
    pub(super) record: bool,
    /// The measured y of each footnote entry, built by the height pass.
    pub(super) notes: Vec<(u32, f32)>,
    pub(super) fonts: &'b mut FontContext,
    pub(super) layouts: &'b mut LayoutContext<Brush>,
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
/// Marks a zero-width box that only makes its line taller, to leave room for a ruby annotation.
const SPACER: u64 = 1 << 33;

fn brush(c: Color) -> Brush {
    [c[0], c[1], c[2], 255]
}

impl<'a, 'b> Pager<'a, 'b> {
    pub(super) fn new(
        doc: &'a Document,
        options: &Options,
        fonts: &'b mut FontContext,
        layouts: &'b mut LayoutContext<Brush>,
        record: bool,
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
            sources: Vec::new(),
            record,
            notes: Vec::new(),
            fonts,
            layouts,
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
                // `<hr width>`: narrower than the column, at the left unless `align` says so.
                let attrs = self.doc.attrs_of(b);
                let full = width;
                let width = match attrs.and_then(|a| a.width) {
                    Some(Length::Px(px)) => self.px(px as f32).min(full),
                    Some(Length::Percent(p)) => (full * f32::from(p) / 100.0).min(full),
                    None => full,
                }
                .round();
                let left = match attrs.map(|a| a.align) {
                    Some(Align::Center) => left + ((full - width) / 2.0).round(),
                    Some(Align::Right) => left + full - width,
                    _ => left,
                };
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
            BlockKind::Div { .. } | BlockKind::DefTerm => {
                self.blocks(b, left, width, color, depth + 1);
            }
            BlockKind::DefDesc => self.definition(b, left, width, color, depth),
            BlockKind::Figure { .. } => {
                self.blocks(b, left, width, color, depth + 1);
                self.y += self.px(12.0);
            }
            BlockKind::FigCaption => {
                let muted = self.theme.muted;
                self.blocks(b, left, width, muted, depth + 1);
            }
        }
    }

    pub(super) fn text_layout(&mut self, inline: &Inline, txt: &Txt) -> Layout<Brush> {
        if inline.notes.is_empty() {
            return self.build_layout(inline, txt, &[]);
        }
        // Ruby: lay the text out as it would be, then give each base a zero-width box above its
        // baseline that is tall enough for the annotation, so the line grows upward by exactly that.
        let plain = self.build_layout(inline, txt, &[]);
        let mut spacers = Vec::new();
        for note in &inline.notes {
            let Some(line) = plain
                .lines()
                .find(|line| line.text_range().contains(&note.base.start))
            else {
                continue;
            };
            let metrics = line.metrics();
            let band = self
                .note_layout(note, txt.size, txt.color)
                .1
                .height()
                .round()
                + self.px(1.0);
            spacers.push((
                note.base.start,
                metrics.baseline - metrics.block_min_coord + band,
            ));
        }
        self.build_layout(inline, txt, &spacers)
    }

    /// A ruby annotation as its own small layout, half the size of the text it annotates.
    fn note_layout(&mut self, note: &RubyNote, size: f32, color: Color) -> (Inline, Layout<Brush>) {
        let mut inline = Inline::default();
        inline.push_doc(&note.text, Style::default(), note.doc);
        let txt = Txt {
            size: size * 0.5,
            line_height: 1.2,
            weight: 400.0,
            mono: false,
            color,
            max: None,
        };
        let layout = self.build_layout(&inline, &txt, &[]);
        (inline, layout)
    }

    /// `spacers` are (text offset, height above the baseline) for boxes that draw nothing.
    fn build_layout(
        &mut self,
        inline: &Inline,
        txt: &Txt,
        spacers: &[(usize, f32)],
    ) -> Layout<Brush> {
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
            if style.sub || style.sup {
                // Last, so it keeps whichever colour the run ended up with.
                let color = style.color.unwrap_or(if style.link || style.accent {
                    self.theme.accent
                } else if style.muted {
                    self.theme.muted
                } else {
                    txt.color
                });
                let mut shifted = brush(color);
                shifted[3] = if style.sup {
                    crate::text::RAISED
                } else {
                    crate::text::LOWERED
                };
                builder.push(StyleProperty::Brush(shifted), range.clone());
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
            } else if style.small || style.sub || style.sup || style.caption {
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
        for &(index, height) in spacers {
            builder.push_inline_box(InlineBox {
                id: SPACER,
                kind: InlineBoxKind::InFlow,
                index,
                width: 0.0,
                height,
                baseline: None,
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

    /// The `align` of the nearest `<figure>`, `<div>` or `<center>` a block sits in that sets
    /// one, which its text and images follow.
    pub(super) fn inherited_align(&self, block: u32) -> Align {
        let mut b = block;
        loop {
            let block = &self.doc.blocks[b as usize];
            if let BlockKind::Figure { align } | BlockKind::Div { align } = block.kind
                && align != Align::None
            {
                return align;
            }
            if block.parent == NONE {
                return Align::None;
            }
            b = block.parent;
        }
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
        // The layout's boxes come in the order of `inline.boxes`, spacers aside.
        let mut boxes = inline.boxes.iter();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::InlineBox(inline_box) = item {
                    if inline_box.id & SPACER != 0 {
                        continue;
                    }
                    let image = boxes.next();
                    if self.record
                        && let Some(ImageBox {
                            math: Some((start, end)),
                            ..
                        }) = image
                        && inline_box.width > 0.0
                    {
                        self.sources.push(SourceBox {
                            x: x + inline_box.x,
                            y: y + inline_box.y,
                            w: inline_box.width,
                            h: inline_box.height,
                            source: Source {
                                kind: SourceKind::Formula,
                                start: *start,
                                end: *end,
                            },
                        });
                    }
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
                // Decorations come from the glyph run's own style: one shaped run holds several
                // of them when only a decoration changes, so the run's first cluster cannot say.
                let run_style = glyph_run.style();
                let color = run_style.brush;
                let baseline = baseline + crate::text::baseline_shift(color, dev);
                let color = [color[0], color[1], color[2]];
                let thickness = (dev * 0.065).round().max(1.0);
                if run_style.underline.is_some() {
                    decos.push(Paint::Rect {
                        x: gx,
                        y: (baseline + dev * 0.13).round(),
                        w: advance,
                        h: thickness,
                        radius: 0.0,
                        color,
                    });
                }
                if run_style.strikethrough.is_some() {
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
        // Each annotation sits centred over its base, in the band the line was made taller by. It
        // keeps the colour of the text it annotates.
        let mut notes = Vec::new();
        for note in &inline.notes {
            let Some(first) = Cluster::from_byte_index(&layout, note.base.start) else {
                continue;
            };
            let metrics = *first.line().metrics();
            let start = first.visual_offset().unwrap_or(0.0);
            let last = inline.text[..note.base.end]
                .char_indices()
                .next_back()
                .and_then(|(at, _)| Cluster::from_byte_index(&layout, at))
                .filter(|c| c.path().line_index() == first.path().line_index());
            // A base that wraps is centred over its part on the first line.
            let end = last.map_or(metrics.offset + metrics.advance, |c| {
                c.visual_offset().unwrap_or(start) + c.advance()
            });
            let brush = first.style().brush;
            notes.push((
                note.clone(),
                (start + end) / 2.0,
                metrics.block_min_coord,
                [brush[0], brush[1], brush[2]],
            ));
        }
        self.paints.push(Paint::Text { layout, x, y });
        self.paints.extend(decos);
        for (note, centre, top, color) in notes {
            let (note_inline, note_layout) = self.note_layout(&note, base_size, color);
            let width = note_layout.width();
            let nx = (x + centre - width / 2.0).min(self.width - width).max(x);
            self.emit(&note_inline, note_layout, nx, y + top, base_size * 0.5);
        }
    }
}
