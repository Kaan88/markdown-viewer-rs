//! Hit testing: the per-line caret geometry ([`LineGeom`]) kept with each cached block, and
//! [`DocumentRenderer::hit_test`], which turns a page point into a document offset, a link, a
//! footnote reference or a `<details>` summary.

use super::inline::Inline;
use super::{DocumentRenderer, Hit, Options, Source};
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
    /// How far in from the layout's left edge the line starts: not zero beside a float at the
    /// left. Cluster offsets are measured from there.
    pub(super) shift: f32,
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

/// Where a rendered formula or diagram sits, relative to its block like the lines, so a point
/// inside the picture can be told from one beside it.
#[derive(Clone, Copy)]
pub(super) struct SourceBox {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) source: Source,
}

impl SourceBox {
    pub(super) fn shift(&mut self, dy: f32) {
        self.y += dy;
    }
}

/// Where a task-list box is drawn, relative to its block like the lines, and the block whose
/// text it starts.
#[derive(Clone, Copy)]
pub(super) struct TaskBox {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) block: u32,
}

impl TaskBox {
    pub(super) fn shift(&mut self, dy: f32) {
        self.y += dy;
    }
}

/// Where a floated image is drawn, relative to its block like the lines. It can reach below the
/// block, over the rows of the blocks beside it.
#[derive(Clone, Copy)]
pub(super) struct FloatBox {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
    /// The leaf block the image was written in, and its offset in `Document::text`.
    pub(super) block: u32,
    pub(super) at: u32,
}

impl FloatBox {
    pub(super) fn shift(&mut self, dy: f32) {
        self.y += dy;
    }
}

impl DocumentRenderer {
    /// What is at page position `x, y`: the document offset, plus a link or footnote reference when
    /// the point is over one. Lays out the one block that was hit if the frame has not painted it
    /// yet, so this works before or after [`view_frame`](Self::view_frame) for that row.
    pub fn hit_test(&mut self, doc: &Document, options: &Options, x: f32, y: f32) -> Option<Hit> {
        self.ensure_layout(doc, options);
        self.correct_pending(doc);
        let index = self.page.as_ref()?.index_at(y)?;
        if !self.ready(index) {
            self.measure(doc, index, true, true);
            self.correct_pending(doc);
        }
        if let Some(hit) = self.float_hit(doc, index, x, y) {
            return Some(hit);
        }
        let span = self.page.as_ref()?.spans[index];
        let cached = self.cache.get(&span.block)?;
        let local = y - (span.y - cached.phase);
        let source = cached
            .sources
            .iter()
            .find(|b| x >= b.x && x <= b.x + b.w && local >= b.y && local <= b.y + b.h)
            .map(|b| b.source);
        // A little slack around the box, which is a small target.
        let slack = 3.0 * options.scale;
        let task = cached
            .tasks
            .iter()
            .find(|t| {
                x >= t.x - slack
                    && x <= t.x + t.w + slack
                    && local >= t.y - slack
                    && local <= t.y + t.h + slack
            })
            .map(|t| t.block);
        // The nearest line by height; among lines side by side, as the cells of a table row
        // are, the one the point is in or nearest to.
        let beside = |line: &LineGeom| (line.x0 - x).max(x - line.x1).max(0.0);
        let line = cached.lines.iter().min_by(|a, b| {
            distance(a, local)
                .total_cmp(&distance(b, local))
                .then_with(|| beside(a).total_cmp(&beside(b)))
                .then_with(|| a.top.total_cmp(&b.top))
        });
        // A display formula or a diagram has no text line: the picture itself is the target.
        let offset = match (line, source) {
            (Some(line), _) => offset_at(line, x, local),
            (None, Some(source)) => source.start,
            // A task item with no text is only its box.
            (None, None) => doc.blocks.get(task? as usize)?.text.start,
        };
        let on_text = line.is_some_and(|line| {
            let slack = 2.0 * options.scale;
            distance(line, local) == 0.0 && x >= line.x0 - slack && x <= line.x1 + slack
        });
        let mut hit = Hit {
            block: span.block,
            offset,
            link: None,
            footnote: None,
            details: None,
            task,
            source,
            on_text,
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
            let spans = if on_text {
                doc.blocks[inner as usize].spans.usize()
            } else {
                0..0
            };
            for i in spans {
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
}

impl DocumentRenderer {
    /// A point on a floated image: the image's own block and position, and the link it sits in.
    /// The image can hang over the rows of the blocks after its own, so the blocks whose floats
    /// are open at block `index` are asked as well as that block.
    fn float_hit(&mut self, doc: &Document, index: usize, x: f32, y: f32) -> Option<Hit> {
        let mut owners = self.floats.owners(index);
        owners.push(index);
        for &owner in &owners {
            if !self.ready(owner) {
                self.measure(doc, owner, true, true);
            }
        }
        self.correct_pending(doc);
        let page = self.page.as_ref()?;
        for owner in owners {
            let span = *page.spans.get(owner)?;
            let Some(cached) = self.cache.get(&span.block) else {
                continue;
            };
            let local = y - (span.y - cached.phase);
            let found = cached
                .floats
                .iter()
                .find(|b| x >= b.x && x <= b.x + b.w && local >= b.y && local <= b.y + b.h);
            if let Some(found) = found {
                return Some(Hit {
                    block: span.block,
                    offset: found.at,
                    link: super::floats::link_of_image(doc, found.block, found.at),
                    footnote: None,
                    details: None,
                    task: None,
                    source: None,
                    on_text: false,
                });
            }
        }
        None
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
