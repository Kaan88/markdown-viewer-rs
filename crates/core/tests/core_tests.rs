//! The old app's core_tests that still apply, expressed against the document model. `math` and
//! `families` exercised renderers, which move to the engine in Phase 4, and are not ported here.

mod common;

use mdview_core::model::*;
use mdview_core::parse;
use mdview_core::slug::Slugger;

fn kinds(doc: &Document) -> Vec<BlockKind> {
    doc.blocks.iter().map(|b| b.kind).collect()
}

fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = common::temp_dir("core");
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn anchors() {
    let mut used = Slugger::default();
    assert_eq!(used.slug("Hello, world!"), "hello-world");
    assert_eq!(used.slug("Hello, world!"), "hello-world-1");
    assert_eq!(used.slug("Türkçe"), "türkçe");
}

#[test]
fn gfm() {
    let d = parse(
        "# Title\n\n| A | B |\n|---|---|\n|1|2|\n\n- [x] Done\n\n~~old~~\n\n<script>alert(1)</script>",
    );
    assert!(kinds(&d).contains(&BlockKind::Table));
    assert!(
        d.spans
            .iter()
            .any(|s| s.kind == SpanKind::Checkbox { checked: true })
    );
    assert!(d.spans.iter().any(|s| s.kind == SpanKind::Strike));
    // GitHub's tag filter shows a script tag as literal text, so the tag must not vanish.
    assert!(d.text.contains("<script>") && d.text.contains("</script>"));
    assert_eq!(d.headings.len(), 1);
}

