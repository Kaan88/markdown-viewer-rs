//! Keeping the reader's place when the text under it is not the text it was taken from.
//!
//! Two cases, both about a [`ScrollPos`] (a top-level block and an offset into it):
//!
//! - [`ReadingPlace`] is a position that outlives the run. Beside the block's index it records
//!   what the block said and the heading it stood under, so it can be found again in a file that
//!   was edited in between, at any window size, zoom or theme. It goes into the session as text.
//! - [`Remap`] compares the document on screen with the one just read from the changed file and
//!   says where each block went, so a reload leaves the content at the top of the window where
//!   it was, and a selection or the current find match can follow its text.

use crate::model::{Document, NONE, ScrollPos};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The top-level block holding `block`; an index past the end counts as the last block.
fn top_level(doc: &Document, block: u32) -> Option<u32> {
    let mut block = block.min(doc.blocks.len().checked_sub(1)? as u32);
    loop {
        let parent = doc.blocks[block as usize].parent;
        if parent == NONE {
            return Some(block);
        }
        block = parent;
    }
}

fn top_blocks(doc: &Document) -> Vec<u32> {
    doc.children(NONE).collect()
}

/// A reading position to keep between runs: the block at the top of the window, how far into it
/// the window started, and enough about the block to find it again after the file changed.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadingPlace {
    /// The top-level block at the top of the window.
    pub block: u32,
    /// Logical px from the block's top to the window's top.
    pub offset: f32,
    /// How many blocks the document had.
    pub blocks: u32,
    /// A hash of the block's text. It is written to the session, so it must not depend on the
    /// build: FNV-1a of the bytes.
    pub hash: u64,
    /// The slug of the last heading at or above the block; empty when there is none.
    pub heading: String,
    /// How many top-level blocks below that heading's the block is.
    pub below: u32,
}

/// The first word of a place written to the session, so a later format is not misread.
const FORMAT: &str = "1";

impl ReadingPlace {
    /// The place `pos` is in `doc`. `None` for an empty document.
    pub fn of(doc: &Document, pos: ScrollPos) -> Option<ReadingPlace> {
        let block = top_level(doc, pos.block)?;
        let heading = doc.headings.iter().rev().find(|h| h.block <= block);
        let below = heading.map_or(0, |h| {
            let from = top_level(doc, h.block).unwrap_or(h.block);
            doc.children(NONE)
                .skip_while(|&b| b < from)
                .take_while(|&b| b < block)
                .count() as u32
        });
        Some(ReadingPlace {
            block,
            offset: if pos.offset.is_finite() {
                pos.offset
            } else {
                0.0
            },
            blocks: doc.blocks.len() as u32,
            hash: fnv1a(doc.block_text(block).as_bytes()),
            heading: heading.map_or_else(String::new, |h| doc.str(h.slug).to_string()),
            below,
        })
    }

    /// True at the very top of the document, which needs no remembering.
    pub fn is_top(&self) -> bool {
        self.block == 0 && self.offset <= 0.0
    }

