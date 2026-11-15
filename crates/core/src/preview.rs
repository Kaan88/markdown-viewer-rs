//! What a hover card shows: a footnote's text, or a heading with the first lines under it. Plain
//! text taken from a parsed document, short enough for a small card; the shell lays it out.

use crate::model::{BlockKind, Document, NONE, SpanKind};

/// The longest body kept, in characters. A card shows a few lines, so more is never read.
pub const MAX_BODY_CHARS: usize = 360;

/// A card's text: a title line (empty for none) and a body of lines separated by `\n`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Preview {
    pub title: String,
    pub body: String,
}

/// Footnote `index` of the document: "Footnote N" over the text of its definition. `None` when it
/// has no definition of its own to show.
pub fn footnote(doc: &Document, index: u32) -> Option<Preview> {
    let f = doc.footnotes.get(index as usize)?;
    if f.block == NONE || f.number == 0 {
        return None;
    }
    let mut body = String::new();
    for child in doc.children(f.block) {
        push_block(doc, child, &mut body);
    }
    cut(&mut body);
    (!body.is_empty()).then(|| Preview {
        title: format!("Footnote {}", f.number),
        body,
    })
}

/// An `<abbr title>`: its title as a card with no title line of its own. `None` when span `index`
/// is not one.
pub fn abbr(doc: &Document, index: u32) -> Option<Preview> {
    let span = doc.spans.get(index as usize)?;
    let SpanKind::Abbr(title) = span.kind else {
        return None;
    };
    let mut body: String = doc.str(title).trim().to_string();
    cut(&mut body);
    (!body.is_empty()).then(|| Preview {
        title: String::new(),
        body,
    })
}

/// The heading at `block` with the first lines of what follows it, up to the next heading.
pub fn heading(doc: &Document, block: u32) -> Option<Preview> {
    let b = doc.blocks.get(block as usize)?;
    if !matches!(b.kind, BlockKind::Heading { .. }) {
        return None;
    }
    let title = heading_title(doc, block)?;
    let mut body = String::new();
    body_from(doc, block, b.end, &mut body);
    Some(Preview { title, body })
}

/// The start of a document: its first heading as the title (empty when it has none) over the
/// text that opens it. Prose before the first heading is the body; when there is none (the
/// heading comes first, or after front matter, which the page shows as a table) the body is what
/// follows the heading.
pub fn start(doc: &Document) -> Option<Preview> {
    doc.blocks.first()?;
    let mut body = String::new();
    body_from(doc, NONE, 0, &mut body);
    let first = doc.headings.first().filter(|h| {
        doc.blocks
            .get(h.block as usize)
            .is_some_and(|b| b.parent == NONE)
    });
    if body.is_empty()
        && let Some(h) = first
    {
        let title = heading_title(doc, h.block)?;
        body_from(doc, h.block, doc.blocks[h.block as usize].end, &mut body);
        return Some(Preview { title, body });
    }
    let title = doc
        .headings
        .first()
        .map(|h| doc.str(h.title).trim().to_string())
        .unwrap_or_default();
    (!body.is_empty() || !title.is_empty()).then_some(Preview { title, body })
}

fn heading_title(doc: &Document, block: u32) -> Option<String> {
    let h = doc.headings.iter().find(|h| h.block == block)?;
    let title = doc.str(h.title).trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// Collects the text of the blocks that follow `after` among its siblings, starting at block
/// `from`, until a heading or the card is full. `after` is the heading itself, or `NONE` for the
/// top of the document.
fn body_from(doc: &Document, after: u32, from: u32, out: &mut String) {
    let (parent, mut i) = if after == NONE {
        (NONE, 0)
    } else {
        (doc.blocks[after as usize].parent, from)
    };
    let end = if parent == NONE {
        doc.blocks.len() as u32
    } else {
        doc.blocks[parent as usize].end
    };
    while i < end && out.chars().count() < MAX_BODY_CHARS {
        let b = &doc.blocks[i as usize];
        if matches!(b.kind, BlockKind::Heading { .. }) {
            break;
        }
        push_block(doc, i, out);
        i = b.end.max(i + 1);
    }
    cut(out);
}

/// Adds the visible lines of one block, whitespace made single spaces, skipping what is no prose.
fn push_block(doc: &Document, block: u32, out: &mut String) {
    let b = &doc.blocks[block as usize];
    if matches!(
        b.kind,
        BlockKind::ThematicBreak
            | BlockKind::CodeBlock { .. }
            | BlockKind::Table
            | BlockKind::Diagram { .. }
            | BlockKind::Math { .. }
            | BlockKind::FootnoteDefinition { .. }
    ) {
        return;
    }
    for line in doc.text_of(b.text).lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
        if out.chars().count() >= MAX_BODY_CHARS {
            break;
        }
    }
}

