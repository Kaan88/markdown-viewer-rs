//! Selection geometry: one page rectangle per selected line, from the cached caret geometry.

use super::hit::LineGeom;
use super::{DocumentRenderer, Options, PageLayout, Rect};
use mdview_core::model::Document;
use parley::Cluster;

impl DocumentRenderer {
    /// The page rectangles a selection between two document offsets paints. Multi-line ranges give
    /// one rect per line, and blocks that are not currently painted are laid out for the test. The
    /// selected pixels are the caller's to draw, so the frame delta stays about the document only.
    pub fn selection_rects(
        &mut self,
        doc: &Document,
        options: &Options,
        page: &PageLayout,
        from: u32,
        to: u32,
    ) -> Vec<Rect> {
        let (start, end) = if from <= to { (from, to) } else { (to, from) };
        if start == end {
            return Vec::new();
        }
        let mut rects = Vec::new();
        for (index, span) in page.spans.iter().enumerate() {
            let text = doc.blocks[span.block as usize].text;
            if text.end <= start || text.start >= end {
                continue;
            }
            self.ensure_block(doc, options, page, index);
            let Some(cached) = self.cache.get(&span.block) else {
                continue;
            };
            for line in &cached.lines {
                let first = start.max(line.doc_start);
                let last = end.min(line.doc_end);
                if first >= last {
                    continue;
                }
                let x0 = x_at(line, first);
                let x1 = x_at(line, last);
                if x1 - x0 > 0.25 {
                    rects.push(Rect {
                        x: x0,
                        y: span.y + line.top,
                        w: x1 - x0,
                        h: line.bottom - line.top,
                    });
                }
            }
        }
        rects
    }
}

/// The page x of a document offset on a line, from the line's own layout.
fn x_at(line: &LineGeom, offset: u32) -> f32 {
    if offset <= line.doc_start {
        return line.x0;
    }
    if offset >= line.doc_end {
        return line.x1;
    }
    let Some(at) = line.source.inline_at(offset) else {
        return line.x0;
    };
    if at >= line.end {
        return line.x1;
    }
    let Some(cluster) = Cluster::from_byte_index(&line.layout, at) else {
        return line.x0;
    };
    line.x + cluster.visual_offset().unwrap_or(0.0)
}
