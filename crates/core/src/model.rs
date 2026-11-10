//! The compact document model: flat arrays, one text buffer, ranges instead of owned strings.

use crate::limits::{MAX_FILE_BYTES, Oversize};
use std::collections::HashMap;
use std::fmt;

pub const NONE: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

impl TextRange {
    pub fn new(start: usize, end: usize) -> Self {
        TextRange {
            start: start as u32,
            end: end as u32,
        }
    }
    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }
    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start) as usize
    }
    pub fn usize(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// A reference into `Document::strings`, the pool for URLs, titles and other non-visible text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StrRef {
    pub start: u32,
    pub len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    None,
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Ltr,
    Rtl,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
    Baseline,
}

/// From `type` on an HTML list or list item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    Disc,
    Circle,
    Square,
    /// No marker: an item the parser added around stray content in a list, as in `<ul>text</ul>`.
    None,
}

/// `Left` and `Right` float the image with text around it; the others align it within its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageAlign {
    #[default]
    None,
    Left,
    Right,
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    pub fn label(self) -> &'static str {
        match self {
            AlertKind::Note => "Note",
            AlertKind::Tip => "Tip",
            AlertKind::Important => "Important",
            AlertKind::Warning => "Warning",
            AlertKind::Caution => "Caution",
        }
    }
}

/// What kind of block sits where is guaranteed whatever the input, so a renderer need not check:
/// - A `Table` holds only `TableRow`s, a `TableRow` only `TableCell`s, and those have no other parent.
///   Stray content goes into an implied cell, so its text stays in document order.
/// - A `List` holds only `ListItem`s, and those have no other parent. Stray content gets an item with
///   `ListMarker::None`, and a lone `<li>` an implied list.
/// - `colspan` is within 1 to `MAX_COLSPAN`, and `rowspan` within 1 to `MAX_ROWSPAN`.
/// - Nesting stays within a few levels of `MAX_BLOCK_DEPTH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph {
        align: Align,
    },
    /// Inline content with no paragraph around it: tight list items, text directly in HTML containers.
    Plain,
    Heading {
        level: u8,
        align: Align,
        slug: StrRef,
    },
    BlockQuote,
    Alert(AlertKind),
    /// `start` is `None` for a bullet list.
    List {
        start: Option<u32>,
    },
    ListItem {
        task: bool,
    },
    /// Highlighting is a later engine job; `lang` may be empty.
    CodeBlock {
        lang: StrRef,
    },
    Diagram {
        lang: StrRef,
        oversize: Option<Oversize>,
    },
    Math {
        oversize: Option<Oversize>,
    },
    ThematicBreak,
    Table,
    TableRow {
        header: bool,
    },
    TableCell {
        align: Align,
        header: bool,
        colspan: u16,
        rowspan: u16,
    },
    Details {
        open: bool,
    },
    Summary,
    FootnoteDefinition {
        footnote: u32,
    },
    Div {
        align: Align,
    },
}

/// One node of the block tree, stored in preorder. Children of block `i` start at `i + 1`, the next
/// sibling of a child `c` is at `c.end`, and the subtree of `i` ends at `blocks[i].end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub parent: u32,
    pub end: u32,
    /// Visible text of the whole subtree.
    pub text: TextRange,
    /// This block's own inline spans.
    pub spans: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Emphasis,
    Strong,
    Strike,
    Underline,
    Code,
    Kbd,
    Sub,
    Sup,
    Mark,
    Small,
    /// Index into `Document::links`.
    Link(u32),
    /// Atom. Index into `Document::images`; the alt text is not part of `Document::text`.
    Image(u32),
    /// The range holds the TeX source.
    Math {
        display: bool,
        oversize: Option<Oversize>,
    },
    /// Atom. Index into `Document::footnotes`.
    FootnoteRef(u32),
    /// Atom.
    Checkbox {
        checked: bool,
    },
}