#[test]
fn utf8_bom_and_unicode() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("# Café — 日本語\n\nUnicode text.\n".as_bytes());
    let path = temp_file("Unicode.md", &bytes);
    let (d, _) = mdview_core::files::load_document(&path).unwrap();
    assert_eq!(d.headings.len(), 1);
    assert_eq!(d.str(d.headings[0].title), "Café — 日本語");
    assert!(d.warnings.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn details() {
    let d = parse("<details><summary>More</summary><p>Secret</p></details>");
    assert_eq!(d.blocks[0].kind, BlockKind::Details { open: false });
    let summary = d
        .children(0)
        .find(|&c| d.blocks[c as usize].kind == BlockKind::Summary)
        .unwrap();
    assert_eq!(d.block_text(summary).trim(), "More");
    // Closed or open is view state now; the content is in the model either way.
    assert!(d.block_text(0).contains("Secret"));
    let open = parse("<details open><summary>More</summary></details>");
    assert_eq!(open.blocks[0].kind, BlockKind::Details { open: true });
}

#[test]
fn html() {
    let d = parse(
        "<p align='center'><a href='https://example.com'><img src='badge.svg' alt='Badge'></a></p>",
    );
    assert_eq!(
        d.blocks[0].kind,
        BlockKind::Paragraph {
            align: Align::Center
        }
    );
    assert_eq!(d.str(d.links[0].dest), "https://example.com");
    assert_eq!(d.str(d.images[0].src), "badge.svg");
    assert_eq!(d.str(d.images[0].alt), "Badge");
}

#[test]
fn math_and_markdown_inside_details() {
    let d = parse(
        "Inline $x_1^2 + \\frac{a}{b}$\n\n$$\n\\int_0^1 x dx\n$$\n\n<details><summary>More</summary>\n\n**Bold secret**\n\n</details>",
    );
    let jobs = d.engine_jobs();
    assert!(
        jobs.iter()
            .any(|j| matches!(j, EngineJob::MathInline { .. }))
    );
    assert!(
        jobs.iter()
            .any(|j| matches!(j, EngineJob::MathBlock { .. }))
    );
    let strong = d.spans.iter().find(|s| s.kind == SpanKind::Strong).unwrap();
    assert_eq!(
        &d.text[strong.start as usize..strong.end as usize],
        "Bold secret"
    );
    let details = d
        .blocks
        .iter()
        .position(|b| matches!(b.kind, BlockKind::Details { .. }))
        .unwrap();
    assert!(d.block_text(details as u32).contains("Bold secret"));
}

#[test]
fn malformed_limits() {
    let big = format!("```mermaid\n{}\n```\n", "x".repeat(65537));
    let d = parse(&big);
    assert!(matches!(
        d.blocks[0].kind,
        BlockKind::Diagram {
            oversize: Some(_),
            ..
        }
    ));
    assert!(
        d.block_text(0).len() > 65536,
        "oversize blocks keep their source"
    );
    assert!(d.engine_jobs().is_empty());
}

// ---- source lines ----------------------------------------------------------------------------

/// The line of each block of kind `want` (by name), in order.
fn lines_of(doc: &Document, want: fn(&BlockKind) -> bool) -> Vec<u32> {
    doc.blocks
        .iter()
        .filter(|b| want(&b.kind))
        .map(|b| b.line)
        .collect()
}

const LINES: &str = "\
# Title

Intro paragraph
that runs two lines.

- one
- two
  - nested

| a | b |
|---|---|
| 1 | 2 |
| 3 | 4 |

```rust
fn main() {}
```

> quoted

## Last

end
";

#[test]
fn blocks_know_their_source_line() {
    let doc = parse(LINES);
    let heading = |b: &BlockKind| matches!(b, BlockKind::Heading { .. });
    assert_eq!(lines_of(&doc, heading), [1, 21]);
    let paragraph = |b: &BlockKind| matches!(b, BlockKind::Paragraph { .. });
    assert_eq!(lines_of(&doc, paragraph)[..1], [3]);
    assert_eq!(
        lines_of(&doc, |b| matches!(b, BlockKind::List { .. })),
        [6, 8]
    );
    assert_eq!(
        lines_of(&doc, |b| matches!(b, BlockKind::ListItem { .. })),
        [6, 7, 8]
    );
    assert_eq!(lines_of(&doc, |b| matches!(b, BlockKind::Table)), [10]);
    assert_eq!(
        lines_of(&doc, |b| matches!(b, BlockKind::TableRow { .. })),
        [10, 12, 13]
    );
    assert_eq!(
        lines_of(&doc, |b| matches!(b, BlockKind::CodeBlock { .. })),
        [15]
    );
    assert_eq!(lines_of(&doc, |b| matches!(b, BlockKind::BlockQuote)), [19]);
    // The lines never go backwards through the tree.
    assert!(doc.blocks.windows(2).all(|w| w[0].line <= w[1].line));
}

#[test]
fn the_block_for_a_line() {
    let doc = parse(LINES);
    let kind_at = |line| doc.blocks[doc.block_at_line(line).unwrap() as usize].kind;
    assert!(matches!(kind_at(1), BlockKind::Heading { .. }));
    assert!(
        matches!(kind_at(2), BlockKind::Heading { .. }),
        "a blank line belongs to the block above"
    );
    // The second line of a paragraph is the paragraph.
    let intro = doc.block_at_line(4).unwrap();
    assert_eq!(doc.blocks[intro as usize].line, 3);
    assert!(doc.block_text(intro).starts_with("Intro"));
    // The innermost: the plain text of the nested item, not the list.
    let nested = doc.block_at_line(8).unwrap();
    assert_eq!(doc.block_text(nested).trim(), "nested");
    let cell = doc.block_at_line(13).unwrap();
    assert!(matches!(
        doc.blocks[cell as usize].kind,
        BlockKind::TableCell { .. }
    ));
    assert!(doc.block_text(cell).starts_with('4'));
    // Inside a code block it is the block; past the end it is the last one; 0 is the first.
    assert!(matches!(kind_at(16), BlockKind::CodeBlock { .. }));
    assert_eq!(
        doc.block_text(doc.block_at_line(999).unwrap()).trim(),
        "end"
    );
    assert_eq!(doc.block_at_line(0), Some(0));
    assert_eq!(parse("").block_at_line(1), None);
}

#[test]
fn front_matter_does_not_move_the_lines() {
    // Four lines of front matter that the page shows as a table of a different height.
    let doc = parse("---\ntitle: A\nauthor: B\n---\n\n# Head\n\ntext\n");
    let head = doc.headings[0].block;
    assert_eq!(doc.blocks[head as usize].line, 6);
    let text = doc.block_at_line(8).unwrap();
    assert_eq!(doc.block_text(text).trim(), "text");
    // The table that stands for it starts on line 1.
    assert_eq!(doc.blocks[0].line, 1);
}

#[test]
fn lines_survive_what_the_parser_rewrites() {
    // A hidden `$`, an autolink, GitHub math and a CRLF file: none moves a line break.
    let src = "a $5 and $6\r\n\r\nsee http://x.y/a_b_c and $`x`$\r\n\r\n# Last\r\n";
    let doc = parse(src);
    let last = doc.headings[0].block;
    assert_eq!(doc.blocks[last as usize].line, 5);
    let doc = parse("\u{FEFF}# A\n\ntext\n");
    assert_eq!(doc.blocks[doc.block_at_line(3).unwrap() as usize].line, 3);
}

#[test]
fn every_fixture_has_sane_block_lines() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let doc = mdview_core::model::parse_bytes(&bytes);
        let lines = bytes.iter().filter(|&&b| b == b'\n').count() as u32 + 1;
        assert!(
            doc.blocks.iter().all(|b| (1..=lines).contains(&b.line)),
            "{}: a line outside the file",
            path.display()
        );
        let backwards = doc
            .blocks
            .windows(2)
            .filter(|w| w[0].line > w[1].line)
            .count();
        assert_eq!(backwards, 0, "{}: lines go backwards", path.display());
        seen += 1;
    }
    assert!(seen > 5);
}

