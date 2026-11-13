//! Floated images (`<img align="left|right">`) and the text that runs beside them, across blocks.
//!
//! A floated image leaves its paragraph's line and stands at one side of the column. It stays
//! open until the page has passed its bottom, so the blocks after its paragraph are laid out
//! beside it too: text lines are shortened one by one, boxes (code, quotes, `<details>`) are
//! made narrower as a whole, and what cannot narrow (tables, diagrams, display math) starts below
//! it. A container (a list item, a quote, a `<details>`, a figure) grows to hold the floats that
//! start inside it, and the last block of the page does the same.
//!
//! **Out of order.** The page is laid out block by block, in any order, and a block beside a
//! float depends on the blocks before it. What a top-level block passes on is its *float
//! state*: the floats still open at its bottom edge, relative to that edge ([`Float`] with
//! relative rows, rounded to whole rows). [`FloatBook`] keeps the state of every measured block
//! that leaves one. A block's layout is a function of the document, the options, the image store
//! and the state the block before it left, so measuring it needs that state and nothing else:
//!
//! - Only a top-level block that holds a floated image (a *starter*, known from the model alone)
//!   can open a float. With no starter before a block, the state is empty and nothing is looked
//!   up: a document without floated images pays one comparison per measured block.
//! - Otherwise the state comes from the nearest measured block before it, or, when the blocks
//!   back to the last starter are all unmeasured, from measuring that starter and the blocks
//!   after it, in order, until its floats are closed (`relayout.rs`, `floats_before`). The same
//!   chain is walked whatever was measured before, so the result does not depend on the order.
//! - Blocks in such a run are always measured with loading allowed: a stand-in for an image
//!   would make every block after it a guess. So refinement does decode the images of floated
//!   runs, and only those.
//! - When a block's state changes (its image arrived, a `<details>` in the run was toggled), the
//!   block after it is measured again, and so on until a state comes out unchanged.
//!
//! A floated image is painted by the block that holds it and can hang below that block, so a
//! frame also paints the owners of the floats open at its first block (`frame.rs`).

use super::hit::FloatBox;
use super::inline::{ImageBox, Inline};
use super::pager::{Pager, Wrap};
use super::paint::Paint;
use mdview_core::model::{Align, BlockKind, Document, ImageAlign, SpanKind};
use std::collections::HashMap;
use std::rc::Rc;

/// Logical px between a floated image and the text beside it.
const GAP: f32 = 16.0;
/// Text keeps this far below a float before it runs under it.
const CLEAR: f32 = 4.0;
/// From a float's keep-out bottom to the next block that starts below it; with [`CLEAR`] the
/// usual 16 px between blocks.
const BELOW: f32 = 12.0;
/// The narrowest column text is laid out in beside a float; with less, it starts below.
pub(super) const MIN_TEXT: f32 = 120.0;
/// The same for a box (code, a quote, `<details>`).
const MIN_BOX: f32 = 240.0;

/// A floated image the layout has not passed yet. In a [`Pager`] the rows are page rows; in a
/// block's float state they are relative to the block's bottom edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Float {
    /// At the left of its column, so text starts after `x1`; otherwise text ends at `x0`.
    pub(super) left: bool,
    /// The columns text keeps out of: the image and the gap beside it.
    pub(super) x0: f32,
    pub(super) x1: f32,
    pub(super) top: f32,
    /// The image's bottom plus the clearance text keeps.
    pub(super) bottom: f32,
    /// The index of the top-level block that paints it; `u32::MAX` while that block is laid out.
    pub(super) owner: u32,
}

/// The float state of every measured top-level block that leaves one, and which blocks can
/// start a float at all.
#[derive(Default)]
pub(super) struct FloatBook {
    /// Indices (into the layout's blocks) of the top-level blocks that hold a floated image,
    /// in order.
    starters: Vec<u32>,
    /// The floats open below each block's bottom edge. Blocks that leave none have no entry.
    out: HashMap<u32, Rc<[Float]>>,
}