/// An inline span. Spans are in preorder within their block and nest through `parent`, so empty
/// siblings at one position stay distinct from nested ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub parent: u32,
    pub kind: SpanKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkInfo {
    pub dest: StrRef,
    pub title: StrRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    /// At most `MAX_HTML_LENGTH_PX`.
    Px(u32),
    Percent(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImageInfo {
    /// Empty when the HTML had no usable `src`; the alt text is then all there is to show.
    pub src: StrRef,
    pub alt: StrRef,
    pub title: StrRef,
    pub width: Option<Length>,
    pub height: Option<Length>,
    /// From `<picture><source media="(prefers-color-scheme: ...)">`. Empty when absent.
    pub dark_src: StrRef,
    pub light_src: StrRef,
    pub align: ImageAlign,
}

/// Presentation attributes from an HTML tag that do not fit in `BlockKind`, for the few blocks that
/// have them. Sorted by `block`; use `Document::attrs_of`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockAttrs {
    pub block: u32,
    pub dir: Option<Dir>,
    /// For kinds with no `align` of their own: tables, rows, rules, lists and the like.
    pub align: Align,
    pub valign: Option<VAlign>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub marker: Option<ListMarker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub block: u32,
    /// The outline's text: trimmed and on one line, which the block's own text may not be.
    pub title: StrRef,
    pub slug: StrRef,
}

/// An explicit `id` or `name` target from embedded HTML. Headings are looked up separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub name: StrRef,
    pub block: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footnote {
    pub label: StrRef,
    /// 1-based display number in order of first reference; 0 when never referenced.
    pub number: u32,
    /// The definition, when one got a block of its own. `NONE` for a flattened definition, or one
    /// that was never defined; `defined` tells those apart.
    pub block: u32,
    /// Whether a definition for this label was seen, even if it was too deep to own a block.
    pub defined: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    InvalidUtf8,
    FileTooLarge,
    Oversize(Oversize),
    UnclosedHtmlDropped,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::InvalidUtf8 => {
                f.write_str("Some bytes were not valid UTF-8 and were replaced")
            }
            Warning::FileTooLarge => f.write_str("File exceeds the 16 MiB viewer limit"),
            Warning::Oversize(o) => o.fmt(f),
            Warning::UnclosedHtmlDropped => f.write_str(
                "Too many unclosed HTML tags: their formatting stops partway through the document",
            ),
        }
    }
}