    /// Where the place is in `doc`, which may be an edited version of the document it was taken
    /// from. In order: the same block where it was; a block with the same text, the nearest to
    /// where it was (inside the same heading's section when that heading still exists); the same
    /// number of blocks below the same heading; the same index if the document is the same
    /// length; the same share of the way down; the top.
    pub fn resolve(&self, doc: &Document) -> ScrollPos {
        if doc.blocks.is_empty() {
            return ScrollPos::default();
        }
        let hash_of = |block: u32| fnv1a(doc.block_text(block).as_bytes());
        let at = |block: u32| ScrollPos {
            block,
            offset: self.offset,
        };
        let is_top = |block: u32| {
            doc.blocks
                .get(block as usize)
                .is_some_and(|b| b.parent == NONE)
        };
        if is_top(self.block) && hash_of(self.block) == self.hash {
            return at(self.block);
        }
        let tops = top_blocks(doc);
        // The heading's section: the top-level blocks from the heading to the next heading.
        let section = (!self.heading.is_empty())
            .then(|| {
                let i = doc
                    .headings
                    .iter()
                    .position(|h| doc.str(h.slug) == self.heading)?;
                let from = top_level(doc, doc.headings[i].block)?;
                let to = doc.headings[i + 1..]
                    .iter()
                    .filter_map(|h| top_level(doc, h.block))
                    .find(|&b| b > from)
                    .unwrap_or(doc.blocks.len() as u32);
                let first = tops.partition_point(|&b| b < from);
                let end = tops.partition_point(|&b| b < to);
                Some((first, end))
            })
            .flatten();
        let expected = match section {
            Some((first, _)) => tops[(first + self.below as usize).min(tops.len() - 1)],
            None => self.block,
        };
        let same_text = |range: &[u32]| {
            range
                .iter()
                .copied()
                .filter(|&b| hash_of(b) == self.hash)
                .min_by_key(|&b| b.abs_diff(expected))
        };
        let found = section
            .and_then(|(first, end)| same_text(&tops[first..end]))
            .or_else(|| same_text(&tops));
        if let Some(block) = found {
            return at(block);
        }
        if let Some((first, end)) = section {
            return at(tops[(first + self.below as usize).min(end.max(first + 1) - 1)]);
        }
        if doc.blocks.len() as u32 == self.blocks {
            return at(top_level(doc, self.block).unwrap_or(0));
        }
        let share = f64::from(self.block) / f64::from(self.blocks.max(1));
        let index = (share * doc.blocks.len() as f64) as u32;
        ScrollPos {
            block: top_level(doc, index).unwrap_or(0),
            offset: 0.0,
        }
    }

    /// The place as one line of text for the session.
    pub fn to_setting(&self) -> String {
        format!(
            "{FORMAT} {} {} {} {:x} {} {}",
            self.block, self.offset, self.blocks, self.hash, self.below, self.heading
        )
    }

    /// Reads what [`to_setting`](Self::to_setting) wrote; `None` for anything else.
    pub fn from_setting(text: &str) -> Option<ReadingPlace> {
        let mut words = text.splitn(7, ' ');
        if words.next()? != FORMAT {
            return None;
        }
        let block = words.next()?.parse().ok()?;
        let offset: f32 = words.next()?.parse().ok()?;
        let blocks = words.next()?.parse().ok()?;
        let hash = u64::from_str_radix(words.next()?, 16).ok()?;
        let below = words.next()?.parse().ok()?;
        let heading = words.next().unwrap_or("").to_string();
        offset.is_finite().then_some(ReadingPlace {
            block,
            offset,
            blocks,
            hash,
            heading,
            below,
        })
    }
}

/// How well a block of the old document was found in the new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// The same block: same kind, same text.
    Same,
    /// The block that stands where it stood, between the same neighbours, with other text: it
    /// was edited.
    Edited,
    /// The block is gone, or the text around it changed too much to tell; this is the nearest
    /// place that survived.
    Near,
}

/// How many times [`locate`] narrows its search between landmarks before settling.
const NARROWINGS: usize = 24;

/// Where the top-level blocks of one version of a document are in the next.
///
/// Blocks are compared by kind and text. The versions are first compared from both ends; a block
/// in the unchanged start or end is found at once. For one in the changed middle, the blocks whose
/// text occurs exactly once in both middles are landmarks (headings usually are): the search
/// narrows to the stretch between the landmarks on either side of the block and starts over
/// there. Text that repeats ("Fixed", an empty line, a table cell) therefore never matches a
/// block far away; it is only compared inside a stretch that unique text has already pinned.
pub struct Remap<'a> {
    old: &'a Document,
    new: &'a Document,
    old_tops: Vec<u32>,
    new_tops: Vec<u32>,
    old_keys: Vec<u64>,
    new_keys: Vec<u64>,
}