impl FloatBook {
    /// Finds the starters of `doc`, whose top-level blocks are `blocks` in order.
    pub(super) fn new(doc: &Document, blocks: impl Iterator<Item = u32>) -> Self {
        let floated = |index: u32| {
            matches!(
                doc.images[index as usize].align,
                ImageAlign::Left | ImageAlign::Right
            )
        };
        let mut starters = Vec::new();
        if (0..doc.images.len() as u32).any(floated) {
            for (index, block) in blocks.enumerate() {
                let end = doc.blocks[block as usize].end;
                let holds = (block..end).any(|b| {
                    doc.spans[doc.blocks[b as usize].spans.usize()]
                        .iter()
                        .any(|span| matches!(span.kind, SpanKind::Image(i) if floated(i)))
                });
                if holds {
                    starters.push(index as u32);
                }
            }
        }
        FloatBook {
            starters,
            out: HashMap::new(),
        }
    }

    /// Whether no float can be open at the top of block `index`: nothing before it starts one.
    pub(super) fn none_before(&self, index: usize) -> bool {
        self.starters.first().is_none_or(|&s| s as usize >= index)
    }

    pub(super) fn is_starter(&self, index: usize) -> bool {
        self.starters.binary_search(&(index as u32)).is_ok()
    }

    /// The last starter before `index`.
    pub(super) fn starter_before(&self, index: usize) -> Option<usize> {
        let at = self.starters.partition_point(|&s| (s as usize) < index);
        at.checked_sub(1).map(|at| self.starters[at] as usize)
    }

    /// The first starter at or after `index`.
    pub(super) fn starter_from(&self, index: usize) -> Option<usize> {
        let at = self.starters.partition_point(|&s| (s as usize) < index);
        self.starters.get(at).map(|&s| s as usize)
    }

    /// The floats block `index` left open, as last measured.
    pub(super) fn out(&self, index: usize) -> Option<Rc<[Float]>> {
        self.out.get(&(index as u32)).cloned()
    }

    /// Records what block `index` leaves open. True when that differs from what it left before.
    pub(super) fn set_out(&mut self, index: usize, floats: Vec<Float>) -> bool {
        let key = index as u32;
        match self.out.get(&key) {
            None if floats.is_empty() => false,
            Some(old) if **old == *floats => false,
            _ => {
                if floats.is_empty() {
                    self.out.remove(&key);
                } else {
                    self.out.insert(key, floats.into());
                }
                true
            }
        }
    }

    /// The blocks whose floated images hang below the top of block `first`, in order: a frame
    /// that starts there paints them as well.
    pub(super) fn owners(&self, first: usize) -> Vec<usize> {
        if self.out.is_empty() || first == 0 {
            return Vec::new();
        }
        let Some(open) = self.out.get(&(first as u32 - 1)) else {
            return Vec::new();
        };
        let mut owners: Vec<usize> = open.iter().map(|f| f.owner as usize).collect();
        owners.sort_unstable();
        owners.dedup();
        owners
    }
}

/// The free columns of `lo..hi` over the rows `top..bottom`, beside the floats that reach them.
pub(super) fn band(floats: &[Float], lo: f32, hi: f32, top: f32, bottom: f32) -> (f32, f32) {
    let (mut lo, mut hi) = (lo, hi);
    for float in floats {
        if float.top < bottom && top < float.bottom && float.x0 < hi && float.x1 > lo {
            if float.left {
                lo = lo.max(float.x1);
            } else {
                hi = hi.min(float.x0);
            }
        }
    }
    (lo, hi.max(lo))
}

impl<'a, 'b> Pager<'a, 'b> {
    /// Starts a top-level block below a block that left `open` (rows relative to its bottom
    /// edge, which is `self.y`).
    pub(super) fn floats_in(&mut self, open: &[Float]) {
        let y = self.y;
        self.floats.clear();
        self.floats.extend(open.iter().map(|float| Float {
            top: float.top + y,
            bottom: float.bottom + y,
            ..*float
        }));
        self.float_base = 0;
    }

    /// The floats still open at `self.y`, the bottom edge of a top-level block, relative to it
    /// and on whole rows. The one-pass page and the block-by-block layout both hand floats over
    /// through this, which is what makes them agree.
    pub(super) fn floats_out(&mut self) -> Vec<Float> {
        let y = self.y;
        self.floats
            .drain(..)
            .filter_map(|float| {
                let bottom = (float.bottom - y).round();
                (bottom > 0.0).then(|| Float {
                    top: (float.top - y).round(),
                    bottom,
                    ..float
                })
            })
            .collect()
    }

