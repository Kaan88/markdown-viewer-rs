//! [`PageLayout`], the vertical extent of every top-level block, and the footnote list at the end
//! of the page.
//!
//! Each block keeps its own height; positions are derived. A correction (a block measured for
//! the first time, re-measured after a `<details>` toggle or an image arrival) changes one height
//! and marks the positions from there on stale; [`PageLayout::settle`] rebuilds them once per
//! batch of corrections with a single prefix-sum pass from the first changed block. A Fenwick tree
//! would make each correction O(log n), but then every position lookup is O(log n) too, `blocks()`
//! could no longer hand out a slice, and the binary searches by row would need a tree walk; the
//! linear rebuild costs about a millisecond per million blocks and runs once per batch, which the
//! batching (a frame, an idle slice) keeps rare.
//!
//! A block is measured on its own, at its current top, and its height is recorded apart from
//! where it sits: measuring it now or after the blocks above it change gives the same number
//! (only rounding to page rows inside it depends on the fraction of a row it starts at, which is
//! why cached paints remember that fraction), and a lazily corrected layout converges to exactly
//! the eager one.

use super::inline::{Inline, Style};
use super::pager::{Pager, Txt};
use super::paint::Paint;
use super::{BlockBox, Options};
use mdview_core::model::{Document, Footnote, NONE};

/// How far a block's extent can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Measure {
    /// A guess from the block's kind and text length; nothing was shaped. Also a block whose
    /// last measurement no longer holds (a toggled `<details>`, an arrived image).
    Estimated,
    /// Laid out, standing in for a local image, formula or diagram that the pass was not allowed
    /// to load (refinement never decodes for off-screen blocks). It becomes exact when shown.
    Provisional,
    /// Laid out with everything it draws.
    Exact,
}

/// The footnote list after the last block. In index terms it is the block after the last one.
#[derive(Clone)]
pub(super) struct Footnotes {
    pub(super) y: f32,
    pub(super) height: f32,
    /// Each entry's number and its y relative to the list's top.
    pub(super) notes: Vec<(u32, f32)>,
    pub(super) measure: Measure,
}

/// Every top-level block's vertical extent, without any painting. The renderer owns it and
/// corrects it in place; a shell reads it through `DocumentRenderer::page_layout`.
#[derive(Clone)]
pub struct PageLayout {
    pub(super) spans: Vec<BlockBox>,
    pub(super) measure: Vec<Measure>,
    pub(super) foot: Footnotes,
    /// Space above the first block and below the footnotes, device px.
    pub(super) top: f32,
    pub(super) bottom: f32,
    pub(super) total_height: f32,
    /// The first index whose `y` is stale; `usize::MAX` when everything is settled.
    dirty: usize,
    generation: u64,
    estimated: usize,
    pub(super) width: f32,
    pub(super) scale: f32,
    pub(super) theme: &'static str,
    /// The document's text and block counts, so a layout used with another document is caught.
    pub(super) shape: (usize, usize),
}

impl PageLayout {
    /// A layout whose blocks all carry the given (estimated) heights.
    pub(super) fn new(doc: &Document, options: &Options, spans: Vec<BlockBox>, top: f32) -> Self {
        let scale = options.scale.max(0.1);
        let count = spans.len();
        let mut page = PageLayout {
            spans,
            measure: vec![Measure::Estimated; count],
            foot: Footnotes {
                y: 0.0,
                height: 0.0,
                notes: Vec::new(),
                measure: Measure::Estimated,
            },
            top,
            bottom: (24.0 * scale).round(),
            total_height: 0.0,
            dirty: 0,
            generation: 0,
            estimated: count,
            width: options.width,
            scale: options.scale,
            theme: options.theme.name,
            shape: (doc.text.len(), doc.blocks.len()),
        };
        page.settle();
        page
    }

    pub fn total_height(&self) -> f32 {
        self.total_height
    }

    /// Every top-level block's extent, in page order. Positions of blocks not measured yet are
    /// estimates; see [`estimated_blocks`](Self::estimated_blocks).
    pub fn blocks(&self) -> &[BlockBox] {
        &self.spans
    }

    /// How many top-level blocks (and the footnote list) still have estimated heights. Zero means
    /// every position comes from a real layout.
    pub fn estimated_blocks(&self) -> usize {
        self.estimated + usize::from(self.foot.measure == Measure::Estimated)
    }

    /// Bumped whenever any position changes, so a shell knows when to recompute what it derived
    /// from them (heading rows, the current heading).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The top-level block at page row `y`. Clamps into the first or last block outside the
    /// content, so a click in the page margins still lands on text.
    pub fn block_at(&self, y: f32) -> Option<BlockBox> {
        self.spans.get(self.index_at(y)?).copied()
    }

    /// The index of the block at page row `y`, for callers that keep parallel data.
    pub fn index_at(&self, y: f32) -> Option<usize> {
        if self.spans.is_empty() {
            return None;
        }
        if y < self.spans[0].y {
            return Some(0);
        }
        let index = self
            .spans
            .partition_point(|span| span.y + span.height <= y)
            .min(self.spans.len() - 1);
        Some(index)
    }

    /// The index of top-level block `block`; `blocks()` lists them in increasing order.
    pub(super) fn index_of(&self, block: u32) -> Option<usize> {
        self.spans.binary_search_by_key(&block, |s| s.block).ok()
    }

    /// The page row of a block, including one nested in a list, quote, table or details, so an
    /// anchor can be scrolled to. Footnote entries are not drawn where their definitions are; use
    /// [`footnote_y`](Self::footnote_y) for those. To bring a block into view while heights above
    /// it are still estimates, use `DocumentRenderer::jump_to`, which measures around it first.
    pub fn y_of(&self, doc: &Document, block: u32) -> Option<f32> {
        let top = top_level(doc, block)?;
        self.index_of(top).map(|i| self.spans[i].y)
    }

