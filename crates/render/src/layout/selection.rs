//! Selection geometry: one page rectangle per selected line, from the cached caret geometry.

use super::hit::LineGeom;
use super::{DocumentRenderer, Options, Rect};
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
        from: u32,
        to: u32,
    ) -> Vec<Rect> {
        let (start, end) = if from <= to { (from, to) } else { (to, from) };
        if start == end {
            return Vec::new();
        }
        self.ensure_layout(doc, options);
        self.correct_pending(doc);
        let Some(page) = self.page.as_ref() else {
            return Vec::new();
        };
        // Top-level blocks are in text order, so the ones the range touches are a run.
        let text = |index: usize| doc.blocks[page.spans[index].block as usize].text;
        let first = page
            .spans
            .partition_point(|span| doc.blocks[span.block as usize].text.end <= start);
        let mut last = first;
        while last < page.len() && text(last).start < end {
            last += 1;
        }
        let touched: Vec<usize> = (first..last).filter(|&i| text(i).end > start).collect();
        for index in touched {
            if !self.ready(index) {
                self.measure(doc, index, true, true);
            }
        }
        self.correct_pending(doc);
        let Some(page) = self.page.as_ref() else {
            return Vec::new();
        };
        let mut rects = Vec::new();
        for span in &page.spans[first..last] {
            let text = doc.blocks[span.block as usize].text;
            if text.end <= start || text.start >= end {
                continue;
            }
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
                        y: span.y - cached.phase + line.top,
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
    line.x + line.shift + cluster.visual_offset().unwrap_or(0.0)
}