fn keys(doc: &Document, tops: &[u32]) -> Vec<u64> {
    tops.iter()
        .map(|&block| {
            let b = &doc.blocks[block as usize];
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::mem::discriminant(&b.kind).hash(&mut hasher);
            (b.end - block).hash(&mut hasher);
            doc.text_of(b.text).hash(&mut hasher);
            hasher.finish()
        })
        .collect()
}

impl<'a> Remap<'a> {
    pub fn new(old: &'a Document, new: &'a Document) -> Remap<'a> {
        let (old_tops, new_tops) = (top_blocks(old), top_blocks(new));
        Remap {
            old_keys: keys(old, &old_tops),
            new_keys: keys(new, &new_tops),
            old,
            new,
            old_tops,
            new_tops,
        }
    }

    /// Where `block` of the old document (or the top-level block holding it) is in the new one.
    /// `None` when either document is empty.
    pub fn block(&self, block: u32) -> Option<(u32, Fit)> {
        let top = top_level(self.old, block)?;
        let at = self.old_tops.binary_search(&top).ok()?;
        if self.new_keys.is_empty() {
            return None;
        }
        let (index, fit) = locate(&self.old_keys, &self.new_keys, at);
        Some((self.new_tops[index], fit))
    }

    /// The reading position after the change. The offset into the block is kept when the block
    /// is the same or was edited where it stood; a block that is only near starts at its top.
    pub fn scroll(&self, pos: ScrollPos) -> (ScrollPos, Fit) {
        match self.block(pos.block) {
            Some((block, fit)) => (
                ScrollPos {
                    block,
                    offset: if fit == Fit::Near { 0.0 } else { pos.offset },
                },
                fit,
            ),
            None => (ScrollPos::default(), Fit::Near),
        }
    }

    /// Where the text offset `offset` of the old document is in the new one, when the block
    /// holding it is the same there.
    pub fn offset(&self, offset: u32) -> Option<u32> {
        let i = self
            .old_tops
            .partition_point(|&b| self.old.blocks[b as usize].text.end <= offset);
        let block = *self.old_tops.get(i)?;
        let start = self.old.blocks[block as usize].text.start;
        if offset < start {
            return None;
        }
        match self.block(block)? {
            (to, Fit::Same) => Some(self.new.blocks[to as usize].text.start + (offset - start)),
            _ => None,
        }
    }

    /// Where the text range `from..to` of the old document is in the new one, if both ends are in
    /// blocks that are the same there and the text between them is still what it was.
    pub fn range(&self, from: u32, to: u32) -> Option<(u32, u32)> {
        if from >= to {
            return None;
        }
        let text = self.old.text.get(from as usize..to as usize)?;
        let start = self.offset(from)?;
        // The end is exclusive, so it is the last selected byte that has to be in a block.
        let end = self.offset(to - 1)? + 1;
        (self.new.text.get(start as usize..end as usize)? == text).then_some((start, end))
    }
}

