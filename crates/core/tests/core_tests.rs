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
