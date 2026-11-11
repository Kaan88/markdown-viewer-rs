//! The lazy layout: [`DocumentRenderer::layout`] estimates every block without shaping, and
//! [`DocumentRenderer::refine`] measures the estimated ones in time slices the shell hands over
//! between events.
//!
//! What a frame, a hit test, a selection or a jump touches is measured exactly on the spot (see
//! `relayout.rs`), so refinement only has to make the off-screen part of the page right: the
//! scroll bar, the total height, the positions a jump or the outline reads. It never loads
//! anything for that: a block that draws a local image, formula or diagram the store does not
//! have is measured with a typical box in its place and left [`Measure::Provisional`]. It
//! becomes exact (loaded and laid out with everything it draws) when it is first shown.
//!
//! So when refinement is done, every block is exact or provisional, and the layout matches an
//! eager one ([`DocumentRenderer::layout_exact`]) except for the provisional blocks; scrolling
//! through the page makes those exact too, after which the two agree byte for byte.

use super::page::Measure;
use super::pager::Pager;
use super::{DocumentRenderer, Options, PageLayout};
use mdview_core::model::Document;
use std::time::{Duration, Instant};

impl DocumentRenderer {
    /// Lays the document out for these options and returns the layout, which the renderer keeps
    /// and corrects. Every height is an estimate (nothing is shaped, decoded or sent to the
    /// engine); a frame measures what it shows, and [`refine`](Self::refine) the rest. Do this
    /// once per document, width, scale and theme; everything else corrects it in place.
    pub fn layout(&mut self, doc: &Document, options: &Options) -> &PageLayout {
        self.start_layout(doc, options);
        let metrics = self.metrics(doc, options);
        let blocks: Vec<u32> = self
            .page
            .as_ref()
            .map(|page| page.spans.iter().map(|span| span.block).collect())
            .unwrap_or_default();
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            false,
            &mut self.store,
            &self.details,
        );
        pager.allow = false;
        let width = pager.content_width();
        let heights: Vec<f32> = blocks
            .iter()
            .map(|&block| pager.estimate(&metrics, block, width))
            .collect();
        let foot = pager.estimate_footnotes(&metrics);
        drop(pager);
        let page = self.page.as_mut().expect("laid out");
        for (index, height) in heights.into_iter().enumerate() {
            page.set(index, height, Measure::Estimated);
        }
        let count = page.len();
        if page.measure_at(count) == Measure::Estimated {
            page.set(count, foot, Measure::Estimated);
        }
        page.settle();
        self.page.as_ref().expect("laid out")
    }

    /// Measures estimated blocks for about `budget`, nearest the viewport first (from the anchor
    /// down, then from the top), and returns whether any are left. Nothing is loaded for this
    /// (see the module docs); corrections above the anchor keep the content on screen still, and
    /// [`top`](Self::top) reports the adjusted offset. A shell calls this between events while it
    /// returns true and then stops: a refined layout needs no more work.
    pub fn refine(&mut self, doc: &Document, options: &Options, budget: Duration) -> bool {
        self.ensure_layout(doc, options);
        self.correct_pending(doc);
        let started = Instant::now();
        while let Some(index) = self.next_estimated() {
            self.measure(doc, index, false, false);
            if started.elapsed() >= budget {
                break;
            }
        }
        self.settle();
        self.estimating()
    }

    /// Whether any block (or the footnote list) still has an estimated height.
    pub fn estimating(&self) -> bool {
        self.page.as_ref().is_some_and(|page| {
            page.estimated_blocks() > 0 || page.measure_at(page.len()) == Measure::Estimated
        })
    }

    /// The next estimated index for refinement, moving the cursors past what is measured.
    fn next_estimated(&mut self) -> Option<usize> {
        let page = self.page.as_ref()?;
        let last = page.len();
        let anchor = self.view.anchor.index.min(last);
        let view = &mut self.view;
        if view.ahead < anchor {
            view.ahead = anchor;
        }
        while view.ahead <= last {
            let index = view.ahead;
            view.ahead += 1;
            if page.measure_at(index) == Measure::Estimated {
                return Some(index);
            }
        }
        while view.behind <= last {
            let index = view.behind;
            view.behind += 1;
            if page.measure_at(index) == Measure::Estimated {
                return Some(index);
            }
        }
        None
    }

    /// The average glyph advances for these options, shaped once per scale and theme.
    fn metrics(&mut self, doc: &Document, options: &Options) -> super::estimate::Metrics {
        let key = (options.scale.to_bits(), options.theme.name);
        if let Some((scale, theme, metrics)) = self.metrics
            && (scale, theme) == key
        {
            return metrics;
        }
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            false,
            &mut self.store,
            &self.details,
        );
        let metrics = super::estimate::Metrics::measure(&mut pager);
        self.metrics = Some((key.0, key.1, metrics));
        metrics
    }
}
