//! Inline content: [`Inline`] runs with their styles, image boxes and document-offset origins, and
//! the conversion from a model leaf block (spans, atoms, inline math, images, footnote marks,
//! checkboxes) into those runs, including the merged text of table cells.

use super::pager::Pager;
use super::store::Found;
use crate::theme::Color;
use mdview_core::model::{BlockKind, ImageAlign, SpanKind};
use std::ops::Range;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Style {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) strike: bool,
    pub(super) underline: bool,
    pub(super) code: bool,
    pub(super) kbd: bool,
    pub(super) math: bool,
    pub(super) link: bool,
    /// A wiki-link whose note was not found: muted, with a dashed underline.
    pub(super) broken: bool,
    pub(super) muted: bool,
    pub(super) accent: bool,
    pub(super) small: bool,
    pub(super) sub: bool,
    pub(super) sup: bool,
    pub(super) mark: bool,
    /// Text of a `<figcaption>`: a little smaller than the body, in the muted colour.
    pub(super) caption: bool,
    /// A token colour from the syntax highlighter; overrides the theme text colour.
    pub(super) color: Option<Color>,
}

impl Style {
    pub(super) fn size_factor(self) -> f32 {
        if self.code || self.kbd || self.math {
            0.92
        } else if self.small {
            0.78
        } else if self.sub || self.sup {
            0.8
        } else if self.caption {
            0.9
        } else {
            1.0
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Inline {
    pub(super) text: String,
    pub(super) runs: Vec<(Range<usize>, Style)>,
    /// A task-list checkbox at the start of the content, drawn before the text.
    pub(super) task: Option<bool>,
    /// Images placed inline, at their byte offset in `text`.
    pub(super) boxes: Vec<ImageBox>,
    /// Inline byte range -> offset in `Document::text`, so a caret position can be turned back into
    /// a document position. `fixed` ranges stand for an atom (an image placeholder, a footnote
    /// mark): every offset in them maps to the atom's one position.
    pub(super) origins: Vec<(Range<usize>, u32, bool)>,
    /// Ruby annotations. Their text is in `Document::text` but not in `text` above: the line is
    /// laid out without it and the annotation is drawn small above its base.
    pub(super) notes: Vec<RubyNote>,
}

/// One `<rt>`: `text` is drawn above the inline byte range `base`, and `doc` is where it starts in
/// `Document::text`, so find, selection and a click on it still map to the document.
#[derive(Clone)]
pub(super) struct RubyNote {
    pub(super) base: Range<usize>,
    pub(super) text: String,
    pub(super) doc: u32,
}

/// One inline image: its text offset, the store id it draws, its size in device px and, for math,
/// the distance from its top to the baseline.
#[derive(Clone, Copy)]
pub(super) struct ImageBox {
    pub(super) at: usize,
    /// The image store id; `u32::MAX` for a reserved box with nothing to draw yet.
    pub(super) id: u32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) baseline: Option<f32>,
    /// The TeX source's document range for a math box. Its raster depends on the text size and
    /// colour around it, which only `text_layout` knows, so the id here is the body-size probe.
    pub(super) math: Option<(u32, u32)>,
    /// An image whose pixels are not here (a remote one pending or blocked) but whose size the
    /// document declared: the box is reserved and drawn as a tinted placeholder.
    pub(super) reserved: bool,
    /// `<img align>`: where the box sits against its line, or at which side of its paragraph.
    pub(super) align: ImageAlign,
}

impl Inline {
    /// Drops spaces at the very start, as a line does once the image before them has left it.
    pub(super) fn trim_start(&mut self) {
        let n = self.text.len() - self.text.trim_start_matches(' ').len();
        if n == 0 || self.boxes.iter().any(|image| image.at < n) {
            return;
        }
        self.text.drain(..n);
        let cut = |range: &mut Range<usize>| {
            *range = range.start.saturating_sub(n)..range.end.saturating_sub(n);
        };
        self.runs.iter_mut().for_each(|(range, _)| cut(range));
        self.runs.retain(|(range, _)| !range.is_empty());
        for (range, origin, fixed) in &mut self.origins {
            // What is cut from the front of a range moves the document position it starts at.
            let lost = n.saturating_sub(range.start).min(range.len());
            if !*fixed {
                *origin += lost as u32;
            }
            cut(range);
        }
        self.origins.retain(|(range, _, _)| !range.is_empty());
        self.boxes.iter_mut().for_each(|image| image.at -= n);
        self.notes.iter_mut().for_each(|note| cut(&mut note.base));
    }

    /// Puts `s` before everything else, as text that belongs to the first document position.
    pub(super) fn prepend(&mut self, s: &str) {
        let Some(first) = self.origins.first().map(|(_, origin, _)| *origin) else {
            return;
        };
        let n = s.len();
        self.text.insert_str(0, s);
        for (range, _) in &mut self.runs {
            *range = range.start + n..range.end + n;
        }
        self.runs.insert(0, (0..n, Style::default()));
        for (range, _, _) in &mut self.origins {
            *range = range.start + n..range.end + n;
        }
        self.origins.insert(0, (0..n, first, true));
        for image in &mut self.boxes {
            image.at += n;
        }
        for note in &mut self.notes {
            note.base = note.base.start + n..note.base.end + n;
        }
    }

    pub(super) fn push(&mut self, s: &str, style: Style) {
        if s.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(s);
        let end = self.text.len();
        if let Some((range, last)) = self.runs.last_mut()
            && *last == style
            && range.end == start
        {
            range.end = end;
            return;
        }
        self.runs.push((start..end, style));
    }

    /// Pushes a slice copied from the document, remembering the document offset it starts at.
    pub(super) fn push_doc(&mut self, s: &str, style: Style, at: u32) {
        if s.is_empty() {
            return;
        }
        let start = self.text.len();
        self.push(s, style);
        let end = self.text.len();
        self.origins.push((start..end, at, false));
    }

    /// Pushes text that stands for the atom at `at`, which has no text of its own in the document.
    pub(super) fn push_atom(&mut self, s: &str, style: Style, at: u32) {
        if s.is_empty() {
            return;
        }
        let start = self.text.len();
        self.push(s, style);
        let end = self.text.len();
        self.origins.push((start..end, at, true));
    }

    /// The document offset for an inline byte offset, or `None` for text the renderer invented
    /// (a list marker, a language label, an alert title) that is not part of the document.
    pub(super) fn doc_at(&self, at: usize) -> Option<u32> {
        let mut last = None;
        for (range, origin, fixed) in &self.origins {
            if at < range.start {
                continue;
            }
            if at < range.end {
                return Some(if *fixed {
                    *origin
                } else {
                    origin + (at - range.start) as u32
                });
            }
            last = Some(if *fixed {
                *origin
            } else {
                origin + range.len() as u32
            });
        }
        last
    }

    /// The inline byte offset for a document offset, the inverse of [`doc_at`](Self::doc_at). An
    /// atom maps to the start of the text that stands for it.
    pub(super) fn inline_at(&self, doc: u32) -> Option<usize> {
        for (range, origin, fixed) in &self.origins {
            if *fixed {
                if *origin == doc {
                    return Some(range.start);
                }
            } else if doc >= *origin && doc <= *origin + range.len() as u32 {
                return Some(range.start + (doc - *origin) as usize);
            }
        }
        // An offset inside text this inline leaves out (a ruby annotation) sits at the start of the
        // next text that is drawn.
        self.origins
            .iter()
            .filter(|(_, origin, _)| *origin > doc)
            .min_by_key(|(_, origin, _)| *origin)
            .map(|(range, _, _)| range.start)
    }

    pub(super) fn style_at(&self, at: usize) -> Style {
        self.runs
            .iter()
            .find(|(range, _)| range.start <= at && at < range.end)
            .map_or(Style::default(), |(_, style)| *style)
    }
}

impl<'a, 'b> Pager<'a, 'b> {
    /// The model's inline content of a leaf block as styled runs, with atoms replaced by the text
    /// the page shows for them.
    pub(super) fn inline_of(&mut self, block: u32) -> Inline {
        let block_index = block;
        let block = self.doc.blocks[block as usize];
        let base = block.text.start as usize;
        let end = block.text.end as usize;
        let src = &self.doc.text[base..end];
        let (term, caption) = self.lineage(block_index);
        let mut cuts = vec![base, end];
        let mut spans: Vec<(usize, usize, SpanKind)> = Vec::new();
        let mut atoms: Vec<(usize, usize, SpanKind)> = Vec::new();
        let mut math_ranges: Vec<(usize, usize)> = Vec::new();
        let mut rubies: Vec<(usize, usize)> = Vec::new();
        let mut notes: Vec<(usize, usize)> = Vec::new();
        for span in &self.doc.spans[block.spans.usize()] {
            let (start, stop) = (span.start as usize, span.end as usize);
            if start == stop {
                if start >= base && start <= end {
                    atoms.push((start, start, span.kind));
                    cuts.push(start);
                }
                continue;
            }
            let (start, stop) = (start.max(base), stop.min(end));
            if start < stop {
                cuts.push(start);
                cuts.push(stop);
                if matches!(span.kind, SpanKind::Math { display: false, .. }) {
                    // Inline math becomes one image atom: no source text reaches the line.
                    atoms.push((start, stop, span.kind));
                    math_ranges.push((start, stop));
                } else if span.kind == SpanKind::Ruby {
                    rubies.push((start, stop));
                } else if span.kind == SpanKind::RubyNote {
                    notes.push((start, stop));
                } else {
                    spans.push((start, stop, span.kind));
                }
            }
        }
        // A note's base is the text between the previous note (or the start of its `<ruby>`) and
        // the note. A note with no base to sit over is ordinary small text.
        let mut bases: Vec<(usize, usize, usize)> = Vec::new();
        for &(start, stop) in &notes {
            let ruby = rubies
                .iter()
                .filter(|&&(from, to)| from <= start && stop <= to)
                .min_by_key(|&&(from, to)| to - from);
            let from = ruby.map(|&(from, _)| {
                notes
                    .iter()
                    .map(|&(_, previous)| previous)
                    .filter(|&previous| previous <= start && previous > from)
                    .max()
                    .unwrap_or(from)
            });
            match from {
                Some(from) if from < start => {
                    bases.push((from, start, stop));
                    cuts.push(from);
                }
                _ => spans.push((start, stop, SpanKind::Small)),
            }
        }
        let hidden: Vec<(usize, usize)> = bases
            .iter()
            .map(|&(_, start, stop)| (start, stop))
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        // The inline text length at each cut, to turn a note's document range into a byte range.
        let mut at_inline: Vec<(usize, usize)> = Vec::with_capacity(cuts.len());
        let mut out = Inline::default();
        for i in 0..cuts.len() {
            let at = cuts[i];
            at_inline.push((at, out.text.len()));
            for (position, end, kind) in &atoms {
                if *position == at {
                    self.push_atom(&mut out, *kind, *position as u32, *end as u32);
                }
            }
            if i + 1 == cuts.len() {
                continue;
            }
            let to = cuts[i + 1];
            if to <= at {
                continue;
            }
            if math_ranges
                .iter()
                .chain(&hidden)
                .any(|(start, end)| at >= *start && at < *end)
            {
                continue;
            }
            let mut style = self.style_at(&spans, at);
            style.bold |= term;
            style.caption = caption;
            out.push_doc(&src[at - base..to - base], style, at as u32);
        }
        for (from, start, stop) in bases {
            let inline_at = |doc: usize| {
                at_inline
                    .iter()
                    .find(|&&(at, _)| at >= doc)
                    .map_or(out.text.len(), |&(_, inline)| inline)
            };
            let range = inline_at(from)..inline_at(start);
            if range.is_empty() {
                continue;
            }
            out.notes.push(RubyNote {
                base: range,
                text: src[start - base..stop - base].to_string(),
                doc: start as u32,
            });
        }
        out
    }

    /// Whether a block sits inside a `<dt>` (its text is bold) or a `<figcaption>` (small).
    fn lineage(&self, block: u32) -> (bool, bool) {
        let (mut term, mut caption) = (false, false);
        let mut b = block;
        loop {
            let current = &self.doc.blocks[b as usize];
            match current.kind {
                BlockKind::DefTerm => term = true,
                BlockKind::FigCaption => caption = true,
                _ => {}
            }
            if current.parent == mdview_core::model::NONE {
                return (term, caption);
            }
            b = current.parent;
        }
    }

    pub(super) fn style_at(&self, spans: &[(usize, usize, SpanKind)], at: usize) -> Style {
        let mut style = Style::default();
        for (start, stop, kind) in spans {
            if *start > at || at >= *stop {
                continue;
            }
            match kind {
                SpanKind::Emphasis => style.italic = true,
                SpanKind::Strong => style.bold = true,
                SpanKind::Strike => style.strike = true,
                SpanKind::Underline => style.underline = true,
                SpanKind::Code => style.code = true,
                SpanKind::Kbd => style.kbd = true,
                SpanKind::Sub => style.sub = true,
                SpanKind::Sup => style.sup = true,
                SpanKind::Mark => style.mark = true,
                SpanKind::Small => style.small = true,
                SpanKind::Link(index) => {
                    style.link = true;
                    style.broken = self
                        .doc
                        .links
                        .get(*index as usize)
                        .is_some_and(|l| l.missing);
                }
                SpanKind::Math { display: false, .. } => style.math = true,
                _ => {}
            }
        }
        style
    }

    pub(super) fn push_atom(&mut self, out: &mut Inline, kind: SpanKind, at: u32, end: u32) {
        match kind {
            SpanKind::Math {
                display: false,
                oversize,
            } => {
                if oversize.is_none() {
                    let source = self.doc.text.get(at as usize..end as usize).unwrap_or("");
                    // The probe at body size in the text colour decides whether the formula
                    // renders at all; most formulas are drawn at exactly this key. Twice the body
                    // size keeps the raster crisp; the box and baseline are halved in `text_layout`.
                    let size = self.theme.body_size * 2.0 * self.scale;
                    let (id, found) =
                        self.store
                            .math(source, size, self.theme.text, false, self.allow);
                    self.note(id, &found, true);
                    if matches!(found, Found::Failed) {
                        self.undrawn.push((at, end));
                    }
                    if let Found::Ready(image, _) = found {
                        out.boxes.push(ImageBox {
                            at: out.text.len(),
                            id,
                            width: image.width as f32 / 2.0,
                            height: image.height as f32 / 2.0,
                            baseline: Some(image.baseline as f32 / 2.0),
                            math: Some((at, end)),
                            reserved: false,
                            align: ImageAlign::None,
                        });
                        return;
                    }
                }
                // No engine, oversize source, or TeX it could not parse: keep it readable.
                let source = self.doc.text.get(at as usize..end as usize).unwrap_or("");
                out.push_doc(
                    source,
                    Style {
                        math: true,
                        ..Style::default()
                    },
                    at,
                );
            }
            SpanKind::Image(index) => {
                let info = self.doc.images[index as usize];
                let (id, found) = self.image(index);
                let declared = match (info.width, info.height) {
                    (Some(w), Some(h @ mdview_core::model::Length::Px(_))) => Some((w, h)),
                    _ => None,
                };
                // A refinement pass stands a typical box in for a local file it may not load.
                let guess = self.unloaded_local(index, &found).then(|| {
                    let (w, h) = super::media::GUESS_IMAGE;
                    ((self.px(w).min(self.content_width()), self.px(h)), true)
                });
                let sized = match found {
                    Found::Ready(image, density) => {
                        let natural = (
                            image.width as f32 / density * self.scale,
                            image.height as f32 / density * self.scale,
                        );
                        Some((self.sized_image(natural, info.width, info.height), false))
                    }
                    // Not here yet (or not allowed): a declared size keeps its box, so the
                    // arrival does not move the text after it.
                    Found::Pending | Found::Blocked | Found::Unknown => declared
                        .map(|(w, h)| (self.sized_image((1.0, 1.0), Some(w), Some(h)), true))
                        .or(guess),
                    Found::Failed => None,
                };
                if let Some(((width, height), reserved)) = sized {
                    out.boxes.push(ImageBox {
                        at: out.text.len(),
                        id,
                        width,
                        height,
                        baseline: None,
                        math: None,
                        reserved,
                        align: info.align,
                    });
                    return;
                }
                let alt = self.doc.str(self.doc.images[index as usize].alt);
                if alt.is_empty() {
                    out.push_atom(
                        "[image]",
                        Style {
                            muted: true,
                            ..Style::default()
                        },
                        at,
                    );
                } else {
                    out.push_atom(
                        &format!("[image: {alt}]"),
                        Style {
                            muted: true,
                            italic: true,
                            ..Style::default()
                        },
                        at,
                    );
                }
            }
            SpanKind::FootnoteRef(index) => {
                let number = self.doc.footnotes[index as usize].number;
                out.push_atom(
                    &format!("[{number}]"),
                    Style {
                        link: true,
                        small: true,
                        ..Style::default()
                    },
                    at,
                );
            }
            SpanKind::Checkbox { checked } => {
                if out.text.is_empty() && out.task.is_none() {
                    out.task = Some(checked);
                } else {
                    // Only a task item starts with its box; anything else keeps a readable mark.
                    out.push_atom(
                        if checked { "[x] " } else { "[ ] " },
                        Style {
                            muted: true,
                            ..Style::default()
                        },
                        at,
                    );
                }
            }
            _ => {}
        }
    }

    pub(super) fn cell_inline(&mut self, cell: u32) -> Inline {
        let mut leaves = Vec::new();
        self.collect_leaves(cell, &mut leaves);
        let mut merged = Inline::default();
        let mut first = true;
        for leaf in leaves {
            let inline = self.inline_of(leaf);
            if inline.text.is_empty() && inline.boxes.is_empty() {
                continue;
            }
            if !first {
                merged.push(" ", Style::default());
            }
            let start = merged.text.len();
            for (range, style) in &inline.runs {
                let at = inline.doc_at(range.start).unwrap_or(0);
                merged.push_doc(&inline.text[range.clone()], *style, at);
            }
            // `inline_of` measured boxes in this leaf's own text; shift them into the merged text.
            for mut image in inline.boxes {
                image.at += start;
                merged.boxes.push(image);
            }
            for mut note in inline.notes {
                note.base = note.base.start + start..note.base.end + start;
                merged.notes.push(note);
            }
            first = false;
        }
        merged
    }

    pub(super) fn collect_leaves(&self, b: u32, out: &mut Vec<u32>) {
        let block = self.doc.blocks[b as usize];
        let kind = block.kind;
        // A Markdown table cell holds its inline content directly, so it is a leaf when it has no
        // child blocks; cells with block content recurse.
        let leaf = matches!(
            kind,
            BlockKind::Paragraph { .. }
                | BlockKind::Plain
                | BlockKind::Heading { .. }
                | BlockKind::CodeBlock { .. }
                | BlockKind::Math { .. }
                | BlockKind::Diagram { .. }
        ) || (matches!(kind, BlockKind::TableCell { .. }) && block.end == b + 1);
        if leaf {
            out.push(b);
            return;
        }
        for child in self.doc.children(b) {
            self.collect_leaves(child, out);
        }
    }
}