/// What the document's text has from the place source line `line`, byte `col`, is shown at.
fn shown_at(doc: &Document, line: u32, col: u32) -> &str {
    &doc.text[doc.text_at_line(line, col).unwrap() as usize..]
}

const INSIDE: &str = "\
# Title

First line of a paragraph
second line with *emphasis* in it
third line.

- item one
  goes on here
- item two

```rust
fn main() {

    let x = 1;
}
```

| a | b |
|---|---|
| c | d |
| e | f |

    indented one
    indented two

end
";

#[test]
fn a_line_inside_a_paragraph_or_a_list_item_is_found_where_its_text_begins() {
    let doc = parse(INSIDE);
    assert!(shown_at(&doc, 3, 0).starts_with("First line"));
    assert!(shown_at(&doc, 4, 0).starts_with("second line with emphasis"));
    assert!(shown_at(&doc, 5, 0).starts_with("third line."));
    // A place along a line, as far along its text: markup that shows nothing (two stars
    // here) makes it that much late.
    assert!(shown_at(&doc, 4, 7).starts_with("line with emphasis"));
    assert!(shown_at(&doc, 4, 29).starts_with("it"));
    // Past the end of the line: still that line's text.
    assert!(shown_at(&doc, 4, 500).starts_with(" third"));
    // Before the line's first text (a list marker, an indent): that text.
    assert!(shown_at(&doc, 8, 1).starts_with("goes on here"));
    assert!(shown_at(&doc, 7, 0).starts_with("item one"));
    assert!(shown_at(&doc, 8, 0).starts_with("goes on here"));
    assert!(shown_at(&doc, 9, 0).starts_with("item two"));
    // A line with no text of its own (the blank one after the paragraph): the line before it.
    assert!(shown_at(&doc, 6, 0).starts_with("third line."));
    // The other way round.
    let at = doc.text.find("second line").unwrap() as u32;
    assert_eq!(doc.line_at_text(at), Some((4, 0)));
    assert_eq!(doc.line_at_text(at + 7), Some((4, 7)));
    let at = doc.text.find("goes on").unwrap() as u32;
    assert_eq!(doc.line_at_text(at + 2).map(|(line, _)| line), Some(8));
}

#[test]
fn a_line_of_a_code_block_is_found_exactly() {
    let doc = parse(INSIDE);
    // The fence is the block's start; the lines after it are the code's.
    assert!(shown_at(&doc, 11, 0).starts_with("fn main() {"));
    assert!(shown_at(&doc, 12, 0).starts_with("fn main() {"));
    assert!(shown_at(&doc, 14, 0).starts_with("    let x = 1;"));
    assert!(shown_at(&doc, 14, 4).starts_with("let x = 1;"));
    assert!(shown_at(&doc, 15, 0).starts_with("}"));
    // The closing fence: the last line of the code.
    assert!(shown_at(&doc, 16, 0).starts_with("}"));
    assert!(shown_at(&doc, 23, 0).starts_with("indented one"));
    assert!(shown_at(&doc, 24, 0).starts_with("indented two"));
    for (needle, line) in [("let x", 14), ("fn main", 12), ("indented two", 24)] {
        let at = doc.text.find(needle).unwrap() as u32;
        assert_eq!(doc.line_at_text(at).map(|(l, _)| l), Some(line), "{needle}");
    }
}