/// Keeps `s` to `MAX_BODY_CHARS` characters, at a character boundary; a longer text ends in an
/// ellipsis, after a whole word when the cut fell inside one that began not far back.
fn cut(s: &mut String) {
    if let Some((i, _)) = s.char_indices().nth(MAX_BODY_CHARS) {
        let keep = s
            .char_indices()
            .nth(MAX_BODY_CHARS - 1)
            .map_or(i, |(k, _)| k);
        let inside_a_word = s[keep..].chars().next().is_some_and(|c| !c.is_whitespace());
        s.truncate(keep);
        if inside_a_word
            && let Some(space) = s.rfind(char::is_whitespace)
            && s[space..].chars().count() <= 24
        {
            s.truncate(space);
        }
        let trimmed = s.trim_end().len();
        s.truncate(trimmed);
        s.push('…');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn a_footnote_shows_its_definition() {
        let doc = parse(
            "Text[^a] and more[^b].\n\n[^a]: The **first** note,\nover two lines.\n[^b]: Second.\n",
        );
        let p = footnote(&doc, 0).expect("a preview");
        assert_eq!(p.title, "Footnote 1");
        assert_eq!(p.body, "The first note, over two lines.");
        assert_eq!(footnote(&doc, 1).unwrap().title, "Footnote 2");
        assert!(footnote(&doc, 5).is_none());
    }

    #[test]
    fn an_undefined_footnote_has_no_preview() {
        let doc = parse("Text[^missing].\n");
        let shown: Vec<_> = (0..doc.footnotes.len() as u32)
            .map(|i| footnote(&doc, i))
            .collect();
        assert!(shown.iter().all(Option::is_none));
    }

    #[test]
    fn a_heading_shows_the_lines_under_it_up_to_the_next_heading() {
        let doc = parse(
            "# One\n\nintro\n\n## Two\n\nFirst   line\n\nSecond.\n\n- a\n- b\n\n## Three\n\nlater\n",
        );
        let two = doc
            .headings
            .iter()
            .find(|h| doc.str(h.title) == "Two")
            .unwrap();
        let p = heading(&doc, two.block).expect("a preview");
        assert_eq!(p.title, "Two");
        assert_eq!(p.body, "First line\nSecond.\na\nb");
        // A block that is not a heading has none.
        assert!(heading(&doc, two.block + 1).is_none());
    }

    #[test]
    fn a_heading_with_nothing_under_it_is_the_title_alone() {
        let doc = parse("# One\n## Two\n");
        let p = heading(&doc, doc.headings[0].block).unwrap();
        assert_eq!((p.title.as_str(), p.body.as_str()), ("One", ""));
    }

    #[test]
    fn formulas_diagrams_and_rules_are_not_prose() {
        let doc = parse("# T\n\n---\n\n$$\nx\n$$\n\n```mermaid\ngraph TD\n```\n\nBody.\n");
        assert_eq!(heading(&doc, 0).unwrap().body, "Body.");
    }

    #[test]
    fn the_body_is_cut_to_the_limit() {
        let long = "word ".repeat(200);
        let doc = parse(&format!("# T\n\n{long}\n"));
        let p = heading(&doc, 0).unwrap();
        assert!(p.body.ends_with("word…"));
        assert!(p.body.chars().count() <= MAX_BODY_CHARS);
        let doc = parse(&format!("# T\n\n{}\n", "é".repeat(500)));
        let body = heading(&doc, 0).unwrap().body;
        assert_eq!(body.chars().count(), MAX_BODY_CHARS);
        assert!(body.ends_with("é…"));
    }

    #[test]
    fn the_start_of_a_document_is_its_title_and_opening() {
        let doc = parse("# Notes\n\nOpening text.\n\n## Part\n\nmore\n");
        let p = start(&doc).unwrap();
        assert_eq!(
            (p.title.as_str(), p.body.as_str()),
            ("Notes", "Opening text.")
        );
        // Text before the first heading is the body, the heading still the title.
        let doc = parse("Intro first.\n\n# Later\n");
        let p = start(&doc).unwrap();
        assert_eq!(
            (p.title.as_str(), p.body.as_str()),
            ("Later", "Intro first.")
        );
        let doc = parse("Just text.\n");
        let p = start(&doc).unwrap();
        assert_eq!((p.title.as_str(), p.body.as_str()), ("", "Just text."));
        assert!(start(&parse("")).is_none());
    }

    #[test]
    fn a_heading_inside_a_quote_stops_at_the_end_of_the_quote() {
        let doc = parse("> ## Inner\n>\n> inside\n\noutside\n");
        let h = doc.headings.first().unwrap();
        let p = heading(&doc, h.block).unwrap();
        assert_eq!(p.body, "inside");
    }

    #[test]
    fn front_matter_code_and_tables_are_not_card_text() {
        let doc = crate::parse_bytes(
            b"---\ntitle: T\ntags: [a, b]\n---\n\n# Head\n\nBody text.\n\n```rust\nfn x() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nAfter.\n",
        );
        let p = start(&doc).expect("a preview");
        assert_eq!(p.title, "Head");
        assert_eq!(p.body, "Body text.\nAfter.");
        // A section that holds only a table has a title and no lines.
        let doc = parse("# One\n\n| a |\n|---|\n| 1 |\n\n## Two\n\nText.\n");
        let p = heading(&doc, doc.headings[0].block).unwrap();
        assert_eq!((p.title.as_str(), p.body.as_str()), ("One", ""));
    }

    #[test]
    fn the_cut_falls_after_a_whole_word() {
        let long = "seven77 ".repeat(100);
        let doc = parse(&format!(
            "# T

{long}
"
        ));
        let body = heading(&doc, 0).unwrap().body;
        assert!(body.ends_with("seven77…"), "{body}");
        assert!(body.chars().count() <= MAX_BODY_CHARS);
    }
}
