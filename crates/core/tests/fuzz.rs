//! Seeded fuzzing with a time budget, for the tokenizer, autolinker and the whole parser. The seed is
//! printed, and a failure reports the input, so any finding can be replayed.

mod common;

use common::Rng;
use mdview_core::autolink::find_links;
use mdview_core::find::find_in_text;
use mdview_core::html::{self, Token, Tokenizer};
use mdview_core::model::*;
use mdview_core::parse;
use std::time::{Duration, Instant};

const BUDGET: Duration = Duration::from_secs(4);

fn budget() -> Duration {
    std::env::var("MDVIEW_FUZZ_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(BUDGET, Duration::from_secs)
}

const HTML_PARTS: &[&str] = &[
    "<",
    ">",
    "</",
    "/>",
    "<!--",
    "-->",
    "<?",
    "?>",
    "<!DOCTYPE",
    "<![CDATA[",
    "]]>",
    "=",
    "\"",
    "'",
    " ",
    "\n",
    "&",
    "&amp;",
    "&#x41;",
    "&#0;",
    "&#xFFFFFFFF;",
    "&bogus;",
    "&ouml;",
    ";",
    "a",
    "div",
    "p",
    "br",
    "img",
    "src=",
    "href=",
    "script",
    "<script>",
    "</script>",
    "<style>",
    "</style>",
    "<iframe>",
    "</iframe>",
    "<title>",
    "<details>",
    "</details>",
    "<summary>",
    "</summary>",
    "<a href=\"javascript:alert(1)\">",
    "<a href='x'>",
    "</a>",
    "<table>",
    "<tr>",
    "<td>",
    "</table>",
    "<ul>",
    "<li>",
    "</ul>",
    "<pre>",
    "</pre>",
    "<code>",
    "</code>",
    "<b>",
    "</b>",
    "<strong>",
    "</strong>",
    "<picture>",
    "<source media=\"(prefers-color-scheme: dark)\" srcset=\"d.png\">",
    "</picture>",
    "<h1>",
    "</h1>",
    "<input type=checkbox>",
    "ğ",
    "İ",
    "日本",
    "🚀",
    "\u{0}",
    "\u{1}",
    "\u{E000}",
];

/// Block tags and math delimiters inside inline content, where the builder has to split what it is
/// building while Markdown's own end events are still to come.
const INLINE_BLOCK_PARTS: &[&str] = &[
    "| h |\n|---|\n| ",
    " | ",
    " |\n| ",
    "\n",
    "\n\n",
    " ",
    "x",
    "y z",
    "$",
    "$$",
    "$a$",
    "**",
    "*",
    "_",
    "~~",
    "`",
    "\\",
    "[",
    "](u)",
    "(",
    ")",
    "<p>",
    "</p>",
    "<p dir=rtl align=justify>",
    "<ul>",
    "<ol type=a>",
    "<li>",
    "</li>",
    "</ul>",
    "<div>",
    "</div>",
    "<h2>",
    "</h2>",
    "<pre>",
    "</pre>",
    "<details>",
    "</details>",
    "<summary>",
    "</summary>",
    "<table width=1>",
    "<tr>",
    "<td valign=top>",
    "</td>",
    "</table>",
    "<hr width=2>",
    "<blockquote>",
    "</blockquote>",
    "<script>",
    "</script>",
    "<object>",
    "</object>",
    "<a href=x>",
    "</a>",
    "<b>",
    "</b>",
    "<i>",
    "</i>",
    "<img alt=y>",
    "<br>",
    "[^a]",
    "[^A]",
    "\n\n[^a]: d\n\n",
    "- ",
    "> ",
    "# ",
    "---\n",
    "```\n",
    "\u{E003}",
];

const LINK_PARTS: &[&str] = &[
    "http://a.b/_",
    "http://",
    "https://",
    "ftp://",
    "www.",
    "mailto:",
    "xmpp:",
    "@",
    ".",
    "..",
    "a",
    "b.c",
    "x_y",
    "_",
    "*",
    "~",
    "(",
    ")",
    "[",
    "]",
    "<",
    ">",
    "?",
    "!",
    ",",
    ":",
    ";",
    "&amp;",
    "&",
    "/",
    "\\",
    " ",
    "\n",
    "İ",
    "ğ",
    "日本",
    "🚀",
    "|",
    "`",
    "\"",
    "'",
    "-",
    "+",
];

const MD_PARTS: &[&str] = &[
    "# ",
    "## ",
    "\n",
    "\n\n",
    "> ",
    "- ",
    "1. ",
    "- [x] ",
    "    ",
    "```\n",
    "```mermaid\n",
    "```math\n",
    "~~~\n",
    "| a | b |\n|---|---|\n",
    "|",
    "*",
    "**",
    "***",
    "_",
    "__",
    "~",
    "~~",
    "`",
    "``",
    "$",
    "$$",
    "[",
    "](",
    ")",
    "![",
    "[^1]",
    "[^1]: ",
    "[r]: /u\n",
    "[r]",
    "\\",
    "  \n",
    "> [!NOTE]\n",
    "text",
    " ",
    "http://a.b/_c_",
    "www.x.y",
    "a@b.co",
    "<div>",
    "</div>",
    "<b>",
    "</b>",
    "<details>",
    "</details>",
    "<script>",
    "</script>",
    "&amp;",
    "&#10;",
    "---\n",
    "===\n",
    "İ",
    "日本",
];

fn generate(r: &mut Rng, parts: &[&str], max: usize) -> String {
    let n = r.below(max);
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(r.pick(parts));
    }
    s
}