#[test]
fn a_line_of_a_table_is_its_row() {
    let doc = parse(INSIDE);
    let row = |line: u32| {
        let at = doc.text_at_line(line, 0).unwrap();
        let cell = doc.block_at(at).unwrap();
        doc.blocks[doc.blocks[cell as usize].parent as usize].line
    };
    assert_eq!((row(18), row(20), row(21)), (18, 20, 21));
    let at = doc
        .text
        .find('f')
        .map(|_| doc.text.rfind('f').unwrap())
        .unwrap() as u32;
    assert_eq!(doc.line_at_text(at).map(|(l, _)| l), Some(21));
}

#[test]
fn a_paragraph_written_as_one_long_line_is_marked_along_it() {
    let words: Vec<String> = (0..120).map(|n| format!("word{n:03}")).collect();
    let src = format!("# T\n\n{}\n\nend\n", words.join(" "));
    let doc = parse(&src);
    // Byte 8 * n of the line is where word n starts, in the file and in the text.
    for n in [0u32, 1, 17, 60, 119] {
        assert!(
            shown_at(&doc, 3, 8 * n).starts_with(&words[n as usize]),
            "{n}"
        );
        let at = doc.text.find(&words[n as usize]).unwrap() as u32;
        assert_eq!(doc.line_at_text(at), Some((3, 8 * n)));
    }
    // With markup the place is late by what the markup hides since the last mark, and a mark
    // follows a long hidden stretch.
    let src = "One **two** [three](https://example.com/a/very/long/address/that/shows/nothing) four five six seven eight nine ten eleven twelve thirteen\n";
    let doc = parse(src);
    let col = src.find("thirteen").unwrap() as u32;
    assert!(shown_at(&doc, 1, col).starts_with("thirteen"));
    let col = src.find("four").unwrap() as u32;
    assert!(shown_at(&doc, 1, col).starts_with("four"));
    // One mark a line for short lines, a few along a long one: not one per piece of text.
    let short = parse("a *b* c `d` e\nf *g* h\n");
    assert_eq!(short.marks.len(), 2);
}

#[test]
fn every_fixture_keeps_its_marks_in_order_and_inside_the_text() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let doc = parse(&src);
        let lines = src.lines().count() as u32 + 1;
        assert!(doc.marks.windows(2).all(|w| w[0].text <= w[1].text));
        assert!(doc.marks.iter().all(|m| m.text as usize <= doc.text.len()));
        assert!(doc.marks.iter().all(|m| m.line >= 1 && m.line <= lines));
        // Every line and every place answers with a place inside the text, on a character.
        for line in 0..lines + 2 {
            for col in [0, 3, 70, 4000] {
                if let Some(at) = doc.text_at_line(line, col) {
                    assert!(doc.text.is_char_boundary(at as usize), "{}", path.display());
                    assert!((at as usize) < doc.text.len());
                }
            }
        }
        seen += 1;
    }
    assert!(seen > 5);
}

/// Prints what a parse costs, for comparing before and after a change to the parser (run it
/// with `--release -- --ignored --nocapture parse_cost`).
#[test]
#[ignore = "a measurement"]
fn parse_cost() {
    let showcase =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/showcase.md");
    let one = std::fs::read_to_string(showcase).unwrap();
    // About 8 MB of ordinary Markdown, and 8 MB of one-line paragraphs.
    let big = one.repeat(8 * 1024 * 1024 / one.len() + 1);
    let line = "Lorem ipsum dolor sit amet, *consectetur* adipiscing elit, sed do `eiusmod` tempor incididunt ut labore et dolore magna aliqua. ".repeat(12);
    let long = format!("{line}\n\n").repeat(8 * 1024 * 1024 / (line.len() + 2));
    for (name, src) in [("showcase x n", &big), ("long lines", &long)] {
        let mut times: Vec<f64> = (0..9)
            .map(|_| {
                let t = std::time::Instant::now();
                let doc = parse(src);
                let ms = t.elapsed().as_secs_f64() * 1e3;
                assert!(!doc.blocks.is_empty());
                ms
            })
            .collect();
        times.sort_by(f64::total_cmp);
        eprintln!(
            "{name}: {:.1} MB, min {:.1} ms, median {:.1} ms",
            src.len() as f64 / 1048576.0,
            times[0],
            times[4]
        );
    }
}