    /// The page row of a footnote entry, by the number shown in the text. `None` until the
    /// footnote list has been measured.
    pub fn footnote_y(&self, number: u32) -> Option<f32> {
        if self.foot.measure == Measure::Estimated {
            return None;
        }
        self.foot
            .notes
            .iter()
            .find(|(n, _)| *n == number)
            .map(|(_, y)| self.foot.y + *y)
    }

    pub fn matches(&self, options: &Options) -> bool {
        self.width == options.width
            && (self.scale - options.scale).abs() < 0.001
            && self.theme == options.theme.name
    }

    /// Top-level blocks; the footnote list is index `len()`.
    pub(super) fn len(&self) -> usize {
        self.spans.len()
    }

    pub(super) fn y_at(&self, index: usize) -> f32 {
        self.spans.get(index).map_or(self.foot.y, |s| s.y)
    }

    pub(super) fn height_at(&self, index: usize) -> f32 {
        self.spans.get(index).map_or(self.foot.height, |s| s.height)
    }

    pub(super) fn measure_at(&self, index: usize) -> Measure {
        self.measure
            .get(index)
            .copied()
            .unwrap_or(self.foot.measure)
    }

    /// The first index whose extent reaches below `y`, the footnote list counting as the last;
    /// `len() + 1` past the end.
    pub(super) fn first_below(&self, y: f32) -> usize {
        let index = self.spans.partition_point(|span| span.y + span.height <= y);
        if index < self.spans.len() || self.foot.y + self.foot.height > y {
            index
        } else {
            self.spans.len() + 1
        }
    }

    /// Sets one block's height and how far it can be trusted. True when the height changed.
    pub(super) fn set(&mut self, index: usize, height: f32, measure: Measure) -> bool {
        let was = self.measure_at(index);
        if index < self.len() && (was == Measure::Estimated) != (measure == Measure::Estimated) {
            if measure == Measure::Estimated {
                self.estimated += 1;
            } else {
                self.estimated -= 1;
            }
        }
        let old = self.height_at(index);
        match self.spans.get_mut(index) {
            Some(span) => span.height = height,
            None => self.foot.height = height,
        }
        match self.measure.get_mut(index) {
            Some(m) => *m = measure,
            None => self.foot.measure = measure,
        }
        let changed = old.to_bits() != height.to_bits();
        if changed {
            self.dirty = self.dirty.min(index + 1);
        }
        changed
    }

    /// Rebuilds the positions from the first changed block. True when anything moved.
    pub(super) fn settle(&mut self) -> bool {
        if self.dirty == usize::MAX {
            return false;
        }
        let from = self.dirty.min(self.spans.len());
        let mut y = match from {
            0 => self.top,
            _ => self.spans[from - 1].y + self.spans[from - 1].height,
        };
        for span in &mut self.spans[from..] {
            span.y = y;
            y += span.height;
        }
        self.foot.y = y;
        self.total_height = y + self.foot.height + self.bottom;
        self.dirty = usize::MAX;
        self.generation += 1;
        true
    }
}

/// The top-level block a block sits in.
pub(super) fn top_level(doc: &Document, block: u32) -> Option<u32> {
    let mut block = block;
    loop {
        let parent = doc.blocks.get(block as usize)?.parent;
        if parent == NONE {
            return Some(block);
        }
        block = parent;
    }
}

impl<'a, 'b> Pager<'a, 'b> {
    pub(super) fn footnotes(&mut self) {
        let mut notes: Vec<&Footnote> = self
            .doc
            .footnotes
            .iter()
            .filter(|f| f.number > 0 && f.defined)
            .collect();
        if notes.is_empty() {
            return;
        }
        notes.sort_by_key(|f| f.number);
        self.y += self.px(20.0);
        let left = self.left;
        let width = self.content_width();
        let hr = self.hair();
        self.paints.push(Paint::Rect {
            x: left,
            y: self.y,
            w: width,
            h: hr,
            radius: 0.0,
            color: self.theme.border,
        });
        self.y += hr + self.px(10.0);
        // The links borrow `self.doc`, so collect the text first.
        let entries: Vec<(u32, Inline)> = notes
            .iter()
            .map(|note| {
                let mut inline = Inline::default();
                inline.push(
                    &format!("{}. ", note.number),
                    Style {
                        muted: true,
                        ..Style::default()
                    },
                );
                if note.block != NONE {
                    let mut leaves = Vec::new();
                    self.collect_leaves(note.block, &mut leaves);
                    let mut first = true;
                    for leaf in leaves {
                        let body = self.inline_of(leaf);
                        if body.text.is_empty() {
                            continue;
                        }
                        if !first {
                            inline.push(" ", Style::default());
                        }
                        for (range, style) in &body.runs {
                            let at = body.doc_at(range.start).unwrap_or(0);
                            inline.push_doc(&body.text[range.clone()], *style, at);
                        }
                        first = false;
                    }
                } else {
                    inline.push(
                        "definition was too deep to keep",
                        Style {
                            muted: true,
                            italic: true,
                            ..Style::default()
                        },
                    );
                }
                (note.number, inline)
            })
            .collect();
        for (number, inline) in entries {
            let txt = Txt {
                size: 14.0,
                line_height: 1.6,
                weight: 400.0,
                mono: false,
                color: self.theme.muted,
                max: Some(width),
            };
            let layout = self.text_layout(&inline, &txt);
            let height = layout.height().round();
            let size = txt.size;
            self.notes.push((number, self.y));
            self.emit(&inline, layout, left, self.y, size);
            self.y += height + self.px(6.0);
        }
    }
}