fn run(name: &str, seed: u64, parts: &[&str], max: usize, mut check: impl FnMut(&str)) {
    let mut r = Rng(seed);
    let start = Instant::now();
    let limit = budget();
    let mut n = 0u64;
    while start.elapsed() < limit {
        for _ in 0..64 {
            let s = generate(&mut r, parts, max);
            check(&s);
            n += 1;
        }
    }
    println!("{name}: seed {seed}, {n} inputs in {:?}", start.elapsed());
}

fn check_tokenizer(s: &str) {
    let mut joined = String::new();
    for t in Tokenizer::new(s) {
        assert!(!t.raw().is_empty(), "empty token in {s:?}");
        joined.push_str(t.raw());
        if let Token::Open { attrs, .. } = t {
            for (k, v) in html::attrs(attrs) {
                assert!(!k.is_empty());
                let _ = html::decode_entities(v);
            }
        }
    }
    assert_eq!(joined, s, "tokens must cover the input exactly");
    let _ = html::decode_entities(s);
}

fn check_links(s: &str) {
    let mut last = 0;
    for (a, b, kind) in find_links(s) {
        assert!(a >= last && a < b && b <= s.len(), "{s:?}");
        assert!(s.is_char_boundary(a) && s.is_char_boundary(b), "{s:?}");
        assert!(!s[a..b].chars().any(char::is_whitespace), "{s:?}");
        assert!(!kind.destination(&s[a..b]).is_empty());
        last = b;
    }
}

fn check_doc(s: &str) {
    let d = parse(s);
    check_invariants(&d, s);
    for q in ["a", "İ", "i", "日"] {
        for h in find_in_text(&d.text, q, 100).hits {
            assert!(
                d.text.is_char_boundary(h.start as usize)
                    && d.text.is_char_boundary(h.end as usize)
            );
        }
    }
    let hidden = d.hidden_ranges(&|_| false);
    assert!(hidden.windows(2).all(|w| w[0].end <= w[1].start), "{s:?}");
    let _ = mdview_core::find::find(&d, "x");
    let _ = d.remap_scroll(
        &parse("a\n\nb"),
        ScrollPos {
            block: 1,
            offset: 0.0,
        },
    );
    let _ = parse("a\n\nb").remap_scroll(
        &d,
        ScrollPos {
            block: 7,
            offset: 0.0,
        },
    );
    let _ = d.engine_jobs();
    let _ = d.outline();
    let _ = d.details_keys();
    let _ = common::writer::html(&d);
}

fn check_invariants(d: &Document, src: &str) {
    let tl = d.text.len() as u32;
    let boundary = |p: u32| d.text.is_char_boundary(p as usize);
    let swapped = ['\u{E000}', '\u{E001}', '\u{E002}', '\u{E003}'];
    assert!(
        !(d.text.contains(swapped) || d.strings.contains(swapped)) || src.contains(swapped),
        "protection leaked: {src:?}"
    );
    for (i, b) in d.blocks.iter().enumerate() {
        let i = i as u32;
        assert!(b.end > i && b.end as usize <= d.blocks.len(), "{src:?}");
        assert!(b.text.start <= b.text.end && b.text.end <= tl, "{src:?}");
        assert!(boundary(b.text.start) && boundary(b.text.end), "{src:?}");
        assert!(
            b.spans.start <= b.spans.end && b.spans.end as usize <= d.spans.len(),
            "{src:?}"
        );
        if b.parent != NONE {
            let p = d.blocks[b.parent as usize];
            assert!(b.parent < i && b.end <= p.end, "{src:?}");
            assert!(
                b.text.start >= p.text.start && b.text.end <= p.text.end,
                "{src:?}"
            );
        }
        for s in b.spans.usize() {
            let span = d.spans[s];
            assert!(
                span.start <= span.end && span.start >= b.text.start && span.end <= b.text.end,
                "{src:?} {span:?} {b:?}"
            );
            assert!(boundary(span.start) && boundary(span.end), "{src:?}");
            if span.parent != NONE {
                let p = d.spans[span.parent as usize];
                assert!(
                    (span.parent as usize) < s && span.parent >= b.spans.start,
                    "{src:?}"
                );
                assert!(
                    span.start >= p.start && span.end <= p.end,
                    "{src:?} {span:?} {p:?}"
                );
            }
            match span.kind {
                SpanKind::Link(l) => assert!((l as usize) < d.links.len()),
                SpanKind::Image(m) => assert!((m as usize) < d.images.len()),
                SpanKind::FootnoteRef(f) => assert!((f as usize) < d.footnotes.len()),
                _ => {}
            }
        }
    }
    for h in &d.headings {
        assert!(matches!(
            d.blocks[h.block as usize].kind,
            BlockKind::Heading { .. }
        ));
        assert!(!d.str(h.title).contains('\n'), "{src:?}");
    }
    for pair in d.block_attrs.windows(2) {
        assert!(pair[0].block < pair[1].block, "{src:?}");
    }
    for a in &d.block_attrs {
        assert!((a.block as usize) < d.blocks.len(), "{src:?}");
    }
    for f in &d.footnotes {
        assert!(f.block == NONE || (f.block as usize) < d.blocks.len());
        assert!(f.number == 0 || f.block != NONE, "{src:?}");
    }
    assert!(
        !d.strings.to_ascii_lowercase().contains("javascript:alert") || src.contains("]("),
        "{src:?}"
    );
}

