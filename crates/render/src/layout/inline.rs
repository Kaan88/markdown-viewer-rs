//! Inline content: [`Inline`] runs with their styles, image boxes and document-offset origins, and
//! the conversion from a model leaf block (spans, atoms, inline math, images, footnote marks,
//! checkboxes) into those runs, including the merged text of table cells.

use super::pager::Pager;
use crate::engine;
use crate::theme::Color;
use mdview_core::model::{BlockKind, SpanKind};
use std::ops::Range;
use std::rc::Rc;

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
    pub(super) muted: bool,
    pub(super) accent: bool,
    pub(super) small: bool,
    pub(super) sub: bool,
    pub(super) sup: bool,
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
}

/// One inline image: its text offset, the image it draws, its size in device px and, for math, the
/// distance from its top to the baseline.
#[derive(Clone, Copy)]
pub(super) struct ImageBox {
    pub(super) at: usize,
    pub(super) index: u32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) baseline: Option<f32>,
    /// End of the TeX source in the document for a math box; zero for a file image.
    pub(super) math_end: u32,
}

/// Math images live above the file-image indices in the shared cache, keyed by the source's
/// document offset (there is at most one math span per offset).
const MATH_INDEX: u32 = 0x8000_0000;
/// Diagrams get their own band, in case a diagram and a math block ever share an offset.
const DIAGRAM_INDEX: u32 = 0x4000_0000;

pub(super) fn math_index(offset: u32) -> u32 {
    MATH_INDEX | (offset & 0x7fff_ffff)
}

pub(super) fn diagram_index(offset: u32) -> u32 {
    DIAGRAM_INDEX | (offset & 0x3fff_ffff)
}

impl Inline {
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
        None
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
        let block = self.doc.blocks[block as usize];
        let base = block.text.start as usize;
        let end = block.text.end as usize;
        let src = &self.doc.text[base..end];
        let mut cuts = vec![base, end];
        let mut spans: Vec<(usize, usize, SpanKind)> = Vec::new();
        let mut atoms: Vec<(usize, usize, SpanKind)> = Vec::new();
        let mut math_ranges: Vec<(usize, usize)> = Vec::new();
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
                } else {
                    spans.push((start, stop, span.kind));
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Inline::default();
        for i in 0..cuts.len() {
            let at = cuts[i];
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
                .any(|(start, end)| at >= *start && at < *end)
            {
                continue;
            }
            let style = self.style_at(&spans, at);
            out.push_doc(&src[at - base..to - base], style, at as u32);
        }
        out
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
                SpanKind::Mark => {}
                SpanKind::Small => style.small = true,
                SpanKind::Link(_) => style.link = true,
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
                let key = math_index(at);
                if oversize.is_none() && !self.images.contains_key(&key) {
                    let source = self.doc.text.get(at as usize..end as usize).unwrap_or("");
                    // Twice the body size keeps the raster crisp when a heading or a zoom
                    // enlarges it; the box and baseline are halved back in `text_layout`.
                    let size = self.theme.body_size * 2.0 * self.scale;
                    let decoded =
                        engine::render_math(source, false, size, self.theme.text).map(Rc::new);
                    self.images.insert(key, decoded);
                }
                if oversize.is_none()
                    && let Some(Some(image)) = self.images.get(&key)
                {
                    let width = image.width as f32 / 2.0;
                    let height = image.height as f32 / 2.0;
                    let baseline = image.baseline as f32 / 2.0;
                    out.boxes.push(ImageBox {
                        at: out.text.len(),
                        index: key,
                        width,
                        height,
                        baseline: Some(baseline),
                        math_end: end,
                    });
                    return;
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
                if let Some(image) = self.image(index) {
                    let natural = (
                        image.width as f32 * self.scale,
                        image.height as f32 * self.scale,
                    );
                    let info = self.doc.images[index as usize];
                    let (width, height) = self.sized_image(natural, info.width, info.height);
                    out.boxes.push(ImageBox {
                        at: out.text.len(),
                        index,
                        width,
                        height,
                        baseline: None,
                        math_end: 0,
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
