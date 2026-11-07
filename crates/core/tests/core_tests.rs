//! The old app's core_tests that still apply, expressed against the document model. `math` and
//! `families` exercised renderers, which move to the engine in Phase 4, and are not ported here.

use mdview_core::model::*;
use mdview_core::parse;
use mdview_core::slug::Slugger;

fn kinds(doc: &Document) -> Vec<BlockKind> {
    doc.blocks.iter().map(|b| b.kind).collect()
}

fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mdview-core-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
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