/// A block or span waiting for the engine process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineJob {
    Highlight {
        block: u32,
        lang: StrRef,
        source: TextRange,
    },
    Diagram {
        block: u32,
        lang: StrRef,
        source: TextRange,
    },
    MathBlock {
        block: u32,
        source: TextRange,
    },
    MathInline {
        block: u32,
        span: u32,
        source: TextRange,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutlineItem {
    /// Index into `Document::headings`.
    pub heading: u32,
    /// Index of the parent heading, or `NONE` at the top level.
    pub parent: u32,
}

#[derive(Debug, Default, Clone)]
pub struct Document {
    pub text: String,
    pub strings: String,
    pub blocks: Vec<Block>,
    pub spans: Vec<Span>,
    pub links: Vec<LinkInfo>,
    pub images: Vec<ImageInfo>,
    pub headings: Vec<Heading>,
    pub anchors: Vec<Anchor>,
    pub footnotes: Vec<Footnote>,
    pub block_attrs: Vec<BlockAttrs>,
    pub warnings: Vec<Warning>,
}

impl Document {
    pub fn text_of(&self, r: TextRange) -> &str {
        &self.text[r.usize()]
    }

    pub fn str(&self, r: StrRef) -> &str {
        &self.strings[r.start as usize..(r.start + r.len) as usize]
    }

    pub fn block_text(&self, block: u32) -> &str {
        self.text_of(self.blocks[block as usize].text)
    }

    pub fn attrs_of(&self, block: u32) -> Option<&BlockAttrs> {
        let i = self
            .block_attrs
            .binary_search_by_key(&block, |a| a.block)
            .ok()?;
        Some(&self.block_attrs[i])
    }

    /// Direct children of `parent`, or the top-level blocks for `NONE`.
    pub fn children(&self, parent: u32) -> impl Iterator<Item = u32> + '_ {
        let (mut i, end) = if parent == NONE {
            (0, self.blocks.len() as u32)
        } else {
            (parent + 1, self.blocks[parent as usize].end)
        };
        std::iter::from_fn(move || {
            (i < end).then(|| {
                let c = i;
                i = self.blocks[c as usize].end;
                c
            })
        })
    }

    /// Headings nested by level: H1 then H3 makes the H3 a child of the H1.
    pub fn outline(&self) -> Vec<OutlineItem> {
        let mut stack: Vec<(u8, u32)> = Vec::new();
        self.headings
            .iter()
            .enumerate()
            .map(|(i, h)| {
                while stack.last().is_some_and(|&(level, _)| level >= h.level) {
                    stack.pop();
                }
                let parent = stack.last().map_or(NONE, |&(_, p)| p);
                stack.push((h.level, i as u32));
                OutlineItem {
                    heading: i as u32,
                    parent,
                }
            })
            .collect()
    }

    /// The block a URL fragment points at. This is the one place a fragment is percent-decoded, so
    /// pass it as the link wrote it. GitHub's `user-content-` prefix is accepted, and an exact match
    /// wins over a case-insensitive one.
    pub fn resolve_fragment(&self, fragment: &str) -> Option<u32> {
        let decoded = crate::links::percent_decode(fragment);
        let f = decoded.strip_prefix("user-content-").unwrap_or(&decoded);
        let names = || {
            let heads = self.headings.iter().map(|h| (self.str(h.slug), h.block));
            heads.chain(self.anchors.iter().map(|a| (self.str(a.name), a.block)))
        };
        names()
            .find(|(n, _)| *n == f)
            .or_else(|| names().find(|(n, _)| n.to_lowercase() == f.to_lowercase()))
            .map(|(_, b)| b)
    }

    /// Everything a later engine job has to do, in document order. Oversize items keep their source
    /// and a reason, so they are not listed.
    pub fn engine_jobs(&self) -> Vec<EngineJob> {
        let mut out = Vec::new();
        for (i, b) in self.blocks.iter().enumerate() {
            let block = i as u32;
            match b.kind {
                BlockKind::CodeBlock { lang } if lang.len > 0 => {
                    out.push(EngineJob::Highlight {
                        block,
                        lang,
                        source: b.text,
                    });
                }
                BlockKind::Diagram {
                    lang,
                    oversize: None,
                } => {
                    out.push(EngineJob::Diagram {
                        block,
                        lang,
                        source: b.text,
                    });
                }
                BlockKind::Math { oversize: None } => out.push(EngineJob::MathBlock {
                    block,
                    source: b.text,
                }),
                _ => {}
            }
            for s in b.spans.usize() {
                if let SpanKind::Math { oversize: None, .. } = self.spans[s].kind {
                    let span = &self.spans[s];
                    let source = TextRange {
                        start: span.start,
                        end: span.end,
                    };
                    out.push(EngineJob::MathInline {
                        block,
                        span: s as u32,
                        source,
                    });
                }
            }
        }
        out
    }

    /// Text ranges the reader cannot see, in order: source the engine draws over, where oversize
    /// items stay visible and so searchable, and the body of each `<details>` for which `is_open`
    /// says no. Open state is view state, which only the shell knows.
    pub fn hidden_ranges(&self, is_open: &impl Fn(u32) -> bool) -> Vec<TextRange> {
        let mut out = Vec::new();
        self.collect_hidden(0, self.blocks.len() as u32, is_open, &mut out);
        out
    }

    fn collect_hidden(
        &self,
        from: u32,
        to: u32,
        is_open: &impl Fn(u32) -> bool,
        out: &mut Vec<TextRange>,
    ) {
        let mut i = from;
        while i < to {
            let b = &self.blocks[i as usize];
            if matches!(b.kind, BlockKind::Details { .. }) && !is_open(i) {
                let mut summary_seen = false;
                for c in self.children(i) {
                    let child = &self.blocks[c as usize];
                    if !summary_seen && matches!(child.kind, BlockKind::Summary) {
                        summary_seen = true;
                        self.collect_hidden(c, child.end, is_open, out);
                    } else {
                        out.push(child.text);
                    }
                }
                i = b.end;
                continue;
            }
            if matches!(
                b.kind,
                BlockKind::Diagram { oversize: None, .. } | BlockKind::Math { oversize: None }
            ) {
                out.push(b.text);
            }
            for s in &self.spans[b.spans.usize()] {
                if matches!(s.kind, SpanKind::Math { oversize: None, .. }) {
                    out.push(TextRange {
                        start: s.start,
                        end: s.end,
                    });
                }
            }
            i += 1;
        }
    }

    /// The open state a details block was written with, for a shell that has not changed any.
    pub fn details_open_as_written(&self, block: u32) -> bool {
        matches!(
            self.blocks[block as usize].kind,
            BlockKind::Details { open: true }
        )
    }

    /// The innermost block whose text contains `pos`.
    pub fn block_at(&self, pos: u32) -> Option<u32> {
        let mut best = None;
        for (i, b) in self.blocks.iter().enumerate() {
            if b.text.start > pos {
                break;
            }
            if pos < b.text.end {
                best = Some(i as u32);
            }
        }
        best
    }

    /// Stable keys for details blocks: a hash of the summary text plus its occurrence, so an edit
    /// elsewhere in the file does not change which block a saved open state refers to.
    pub fn details_keys(&self) -> Vec<(u32, u64)> {
        let mut seen: HashMap<u64, u32> = HashMap::new();
        let mut out = Vec::new();
        for (i, b) in self.blocks.iter().enumerate() {
            if !matches!(b.kind, BlockKind::Details { .. }) {
                continue;
            }
            let block = i as u32;
            let summary = self
                .children(block)
                .find(|&c| matches!(self.blocks[c as usize].kind, BlockKind::Summary))
                .map_or("", |c| self.block_text(c).trim_end());
            let hash = fnv1a(summary.as_bytes());
            let count = seen.entry(hash).or_insert(0);
            let nth = *count;
            *count += 1;
            out.push((
                block,
                hash ^ u64::from(nth).wrapping_mul(0x9E37_79B9_7F4A_7C15),
            ));
        }
        out
    }

    /// Where the same reading position lands after a reload. The two versions are compared from both
    /// ends: a block before the edit keeps its index, one after it shifts by the change in length,
    /// and one inside it stays at the same index. Searching for equal text would be wrong, since
    /// repeated text such as "Fixed" or a table cell can match a block far away.
    pub fn remap_scroll(&self, old: &Document, pos: ScrollPos) -> ScrollPos {
        if self.blocks.is_empty() {
            return ScrollPos::default();
        }
        let same = |new_i: usize, old_i: usize| {
            let (x, y) = (&self.blocks[new_i], &old.blocks[old_i]);
            let (x_leaf, y_leaf) = (x.end as usize == new_i + 1, y.end as usize == old_i + 1);
            std::mem::discriminant(&x.kind) == std::mem::discriminant(&y.kind)
                && x_leaf == y_leaf
                // A container's text is its leaves' text, so comparing it too would only add cost.
                && (!x_leaf || self.text_of(x.text) == old.text_of(y.text))
        };
        let (new_len, old_len) = (self.blocks.len(), old.blocks.len());
        let shorter = new_len.min(old_len);
        let prefix = (0..shorter).take_while(|&i| same(i, i)).count();
        let suffix = (0..shorter - prefix)
            .take_while(|&i| same(new_len - 1 - i, old_len - 1 - i))
            .count();
        let at = pos.block as usize;
        let block = if at >= old_len - suffix && at < old_len {
            at + new_len - old_len
        } else {
            at.min(new_len - 1)
        };
        ScrollPos {
            block: block as u32,
            offset: pos.offset,
        }
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Scroll position as the plan stores it: a block and a pixel offset into it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollPos {
    pub block: u32,
    pub offset: f32,
}

/// Parses raw file bytes: strips a BOM, decodes UTF-16 files, replaces invalid UTF-8. The file limit
/// applies to these bytes, however much text they decode to.
pub fn parse_bytes(bytes: &[u8]) -> Document {
    if bytes.len() > MAX_FILE_BYTES {
        return Document {
            warnings: vec![Warning::FileTooLarge],
            ..Document::default()
        };
    }
    let (text, invalid) = crate::files::decode(bytes);
    let mut doc = crate::parse(&text);
    if invalid {
        doc.warnings.push(Warning::InvalidUtf8);
    }
    doc
}
