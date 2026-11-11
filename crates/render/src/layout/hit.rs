//! Hit testing: the per-line caret geometry ([`LineGeom`]) kept with each cached block, and
//! [`DocumentRenderer::hit_test`], which turns a page point into a document offset, a link, a
//! footnote reference or a `<details>` summary.

use super::inline::Inline;
use super::pager::Pager;
use super::{DocumentRenderer, Hit, Options, PageLayout};
use crate::text::Brush;
use mdview_core::model::{BlockKind, Document, NONE, SpanKind};
use parley::{Cluster, ClusterSide, Layout};
use std::rc::Rc;

/// One laid-out line: where it sits, what text it covers and the layout that owns it, so a click
/// can be resolved back to a document offset without keeping a caret table per glyph.
#[derive(Clone)]
pub(super) struct LineGeom {
    pub(super) top: f32,
    pub(super) bottom: f32,
    /// The layout this line belongs to, and where its origin sits in page coordinates.
    pub(super) layout: Rc<Layout<Brush>>,
    pub(super) source: Rc<Inline>,
    pub(super) x: f32,
    pub(super) y: f32,
    /// The line's inline byte range.
    pub(super) end: usize,
    /// The line's x range: start and end of the text, in page coordinates.
    pub(super) x0: f32,
    pub(super) x1: f32,
    /// The document offsets the line covers.
    pub(super) doc_start: u32,
    pub(super) doc_end: u32,
}

impl LineGeom {
    pub(super) fn shift(&mut self, dy: f32) {
        self.top += dy;
        self.bottom += dy;
        self.y += dy;
    }
}

impl DocumentRenderer {
    /// What is at page position `x, y`: the document offset, plus a link or footnote reference when
    /// the point is over one. Lays out the one block that was hit if the frame has not painted it
    /// yet, so this works before or after [`view_frame`](Self::view_frame) for that row.
    pub fn hit_test(
        &mut self,
        doc: &Document,
        options: &Options,
        page: &PageLayout,
        x: f32,
        y: f32,
    ) -> Option<Hit> {
        let index = page.index_at(y)?;
        let span = page.spans[index];
        self.ensure_block(doc, options, page, index);
        let cached = self.cache.get(&span.block)?;
        let local = y - span.y;
        let line = cached.lines.iter().min_by(|a, b| {
            distance(a, local)
                .total_cmp(&distance(b, local))
                .then_with(|| a.top.total_cmp(&b.top))
        })?;
        let offset = offset_at(line, x, local);
        let mut hit = Hit {
            block: span.block,
            offset,
            link: None,
            footnote: None,
            details: None,
        };
        if let Some(inner) = doc.block_at(offset) {
            let mut block = inner;
            loop {
                let current = &doc.blocks[block as usize];
                if matches!(current.kind, BlockKind::Summary)
                    && current.parent != NONE
                    && matches!(
                        doc.blocks[current.parent as usize].kind,
                        BlockKind::Details { .. }
                    )
                {
                    hit.details = Some(current.parent);
                    break;
                }
                if current.parent == NONE || matches!(current.kind, BlockKind::Details { .. }) {
                    break;
                }
                block = current.parent;
            }
            for i in doc.blocks[inner as usize].spans.usize() {
                let span = &doc.spans[i];
                match span.kind {
                    SpanKind::Link(link) if span.start <= offset && offset < span.end => {
                        hit.link = Some(link);
                    }
                    SpanKind::FootnoteRef(note) if span.start == offset => {
                        hit.footnote = Some(note);
                    }
                    _ => {}
                }
            }
        }
        Some(hit)
    }

    /// Lays out one block so its paint cache and caret stops exist. Blocks painted by a frame are
    /// already cached; this covers a click before the block has come on screen.
    pub(super) fn ensure_block(
        &mut self,
        doc: &Document,
        options: &Options,
        page: &PageLayout,
        index: usize,
    ) {
        if self.cache.contains_key(&page.spans[index].block) {
            return;
        }
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            Some(&mut self.cache),
            &mut self.store,
            &self.details,
        );
        pager.block_paints(page, index);
    }
}

/// Vertical distance from `y` to a line, zero inside it.
pub(super) fn distance(line: &LineGeom, y: f32) -> f32 {
    if y < line.top {
        line.top - y
    } else if y > line.bottom {
        y - line.bottom
    } else {
        0.0
    }
}

/// The document offset under a point on a line, using the line's own layout for the search.
fn offset_at(line: &LineGeom, x: f32, y: f32) -> u32 {
    let px = (x - line.x).max(0.0);
    let py = y - line.y;
    let Some((cluster, side)) = Cluster::from_point(&line.layout, px, py) else {
        return line.doc_start;
    };
    let range = cluster.text_range();
    let at = if side == ClusterSide::Right {
        range.end
    } else {
        range.start
    };
    line.source.doc_at(at).unwrap_or(line.doc_start)
}
