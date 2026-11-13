//! Measuring blocks and correcting the layout in place.
//!
//! The renderer keeps an anchor for the viewport: a block and the offset of the viewport's top
//! from that block's top. A correction to a block above the anchor moves the anchor block, and
//! the viewport with it, so what is on screen stays where it was; a correction to the anchor block
//! or below it leaves the top alone. `top()` reads the anchored top back for the shell.
//!
//! A kept frame survives a correction above it when the shift is a whole number of rows (only
//! its `top` moves); any other correction that reaches what it shows marks it stale, and the next
//! frame is drawn whole with `band = None` rather than with a shift that would be wrong.

use super::page::{Measure, top_level};
use super::pager::Pager;
use super::paint::CachedBlock;
use super::store::Slot;
use super::{DocumentRenderer, Options, PageLayout};
use crate::image::DecodedImage;
use mdview_core::model::{Document, NONE};
use std::rc::Rc;

/// Where the viewport is: a block (`index`; the footnote list is the index after the last block)
/// and the offset of the viewport's top from that block's top, which may be negative (a jump
/// leaves a margin above its target).
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Anchor {
    pub(super) index: usize,
    pub(super) offset: f32,
}

#[derive(Default)]
pub(super) struct ViewState {
    pub(super) anchor: Anchor,
    /// The viewport height of the last frame or jump, for clamping `top()`.
    pub(super) height: f32,
    /// The kept frame's first and last block index, while it still shows the layout.
    pub(super) span: Option<(usize, usize)>,
    /// Corrections above the kept frame since it was drawn, device px.
    pub(super) shift: f32,
    /// A correction reached what the kept frame shows; the next frame is drawn whole.
    pub(super) stale: bool,
    /// Page rows where an animated image changed frame since the kept frame was drawn.
    pub(super) repaint: Option<(f32, f32)>,
    /// Blocks a toggle or an arrival invalidated, measured again by the next call that has the
    /// document (a frame, a jump, a hit test, a selection) wherever they are, so a correction is
    /// never left waiting for the block to scroll into view.
    pub(super) pending: Vec<usize>,
    /// Refinement's two cursors: the next index to look at from the anchor down, and from the
    /// top of the page. Each only moves forward, so a whole refinement looks at each block once.
    pub(super) ahead: usize,
    pub(super) behind: usize,
}

/// Something to bring into view with [`DocumentRenderer::jump_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// A block, nested or not: its top-level block goes to the top of the viewport.
    Block(u32),
    /// A footnote entry, by the number the text shows.
    Footnote(u32),
    /// An offset into `Document::text`: the line holding it goes to the top (a find match).
    Text(u32),
}

/// What the shell decided about a remote image, for [`DocumentRenderer::set_remote`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteState {
    /// Being fetched: the placeholder (or a reserved box) until it arrives.
    Pending,
    /// Not permitted: the placeholder stays.
    Blocked,
}

impl DocumentRenderer {
    /// Lays out every block now, loading every image, formula and diagram: the eager form of
    /// [`layout`](Self::layout), for tests and one-shot renders.
    pub fn layout_exact(&mut self, doc: &Document, options: &Options) -> &PageLayout {
        self.start_layout(doc, options);
        let count = self.page.as_ref().map_or(0, PageLayout::len);
        for index in 0..=count {
            // Only the block being measured is protected from eviction.
            self.store.begin_frame();
            self.measure(doc, index, true, false);
            // Place the next block now, so it is measured at its own top; the settle below adds
            // the same heights in the same order.
            if let Some(page) = self.page.as_mut() {
                let y = page.y_at(index) + page.height_at(index);
                match page.spans.get_mut(index + 1) {
                    Some(span) => span.y = y,
                    None if index < count => page.foot.y = y,
                    None => {}
                }
            }
        }
        self.settle();
        self.page.as_ref().expect("laid out")
    }

    /// A fresh layout with every height still zero and nothing measured.
    pub(super) fn start_layout(&mut self, doc: &Document, options: &Options) {
        self.drop_layout();
        let scale = options.scale.max(0.1);
        let spans = doc
            .children(NONE)
            .map(|block| super::BlockBox {
                block,
                y: 0.0,
                height: 0.0,
            })
            .collect();
        let mut page = PageLayout::new(doc, options, spans, (36.0 * scale).round());
        if !doc.footnotes.iter().any(|f| f.number > 0 && f.defined) {
            let count = page.len();
            page.set(count, 0.0, Measure::Exact);
        }
        self.page = Some(page);
        self.options = Some(options.clone());
    }

