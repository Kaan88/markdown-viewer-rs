//! [`PageLayout`], the measured extent of every top-level block, and the passes that build and
//! use it: the height pass (`measure_page`), the viewport pass (`slice_page`), per-block paint
//! caching (`block_paints`), and the footnote list at the end of the page.

use super::inline::{Inline, Style};
use super::pager::{Pager, Txt};
use super::paint::{CachedBlock, Paint};
use super::{BlockBox, Options};
use mdview_core::model::{Document, Footnote, NONE};

/// Every top-level block's measured vertical extent, without any painting. Build it once per
/// width/scale/theme and reuse it for scroll frames; a 1 MB document costs one height pass.
pub struct PageLayout {
    pub(super) spans: Vec<BlockBox>,
    /// The measured y of each footnote entry, by footnote number, in reading order.
    pub(super) notes: Vec<(u32, f32)>,
    footnotes_y: f32,
    footnotes_height: f32,
    pub(super) total_height: f32,
    pub(super) width: f32,
    pub(super) scale: f32,
    pub(super) theme: &'static str,
}

impl PageLayout {
    pub fn total_height(&self) -> f32 {
        self.total_height
    }

    /// Every top-level block's measured extent, in page order.
    pub fn blocks(&self) -> &[BlockBox] {
        &self.spans
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

    /// The page row of a block, including one nested in a list, quote, table or details, so an
    /// anchor can be scrolled to. Footnote entries are not drawn where their definitions are; use
    /// [`footnote_y`](Self::footnote_y) for those.
    pub fn y_of(&self, doc: &Document, block: u32) -> Option<f32> {
        let mut block = block;
        loop {
            let parent = doc.blocks.get(block as usize)?.parent;
            if parent == NONE {
                break;
            }
            block = parent;
        }
        self.spans
            .iter()
            .find(|span| span.block == block)
            .map(|span| span.y)
    }

    /// The page row of a footnote entry, by the number shown in the text.
    pub fn footnote_y(&self, number: u32) -> Option<f32> {
        self.notes
            .iter()
            .find(|(n, _)| *n == number)
            .map(|(_, y)| *y)
    }

    pub fn matches(&self, options: &Options) -> bool {
        self.width == options.width
            && (self.scale - options.scale).abs() < 0.001
            && self.theme == options.theme.name
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

    /// Measures every top-level block without keeping its paints, so a viewport can be placed and
    /// the total page height known without allocating a page-sized bitmap.
    pub(super) fn measure_page(mut self) -> PageLayout {
        self.y = self.px(36.0);
        let content = self.content_width();
        let children: Vec<u32> = self.doc.children(NONE).collect();
        let mut spans = Vec::with_capacity(children.len());
        for block in children {
            // Only the block being measured is protected from eviction.
            self.store.begin_frame();
            let y = self.y;
            let mark = self.paints.len();
            self.block(block, self.left, content, self.theme.text, 0);
            spans.push(BlockBox {
                block,
                y,
                height: self.y - y,
            });
            self.paints.truncate(mark);
        }
        let footnotes_y = self.y;
        self.footnotes();
        let footnotes_height = self.y - footnotes_y;
        self.paints.clear();
        self.hits.clear();
        let total_height = self.y + self.px(24.0);
        PageLayout {
            spans,
            notes: self.notes,
            footnotes_y,
            footnotes_height,
            total_height,
            width: self.width / self.scale,
            scale: self.scale,
            theme: self.theme.name,
        }
    }

    /// Lays out and paints only the blocks that reach the viewport.
    pub(super) fn slice_page(&mut self, page: &PageLayout, top: f32, height: f32) {
        self.y = self.px(36.0);
        let bottom = top + height;
        for index in 0..page.spans.len() {
            let span = page.spans[index];
            if span.y + span.height <= top || span.y >= bottom {
                continue;
            }
            self.block_paints(page, index);
        }
        if page.footnotes_height > 0.0
            && page.footnotes_y + page.footnotes_height > top
            && page.footnotes_y < bottom
        {
            self.y = page.footnotes_y;
            self.notes.clear();
            self.footnotes();
        }
    }

    /// Paints one top-level block, its paints and caret stops cached relative to its top so scroll
    /// frames only move them. The cache is the renderer's, so hit tests can read it later.
    pub(super) fn block_paints(&mut self, page: &PageLayout, index: usize) {
        let span = page.spans[index];
        let content = self.content_width();
        if let Some(cache) = self.cache.as_deref()
            && let Some(cached) = cache.get(&span.block)
        {
            let mut paints = cached.paints.clone();
            for paint in &mut paints {
                paint.shift(span.y);
            }
            self.paints.extend(paints);
            return;
        }
        let paints_mark = self.paints.len();
        let hits_mark = self.hits.len();
        self.y = span.y;
        self.block(span.block, self.left, content, self.theme.text, 0);
        if let Some(cache) = self.cache.as_deref_mut() {
            let mut made = self.paints.split_off(paints_mark);
            let mut lines = self.hits.split_off(hits_mark);
            for paint in &mut made {
                paint.shift(-span.y);
            }
            for line in &mut lines {
                line.shift(-span.y);
            }
            cache.insert(
                span.block,
                CachedBlock {
                    paints: made.clone(),
                    lines,
                },
            );
            for paint in &mut made {
                paint.shift(span.y);
            }
            self.paints.extend(made);
        }
    }
}