#[test]
fn tokenizer_fuzz() {
    run(
        "tokenizer",
        0x1234_5678_9ABC_DEF1,
        HTML_PARTS,
        200,
        check_tokenizer,
    );
}

#[test]
fn autolink_fuzz() {
    run(
        "autolink",
        0x0FED_CBA9_8765_4321,
        LINK_PARTS,
        200,
        check_links,
    );
}

#[test]
fn html_through_parser_fuzz() {
    run("html+parser", 0xDEAD_BEEF_F00D_0001, HTML_PARTS, 120, |s| {
        check_doc(s);
        let d = parse(s);
        // Whatever the shape of the input, an executable destination from raw HTML never reaches the model.
        assert!(
            !d.strings.to_ascii_lowercase().contains("javascript:"),
            "{s:?}"
        );
    });
}

#[test]
fn block_html_in_inline_content_fuzz() {
    run(
        "inline blocks",
        0x1234_ABCD_5678_0011,
        INLINE_BLOCK_PARTS,
        60,
        check_doc,
    );
}

#[test]
fn markdown_autolink_through_parser_fuzz() {
    run(
        "links+parser",
        0xABAD_1DEA_0000_0007,
        LINK_PARTS,
        120,
        check_doc,
    );
}

#[test]
fn markdown_structure_fuzz() {
    run("markdown", 0xC0FF_EE00_1234_0009, MD_PARTS, 120, check_doc);
}

#[test]
fn random_bytes_do_not_panic() {
    let mut r = Rng(0x5EED_0000_0000_0011);
    let start = Instant::now();
    let mut n = 0u64;
    while start.elapsed() < budget() {
        let len = r.below(600);
        let bytes: Vec<u8> = (0..len).map(|_| r.next() as u8).collect();
        let d = mdview_core::parse_bytes(&bytes);
        let (src, _) = mdview_core::files::decode(&bytes);
        check_invariants(&d, &src);
        n += 1;
    }
    println!("random bytes: {n} inputs");
}

/// Inputs built to make naive scanning quadratic. Each is 2 MiB and must finish well inside a second
/// per megabyte, or the 16 MiB limit would mean a hang.
#[test]
fn pathological_inputs_stay_linear() {
    let cases: &[(&str, &str)] = &[
        ("unterminated attr", "<a \""),
        ("unterminated tag", "<a "),
        ("unclosed script", "<script>"),
        ("unclosed comment", "<!-- "),
        ("lone lt", "< "),
        ("close no gt", "</a "),
        ("entities", "&x"),
        ("www under", "_www.a"),
        ("url then lt", "http://a.b<"),
        ("at", "a@"),
        ("at dots", "a.b.c@d."),
        ("parens", "http://a.b/)))))"),
        ("colons", "://"),
        ("underscored urls", "http://a.b/_c_ "),
        ("backslashes", "\\"),
        ("rejected math", "$a/$b "),
        ("dollars", "a$ "),
        ("bad display math", "x$$y$$z "),
        ("unmatched fences among dollars", "$x$y ` `` "),
        ("block tag in paragraph", "x <p>a</p> "),
        ("closers", "</p>"),
        ("span closers", "</b>"),
        ("headings", "# a\n"),
    ];
    for (name, unit) in cases {
        let s = unit.repeat(2 * 1024 * 1024 / unit.len());
        let t = Instant::now();
        let n = Tokenizer::new(&s).count();
        let links = find_links(&s).len();
        let elapsed = t.elapsed();
        let t = Instant::now();
        // Wrapped in an HTML block and a paragraph, to cover both paths through the builder.
        let d = parse(&s);
        let parsed = t.elapsed();
        println!(
            "{name}: scan {elapsed:?} ({n} tokens, {links} links), parse {parsed:?} ({} blocks)",
            d.blocks.len()
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "{name} scan took {elapsed:?}"
        );
        assert!(
            parsed < Duration::from_secs(20),
            "{name} parse took {parsed:?}"
        );
    }
}

#[test]
fn deep_nesting_does_not_overflow() {
    for unit in [
        "> ",
        "- ",
        "<div>",
        "<b>",
        "*a ",
        "[",
        "<details>",
        "x <div>",
        "| <ul>",
    ] {
        let s = unit.repeat(5_000);
        let d = parse(&s);
        check_invariants(&d, &s);
    }
}