    /// Forgets the layout and everything drawn from it. The image store stays.
    pub(super) fn drop_layout(&mut self) {
        self.page = None;
        self.options = None;
        self.cache.clear();
        self.frame = None;
        self.uses.clear();
        if !self.undrawn.is_empty() {
            self.undrawn.clear();
            self.undrawn_changes += 1;
        }
        self.view = ViewState::default();
    }

    /// Lays out again if there is no layout or it was made for other options.
    pub(super) fn ensure_layout(&mut self, doc: &Document, options: &Options) {
        let fresh = self
            .options
            .as_ref()
            .is_some_and(|known| known.same_layout(options))
            && self
                .page
                .as_ref()
                .is_some_and(|page| page.shape == (doc.text.len(), doc.blocks.len()));
        if !fresh {
            self.layout(doc, options);
        }
    }

    /// Lays out block `index` (the footnote list for `len()`) at its current top and records its
    /// height. `allow` lets the pass decode and run engine jobs; `keep` caches the paints and caret
    /// geometry for frames and hit tests, relative to the whole row above the top (see
    /// [`CachedBlock`]). True when the height changed.
    pub(super) fn measure(
        &mut self,
        doc: &Document,
        index: usize,
        allow: bool,
        keep: bool,
    ) -> bool {
        let (Some(page), Some(options)) = (self.page.as_ref(), self.options.as_ref()) else {
            return false;
        };
        let block = page.spans.get(index).map(|span| span.block);
        let top = page.y_at(index);
        let base = top.floor();
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            keep,
            &mut self.store,
            &self.details,
        );
        pager.allow = allow;
        pager.max_y = f32::MAX;
        pager.y = top;
        match block {
            Some(block) => {
                let (left, width, color) = (pager.left, pager.content_width(), pager.theme.text);
                pager.block(block, left, width, color, 0);
            }
            None => pager.footnotes(),
        }
        let height = pager.y - top;
        let measure = if pager.provisional {
            Measure::Provisional
        } else {
            Measure::Exact
        };
        let used = std::mem::take(&mut pager.used);
        let undrawn = std::mem::take(&mut pager.undrawn);
        let notes = std::mem::take(&mut pager.notes);
        let mut paints = std::mem::take(&mut pager.paints);
        let mut lines = std::mem::take(&mut pager.hits);
        let mut sources = std::mem::take(&mut pager.sources);
        let mut tasks = std::mem::take(&mut pager.tasks);
        drop(pager);
        match block {
            Some(block) if keep => {
                paints.iter_mut().for_each(|paint| paint.shift(-base));
                lines.iter_mut().for_each(|line| line.shift(-base));
                sources.iter_mut().for_each(|source| source.shift(-base));
                tasks.iter_mut().for_each(|task| task.shift(-base));
                let phase = top - base;
                self.cache.insert(
                    block,
                    CachedBlock {
                        paints,
                        lines,
                        sources,
                        tasks,
                        phase,
                    },
                );
            }
            Some(_) => {}
            None => {
                if let Some(page) = self.page.as_mut() {
                    page.foot.notes = notes.into_iter().map(|(n, y)| (n, y - top)).collect();
                }
            }
        }
        for range in undrawn {
            if let Err(at) = self.undrawn.binary_search(&range) {
                self.undrawn.insert(at, range);
                self.undrawn_changes += 1;
            }
        }
        for id in used {
            let users = self.uses.entry(id).or_default();
            if users.last() != Some(&(index as u32)) && !users.contains(&(index as u32)) {
                users.push(index as u32);
            }
        }
        self.correct(index, height, measure)
    }

    /// Records a block's new height, noting what that does to the kept frame.
    pub(super) fn correct(&mut self, index: usize, height: f32, measure: Measure) -> bool {
        let Some(page) = self.page.as_mut() else {
            return false;
        };
        let old = page.height_at(index);
        let changed = page.set(index, height, measure);
        if changed && let Some((first, last)) = self.view.span {
            if index < first {
                self.view.shift += height - old;
            } else if index <= last {
                self.view.stale = true;
            }
        }
        changed
    }

    /// Marks a block for measuring again and drops its cached paints.
    pub(super) fn invalidate(&mut self, index: usize) {
        let Some(page) = self.page.as_mut() else {
            return;
        };
        if let Some(span) = page.spans.get(index) {
            self.cache.remove(&span.block);
        }
        let height = page.height_at(index);
        page.set(index, height, Measure::Estimated);
        if let Some((first, last)) = self.view.span
            && (first..=last).contains(&index)
        {
            self.view.stale = true;
        }
        if !self.view.pending.contains(&index) {
            self.view.pending.push(index);
        }
    }

    /// Measures the blocks invalidated since the last call and rebuilds positions.
    pub(super) fn correct_pending(&mut self, doc: &Document) {
        for index in std::mem::take(&mut self.view.pending) {
            if !self.ready(index) {
                self.measure(doc, index, true, true);
            }
        }
        self.settle();
    }

    /// Rebuilds positions after a batch of corrections and carries the kept frame along: a whole
    /// row shift above it moves its `top`, anything else makes it stale.
    pub(super) fn settle(&mut self) -> bool {
        let Some(page) = self.page.as_mut() else {
            return false;
        };
        let moved = page.settle();
        if self.view.shift != 0.0 {
            let shift = self.view.shift;
            self.view.shift = 0.0;
            match self.frame.as_mut() {
                Some(frame) if !self.view.stale && shift.fract() == 0.0 => frame.top += shift,
                _ => self.view.stale = true,
            }
        }
        if self.view.stale {
            self.view.span = None;
        }
        moved
    }

    /// Whether block `index` can be drawn as it is: measured with everything loaded and, for a
    /// top-level block, its paints cached.
    pub(super) fn ready(&self, index: usize) -> bool {
        let Some(page) = self.page.as_ref() else {
            return true;
        };
        page.measure_at(index) == Measure::Exact
            && page.spans.get(index).is_none_or(|span| {
                self.cache
                    .get(&span.block)
                    .is_some_and(|cached| cached.fits(span.y))
            })
    }

    /// The anchored viewport top, unclamped.
    fn anchor_top(&self) -> f32 {
        let anchor = self.view.anchor;
        self.page
            .as_ref()
            .map_or(0.0, |page| page.y_at(anchor.index) + anchor.offset)
    }

    /// The viewport top in the current coordinates: where the anchor is now, clamped to the page.
    /// A shell reads this after any call that may have corrected the layout and adopts it as its
    /// scroll offset. `None` before a layout.
    pub fn top(&self) -> Option<f32> {
        let page = self.page.as_ref()?;
        let max = (page.total_height - self.view.height).max(0.0);
        Some(self.anchor_top().clamp(0.0, max))
    }

    /// Moves the viewport's anchor to page row `top` in the current coordinates: the shell
    /// scrolled. [`view_frame`](Self::view_frame) does this itself.
    pub fn set_top(&mut self, top: f32) {
        let Some(page) = self.page.as_ref() else {
            return;
        };
        let count = page.len();
        let index = if page.foot.height > 0.0 && top >= page.foot.y {
            count
        } else {
            page.index_at(top).unwrap_or(count)
        };
        self.view.anchor = Anchor {
            index,
            offset: top - page.y_at(index),
        };
    }

    /// Measures every block the viewport of `height` rows at the anchor shows, keeping the anchor,
    /// until nothing on screen is left to measure. Returns the clamped top.
    pub(super) fn settle_view(&mut self, doc: &Document, height: f32) -> f32 {
        self.correct_pending(doc);
        loop {
            self.settle();
            let Some(page) = self.page.as_ref() else {
                return 0.0;
            };
            let max = (page.total_height - height).max(0.0);
            let top = self.anchor_top().clamp(0.0, max);
            let bottom = top + height;
            let count = page.len();
            let mut need = Vec::new();
            let mut index = page.first_below(top);
            while index <= count && page.y_at(index) < bottom {
                if !self.ready(index) {
                    need.push(index);
                }
                index += 1;
            }
            if need.is_empty() {
                return top;
            }
            for index in need {
                self.measure(doc, index, true, true);
            }
        }
    }

    /// Brings a block or a footnote entry to `margin` device px below the top of a viewport of
    /// `height` rows, measuring what that viewport shows first so the target lands exactly even
    /// while heights elsewhere are estimates. Returns the new top, which the next
    /// [`view_frame`](Self::view_frame) should be given; `None` when the target is not on the page.
    pub fn jump_to(
        &mut self,
        doc: &Document,
        options: &Options,
        target: Jump,
        margin: f32,
        height: f32,
    ) -> Option<f32> {
        self.ensure_layout(doc, options);
        self.store.begin_frame();
        self.correct_pending(doc);
        let anchor = match target {
            Jump::Block(block) => {
                let top = top_level(doc, block)?;
                let index = self.page.as_ref()?.index_of(top)?;
                Anchor {
                    index,
                    offset: -margin,
                }
            }
            Jump::Text(offset) => {
                let top = top_level(doc, doc.block_at(offset)?)?;
                let index = self.page.as_ref()?.index_of(top)?;
                if !self.ready(index) {
                    self.measure(doc, index, true, true);
                    self.settle();
                }
                let span = self.page.as_ref()?.spans[index];
                let cached = self.cache.get(&span.block)?;
                // The line holding the offset, or the first one after it.
                let line = cached
                    .lines
                    .iter()
                    .filter(|line| line.doc_end > offset)
                    .min_by(|a, b| a.top.total_cmp(&b.top))
                    .or(cached.lines.first());
                let line_top = line.map_or(0.0, |line| line.top - cached.phase);
                Anchor {
                    index,
                    offset: line_top - margin,
                }
            }
            Jump::Footnote(number) => {
                let count = self.page.as_ref()?.len();
                if !self.ready(count) {
                    self.measure(doc, count, true, true);
                }
                self.settle();
                let page = self.page.as_ref()?;
                let (_, y) = page.foot.notes.iter().find(|(n, _)| *n == number)?;
                Anchor {
                    index: count,
                    offset: y - margin,
                }
            }
        };
        self.view.anchor = anchor;
        self.view.height = height.max(1.0);
        let top = self.settle_view(doc, height.max(1.0));
        Some(top)
    }

    /// Opens or closes a `<details>`. Only the top-level block holding it is measured again, by the
    /// next frame, jump, hit test or selection; its cached paints go, everything else stays.
    pub fn set_details(&mut self, doc: &Document, block: u32, open: bool) {
        if self.details.insert(block, open) == Some(open) {
            return;
        }
        let index = top_level(doc, block).and_then(|top| self.page.as_ref()?.index_of(top));
        if let Some(index) = index {
            self.invalidate(index);
        }
    }

    /// Delivers a remote image the shell fetched (`None`: the fetch failed), by the URL the
    /// document wrote. Blocks that use it are measured again by the next call that has the
    /// document (see [`set_details`](Self::set_details)). True
    /// when the current layout uses it, so the shell knows to draw again.
    pub fn image_arrived(&mut self, url: &str, image: Option<DecodedImage>) -> bool {
        let slot = match image {
            Some(image) => Slot::Ready(Rc::new(image)),
            None => Slot::Failed,
        };
        let (id, changed) = self.store.set_remote(url.trim(), slot, 1.0);
        changed && self.reflow_users(id)
    }

    /// Delivers the bytes a shell fetched for a remote image (`None`: the fetch failed). Raster
    /// formats are decoded as they are; an SVG is rasterized at the device `scale`, so a badge is
    /// as sharp as the text around it. A shell that changes scale delivers its SVGs again. True
    /// when the current layout uses the image.
    pub fn remote_arrived(&mut self, url: &str, bytes: Option<&[u8]>, scale: f32) -> bool {
        let (image, density) = match bytes {
            Some(bytes) if crate::image::looks_like_svg(bytes) => {
                self.store.count_engine_job();
                (crate::engine::render_svg_at(bytes, scale), scale)
            }
            Some(bytes) => (crate::image::decode(bytes), 1.0),
            None => (None, 1.0),
        };
        let slot = match image {
            Some(image) => Slot::Ready(Rc::new(image)),
            None => Slot::Failed,
        };
        let (id, changed) = self.store.set_remote(url.trim(), slot, density);
        if changed && let Some(bytes) = bytes {
            self.store.offer(id, std::borrow::Cow::Borrowed(bytes));
        }
        changed && self.reflow_users(id)
    }

    /// Records that a remote image is being fetched or will not be. A block whose image had a
    /// declared size keeps its box either way. True when the current layout uses it.
    pub fn set_remote(&mut self, url: &str, state: RemoteState) -> bool {
        let slot = match state {
            RemoteState::Pending => Slot::Pending,
            RemoteState::Blocked => Slot::Blocked,
        };
        let (id, changed) = self.store.set_remote(url.trim(), slot, 1.0);
        changed && self.reflow_users(id)
    }

    /// Remote image URLs that laid-out blocks met and nobody has decided about yet (each once),
    /// for the shell to fetch, or to block with [`set_remote`](Self::set_remote). An image evicted
    /// from the store comes back here, since the renderer cannot fetch it again itself.
    pub fn take_remote_requests(&mut self) -> Vec<String> {
        self.store.take_remote()
    }

    /// Invalidates every block that used store id `id`.
    fn reflow_users(&mut self, id: u32) -> bool {
        let users = self.uses.remove(&id).unwrap_or_default();
        for &index in &users {
            self.invalidate(index as usize);
        }
        !users.is_empty()
    }
}