    /// Closes the floats opened since `mark`: what holds them (a container, or the page at its
    /// last block) reaches down to contain them.
    pub(super) fn end_floats(&mut self, mark: usize) {
        if self.floats.len() <= mark {
            return;
        }
        let below = self.px(BELOW);
        for float in self.floats.drain(mark..) {
            self.y = self.y.max(float.bottom + below);
        }
    }

    /// Moves `self.y` below every open float that shares columns with `left..left + width`, for
    /// a block that cannot stand beside one.
    pub(super) fn clear_floats(&mut self, left: f32, width: f32) {
        if self.floats.is_empty() {
            return;
        }
        let below = self.px(BELOW);
        let y = self.y;
        for float in &self.floats {
            if float.bottom > y && float.x0 < left + width && float.x1 > left {
                self.y = self.y.max(float.bottom + below);
            }
        }
    }

    /// The column for a box that keeps its shape (code, a quote, `<details>`): beside every float
    /// still open, however far down it starts, since the box's height is not known yet. With
    /// less than a usable width left, the box starts below the floats at full width.
    pub(super) fn beside(&mut self, left: f32, width: f32) -> (f32, f32) {
        if self.floats.is_empty() {
            return (left, width);
        }
        let (lo, hi) = band(&self.floats, left, left + width, self.y, f32::MAX);
        if hi - lo >= self.px(MIN_BOX).min(width) {
            (lo, hi - lo)
        } else {
            self.clear_floats(left, width);
            (left, width)
        }
    }

    /// The column for something one line tall (a rule, a list item's first line): beside the
    /// floats that reach the rows `self.y..self.y + rows`.
    pub(super) fn beside_rows(&self, left: f32, width: f32, rows: f32) -> (f32, f32) {
        if self.floats.is_empty() {
            return (left, width);
        }
        let (lo, hi) = band(&self.floats, left, left + width, self.y, self.y + rows);
        (lo, hi - lo)
    }

    /// Moves `self.y` down until a line `line` tall has `need` px of the column beside the open
    /// floats (or the whole column, when that is narrower).
    pub(super) fn clear_narrow(&mut self, left: f32, width: f32, line: f32, need: f32) {
        let below = self.px(BELOW);
        while !self.floats.is_empty() {
            let (y, right) = (self.y, left + width);
            let (lo, hi) = band(&self.floats, left, right, y, y + line);
            if hi - lo >= need.min(width) {
                return;
            }
            let next = self
                .floats
                .iter()
                .filter(|f| f.top < y + line && y < f.bottom && f.x0 < right && f.x1 > left)
                .map(|f| f.bottom)
                .min_by(f32::total_cmp);
            match next {
                Some(bottom) => self.y = bottom + below,
                None => return,
            }
        }
    }

    /// Where a new float `w` by `h` goes in the column `left..left + width`: at `self.y`, or
    /// lower when a float of its own side (opened in the same container) is in the way or the
    /// floats beside it leave too little room. Returns its x and y.
    fn place_float(&self, left: f32, width: f32, w: f32, h: f32, at_left: bool) -> (f32, f32) {
        let gap = self.px(GAP);
        let room = w + gap + self.px(MIN_TEXT);
        let right = left + width;
        let mut y = self.y;
        loop {
            let bottom = y + h;
            let (lo, hi) = band(&self.floats, left, right, y, bottom);
            let reach = |f: &&Float| f.top < bottom && y < f.bottom;
            let same = self.floats[self.float_base.min(self.floats.len())..]
                .iter()
                .filter(reach)
                .filter(|f| f.left == at_left)
                .map(|f| f.bottom)
                .max_by(f32::total_cmp);
            if same.is_none() && hi - lo >= room.min(width) {
                return (if at_left { lo } else { hi - w }, y);
            }
            let next = same.or_else(|| {
                self.floats
                    .iter()
                    .filter(reach)
                    .filter(|f| f.x0 < right && f.x1 > left)
                    .map(|f| f.bottom)
                    .min_by(f32::total_cmp)
            });
            match next {
                Some(next) if next > y => y = next + self.px(CLEAR),
                _ => return (if at_left { lo } else { hi - w }, y),
            }
        }
    }

