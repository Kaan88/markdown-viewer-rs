//! `gen <seed> <bytes> <out>`: writes the seeded benchmark document.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let doc = parser_spike::generate(a[1].parse().unwrap(), a[2].parse().unwrap());
    std::fs::write(&a[3], &doc).unwrap();
    println!("{} bytes, {} lines", doc.len(), doc.lines().count());
    use pulldown_cmark::{Event, Parser, Tag};
    let mut c = std::collections::BTreeMap::new();
    for e in parser_spike::autolink(Parser::new_ext(&doc, parser_spike::pd_viewer())) {
        let k = match e {
            Event::Start(Tag::Heading { .. }) => "heading",
            Event::Start(Tag::Paragraph) => "paragraph",
            Event::Start(Tag::List(_)) => "list",
            Event::Start(Tag::Item) => "item",
            Event::Start(Tag::Table(_)) => "table",
            Event::Start(Tag::CodeBlock(_)) => "codeblock",
            Event::Start(Tag::HtmlBlock) => "htmlblock",
            Event::Start(Tag::BlockQuote(Some(_))) => "alert",
            Event::Start(Tag::BlockQuote(None)) => "blockquote",
            Event::Start(Tag::Link { .. }) => "link",
            Event::Start(Tag::Image { .. }) => "image",
            Event::Start(Tag::Emphasis | Tag::Strong) => "emphasis",
            Event::Start(Tag::Strikethrough) => "strike",
            Event::Start(Tag::FootnoteDefinition(_)) => "footnote",
            Event::InlineMath(_) | Event::DisplayMath(_) => "math",
            Event::TaskListMarker(_) => "task",
            _ => "other",
        };
        *c.entry(k).or_insert(0usize) += 1;
    }
    println!("{c:?}");
}