/// The index in `new` of the block at `old[at]`. `new` must not be empty.
fn locate(old: &[u64], new: &[u64], at: usize) -> (usize, Fit) {
    // The stretches still being compared: `old[o0..o1]` holds `at`, `new[n0..n1]` is its match.
    let (mut o0, mut o1, mut n0, mut n1) = (0, old.len(), 0, new.len());
    for _ in 0..NARROWINGS {
        let (a, b) = (&old[o0..o1], &new[n0..n1]);
        let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
        if at < o0 + prefix {
            return (n0 + (at - o0), Fit::Same);
        }
        let (a, b) = (&a[prefix..], &b[prefix..]);
        let suffix = a
            .iter()
            .rev()
            .zip(b.iter().rev())
            .take_while(|(x, y)| x == y)
            .count();
        if at >= o1 - suffix {
            return (n1 - (o1 - at), Fit::Same);
        }
        (o0, n0, o1, n1) = (o0 + prefix, n0 + prefix, o1 - suffix, n1 - suffix);
        if n0 == n1 {
            break;
        }
        let chain = landmarks(&old[o0..o1], &new[n0..n1]);
        if chain.is_empty() {
            break;
        }
        let rel = at - o0;
        let next = chain.partition_point(|&(o, _)| o < rel);
        if let Some(&(o, n)) = chain.get(next)
            && o == rel
        {
            return (n0 + n, Fit::Same);
        }
        // Narrow to the stretch between the landmark before the block and the one after it.
        let (start_o, start_n) = (o0, n0);
        if let Some(&(o, n)) = chain.get(next) {
            (o1, n1) = (start_o + o, start_n + n);
        }
        if let Some(&(o, n)) = next.checked_sub(1).and_then(|i| chain.get(i)) {
            (o0, n0) = (start_o + o + 1, start_n + n + 1);
        }
    }
    let (rel, old_len, new_len) = (at - o0, o1 - o0, n1 - n0);
    if new_len == 0 {
        // Everything between the neighbours went: what followed it is now in its place.
        return (n1.min(new.len() - 1), Fit::Near);
    }
    // The stretch is pinned by unique text on both sides, so a repeat inside it is the block.
    let wanted = n0 + rel * new_len / old_len;
    let repeat = (n0..n1)
        .filter(|&i| new[i] == old[at])
        .min_by_key(|&i| i.abs_diff(wanted));
    if let Some(index) = repeat {
        return (index, Fit::Same);
    }
    if old_len == new_len {
        return (n0 + rel, Fit::Edited);
    }
    (wanted.min(n1 - 1), Fit::Near)
}