    /// Prepares a text block's lines at `self.y` in the column `left..left + width`:
    ///
    /// - `<img align="left|right">`: each such image leaves the line and floats at that side
    ///   (when `may_float`, and when it is narrow enough to leave room for text beside it);
    /// - with floats open, the block moves down until it has room for a line beside them, and
    ///   for its widest inline image;
    /// - its lines are then broken one by one against the floats (`self.wrap`, which the caller
    ///   clears after its `text_layout`). Centred and right-aligned text is laid out in the
    ///   free column at its first line instead, so it is centred in what the floats leave.
    ///
    /// Returns the column to lay the text out in, and whether an image was floated.
    pub(super) fn begin_text(
        &mut self,
        block: u32,
        inline: &mut Inline,
        left: f32,
        width: f32,
        line: f32,
        align: Align,
        may_float: bool,
    ) -> (f32, f32, bool) {
        let gap = self.px(GAP);
        let min_text = self.px(MIN_TEXT);
        let floats = |image: &ImageBox| {
            matches!(image.align, ImageAlign::Left | ImageAlign::Right)
                && image.math.is_none()
                && image.width + gap + min_text <= width
        };
        let mut sides: Vec<ImageBox> = Vec::new();
        if may_float && inline.boxes.iter().any(floats) {
            sides.extend(inline.boxes.iter().copied().filter(floats));
            inline.boxes.retain(|image| !floats(image));
        }
        if sides.is_empty() && self.floats.is_empty() {
            return (left, width, false);
        }
        if !sides.is_empty() {
            inline.trim_start();
        }
        let has_text = !inline.text.trim().is_empty() || !inline.boxes.is_empty();
        if has_text {
            let widest = inline
                .boxes
                .iter()
                .filter(|image| image.math.is_none())
                .map(|image| image.width.min(width))
                .fold(min_text, f32::max);
            self.clear_narrow(left, width, line, widest);
        }
        let floated = !sides.is_empty();
        for image in sides {
            let at_left = image.align == ImageAlign::Left;
            let (x, y) = self.place_float(left, width, image.width, image.height, at_left);
            self.paints.push(if image.reserved {
                Paint::Rect {
                    x,
                    y,
                    w: image.width,
                    h: image.height,
                    radius: self.px(3.0),
                    color: self.theme.code_bg,
                }
            } else {
                Paint::Image {
                    index: image.id,
                    x,
                    y,
                    w: image.width,
                    h: image.height,
                }
            });
            if self.record {
                self.float_boxes.push(FloatBox {
                    x,
                    y,
                    w: image.width,
                    h: image.height,
                    block,
                    at: image.doc,
                });
            }
            self.floats.push(Float {
                left: at_left,
                x0: if at_left { x } else { x - gap },
                x1: if at_left {
                    x + image.width + gap
                } else {
                    x + image.width
                },
                top: y,
                bottom: y + image.height.round() + self.px(CLEAR),
                owner: u32::MAX,
            });
        }
        if matches!(align, Align::Center | Align::Right) {
            let (lo, hi) = band(&self.floats, left, left + width, self.y, self.y + line);
            return (lo, hi - lo, floated);
        }
        self.wrap = Some(Wrap {
            x: left,
            y: self.y,
            line,
        });
        (left, width, floated)
    }

    /// Whether a block's children keep their floats to themselves. A `<div>` does not: it is
    /// only a wrapper, and an image floated in it runs on past its end as in a browser.
    pub(super) fn bounds_floats(&self, parent: u32) -> bool {
        !matches!(self.doc.blocks[parent as usize].kind, BlockKind::Div { .. })
    }
}

/// The link a floated image sits in, if any: the nearest `Link` span around the image's own
/// in leaf block `block`.
pub(super) fn link_of_image(doc: &Document, block: u32, at: u32) -> Option<u32> {
    let range = doc.blocks.get(block as usize)?.spans.usize();
    let image = range
        .into_iter()
        .find(|&i| doc.spans[i].start == at && matches!(doc.spans[i].kind, SpanKind::Image(_)))?;
    let mut span = doc.spans[image].parent;
    while let Some(parent) = doc.spans.get(span as usize) {
        match parent.kind {
            SpanKind::Link(link) => return Some(link),
            _ => span = parent.parent,
        }
    }
    None
}
