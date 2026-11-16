//! What a double or a triple click selects on the page: a word, or the innermost block. Offsets
//! are into `Document::text`; mdview's page and the preview pane of an editor share them.

use crate::model::Document;

/// The word (or run of spaces, or single punctuation mark) at `offset`, within its block.
pub fn word_range(doc: &Document, offset: u32) -> (u32, u32) {
    let text = doc.text.as_str();
    let (lo, hi) = block_bounds(doc, offset);
    let mut pos = (offset as usize).clamp(lo, hi);
    while !text.is_char_boundary(pos) {
        pos -= 1;
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    };
    // At the end of a line, the character before the caret is the one meant.
    let c = match text[pos..hi].chars().next() {
        Some(c) => c,
        None => match text[lo..pos].chars().next_back() {
            Some(c) => {
                pos -= c.len_utf8();
                c
            }
            None => return (offset, offset),
        },
    };
    let kind = class(c);
    if kind == 2 {
        return (pos as u32, (pos + c.len_utf8()) as u32);
    }
    let start = text[lo..pos]
        .char_indices()
        .rev()
        .take_while(|&(_, ch)| class(ch) == kind)
        .last()
        .map_or(pos, |(i, _)| lo + i);
    let end = text[pos..hi]
        .char_indices()
        .find(|&(_, ch)| class(ch) != kind)
        .map_or(hi, |(i, _)| pos + i);
    (start as u32, end as u32)
}

/// The innermost block at `offset`, trailing whitespace left out.
pub fn block_range(doc: &Document, offset: u32) -> (u32, u32) {
    let (lo, hi) = block_bounds(doc, offset);
    let trimmed = doc.text.get(lo..hi).map_or(0, |s| s.trim_end().len());
    (lo as u32, (lo + trimmed) as u32)
}

/// The byte range of the innermost block containing `offset` (or ending at it), or the whole text.
fn block_bounds(doc: &Document, offset: u32) -> (usize, usize) {
    let len = doc.text.len();
    let block = doc
        .block_at(offset)
        .or_else(|| offset.checked_sub(1).and_then(|o| doc.block_at(o)));
    let (lo, hi) = block
        .and_then(|b| doc.blocks.get(b as usize))
        .map_or((0, len), |b| (b.text.start as usize, b.text.end as usize));
    let hi = hi.min(len);
    let lo = lo.min(hi);
    let lo = (0..=lo)
        .rev()
        .find(|&i| doc.text.is_char_boundary(i))
        .unwrap_or(0);
    let hi = (hi..=len)
        .find(|&i| doc.text.is_char_boundary(i))
        .unwrap_or(len);
    (lo, hi)
}