/// The blocks whose key occurs once in `old` and once in `new`, as `(old index, new index)`
/// pairs in increasing order of both: of the pairs that cross (a block moved past another), the
/// longest run that does not is kept.
fn landmarks(old: &[u64], new: &[u64]) -> Vec<(usize, usize)> {
    // Per key: how often it occurs in each, and where it was last seen in `new`.
    let mut seen: HashMap<u64, (u32, u32, usize)> = HashMap::with_capacity(old.len());
    for &key in old {
        seen.entry(key).or_insert((0, 0, 0)).0 += 1;
    }
    for (i, &key) in new.iter().enumerate() {
        if let Some(entry) = seen.get_mut(&key) {
            entry.1 += 1;
            entry.2 = i;
        }
    }
    let pairs: Vec<(usize, usize)> = old
        .iter()
        .enumerate()
        .filter_map(|(i, key)| match seen[key] {
            (1, 1, n) => Some((i, n)),
            _ => None,
        })
        .collect();
    // Longest increasing run of the new indexes (patience sorting with back links).
    let mut tails: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; pairs.len()];
    for (i, &(_, n)) in pairs.iter().enumerate() {
        let at = tails.partition_point(|&t| pairs[t].1 < n);
        before[i] = at.checked_sub(1).map(|p| tails[p]);
        if at == tails.len() {
            tails.push(i);
        } else {
            tails[at] = i;
        }
    }
    let mut chain = Vec::with_capacity(tails.len());
    let mut link = tails.last().copied();
    while let Some(i) = link {
        chain.push(pairs[i]);
        link = before[i];
    }
    chain.reverse();
    chain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn doc(paragraphs: &[&str]) -> Document {
        parse(&paragraphs.join("\n\n"))
    }

    fn block_of(doc: &Document, text: &str) -> u32 {
        let at = doc.text.find(text).expect("text in the document") as u32;
        top_level(doc, doc.block_at(at).unwrap()).unwrap()
    }

    fn numbered(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix} {i}")).collect()
    }

    fn joined(parts: &[&[String]]) -> Document {
        let all: Vec<&str> = parts
            .iter()
            .flat_map(|p| p.iter().map(String::as_str))
            .collect();
        doc(&all)
    }

    #[test]
    fn a_block_follows_edits_on_both_sides_of_it() {
        let body = numbered("para", 40);
        let old = joined(&[&["# Title".to_string()], &body, &["end".to_string()]]);
        // The first line and the last change, and three blocks appear near the top: nothing is
        // left of the unchanged start or end, so only the text can say where a block went.
        let new = joined(&[
            &["# Another title".to_string()],
            &numbered("new", 3),
            &body,
            &["a different end".to_string()],
        ]);
        let remap = Remap::new(&old, &new);
        let (to, fit) = remap.block(block_of(&old, "para 25")).unwrap();
        assert_eq!(fit, Fit::Same);
        assert_eq!(new.block_text(to).trim_end(), "para 25");
        let pos = ScrollPos {
            block: block_of(&old, "para 25"),
            offset: 12.5,
        };
        let (moved, _) = remap.scroll(pos);
        assert_eq!((moved.block, moved.offset), (to, 12.5));
    }

    #[test]
    fn an_edited_block_is_the_one_between_the_same_neighbours() {
        let old = doc(&["a", "b", "c", "d", "e"]);
        let new = doc(&["new", "a", "b", "c was edited", "d", "e", "more"]);
        let remap = Remap::new(&old, &new);
        let (to, fit) = remap.block(block_of(&old, "c")).unwrap();
        assert_eq!(fit, Fit::Edited);
        assert_eq!(new.block_text(to).trim_end(), "c was edited");
        // Its offset is kept; the caller clamps it to the block's new height.
        let pos = ScrollPos {
            block: block_of(&old, "c"),
            offset: 9.0,
        };
        assert_eq!(remap.scroll(pos).0.offset, 9.0);
    }

    #[test]
    fn a_removed_block_gives_way_to_what_followed_it() {
        let old = doc(&["a", "b", "c", "d", "e"]);
        let new = doc(&["a", "d", "e"]);
        let remap = Remap::new(&old, &new);
        for gone in ["b", "c"] {
            let pos = ScrollPos {
                block: block_of(&old, gone),
                offset: 30.0,
            };
            let (moved, fit) = remap.scroll(pos);
            assert_eq!(fit, Fit::Near);
            assert_eq!(new.block_text(moved.block).trim_end(), "d");
            assert_eq!(moved.offset, 0.0, "a different block starts at its top");
        }
        // Removed at the end: the last block left.
        let cut = doc(&["a", "b"]);
        let (to, fit) = Remap::new(&old, &cut).block(block_of(&old, "e")).unwrap();
        assert_eq!((cut.block_text(to).trim_end(), fit), ("b", Fit::Near));
    }

    #[test]
    fn repeated_text_never_matches_a_block_far_away() {
        // "Fixed" stands at 11 and at 42; the one at 11 becomes "Changed".
        let build = |first: &str| {
            let parts: Vec<String> = (0..60)
                .map(|i| match i {
                    11 => first.to_string(),
                    42 => "Fixed".to_string(),
                    _ => format!("para {i}"),
                })
                .collect();
            joined(&[&parts])
        };
        let (old, new) = (build("Fixed"), build("Changed"));
        let remap = Remap::new(&old, &new);
        assert_eq!(remap.block(11), Some((11, Fit::Edited)));
        assert_eq!(remap.block(42), Some((42, Fit::Same)));
        // Repeats shrinking at the edit point cannot make the two ends overlap.
        let (old, new) = (doc(&["x", "x", "x"]), doc(&["x", "x"]));
        let remap = Remap::new(&old, &new);
        for block in 0..3 {
            assert!(remap.block(block).unwrap().0 <= 1);
        }
    }

    #[test]
    fn a_wholly_different_document_keeps_the_share_of_the_way_down() {
        let old = joined(&[&numbered("old", 100)]);
        let new = joined(&[&numbered("new", 50)]);
        let remap = Remap::new(&old, &new);
        assert_eq!(remap.block(60), Some((30, Fit::Near)));
        assert_eq!(remap.block(99), Some((49, Fit::Near)));
        let empty = parse("");
        assert_eq!(Remap::new(&old, &empty).block(3), None);
        assert_eq!(Remap::new(&empty, &new).block(3), None);
        let pos = ScrollPos {
            block: 3,
            offset: 4.0,
        };
        assert_eq!(
            Remap::new(&old, &empty).scroll(pos),
            (ScrollPos::default(), Fit::Near)
        );
        // An index past the end is the last block.
        assert_eq!(remap.block(9999), Some((49, Fit::Near)));
    }

    #[test]
    fn blocks_that_changed_places_do_not_confuse_the_ones_between() {
        let old = doc(&["# One", "a", "b", "# Two", "c", "d", "# Three", "z"]);
        let new = doc(&["# Two", "c", "d", "# One", "a", "b", "# Three", "z"]);
        let remap = Remap::new(&old, &new);
        for text in ["a", "b", "c", "d", "z"] {
            let (to, _) = remap.block(block_of(&old, text)).unwrap();
            assert!((to as usize) < new.blocks.len());
        }
        // The longer run that kept its order is found exactly.
        let (to, fit) = remap.block(block_of(&old, "z")).unwrap();
        assert_eq!((new.block_text(to).trim_end(), fit), ("z", Fit::Same));
    }

    #[test]
    fn a_text_range_follows_its_block_and_is_dropped_when_its_text_changed() {
        let old = doc(&[
            "intro",
            "the quick brown fox",
            "- item one\n- item two",
            "end",
        ]);
        let new = doc(&[
            "added",
            "intro",
            "the quick brown fox",
            "- item one\n- item two",
            "the end",
        ]);
        let remap = Remap::new(&old, &new);
        let range = |doc: &Document, text: &str| {
            let at = doc.text.find(text).unwrap() as u32;
            (at, at + text.len() as u32)
        };
        let (from, to) = range(&old, "quick brown");
        assert_eq!(remap.range(from, to), Some(range(&new, "quick brown")));
        // To the very end of a block, inside a list, and across blocks.
        let (from, to) = range(&old, "brown fox");
        assert_eq!(remap.range(from, to), Some(range(&new, "brown fox")));
        let (from, to) = range(&old, "item two");
        assert_eq!(remap.range(from, to), Some(range(&new, "item two")));
        let from = range(&old, "brown").0;
        let to = range(&old, "item one").1;
        let moved = remap.range(from, to).unwrap();
        assert_eq!(
            new.text[moved.0 as usize..moved.1 as usize],
            old.text[from as usize..to as usize]
        );
        // A range reaching into the edited last block is gone, and so is an empty one.
        let to = range(&old, "end").1;
        assert_eq!(remap.range(from, to), None);
        assert_eq!(remap.range(from, from), None);
        assert_eq!(remap.offset(range(&old, "end").0), None);
    }

    fn sample() -> Document {
        parse(
            "# Title\n\nintro\n\n## First\n\none\n\ntwo\n\nthree\n\n## Second\n\nfour\n\nfive\n\nsix\n",
        )
    }

    fn place_at(doc: &Document, text: &str) -> ReadingPlace {
        let pos = ScrollPos {
            block: block_of(doc, text),
            offset: 7.5,
        };
        ReadingPlace::of(doc, pos).unwrap()
    }

    #[test]
    fn a_place_records_its_block_and_heading_and_survives_the_session() {
        let doc = sample();
        let place = place_at(&doc, "five");
        assert_eq!(place.heading, "second");
        assert_eq!(place.below, 2);
        assert_eq!(place.blocks, doc.blocks.len() as u32);
        assert!(!place.is_top());
        assert!(
            ReadingPlace::of(&doc, ScrollPos::default())
                .unwrap()
                .is_top()
        );
        assert_eq!(ReadingPlace::of(&parse(""), ScrollPos::default()), None);
        let text = place.to_setting();
        assert!(!text.contains('\n'));
        assert_eq!(ReadingPlace::from_setting(&text), Some(place.clone()));
        // The same document gives the same position back.
        let pos = place.resolve(&doc);
        assert_eq!((pos.block, pos.offset), (block_of(&doc, "five"), 7.5));
        for damaged in [
            "",
            "2 1 0 1 0 0 x",
            "1 x 0 1 0 0",
            "1 1 NaN 1 0 0 x",
            "1 1 0",
        ] {
            assert_eq!(ReadingPlace::from_setting(damaged), None, "{damaged:?}");
        }
        // A heading with spaces in its slug would still come back whole.
        let odd = ReadingPlace {
            heading: "a b  c".into(),
            ..place
        };
        assert_eq!(ReadingPlace::from_setting(&odd.to_setting()), Some(odd));
    }

    #[test]
    fn a_place_is_found_again_in_an_edited_file() {
        let old = sample();
        let place = place_at(&old, "five");
        let text_at = |doc: &Document, place: &ReadingPlace| {
            let pos = place.resolve(doc);
            (doc.block_text(pos.block).trim_end().to_string(), pos.offset)
        };
        // Blocks added above: the same text is found where it went.
        let added = parse(
            "# Title\n\nintro\n\nmore\n\nand more\n\n## First\n\none\n\ntwo\n\nthree\n\n## Second\n\nfour\n\nfive\n\nsix\n",
        );
        assert_eq!(text_at(&added, &place), ("five".to_string(), 7.5));
        // The block itself was edited, and blocks were added above: the same count below the
        // same heading.
        let edited = parse(
            "# Title\n\nintro\n\nmore\n\n## First\n\none\n\n## Second\n\nfour\n\nfive, edited\n\nsix\n",
        );
        assert_eq!(text_at(&edited, &place), ("five, edited".to_string(), 7.5));
        // The section got shorter: its last block, not one of the next section's.
        let shorter = parse("# Title\n\n## Second\n\nfour\n\n## Third\n\nseven\n\neight\n");
        assert_eq!(text_at(&shorter, &place).0, "four");
        // The same text under another heading loses to the one under its own heading.
        let twice = parse(
            "# Title\n\nfive\n\n## First\n\nfive\n\n## Second\n\nnew\n\nnew too\n\nnewer\n\nfive\n",
        );
        let pos = place.resolve(&twice);
        assert_eq!(pos.block as usize, twice.blocks.len() - 1);
        // The heading is gone but the text is not.
        let renamed = parse("# Title\n\nzero\n\n## Renamed\n\nfour\n\nfive\n");
        assert_eq!(text_at(&renamed, &place).0, "five");
    }

    #[test]
    fn a_place_with_nothing_to_hold_on_to_falls_back_by_length() {
        let old = joined(&[&numbered("old", 100)]);
        let place = ReadingPlace::of(
            &old,
            ScrollPos {
                block: 50,
                offset: 3.0,
            },
        )
        .unwrap();
        assert_eq!(place.heading, "");
        // Same length, other text: the same index, offset kept.
        let same_length = joined(&[&numbered("new", 100)]);
        let pos = place.resolve(&same_length);
        assert_eq!((pos.block, pos.offset), (50, 3.0));
        // Another length: the same share of the way down, at the block's top.
        let shorter = joined(&[&numbered("new", 20)]);
        let pos = place.resolve(&shorter);
        assert_eq!((pos.block, pos.offset), (10, 0.0));
        assert_eq!(place.resolve(&parse("")), ScrollPos::default());
        // A place inside a nested block falls back to the block that holds it.
        let nested = parse("- a\n- b\n- c\n\nlast");
        let place = ReadingPlace {
            block: 3,
            offset: 1.0,
            blocks: nested.blocks.len() as u32,
            hash: 1,
            heading: String::new(),
            below: 0,
        };
        assert_eq!(place.resolve(&nested).block, 0);
    }
}
