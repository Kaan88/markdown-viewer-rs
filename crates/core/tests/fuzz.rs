//! Seeded fuzzing with a time budget, for the tokenizer, autolinker and the whole parser. The seed is
//! printed, and a failure reports the input, so any finding can be replayed.

mod common;

use common::Rng;
use mdview_core::autolink::find_links;
use mdview_core::find::find_in_text;
use mdview_core::html::{self, Token, Tokenizer};
use mdview_core::limits::{MAX_BLOCK_DEPTH, MAX_COLSPAN, MAX_HTML_LENGTH_PX, MAX_ROWSPAN};
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
    "</li>",
    "</ul>",
    "</tr>",
    "</td>",
    "<th colspan=0 rowspan=99999>",
    "<img src=a width=inf height=1e30>",
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
    "> > [!TIP]\n",
    "- > [!tip]\n",
    "<div>\n\n",
    "<table>\n\n",
    "<ul>\n\n",
    "<li>\n\n",
    "[^1]: x\n\n",
    "\r\n",
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
    let swapped = ['\u{E000}', '\u{E001}', '\u{E002}', '\u{E003}', '\u{E004}'];
    assert!(
        !(d.text.contains(swapped) || d.strings.contains(swapped)) || src.contains(swapped),
        "protection leaked: {src:?}"
    );
    assert!(d.spans.len() <= 2 * src.len() + 64, "{src:?}");
    let parent_kind = |b: &Block| (b.parent != NONE).then(|| d.blocks[b.parent as usize].kind);
    let mut depth = vec![0; d.blocks.len()];
    for (i, b) in d.blocks.iter().enumerate() {
        // The shape `BlockKind` promises.
        let parent = parent_kind(b);
        match b.kind {
            BlockKind::TableRow { .. } => {
                assert!(matches!(parent, Some(BlockKind::Table)), "{src:?}")
            }
            BlockKind::TableCell {
                colspan, rowspan, ..
            } => {
                assert!(
                    matches!(parent, Some(BlockKind::TableRow { .. })),
                    "{src:?}"
                );
                assert!((1..=MAX_COLSPAN).contains(&colspan), "{src:?}");
                assert!((1..=MAX_ROWSPAN).contains(&rowspan), "{src:?}");
            }
            BlockKind::ListItem { .. } => {
                assert!(matches!(parent, Some(BlockKind::List { .. })), "{src:?}")
            }
            _ => assert!(
                !matches!(
                    parent,
                    Some(BlockKind::Table | BlockKind::TableRow { .. } | BlockKind::List { .. })
                ),
                "{:?} directly under {parent:?}: {src:?}",
                b.kind
            ),
        }
        depth[i] = if b.parent == NONE {
            1
        } else {
            depth[b.parent as usize] + 1
        };
        // A container at the limit can still gain an implied table row and cell, plus a leaf block
        // inside them, so five levels past the container limit is the real bound.
        assert!(depth[i] <= MAX_BLOCK_DEPTH + 5, "{src:?}");
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
    let px_ok = |l: Option<Length>| !matches!(l, Some(Length::Px(n)) if n > MAX_HTML_LENGTH_PX);
    assert!(
        d.block_attrs
            .iter()
            .all(|a| px_ok(a.width) && px_ok(a.height)),
        "{src:?}"
    );
    assert!(
        d.images.iter().all(|i| px_ok(i.width) && px_ok(i.height)),
        "{src:?}"
    );
    for f in &d.footnotes {
        assert!(f.block == NONE || (f.block as usize) < d.blocks.len());
        assert!(f.number == 0 || f.defined, "{src:?}");
        assert!(f.block == NONE || f.defined, "{src:?}");
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

/// The fastest of `runs` calls. A busy machine only ever adds time, so this is the figure to compare.
fn min_time(best: &mut Duration, runs: usize, mut f: impl FnMut()) {
    for _ in 0..runs {
        let t = Instant::now();
        f();
        *best = (*best).min(t.elapsed());
    }
}

/// Checks that 4x the input takes under 8x the time, where linear code gives about 4x and quadratic
/// about 16x. Comparing the two sizes needs no absolute limit, so a slow machine can't fail it. The
/// sizes take turns so a burst of other work hits both, and each round lowers the minimums further.
///
/// Under memory or CPU contention the large input is hit harder than the small one, so a would-be
/// failure is measured again on its own after a pause before it counts. A real quadratic keeps its
/// ratio through the retry; a loaded machine does not.
fn assert_scales(what: &str, name: &str, mut f: impl FnMut(usize)) {
    const SMALL: usize = 256 * 1024;
    const ROUNDS: usize = 8;
    // Below this the clock and the allocator's mood matter more than the code.
    const FLOOR: Duration = Duration::from_millis(2);
    let (mut small, mut large) = (Duration::MAX, Duration::MAX);
    for _ in 0..ROUNDS {
        for _ in 0..3 {
            min_time(&mut small, 1, || f(SMALL));
            min_time(&mut large, 1, || f(4 * SMALL));
        }
        if large < small * 8 + FLOOR {
            return;
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    let (mut small, mut large) = (Duration::MAX, Duration::MAX);
    for _ in 0..ROUNDS {
        min_time(&mut small, 1, || f(SMALL));
    }
    for _ in 0..ROUNDS {
        min_time(&mut large, 1, || f(4 * SMALL));
    }
    assert!(
        large < small * 8 + FLOOR,
        "{name}: {what} grew from {small:?} to {large:?} on 4x the input"
    );
}

/// The file limit is 16 MiB, where a quadratic term too small to show at 256 KiB dominates. Comparing
/// 256 KiB with 4 MiB (16x) at a 64x threshold catches it; the 4x comparison above cannot. Kept to a
/// few representative paths so the run time stays short.
#[test]
fn scaling_holds_at_the_file_limit() {
    const SMALL: usize = 256 * 1024;
    const LARGE: usize = 4 * 1024 * 1024;
    let check = |what: &str, build: &dyn Fn(usize) -> String, run: &dyn Fn(&str)| {
        let (small, large) = (build(SMALL), build(LARGE));
        let (mut a, mut b) = (Duration::MAX, Duration::MAX);
        for _ in 0..3 {
            min_time(&mut a, 1, || run(&small));
            min_time(&mut b, 1, || run(&large));
        }
        // 16x the input: linear code gives about 16x, so 64x leaves a wide margin for noise.
        assert!(
            b < a * 64,
            "{what}: grew from {a:?} to {b:?} on 16x the input"
        );
    };
    check("parse prose", &|n| "word ".repeat(n / 5), &|s| {
        std::hint::black_box(parse(s));
    });
    check("parse headings", &|n| "# a\n".repeat(n / 4), &|s| {
        std::hint::black_box(parse(s));
    });
    check(
        "scan tags and links",
        &|n| "<b>x</b> http://a.b/_c ".repeat(n / 23),
        &|s| {
            std::hint::black_box((Tokenizer::new(s).count(), find_links(s).len()));
        },
    );
}

/// Inputs built to make naive scanning quadratic, and to make a document far larger than its source.
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
        ("named entities in a url", "http://a.b/&amp;)"),
        ("www under", "_www.a"),
        ("url then lt", "http://a.b<"),
        ("at", "a@"),
        ("at dots", "a.b.c@d."),
        ("parens", "http://a.b/)))))"),
        ("colons", "://"),
        ("underscored urls", "http://a.b/_c_ "),
        ("backslashes", "\\"),
        ("backslash pairs", "\\\\*"),
        ("rejected math", "$a/$b "),
        ("dollars", "a$ "),
        ("bad display math", "x$$y$$z "),
        ("unmatched fences among dollars", "$x$y ` `` "),
        ("block tag in paragraph", "x <p>a</p> "),
        ("closers", "</p>"),
        ("span closers", "</b>"),
        ("closers in one paragraph", "a</div></li></td> "),
        ("unmatched emphasis closers", "a* b_ "),
        ("unmatched brackets", "]"),
        ("duplicate headings", "# a\n"),
        ("carried tags", "<b>\n\nx\n\n"),
        (
            "carried tags of each kind",
            "<i>\n<u>\n<s>\n<a href=x>\n\nx\n\n",
        ),
        ("duplicate footnotes", "[^a]: x\n\n"),
        ("misplaced alerts", "- > [!NOTE]\n  > x\n\n"),
        ("empty alerts", "> [!NOTE]\n\n"),
        ("orphan cells", "<td>x</td>"),
        ("stray table text", "<table>x"),
        ("lone items", "<li>x</li>\n\n"),
        ("quote markers", ">"),
        ("divs", "<div>"),
        ("many details", "<details><summary>s</summary></details>\n"),
    ];
    for (name, unit) in cases {
        let doc = |len: usize| unit.repeat(len / unit.len());
        let (small, large) = (doc(256 * 1024), doc(1024 * 1024));
        assert_scales("scanning", name, |len| {
            let s = pick_size(&small, &large, len);
            std::hint::black_box((Tokenizer::new(s).count(), find_links(s).len()));
        });
        assert_scales("parsing", name, |len| {
            std::hint::black_box(parse(pick_size(&small, &large, len)));
        });
        let d = parse(&large);
        println!("{name}: {} blocks, {} spans", d.blocks.len(), d.spans.len());
        assert!(
            d.spans.len() <= 2 * large.len(),
            "{name}: {} spans",
            d.spans.len()
        );
        assert!(
            d.blocks.len() <= 2 * large.len(),
            "{name}: {} blocks",
            d.blocks.len()
        );
    }
}

fn pick_size<'a>(small: &'a str, large: &'a str, len: usize) -> &'a str {
    if len <= 256 * 1024 { small } else { large }
}

/// Keys are built on every load that has details, and find runs on every keystroke, so they count too.
#[test]
fn details_keys_and_find_stay_linear() {
    let doc = |len: usize| {
        let mut s = String::with_capacity(len);
        let mut i = 0;
        while s.len() < len {
            s.push_str(&format!("<details><summary>s{i}</summary></details>\n"));
            i += 1;
        }
        parse(&s)
    };
    let (small, large) = (doc(256 * 1024), doc(1024 * 1024));
    assert_scales("details_keys", "unique summaries", |len| {
        let d = if len <= 256 * 1024 { &small } else { &large };
        std::hint::black_box(d.details_keys());
    });
    // Find is not compared here: with a fixed query the unbudgeted code is still linear in the text,
    // so no ratio can tell it from the budgeted one. `find_work_is_bounded_on_a_long_query_against_
    // repetitive_text` in regressions.rs is the test that fails when the budget is reverted.
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
