//! One test per "Not ported" bug that lives in core, plus the behaviours the plan calls out.

mod common;

use mdview_core::commands::{self, CommandId, Menu, Shortcut, strings};
use mdview_core::files::{self, FileStamp, LoadError, tab_labels};
use mdview_core::find::{find, find_in_text, find_visible};
use mdview_core::limits::*;
use mdview_core::links::{ImageSource, LinkAction, classify_image, classify_link};
use mdview_core::model::*;
use mdview_core::parse;
use mdview_core::session::*;
use mdview_core::status::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mdview-core-reg-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn span_text(d: &Document, s: &Span) -> String {
    d.text[s.start as usize..s.end as usize].to_string()
}

fn links(d: &Document) -> Vec<(String, String)> {
    d.spans
        .iter()
        .filter_map(|s| match s.kind {
            SpanKind::Link(l) => {
                Some((span_text(d, s), d.str(d.links[l as usize].dest).to_string()))
            }
            _ => None,
        })
        .collect()
}

// Not-ported bugs

#[test]
fn duplicate_parent_tab_labels_differ() {
    let paths: Vec<PathBuf> = ["/x/a/docs/README.md", "/x/b/docs/README.md", "/x/other.md"]
        .iter()
        .map(PathBuf::from)
        .collect();
    let labels = tab_labels(&paths);
    assert_ne!(
        labels[0], labels[1],
        "the old app gave these the same label"
    );
    assert!(labels[0].starts_with("README.md — "));
    assert!(labels[0].contains('a') && labels[1].contains('b'));
    assert_eq!(labels[2], "other.md");
}

#[test]
fn distinct_parents_use_the_parent_name() {
    let paths: Vec<PathBuf> = ["/p/one/README.md", "/p/two/readme.md"]
        .iter()
        .map(PathBuf::from)
        .collect();
    assert_eq!(tab_labels(&paths), ["README.md — one", "readme.md — two"]);
}

#[test]
fn refreshing_status_never_sticks() {
    for outcome in [
        ReloadOutcome::Ok { warnings: vec![] },
        ReloadOutcome::Ok {
            warnings: vec!["w".into()],
        },
        ReloadOutcome::Failed("bad".into()),
        ReloadOutcome::Missing { had_content: true },
        ReloadOutcome::Missing { had_content: false },
    ] {
        let mut t = ReloadTracker::default();
        let (generation, shown) = t.begin(true);
        assert_eq!(shown.unwrap().text, strings::REFRESHING);
        let done = t
            .finished(generation, outcome)
            .expect("the final message replaces Refreshing");
        assert_ne!(done.text, strings::REFRESHING);
        assert!(!t.is_running());
    }
}

#[test]
fn refresh_success_is_transient_and_silent_when_automatic() {
    let mut t = ReloadTracker::default();
    let (generation, shown) = t.begin(false);
    assert!(shown.is_none());
    let done = t
        .finished(generation, ReloadOutcome::Ok { warnings: vec![] })
        .unwrap();
    assert_eq!(done.text, strings::REFRESHED);
    assert_eq!(done.timeout_ms, Some(2500));
}

#[test]
fn timeout_message_is_not_overwritten() {
    let mut t = ReloadTracker::default();
    let (generation, _) = t.begin(false);
    assert_eq!(
        t.timed_out(generation).unwrap().text,
        strings::RENDER_TIMEOUT
    );
    // Killing the worker makes it report a failure. The old app showed that instead.
    assert_eq!(
        t.finished(
            generation,
            ReloadOutcome::Failed("Could not render document: ".into())
        ),
        None
    );
}

#[test]
fn stale_generation_is_ignored() {
    let mut t = ReloadTracker::default();
    let (old, _) = t.begin(false);
    let (new, _) = t.begin(false);
    assert_eq!(t.finished(old, ReloadOutcome::Failed("late".into())), None);
    assert!(
        t.finished(new, ReloadOutcome::Ok { warnings: vec![] })
            .is_some()
    );
}

#[test]
fn failed_images_retry() {
    let mut r = ImageRequests::with_retry_after(Duration::from_secs(30));
    let t0 = Instant::now();
    assert!(r.should_fetch("u", t0));
    assert!(!r.should_fetch("u", t0), "one request at a time");
    r.finished("u", false, t0);
    assert!(
        !r.should_fetch("u", t0 + Duration::from_secs(1)),
        "not in a tight loop"
    );
    assert!(
        r.should_fetch("u", t0 + Duration::from_secs(31)),
        "the old app never retried"
    );
    r.finished("u", false, t0);
    r.on_reload();
    assert!(r.should_fetch("u", t0), "reloading clears failures");
}

#[test]
fn reload_keeps_exact_position_instead_of_snapping_to_heading() {
    let old = parse("# Top\n\nfirst paragraph\n\nsecond paragraph\n\nthird paragraph\n");
    let at_third = old
        .blocks
        .iter()
        .position(|b| old.text_of(b.text).starts_with("third"))
        .unwrap() as u32;
    let pos = ScrollPos {
        block: at_third,
        offset: 37.5,
    };
    let same = parse("# Top\n\nfirst paragraph\n\nsecond paragraph\n\nthird paragraph\n");
    assert_eq!(same.remap_scroll(&old, pos), pos);

    let edited =
        parse("# Top\n\nnew intro\n\nfirst paragraph\n\nsecond paragraph\n\nthird paragraph\n");
    let moved = edited.remap_scroll(&old, pos);
    assert!(edited.block_text(moved.block).starts_with("third"));
    assert_eq!(moved.offset, 37.5);
    assert!(!matches!(
        edited.blocks[moved.block as usize].kind,
        BlockKind::Heading { .. }
    ));
}

// Autolink

#[test]
fn urls_do_not_truncate_at_delimiters() {
    for (src, text) in [
        ("http://a.b/_c_", "http://a.b/_c"),
        ("https://example.com/*x*", "https://example.com/*x"),
        ("https://x.y/~a~ z", "https://x.y/~a"),
        (
            "see https://x.y/a_b_c/d*e*f and more",
            "https://x.y/a_b_c/d*e*f",
        ),
        ("www.x.y/_a_b_", "www.x.y/_a_b"),
    ] {
        let d = parse(src);
        let got = links(&d);
        assert_eq!(got.len(), 1, "{src}");
        assert_eq!(got[0].0, text, "{src}");
        assert!(
            !d.spans.iter().any(|s| matches!(
                s.kind,
                SpanKind::Emphasis | SpanKind::Strike | SpanKind::Strong
            )),
            "{src}"
        );
        assert!(
            d.text
                .contains(src.split_whitespace().next().unwrap_or(src))
                || src.starts_with("see"),
            "{src}"
        );
    }
}

#[test]
fn real_emphasis_next_to_urls_survives() {
    let d = parse("*http://a.b/c* and [x](http://d.e/_f_)*g* **www.h.i**");
    let kinds: Vec<_> = d.spans.iter().map(|s| s.kind).collect();
    assert!(kinds.contains(&SpanKind::Emphasis) && kinds.contains(&SpanKind::Strong));
    let l = links(&d);
    assert_eq!(
        l[0],
        ("http://a.b/c".to_string(), "http://a.b/c".to_string())
    );
    assert_eq!(l[1].1, "http://d.e/_f_", "destinations come back unchanged");
    assert_eq!(l[2].1, "http://www.h.i");
}

#[test]
fn delimiters_inside_code_and_raw_html_are_untouched() {
    let d =
        parse("`http://a.b/_c_` <a href=\"http://d.e/*f*\">x</a>\n\n```\nhttp://g.h/~i~j\n```\n");
    assert!(d.text.contains("http://a.b/_c_"));
    assert!(d.text.contains("http://g.h/~i~j"));
    assert_eq!(links(&d), [("x".to_string(), "http://d.e/*f*".to_string())]);
}

#[test]
fn escaped_character_blocks_a_link() {
    assert!(links(&parse("<foo\\+@bar.example.com>")).is_empty());
    assert_eq!(
        links(&parse("http://a.b/\\_c")),
        [("http://a.b/".to_string(), "http://a.b/".to_string())]
    );
    // An escaped backslash is not an escape of what follows.
    assert_eq!(links(&parse("x\\\\foo@bar.example.com")).len(), 1);
}

#[test]
fn bare_links_are_kept_everywhere_github_keeps_them() {
    let l = links(&parse(
        "Go to https://example.com, www.example.org. Mail a.b+c@example.co.uk now.",
    ));
    assert_eq!(l[0].1, "https://example.com");
    assert_eq!(l[1].1, "http://www.example.org");
    assert_eq!(l[2].1, "mailto:a.b+c@example.co.uk");
    assert_eq!(
        links(&parse("mailto:foo@bar.baz")),
        [(
            "mailto:foo@bar.baz".to_string(),
            "mailto:foo@bar.baz".to_string()
        )]
    );
    assert_eq!(
        links(&parse("xmpp:foo@bar.baz/txt.")),
        [(
            "xmpp:foo@bar.baz/txt".to_string(),
            "xmpp:foo@bar.baz/txt".to_string()
        )]
    );
    assert!(
        links(&parse("[http://c.d](x)"))
            .iter()
            .all(|(_, dest)| dest == "x")
    );
    assert!(links(&parse("`http://a.b`")).is_empty());
    assert!(
        links(&parse("<div>http://a.b</div>")).is_empty(),
        "GitHub does not autolink inside HTML blocks"
    );
}

#[test]
fn turkish_paths_and_unicode_domains_link() {
    let l = links(&parse("https://örnek.com.tr/yol/ığüşöç_İ?a=ş"));
    assert_eq!(l[0].0, "https://örnek.com.tr/yol/ığüşöç_İ?a=ş");
}

// Nested strong

#[test]
fn nested_strong_collapses_in_the_model() {
    for src in [
        "****foo****",
        "**foo **bar** baz**",
        "__foo __bar__ baz__",
        "<b>a <strong>b</strong></b>",
        "**a <b>c</b>**",
    ] {
        let d = parse(src);
        assert_eq!(
            d.spans
                .iter()
                .filter(|s| s.kind == SpanKind::Strong)
                .count(),
            1,
            "{src}"
        );
    }
}

// HTML allow-list

#[test]
fn nothing_executable_survives() {
    let d = parse(concat!(
        "<a href=\"javascript:alert(1)\">x</a> <a href=\" java\nscript:alert(1)\">y</a> <a href=\"JaVaScRiPt:alert(2)\">z</a> ",
        "<a href=\"java&#10;script:alert(3)\">w</a>\n\n<img src=\"javascript:alert(4)\">\n\n<img src=x onerror=alert(5)>\n\n",
        "<script>alert(6)</script>\n\n<iframe src=\"https://evil\"></iframe>\n\n<div onclick=\"alert(7)\">t</div>\n\n",
        "foo <script>alert(8)</script> bar\n\n<style>p{color:red}</style>\n\n<object data=\"x\">alert(9)</object>\n"
    ));
    assert!(!d.text.contains("alert"), "{}", d.text);
    assert!(
        !d.strings.to_lowercase().contains("javascript"),
        "{}",
        d.strings
    );
    assert!(!d.strings.contains("onerror") && !d.strings.contains("onclick"));
    assert!(d.text.contains("foo") && d.text.contains("bar"));
}

#[test]
fn markdown_script_links_never_act() {
    let dir = Path::new("/doc");
    assert_eq!(
        classify_link("javascript:alert(1)", dir),
        LinkAction::Ignored
    );
    assert_eq!(classify_link("data:text/html,x", dir), LinkAction::Ignored);
    assert_eq!(classify_link("vbscript:x", dir), LinkAction::Ignored);
}

#[test]
fn unknown_tags_drop_and_keep_text() {
    let d = parse("<foo-bar baz=1>kept <blink>too</blink></foo-bar>\n");
    assert_eq!(d.text.trim(), "kept too");
    assert!(d.spans.is_empty());
}

#[test]
fn tagfilter_shows_unclosed_filtered_tags_as_text() {
    let d = parse(
        "<strong> <title> <style> <em>\n\n<blockquote>\n  <xmp> is disallowed.  <XMP> is also disallowed.\n</blockquote>\n",
    );
    assert!(d.text.contains("<title>") && d.text.contains("<style>"));
    assert!(
        d.text.contains("<xmp> is disallowed.") && d.text.contains("<XMP> is also disallowed.")
    );
    assert!(d.blocks.iter().any(|b| b.kind == BlockKind::BlockQuote));
}

#[test]
fn allow_listed_inline_tags_map_to_spans() {
    let d = parse(
        "<kbd>K</kbd> <sub>s</sub> <sup>p</sup> <u>u</u> <del>d</del> <s>s2</s> <mark>m</mark> <code>c</code> a<br>b <i>i</i> <b>b</b>\n",
    );
    let has = |k: SpanKind, t: &str| d.spans.iter().any(|s| s.kind == k && span_text(&d, s) == t);
    assert!(has(SpanKind::Kbd, "K") && has(SpanKind::Sub, "s") && has(SpanKind::Sup, "p"));
    assert!(
        has(SpanKind::Underline, "u") && has(SpanKind::Strike, "d") && has(SpanKind::Strike, "s2")
    );
    assert!(
        has(SpanKind::Mark, "m")
            && has(SpanKind::Code, "c")
            && has(SpanKind::Emphasis, "i")
            && has(SpanKind::Strong, "b")
    );
    assert!(d.text.contains("a\nb"));
}

#[test]
fn picture_keeps_theme_variants() {
    let d = parse(
        "<picture>\n <source media=\"(prefers-color-scheme: dark)\" srcset=\"dark.png 2x\">\n <source media=\"(prefers-color-scheme: light)\" srcset=\"light.png\">\n <img alt=\"Logo\" src=\"fallback.png\" width=\"200\" height=\"50%\">\n</picture>\n",
    );
    assert_eq!(d.images.len(), 1);
    let i = d.images[0];
    assert_eq!(
        (
            d.str(i.src),
            d.str(i.dark_src),
            d.str(i.light_src),
            d.str(i.alt)
        ),
        ("fallback.png", "dark.png", "light.png", "Logo")
    );
    assert_eq!(
        (i.width, i.height),
        (Some(Length::Px(200)), Some(Length::Percent(50)))
    );
}

#[test]
fn link_wrapped_in_html_around_markdown_image() {
    let d = parse("<a href=\"https://x.y\">\n\n![badge](b.svg)\n\n</a>\n\nafter\n");
    let para = d
        .blocks
        .iter()
        .position(|b| matches!(b.kind, BlockKind::Paragraph { .. }))
        .unwrap();
    let spans = d.blocks[para].spans;
    assert!(matches!(
        d.spans[spans.start as usize].kind,
        SpanKind::Link(_)
    ));
    assert_eq!(spans.len(), 2);
    assert_eq!(
        d.spans[spans.start as usize + 1].parent,
        spans.start,
        "the image sits inside the link"
    );
    let last = d
        .blocks
        .iter()
        .rposition(|b| matches!(b.kind, BlockKind::Paragraph { .. }))
        .unwrap();
    assert_eq!(
        d.blocks[last].spans.len(),
        0,
        "the closer ended the wrapper"
    );
    assert_eq!(d.blocks.len(), 2, "no empty block is left behind");
}

#[test]
fn centered_div_contains_following_markdown() {
    let d = parse("<div align=\"center\">\n\n# Title\n\n</div>\n\nafter\n");
    assert_eq!(
        d.blocks[0].kind,
        BlockKind::Div {
            align: Align::Center
        }
    );
    assert_eq!(d.blocks[1].parent, 0);
    assert_eq!(d.headings.len(), 1);
    assert_eq!(d.blocks[0].end, 2);
    assert_eq!(d.blocks[2].parent, NONE);
}

#[test]
fn html_tables_and_lists_build_structure() {
    let d = parse(
        "<table><tr><th colspan=2 align=right>h</th></tr><tr><td>a<td>b</table>\n\n<ul><li>x<li>y</ul>\n\n<ol start=3><li>z</ol>\n",
    );
    let cells = d
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .count();
    assert_eq!(cells, 3);
    assert!(d.blocks.iter().any(|b| b.kind
        == BlockKind::TableCell {
            align: Align::Right,
            header: true,
            colspan: 2,
            rowspan: 1
        }));
    assert_eq!(
        d.blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::TableRow { .. }))
            .count(),
        2
    );
    assert_eq!(
        d.blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::ListItem { .. }))
            .count(),
        3
    );
    assert!(
        d.blocks
            .iter()
            .any(|b| b.kind == BlockKind::List { start: Some(3) })
    );
}

#[test]
fn html_headings_join_the_outline() {
    let d =
        parse("<h2 align=\"center\" id=\"custom\">Hello <em>there</em></h2>\n\n## Hello there\n");
    assert_eq!(d.headings.len(), 2);
    assert_eq!(d.str(d.headings[0].slug), "hello-there");
    assert_eq!(d.str(d.headings[1].slug), "hello-there-1");
    assert_eq!(d.resolve_fragment("custom"), Some(0));
    assert_eq!(d.resolve_fragment("user-content-hello-there"), Some(0));
    assert_eq!(
        d.resolve_fragment("HELLO-THERE-1"),
        Some(d.headings[1].block)
    );
}

#[test]
fn anchor_alone_on_a_line_leaves_no_gap_and_targets_the_next_block() {
    let d = parse("intro\n\n<a id=\"x\"></a>\n\n## H\n\n<a name=\"y\">here</a>\n");
    assert_eq!(d.blocks.len(), 3);
    assert!(matches!(
        d.blocks[d.resolve_fragment("x").unwrap() as usize].kind,
        BlockKind::Heading { .. }
    ));
    assert_eq!(
        d.block_text(d.resolve_fragment("y").unwrap()).trim(),
        "here"
    );
    assert_eq!(
        parse("end\n\n<a id=\"z\"></a>\n").resolve_fragment("z"),
        Some(0)
    );
    assert!(parse("<a id=\"only\"></a>\n").anchors.is_empty());
}

#[test]
fn whitespace_collapses_once_outside_code() {
    let d = parse("a  b\tc <!-- x --> d&#10;&#10;e&nbsp; f\n\n`  x  y  `\n\n```\na  b\n```\n");
    assert!(d.text.starts_with("a b c d e\u{a0} f\n"), "{:?}", d.text);
    assert!(
        d.text.contains(" x  y "),
        "code spans keep inner spacing: {:?}",
        d.text
    );
    assert!(d.text.contains("a  b\n"), "code blocks keep their spacing");
}

#[test]
fn spaces_around_images_survive_collapsing() {
    assert_eq!(parse("a ![i](u) b\n").text, "a  b\n");
    assert_eq!(parse("<div><img src=\"u\"> b</div>\n").text, " b\n");
    assert_eq!(parse("- [x] task\n").text, "task\n");
}

#[test]
fn empty_html_containers_leave_nothing_behind() {
    assert!(parse("<p>\n\n<p></p>\n").blocks.is_empty());
    assert!(parse("<p><b>\n").blocks.is_empty());
    let d = parse("<p><b>\n\nbold\n\n</b>\n\nplain\n");
    assert_eq!(d.blocks.len(), 2);
    assert_eq!(
        d.spans
            .iter()
            .filter(|s| s.kind == SpanKind::Strong)
            .count(),
        1
    );
}

// Slugs and outline

#[test]
fn slugs_match_github() {
    let d = parse(
        "# Hello, World!\n\n## `code` and *em*\n\n## C++ & Rust\n\n# Hello, World!\n\n# foo\n\n# foo\n\n# foo-1\n\n## 🚀 Launch\n\n## Türkçe Başlık İ\n\n## snake_case-kept\n",
    );
    let got: Vec<&str> = d.headings.iter().map(|h| d.str(h.slug)).collect();
    assert_eq!(
        &got[..7],
        [
            "hello-world",
            "code-and-em",
            "c--rust",
            "hello-world-1",
            "foo",
            "foo-1",
            "foo-1-1"
        ]
    );
    assert_eq!(got[7], "-launch");
    assert_eq!(got[8], "türkçe-başlık-i\u{307}");
    assert_eq!(got[9], "snake_case-kept");
}

#[test]
fn outline_nesting_follows_levels() {
    let d = parse("# A\n\n### B\n\n## C\n\n# D\n");
    let o = d.outline();
    assert_eq!(
        o.iter().map(|i| i.parent).collect::<Vec<_>>(),
        [NONE, 0, 0, NONE]
    );
    assert_eq!(d.str(d.headings[1].title), "B");
}

// Find

fn count(text: &str, q: &str) -> usize {
    find_in_text(text, q, FIND_CAP).hits.len()
}

#[test]
fn find_turkish_i_forms() {
    assert_eq!(count("İstanbul", "istanbul"), 1, "i finds İ");
    assert_eq!(count("istanbul", "İSTANBUL"), 1, "İ finds i");
    assert_eq!(count("ISPARTA", "ısparta"), 1, "ı finds I");
    assert_eq!(count("ısparta", "ISPARTA"), 1, "I finds ı");
    assert_eq!(count("Install", "install"), 1, "English I/i keeps working");
    assert_eq!(count("install", "INSTALL"), 1);
    assert_eq!(count("sınır", "sinir"), 0, "ı and i are different letters");
    assert_eq!(count("sinir", "sınır"), 0);
    assert_eq!(count("İ", "I"), 0, "İ and I are different letters");
    assert_eq!(count("I", "İ"), 0);
    assert_eq!(count("ŞİŞLİ ve şişli", "şişli"), 2);
    assert_eq!(count("IŞIK ışık", "ışık"), 2);
}

#[test]
fn find_ranges_are_valid_after_length_changing_case() {
    let text = "aİb KELVIN\u{212A} ſtraße ΣΑΣ σας";
    let r = find_in_text(text, "i", 100);
    assert!(
        r.hits
            .iter()
            .all(|h| text.is_char_boundary(h.start as usize)
                && text.is_char_boundary(h.end as usize))
    );
    assert_eq!(&text[r.hits[0].usize()], "İ");
    assert_eq!(count(text, "k"), 2);
    assert_eq!(count(text, "s"), 1, "the long s; Greek sigma is not an s");
    assert_eq!(
        count(text, "σας"),
        2,
        "final sigma folds with both other forms"
    );
}

#[test]
fn find_cap_is_ten_thousand() {
    let text = "ab ".repeat(FIND_CAP + 5);
    let r = find_in_text(&text, "AB", FIND_CAP);
    assert_eq!(r.hits.len(), 10_000);
    assert!(r.capped);
    let exact = "ab ".repeat(FIND_CAP);
    assert!(!find_in_text(&exact, "ab", FIND_CAP).capped);
    assert_eq!(
        strings::find_label(false, 0, 10_000, true),
        "10000+ matches"
    );
}

#[test]
fn find_labels_match_the_old_app() {
    assert_eq!(strings::find_label(true, 0, 0, false), "");
    assert_eq!(strings::find_label(false, 0, 0, false), "No matches");
    assert_eq!(strings::find_label(false, 0, 2, false), "2 matches");
    assert_eq!(strings::find_label(false, 1, 2, false), "1 of 2");
    assert_eq!(
        strings::find_tooltip(2, false),
        "2 matches in this document"
    );
    assert_eq!(
        strings::find_tooltip(0, false),
        "No matches in this document"
    );
}

#[test]
fn find_stays_inside_blocks_and_skips_drawn_source() {
    let d = parse(
        "foo\n\nbar\n\nwrapped\nline\n\n$$\nneedle\n$$\n\ninline $needle$ math\n\n```mermaid\nneedle\n```\n\n```rust\nneedle\n```\n",
    );
    assert_eq!(find(&d, "foobar").hits.len(), 0);
    assert_eq!(find(&d, "foo bar").hits.len(), 0);
    assert_eq!(
        find(&d, "wrapped line").hits.len(),
        1,
        "a soft break reads as a space"
    );
    assert_eq!(
        find(&d, "needle").hits.len(),
        1,
        "only the code block's copy is visible"
    );
    let over = parse(&format!(
        "```mermaid\nneedle {}\n```\n",
        "x".repeat(MAX_DIAGRAM_BYTES)
    ));
    assert_eq!(
        find(&over, "needle").hits.len(),
        1,
        "oversize source is on screen, so it is searchable"
    );
}

#[test]
fn find_empty_query_finds_nothing() {
    assert_eq!(find_in_text("abc", "", 10).hits.len(), 0);
    assert_eq!(find_in_text("", "abc", 10).hits.len(), 0);
}

// Limits, engine jobs

#[test]
fn oversize_blocks_keep_source_and_reason() {
    let eq = "x".repeat(MAX_EQUATION_BYTES + 1);
    let d = parse(&format!("$$\n{eq}\n$$\n\n${eq}$\n\n```math\n{eq}\n```\n"));
    let reasons = d
        .warnings
        .iter()
        .filter(|w| **w == Warning::Oversize(Oversize::Equation))
        .count();
    assert_eq!(reasons, 3);
    assert_eq!(
        d.warnings[0].to_string(),
        "Equation exceeds the 4 KiB limit"
    );
    assert_eq!(d.text.matches(eq.as_str()).count(), 3);
    assert!(d.engine_jobs().is_empty());
    let small = parse(&format!(
        "```mermaid\n{}\n```\n",
        "x".repeat(MAX_DIAGRAM_BYTES - 1)
    ));
    assert!(matches!(
        small.blocks[0].kind,
        BlockKind::Diagram { oversize: None, .. }
    ));
}

#[test]
fn invalid_utf8_file_loads_with_a_warning() {
    let dir = temp_dir("invalid");
    let path = dir.join("bad.md");
    std::fs::write(&path, b"# ok\n\nbad \xFF byte\n").unwrap();
    let (d, stamp) = files::load_document(&path).unwrap();
    assert!(d.text.contains('\u{FFFD}'));
    assert_eq!(d.warnings, [Warning::InvalidUtf8]);
    assert!(matches!(stamp, FileStamp::Present { .. }));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn file_limit_is_enforced() {
    let dir = temp_dir("limit");
    let path = dir.join("big.md");
    std::fs::write(&path, vec![b'a'; MAX_FILE_BYTES + 1]).unwrap();
    assert_eq!(files::load_document(&path).err(), Some(LoadError::TooLarge));
    assert_eq!(
        LoadError::TooLarge.to_string(),
        "File exceeds the 16 MiB viewer limit"
    );
    let d = parse(&"a".repeat(MAX_FILE_BYTES + 1));
    assert_eq!(d.warnings, [Warning::FileTooLarge]);
    assert!(d.blocks.is_empty());
    std::fs::write(&path, vec![b'a'; MAX_FILE_BYTES]).unwrap();
    assert!(files::load_document(&path).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn engine_jobs_record_each_kind() {
    let d = parse(
        "```rust\nfn a() {}\n```\n\n```mermaid\ngraph TD\n```\n\n```math\nx\n```\n\n```\nplain\n```\n\n$a$\n\n$$b$$\n",
    );
    let jobs = d.engine_jobs();
    let names: Vec<&str> = jobs
        .iter()
        .map(|j| match j {
            EngineJob::Highlight { .. } => "hl",
            EngineJob::Diagram { .. } => "diagram",
            EngineJob::MathBlock { .. } => "mathblock",
            EngineJob::MathInline { .. } => "mathinline",
        })
        .collect();
    assert_eq!(
        names,
        ["hl", "diagram", "mathblock", "mathinline", "mathblock"]
    );
    if let EngineJob::Highlight { lang, source, .. } = jobs[0] {
        assert_eq!(d.str(lang), "rust");
        assert_eq!(d.text_of(source), "fn a() {}\n");
    }
}

// Encoding

#[test]
fn bom_and_invalid_utf8_do_not_panic() {
    let d = mdview_core::parse_bytes(b"\xEF\xBB\xBF# Title\n");
    assert_eq!(d.str(d.headings[0].title), "Title");
    assert!(d.warnings.is_empty());
    let d = mdview_core::parse_bytes(b"# Ti\xFFtle \xC3\n\nok\xF0\x9F\n");
    assert_eq!(d.str(d.headings[0].title), "Ti\u{FFFD}tle \u{FFFD}");
    assert_eq!(d.warnings, [Warning::InvalidUtf8]);
    let d = parse("\u{FEFF}# Title\n");
    assert_eq!(d.headings.len(), 1);
    assert!(parse("").blocks.is_empty());
    assert!(mdview_core::parse_bytes(b"").blocks.is_empty());
    let d = parse("a\0b\n");
    assert!(d.text.contains('\u{FFFD}'));
}

#[test]
fn utf16_files_decode() {
    let mut le = vec![0xFF, 0xFE];
    le.extend("# Başlık\n".encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(mdview_core::parse_bytes(&le).headings.len(), 1);
    let mut be = vec![0xFE, 0xFF];
    be.extend("# Başlık\n".encode_utf16().flat_map(u16::to_be_bytes));
    let d = mdview_core::parse_bytes(&be);
    assert_eq!(d.str(d.headings[0].title), "Başlık");
    assert_eq!(
        mdview_core::parse_bytes(&[0xFF, 0xFE, 0x41]).warnings,
        [Warning::InvalidUtf8]
    );
}

#[test]
fn link_destinations_with_delimiters_still_parse() {
    let d = parse(
        "[x](https://a.b/c_d_e) [y](<https://a.b/f*g>) [z][r] ![i](https://a.b/h_i_j.png \"t_u_v\") (https://k.l/m_n_o)\n\n[r]: https://a.b/p_q_r \"T_i_t\"\n",
    );
    let dests: Vec<&str> = d.links.iter().map(|l| d.str(l.dest)).collect();
    assert_eq!(
        dests,
        [
            "https://a.b/c_d_e",
            "https://a.b/f*g",
            "https://a.b/p_q_r",
            "https://k.l/m_n_o"
        ]
    );
    assert_eq!(d.str(d.links[2].title), "T_i_t");
    assert_eq!(d.str(d.images[0].src), "https://a.b/h_i_j.png");
    assert_eq!(d.str(d.images[0].title), "t_u_v");
    assert_eq!(d.text.trim(), "x y z  (https://k.l/m_n_o)");
}

#[test]
fn private_use_characters_in_a_document_are_preserved() {
    let d = parse("a\u{E000}b http://x.y/c_d\n");
    assert!(d.text.contains('\u{E000}'));
    assert_eq!(links(&d)[0].0, "http://x.y/c_d");
    let d = parse("a\u{E001} http://x.y/_c_d_ tail\n");
    assert!(d.text.contains('\u{E001}'));
}

// Model details

#[test]
fn alerts_and_footnotes() {
    let d = parse(
        "> [!WARNING]\n> Careful\n\n> [!FOO]\n> plain\n\nClaim[^b] and[^a] again[^b].\n\n[^a]: First.\n[^b]: Second.\n",
    );
    assert_eq!(d.blocks[0].kind, BlockKind::Alert(AlertKind::Warning));
    assert!(!d.block_text(0).contains("[!WARNING]"));
    assert!(d.blocks.iter().any(|b| b.kind == BlockKind::BlockQuote));
    let numbers: Vec<(&str, u32)> = d
        .footnotes
        .iter()
        .map(|f| (d.str(f.label), f.number))
        .collect();
    assert!(numbers.contains(&("b", 1)) && numbers.contains(&("a", 2)));
    assert_eq!(
        d.spans
            .iter()
            .filter(|s| matches!(s.kind, SpanKind::FootnoteRef(_)))
            .count(),
        3
    );
    let a = d.footnotes.iter().find(|f| d.str(f.label) == "a").unwrap();
    assert!(d.block_text(a.block).contains("First."));
}

#[test]
fn task_items_are_marked_tight_and_loose() {
    let d = parse("- [x] tight\n- [ ] also\n\n- plain\n\n1. [X] loose\n\n   more\n");
    let tasks = d
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::ListItem { task: true })
        .count();
    assert_eq!(tasks, 3);
    assert_eq!(
        d.spans
            .iter()
            .filter(|s| matches!(s.kind, SpanKind::Checkbox { .. }))
            .count(),
        3
    );
}

#[test]
fn block_tree_invariants_on_the_showcase() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/showcase.md"),
    )
    .unwrap();
    let d = parse(&src);
    common_invariants(&d);
    assert_eq!(d.str(d.headings[0].title), "Native Markdown Reader");
    let dup: Vec<&str> = d
        .headings
        .iter()
        .map(|h| d.str(h.slug))
        .filter(|s| s.starts_with("repeated-heading"))
        .collect();
    assert_eq!(dup, ["repeated-heading", "repeated-heading-1"]);
    assert!(
        d.engine_jobs()
            .iter()
            .filter(|j| matches!(j, EngineJob::Diagram { .. }))
            .count()
            >= 8
    );
}

fn common_invariants(d: &Document) {
    for (i, b) in d.blocks.iter().enumerate() {
        assert!(b.end as usize > i && b.end as usize <= d.blocks.len());
        assert!(
            d.text.is_char_boundary(b.text.start as usize)
                && d.text.is_char_boundary(b.text.end as usize)
        );
    }
}

#[test]
fn details_keys_survive_edits_elsewhere() {
    let a = parse(
        "<details><summary>One</summary>x</details>\n\n<details><summary>Two</summary>y</details>\n",
    );
    let b = parse(
        "<details><summary>New</summary>z</details>\n\n<details><summary>One</summary>x</details>\n\n<details><summary>Two</summary>y</details>\n",
    );
    let keys = |d: &Document| d.details_keys().iter().map(|&(_, k)| k).collect::<Vec<_>>();
    assert_eq!(keys(&a)[0], keys(&b)[1]);
    assert_eq!(keys(&a)[1], keys(&b)[2]);
    let same = parse(
        "<details><summary>S</summary>1</details>\n\n<details><summary>S</summary>2</details>\n",
    );
    let k = keys(&same);
    assert_ne!(k[0], k[1]);
}

// Link classification

#[test]
fn link_rules_match_the_old_app() {
    let dir = Path::new("/docs/sub");
    assert_eq!(
        classify_link("#second-page", dir),
        LinkAction::Fragment("second-page".into())
    );
    assert_eq!(classify_link("#", dir), LinkAction::Fragment(String::new()));
    assert_eq!(
        classify_link("linked.md#second-page", dir),
        LinkAction::OpenMarkdown {
            path: "/docs/sub/linked.md".into(),
            fragment: Some("second-page".into())
        }
    );
    for ext in ["a.MD", "a.markdown", "a.mdown", "a.MKD"] {
        assert!(
            matches!(classify_link(ext, dir), LinkAction::OpenMarkdown { .. }),
            "{ext}"
        );
    }
    assert_eq!(
        classify_link("../x.md", dir),
        LinkAction::OpenMarkdown {
            path: "/docs/x.md".into(),
            fragment: None
        }
    );
    assert_eq!(
        classify_link("my%20file.md?raw=1", dir),
        LinkAction::OpenMarkdown {
            path: "/docs/sub/my file.md".into(),
            fragment: None
        }
    );
    assert_eq!(
        classify_link("a.pdf", dir),
        LinkAction::OtherLocalFile("/docs/sub/a.pdf".into())
    );
    assert_eq!(
        classify_link("/abs/b.md", dir),
        LinkAction::OpenMarkdown {
            path: "/abs/b.md".into(),
            fragment: None
        }
    );
    assert_eq!(
        classify_link("file:///abs/b%20c.md#h", dir),
        LinkAction::OpenMarkdown {
            path: "/abs/b c.md".into(),
            fragment: Some("h".into())
        }
    );
    assert_eq!(
        classify_link("https://x.y/a.md", dir),
        LinkAction::External("https://x.y/a.md".into())
    );
    assert_eq!(
        classify_link("HTTP://x.y", dir),
        LinkAction::External("HTTP://x.y".into())
    );
    assert_eq!(
        classify_link("mailto:a@b.c", dir),
        LinkAction::External("mailto:a@b.c".into())
    );
    assert_eq!(classify_link("ftp://x.y", dir), LinkAction::Ignored);
    assert_eq!(classify_link("details:0", dir), LinkAction::Ignored);
    assert_eq!(
        classify_link("file://remote/share/a.md", dir),
        LinkAction::Ignored
    );
}

#[test]
fn image_sources_classify() {
    let dir = Path::new("/docs");
    assert_eq!(
        classify_image("assets/badge.svg", dir),
        ImageSource::Local("/docs/assets/badge.svg".into())
    );
    assert_eq!(
        classify_image("https://x.y/a.png", dir),
        ImageSource::Remote("https://x.y/a.png".into())
    );
    assert_eq!(
        classify_image("data:image/png;base64,AAAA", dir),
        ImageSource::InlineData
    );
    assert_eq!(
        classify_image("data:image/svg+xml,<svg>", dir),
        ImageSource::Unsupported
    );
    assert_eq!(
        classify_image("ftp://x/y.png", dir),
        ImageSource::Unsupported
    );
}

// Session and recents

#[test]
fn recents_cap_order_and_dedupe() {
    let mut r = Recents::default();
    for i in 0..20 {
        r.add(format!("/f{i}.md"));
    }
    assert_eq!(r.paths().len(), MAX_RECENT);
    assert_eq!(r.paths()[0], Path::new("/f19.md"));
    r.add("/f10.md");
    assert_eq!(r.paths()[0], Path::new("/f10.md"));
    assert_eq!(r.paths().len(), MAX_RECENT);
    r.remove(Path::new("/f10.md"));
    assert_eq!(r.paths()[0], Path::new("/f19.md"));
    r.clear();
    assert!(r.is_empty());
}

#[test]
fn recents_mark_missing_files() {
    let dir = temp_dir("recent");
    let real = dir.join("real.md");
    std::fs::write(&real, "x").unwrap();
    let mut r = Recents::default();
    r.add(dir.join("gone.md"));
    r.add(&real);
    let e = r.entries();
    assert_eq!((e[0].label.as_str(), e[0].missing), ("real.md", false));
    assert_eq!(
        (e[1].label.as_str(), e[1].missing),
        ("gone.md (missing)", true)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn session_round_trips_and_tolerates_damage() {
    let dir = temp_dir("session");
    let path = dir.join("nested/settings.ini");
    let mut s = Session {
        tabs: vec!["/a b/Türkçe test.md".into(), "/x%y\n.md".into()],
        active: 1,
        theme: Theme::Dark,
        zoom: 150,
        ..Session::default()
    };
    s.recent.add("/r2.md");
    s.recent.add("/r1.md");
    s.folder = Some("/folder".into());
    s.geometry = "AdnQywAD=\n%x".into();
    s.save(&path).unwrap();
    assert_eq!(Session::load(&path), s);
    assert_eq!(s.recent.paths()[0], Path::new("/r1.md"));

    assert_eq!(Session::load(&dir.join("absent.ini")), Session::default());
    let damaged = Session::parse("garbage\nzoom=9999\ntheme=7\nactive=44\ntab=/a.md\n=\nzoom=\n");
    assert_eq!(damaged.zoom, ZOOM_DEFAULT);
    assert_eq!(damaged.theme, Theme::System);
    assert_eq!(damaged.active, 0);
    assert_eq!(Session::parse("zoom=9999\n").zoom, ZOOM_MAX);
    assert_eq!(Session::parse("zoom=1\n").zoom, ZOOM_MIN);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn session_keeps_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt;
    let p = PathBuf::from(std::ffi::OsString::from_vec(b"/a\xFFb.md".to_vec()));
    let s = Session {
        tabs: vec![p.clone()],
        ..Session::default()
    };
    assert_eq!(Session::parse(&s.serialize()).tabs, [p]);
}

#[test]
fn zoom_range_and_step() {
    assert_eq!(zoom_in(100), 110);
    assert_eq!(zoom_out(100), 90);
    assert_eq!(zoom_in(250), 250);
    assert_eq!(zoom_out(60), 60);
}

#[test]
fn stamps_and_paths() {
    let dir = temp_dir("stamp");
    let p = dir.join("a.md");
    assert_eq!(files::stamp(&p), FileStamp::Missing);
    std::fs::write(&p, "x").unwrap();
    assert!(matches!(
        files::stamp(&p),
        FileStamp::Present { size: 1, .. }
    ));
    assert!(files::canonical_or_absolute(&p).is_absolute());
    assert!(files::canonical_or_absolute(&dir.join("gone.md")).is_absolute());
    assert!(matches!(
        files::load_document(&dir.join("gone.md")),
        Err(LoadError::Io(_))
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

// Command table

#[test]
fn command_table_matches_the_inventory() {
    let keys = |id| commands::command(id).shortcuts.to_vec();
    assert_eq!(keys(CommandId::OpenFiles), [Shortcut::Keys("Ctrl+O")]);
    assert_eq!(
        keys(CommandId::OpenFolder),
        [Shortcut::Keys("Ctrl+Shift+O")]
    );
    assert_eq!(keys(CommandId::Refresh), [Shortcut::Keys("F5")]);
    assert_eq!(
        keys(CommandId::CloseTab),
        [Shortcut::Keys("Ctrl+W"), Shortcut::Keys("Ctrl+F4")]
    );
    assert_eq!(
        keys(CommandId::CloseAllTabs),
        [Shortcut::Keys("Ctrl+Shift+W")]
    );
    assert_eq!(keys(CommandId::Find), [Shortcut::Keys("Ctrl+F")]);
    assert_eq!(keys(CommandId::FindNext), [Shortcut::Keys("F3")]);
    assert_eq!(keys(CommandId::FindPrevious), [Shortcut::Keys("Shift+F3")]);
    assert_eq!(keys(CommandId::ZoomReset), [Shortcut::Keys("Ctrl+0")]);
    assert_eq!(
        keys(CommandId::ToggleFiles),
        [Shortcut::Keys("Ctrl+Shift+1")]
    );
    assert_eq!(
        keys(CommandId::ToggleOutline),
        [Shortcut::Keys("Ctrl+Shift+2")]
    );
    assert_eq!(keys(CommandId::FocusMode), [Shortcut::Keys("Ctrl+Shift+F")]);
    assert_eq!(commands::menu_items(Menu::File).count(), 9);
    assert_eq!(commands::menu_items(Menu::Edit).count(), 4);
    assert_eq!(commands::menu_items(Menu::View).count(), 9);
    assert_eq!(commands::menu_items(Menu::Help).count(), 2);
    assert_eq!(commands::menu_items(Menu::TabContext).count(), 4);
    assert!(commands::menu_items(Menu::File).any(|c| c.label == "Clear recent files"));
    let toolbar: Vec<_> = commands::COMMANDS
        .iter()
        .filter(|c| c.on_toolbar)
        .map(|c| c.id)
        .collect();
    assert_eq!(
        toolbar,
        [
            CommandId::OpenFiles,
            CommandId::OpenFolder,
            CommandId::Refresh,
            CommandId::FindNext,
            CommandId::FindPrevious
        ]
    );
}

#[test]
fn shortcuts_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for c in commands::COMMANDS {
        for s in c.shortcuts {
            if let Shortcut::Keys(k) = s {
                assert!(seen.insert(*k), "{k} bound twice");
            }
        }
    }
    let help = commands::shortcut_help();
    assert!(
        help.iter()
            .any(|(k, l)| k == "Ctrl+W, Ctrl+F4" && *l == "Close tab")
    );
    assert_eq!(
        strings::window_title(Some("a.md")),
        "a.md — Markdown Viewer"
    );
    assert_eq!(strings::window_title(None), "Markdown Viewer");
}

// Review findings: math

fn math_spans(d: &Document) -> Vec<String> {
    d.spans
        .iter()
        .filter(|s| matches!(s.kind, SpanKind::Math { .. }))
        .map(|s| span_text(d, s))
        .collect()
}

#[test]
fn math_needs_clear_neighbours_as_on_github() {
    let d = parse(
        "cp $SRC/$FILE $DEST and $HOME/$USER then ${A}$B; price $5/$10, $1M-$2M, 5$-6$ and $d$e",
    );
    assert!(math_spans(&d).is_empty(), "{:?}", math_spans(&d));
    assert_eq!(
        d.block_text(0).trim_end(),
        "cp $SRC/$FILE $DEST and $HOME/$USER then ${A}$B; price $5/$10, $1M-$2M, 5$-6$ and $d$e"
    );

    let d = parse("echo $FOO$BAR x. $a$b. a$b$.");
    assert!(math_spans(&d).is_empty(), "{:?}", math_spans(&d));
    assert_eq!(d.block_text(0).trim_end(), "echo $FOO$BAR x. $a$b. a$b$.");

    // Every expected value here is the output of `gh api markdown -f mode=gfm -f context=cli/cli`.
    for (src, math) in [
        ("A1 $SRC/$FILE and $x$ end", vec!["x"]),
        ("A2 $x$y$ z", vec![]),
        ("A3 a$b$ c$d$ e", vec![]),
        ("A4 $a$b$c$", vec![]),
        ("A5 x$ $y$ w", vec!["y"]),
        ("A6 $a$_b$ and $c$é d", vec!["c"]),
        ("A7 é$a$ and _$b$_ and $c$_ x", vec![]),
        ("A8 $5 and $6$ w", vec!["6"]),
        (
            "A9 $p$-$q$ and $p$/$q$, ($r$)/($s$)",
            vec!["p", "p", "r", "s"],
        ),
        ("A10 a$$x$$b and $$y$$c", vec![]),
        ("A11 price$5 and $6 and $7", vec![]),
        ("A13 $foo$bar baz$", vec![]),
        ("A14 a$b $c$", vec!["c"]),
        ("S2 **$x$**", vec!["x"]),
        ("S4 ~~$x$~~", vec!["x"]),
        ("S5 [$x$](u)", vec![]),
        ("S6 ($x$) and [$y$] and {$z$}", vec!["x"]),
        ("S8 $p$ - $q$", vec!["p", "q"]),
        ("S11 $a$$b$", vec!["a"]),
        ("S12 `c`$x$", vec!["x"]),
        ("S13 $x$`c`", vec!["x"]),
        ("S14 <b>$x$</b>", vec![]),
        ("T5 **a**$b$", vec!["b"]),
        ("T7 \\($x$\\)", vec!["x"]),
        ("T8 \"$x$\"", vec![]),
        ("T9 ***$x$***", vec![]),
        ("T10 _a_$x$", vec!["x"]),
        ("T11 <b>a</b> $x$ <br>$y$", vec!["x", "y"]),
        ("T12 （$x$） 　$y$", vec![]),
        ("T14 a\u{a0}$x$", vec![]),
        ("T16 $a$\u{a0}b", vec!["a"]),
        ("T17 $$x$$ y", vec!["x"]),
        ("T19 ($$x$$) and **$$y$$**", vec!["x", "y"]),
        ("|h|\n|-|\n|$a$|", vec!["a"]),
        (">$z$", vec!["z"]),
        ("- x\n  $y$", vec!["y"]),
    ] {
        assert_eq!(math_spans(&parse(src)), math, "{src}");
    }
}

#[test]
fn rejected_math_leaves_markdown_between_its_delimiters_alone() {
    let d = parse("$x$ y $y$. Also \\$a$ b$. **$a$** ($b$) a $c$-d $d$e");
    assert_eq!(math_spans(&d), ["x", "y", "a", "b", "c"]);
    let strong = d.spans.iter().find(|s| s.kind == SpanKind::Strong).unwrap();
    assert_eq!(span_text(&d, strong), "a");
    assert!(d.block_text(0).contains("$d$e"), "{:?}", d.block_text(0));

    let d = parse("cost $5 *and* a$b **bold** c$ more");
    assert!(math_spans(&d).is_empty());
    assert!(d.spans.iter().any(|s| s.kind == SpanKind::Emphasis));
    assert!(d.spans.iter().any(|s| s.kind == SpanKind::Strong));
    assert_eq!(d.block_text(0).trim_end(), "cost $5 and a$b bold c$ more");
}

#[test]
fn hidden_dollars_come_back_everywhere() {
    let d = parse(
        "a$b$c [l$m$n](u$v$w \"t$i$e\") ![p$q$r](s$t$u) `x$y$z` <kbd>f$g$h</kbd>\n\n# k$l$m\n",
    );
    let text = &d.text;
    for part in ["a$b$c", "l$m$n", "x$y$z", "f$g$h", "k$l$m"] {
        assert!(text.contains(part), "{text:?}");
    }
    assert!(d.strings.contains("u$v$w") && d.strings.contains("t$i$e"));
    assert!(d.strings.contains("p$q$r") && d.strings.contains("s$t$u"));
    assert!(!d.text.contains('\u{E003}') && !d.strings.contains('\u{E003}'));

    // A file already using the stand-in cannot hide anything, and shows bad math literally instead.
    let d = parse("\u{E003} and $a$b here");
    assert!(math_spans(&d).is_empty());
    assert_eq!(d.block_text(0).trim_end(), "\u{E003} and $a$b here");
}

#[test]
fn a_long_chain_of_rejected_math_settles_quickly() {
    let src = "$a/$b ".repeat(40_000);
    let t = Instant::now();
    let d = parse(&src);
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    assert!(math_spans(&d).is_empty());
    assert!(d.text.contains("$a/$b"));
}

// Review findings: block HTML inside inline content

fn tree(d: &Document) -> String {
    fn name(k: BlockKind) -> &'static str {
        match k {
            BlockKind::Paragraph { .. } => "P",
            BlockKind::Plain => "T",
            BlockKind::Heading { .. } => "H",
            BlockKind::BlockQuote => "Q",
            BlockKind::Alert(_) => "A",
            BlockKind::List { .. } => "L",
            BlockKind::ListItem { .. } => "I",
            BlockKind::CodeBlock { .. } => "C",
            BlockKind::Diagram { .. } => "G",
            BlockKind::Math { .. } => "M",
            BlockKind::ThematicBreak => "R",
            BlockKind::Table => "Tb",
            BlockKind::TableRow { .. } => "Tr",
            BlockKind::TableCell { .. } => "Td",
            BlockKind::Details { .. } => "D",
            BlockKind::Summary => "S",
            BlockKind::FootnoteDefinition { .. } => "F",
            BlockKind::Div { .. } => "V",
        }
    }
    fn walk(d: &Document, parent: u32, out: &mut Vec<String>) {
        for c in d.children(parent) {
            let b = d.blocks[c as usize];
            let has_children = b.end > c + 1;
            if has_children {
                let mut inner = Vec::new();
                walk(d, c, &mut inner);
                out.push(format!("{}[{}]", name(b.kind), inner.join(" ")));
            } else if d.block_text(c).trim().is_empty() {
                out.push(name(b.kind).to_string());
            } else {
                out.push(format!("{}({})", name(b.kind), d.block_text(c).trim_end()));
            }
        }
    }
    let mut out = Vec::new();
    walk(d, NONE, &mut out);
    out.join(" ")
}

/// The structure of one table cell in the body row.
fn cell_tree(row: &str) -> String {
    let whole = tree(&parse(&format!("| h |\n|---|\n| {row} |\n")));
    let body = whole.strip_prefix("Tb[Tr[Td(h)] Tr[").unwrap();
    body.strip_suffix("]]").unwrap().to_string()
}

#[test]
fn block_html_in_a_table_cell_keeps_its_structure() {
    assert_eq!(
        cell_tree("<ul><li>one</li><li>two</li></ul>"),
        "Td[L[I[T(one)] I[T(two)]]]"
    );
    assert_eq!(cell_tree("<ol><li>x</li></ol>"), "Td[L[I[T(x)]]]");
    assert_eq!(cell_tree("<p>p1</p><p>p2</p>"), "Td[P(p1) P(p2)]");
    assert_eq!(
        cell_tree("<details><summary>S</summary>body</details>"),
        "Td[D[S[T(S)] T(body)]]"
    );
    assert_eq!(
        cell_tree("<blockquote>q</blockquote><pre>c  d</pre>"),
        "Td[Q[T(q)] C(c  d)]"
    );
    assert_eq!(
        cell_tree("<table><tr><td>in</td></tr></table>"),
        "Td[Tb[Tr[Td[T(in)]]]]"
    );
    assert_eq!(
        cell_tree("x<hr>y<div>z</div>w"),
        "Td[T(x) R T(y) V[T(z)] T(w)]"
    );
    assert_eq!(
        cell_tree("**a** b<ul><li>c</li></ul>"),
        "Td[T(a b) L[I[T(c)]]]"
    );

    let d = parse("| h |\n|---|\n| <h3>Title</h3> |\n");
    assert_eq!(d.headings.len(), 1);
    assert_eq!(d.str(d.headings[0].slug), "title");
    assert_eq!(d.outline().len(), 1);

    // What came before the tag keeps its formatting, in the child it moved to.
    let d = parse("| h |\n|---|\n| **a** b<ul><li>c</li></ul> |\n");
    let plain = (0..d.blocks.len())
        .find(|&i| d.blocks[i].kind == BlockKind::Plain && d.block_text(i as u32).starts_with('a'))
        .unwrap();
    let spans = &d.spans[d.blocks[plain].spans.usize()];
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, SpanKind::Strong);
    assert_eq!(
        common::writer::html(&d)
            .matches("<strong>a</strong>")
            .count(),
        1
    );

    // A plain cell is untouched.
    assert_eq!(cell_tree("plain *x*"), "Td(plain x)");
}

#[test]
fn block_html_in_a_paragraph_splits_it_as_a_browser_does() {
    let d = parse("Hello <p>inner</p> world and <ul><li>a</li><li>b</li></ul> done");
    assert_eq!(
        tree(&d),
        "P(Hello) P(inner) T(world and) L[I[T(a)] I[T(b)]] T(done)"
    );

    let d = parse("<a href=\"x\"><h2>Link heading</h2></a>");
    assert_eq!(tree(&d), "H(Link heading)");
    assert_eq!(d.str(d.headings[0].slug), "link-heading");
    assert_eq!(links(&d), [("Link heading".to_string(), "x".to_string())]);

    // Formatting opened before the tag continues inside it, and ends with its own end event.
    let d = parse("**a <p>b</p> c** d");
    let strong: Vec<String> = d
        .spans
        .iter()
        .filter(|s| s.kind == SpanKind::Strong)
        .map(|s| span_text(&d, s))
        .collect();
    assert_eq!(strong, ["a ", "b", "c"]);
    assert_eq!(d.block_text(d.blocks.len() as u32 - 1).trim_end(), "d");

    // An HTML block continues what the paragraph left open, with no empty blocks from the newlines.
    // Pulldown ends the paragraph at `<li>`, and a browser only ever sees one stream of tags.
    let d = parse("x <ul>\n<li>a</li>\n</ul>\ny\n\nnext");
    assert_eq!(tree(&d), "P(x) L[I[T(a)]] T(y) P(next)");
    let d = parse("x <div> open\n\nnext");
    assert_eq!(tree(&d), "P(x) V[T(open) P(next)]");

    // Blocks in a tight list item, and a stray closer that closes nothing.
    assert_eq!(tree(&parse("- a <p>b</p>")), "L[I[T(a) P(b)]]");
    assert_eq!(tree(&parse("a </div> b")), "P(a b)");

    // A Markdown heading stays one heading.
    let d = parse("# a <div>b</div> c");
    assert_eq!(tree(&d), "H(a b c)");
    assert_eq!(d.headings.len(), 1);

    // `<pre>` in a paragraph takes Markdown events as plain text.
    let d = parse("a <pre>x  **y**\nz `w`</pre> b");
    assert_eq!(tree(&d), "P(a) C(x  y\nz w) T(b)");
}

// Review findings: footnotes

#[test]
fn footnote_labels_match_ignoring_case() {
    let d = parse("Ref[^Note] and[^note] again[^Note] other[^b].\n\n[^note]: Def\n\n[^B]: Other\n");
    assert_eq!(d.footnotes.len(), 2);
    assert_eq!(d.str(d.footnotes[0].label), "Note");
    assert_eq!(d.footnotes[0].number, 1);
    assert_eq!(d.footnotes[1].number, 2);
    for f in &d.footnotes {
        assert!(matches!(
            d.blocks[f.block as usize].kind,
            BlockKind::FootnoteDefinition { .. }
        ));
    }
    let refs = d
        .spans
        .iter()
        .filter(|s| matches!(s.kind, SpanKind::FootnoteRef(0)))
        .count();
    assert_eq!(refs, 3);
    // Non-ASCII case folds too.
    let d = parse("x[^Ä] y[^ä]\n\n[^ä]: d\n");
    assert_eq!(d.footnotes.len(), 1);
    assert_ne!(d.footnotes[0].block, NONE);
}

// Review findings: slugs

#[test]
fn duplicate_headings_stay_linear() {
    let src = "### Bug Fixes\n\n- x\n\n".repeat(20_000);
    let t = Instant::now();
    let d = parse(&src);
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    assert_eq!(d.str(d.headings[19_999].slug), "bug-fixes-19999");

    let d = parse(&"#\n".repeat(32_768));
    assert_eq!(d.str(d.headings[2].slug), "-2");

    let mut s = mdview_core::slug::Slugger::default();
    let got: Vec<String> = ["a", "a", "a-1", "a", "a-1", "a-2", "a"]
        .iter()
        .map(|t| s.slug(t))
        .collect();
    assert_eq!(got, ["a", "a-1", "a-1-1", "a-2", "a-1-2", "a-2-1", "a-3"]);
}

#[test]
fn slugs_match_github_anchors() {
    // From `gh api markdown -f mode=markdown`: mdview's old output differed on every one of these.
    for (src, slug) in [
        ("# می‌خواهم", "می\u{200c}خواهم"),
        ("# 👨‍💻 Dev", "\u{200d}-dev"),
        ("# x² + y² ½ ① Ⅷ", "x--y---ⅷ"),
        ("# ΣΑΣ Σ", "σασ-σ"),
        ("# a  b", "a--b"),
        ("# a<br>b", "ab"),
        ("# a <br> b", "a--b"),
        ("<h3><img src=x.png><br>Logo Title</h3>", "logo-title"),
        ("<h3>\n  Multi line\n  title\n</h3>", "--multi-line--title"),
        ("Setext\nline two\n===", "setextline-two"),
        ("# tab\there", "tabhere"),
        ("# a[^1] b\n\n[^1]: f", "a1-b"),
        ("# ‿under‿", "‿under‿"),
        ("# မြန်မာ", "မြန်မာ"),
        ("# ខ្មែរ", "ខ្មែរ"),
        // These already agreed and must stay that way.
        ("# foo&nbsp;bar", "foobar"),
        ("# Straße ẞ", "straße-ß"),
        ("# ![alt text](x.png) Title", "-title"),
        ("# $x^2$ math", "x2-math"),
        ("# Q&amp;A", "qa"),
        ("# ⚙️ Config", "\u{fe0f}-config"),
    ] {
        let d = parse(src);
        assert_eq!(d.str(d.headings[0].slug), slug, "{src}");
    }
}

#[test]
fn heading_titles_are_one_line() {
    let d =
        parse("Setext  \nline two\n===\n\n<h2>a<br>b</h2>\n\n<h3><img src=x.png><br>Logo</h3>\n");
    let titles: Vec<&str> = d.headings.iter().map(|h| d.str(h.title)).collect();
    assert_eq!(titles, ["Setext line two", "a b", "Logo"]);
}

// Review findings: limits on hostile input

fn assert_quick(what: &str, limit: Duration, src: &str) -> Document {
    let t = Instant::now();
    let d = parse(src);
    assert!(t.elapsed() < limit, "{what} took {:?}", t.elapsed());
    d
}

#[test]
fn ambient_tags_cannot_multiply_memory() {
    for (name, src) in [
        (
            "repeated",
            format!("{}\n{}", "<i>\n".repeat(30_000), "a\n\n".repeat(30_000)),
        ),
        (
            "div then many",
            format!(
                "<div></div>{}\n\n{}",
                "<i>".repeat(5000),
                "p\n\n".repeat(50_000)
            ),
        ),
        (
            "links",
            format!(
                "{}\n{}",
                "<a href=x>\n".repeat(30_000),
                "a\n\n".repeat(30_000)
            ),
        ),
        (
            "every tag",
            format!(
                "<div>{}\n\n{}",
                "<i><b><u><s><code><kbd><sub><sup><mark><a href=x>".repeat(100),
                "a\n\n".repeat(100_000)
            ),
        ),
    ] {
        let d = assert_quick(name, Duration::from_secs(2), &src);
        assert!(
            d.spans.len() <= src.len() / 2,
            "{name}: {} spans from {} bytes",
            d.spans.len(),
            src.len()
        );
    }
    // The budget ran out on this one, and the reader is told.
    let src = format!("<div><i>\n\n{}", "a\n\n".repeat(100_000));
    let d = parse(&src);
    assert!(d.warnings.contains(&Warning::UnclosedHtmlDropped));
    // An ordinary document with a forgotten closer keeps its formatting throughout.
    let src = format!("<div><b>\n\n{}", "some paragraph text\n\n".repeat(1000));
    let d = parse(&src);
    assert!(d.warnings.is_empty());
    assert_eq!(
        d.spans
            .iter()
            .filter(|s| s.kind == SpanKind::Strong)
            .count(),
        1000
    );
    // Closing works on the single entry that stands for repeats.
    let d = parse("<div><i><i>\n\nslanted\n\n</i>\n\nplain\n");
    let em: Vec<String> = d
        .spans
        .iter()
        .filter(|s| s.kind == SpanKind::Emphasis)
        .map(|s| span_text(&d, s))
        .collect();
    assert_eq!(
        em.iter().filter(|s| !s.is_empty()).collect::<Vec<_>>(),
        ["slanted"]
    );
}

#[test]
fn unmatched_closers_and_deep_html_stay_linear() {
    let n = 300_000;
    for (name, src) in [
        (
            "div closers",
            format!("{}\n{}\n", "<div>".repeat(n), "</p>".repeat(n)),
        ),
        (
            "span closers",
            format!("{}{}", "<i>".repeat(n), "</b>".repeat(n)),
        ),
        (
            "p span closers",
            format!("<p>{}{}", "<i>".repeat(n), "</u>".repeat(n)),
        ),
        (
            "x span closers",
            format!("x{}{}", "<i>".repeat(n), "</b>".repeat(n)),
        ),
        (
            "li",
            format!("{}{}", "<div>".repeat(n), "<li></li>".repeat(n)),
        ),
        (
            "li under list",
            format!("<ul><li>{}{}", "<div>".repeat(n), "<li></li>".repeat(n)),
        ),
        (
            "closer beyond markdown",
            format!("<p>\n\n> {}{}", "<div>".repeat(n), "</p>".repeat(n)),
        ),
        (
            "closed under a block",
            format!("<div><u><p>{}{}", "<i>".repeat(n), "</u>".repeat(n)),
        ),
        (
            "cells",
            format!("<table><tr>{}{}", "<div>".repeat(n), "<td>".repeat(n)),
        ),
    ] {
        let d = assert_quick(name, Duration::from_secs(5), &src);
        let depth = d.blocks.iter().rev().take(1).count();
        assert!(depth <= 1);
    }

    let d = parse(&"<div>".repeat(5000));
    assert!(d.blocks.len() <= mdview_core::limits::MAX_HTML_DEPTH);
    // Text past the limit is still there.
    let d = parse(&format!("{}deep text", "<div>".repeat(5000)));
    assert!(d.text.contains("deep text"));
}

#[test]
fn backslash_runs_stay_linear_and_escape_correctly() {
    let src = "\\".repeat(1024 * 1024);
    let d = assert_quick("backslashes", Duration::from_secs(1), &src);
    assert_eq!(d.text.trim_end().len(), 512 * 1024);

    // Whether the `_` is escaped decides if the email link can include it, for odd and even runs that
    // span several text events.
    for (n, link) in [
        (0, "_a@b.c"),
        (1, "a@b.c"),
        (2, "_a@b.c"),
        (3, "a@b.c"),
        (4, "_a@b.c"),
        (5, "a@b.c"),
    ] {
        let src = format!("{}_a@b.c", "\\".repeat(n));
        assert_eq!(links(&parse(&src))[0].0, link, "{src}");
    }
    assert_eq!(parse("\\\\*a*").spans[0].kind, SpanKind::Emphasis);
    assert_eq!(parse("\\\\\\*a*").spans.len(), 0);
    assert_eq!(parse("\\\\\\*a*").text.trim_end(), "\\*a*");
}

// Review findings: dropped inline elements

#[test]
fn inline_drop_tags_never_swallow_what_follows() {
    let d = parse("- <object> `</object>`\n\nparagraph one\n\n# Heading two\n\nparagraph three\n");
    assert!(d.text.contains("</object>"), "{:?}", d.text);
    assert!(d.text.contains("paragraph one") && d.text.contains("paragraph three"));
    assert_eq!(d.headings.len(), 1);

    let d = parse("- <embed> `</embed>`\n- second item\n\nall the rest\n");
    assert!(d.text.contains("second item") && d.text.contains("all the rest"));

    let d = parse("a <script> `</script>` and more text");
    assert_eq!(
        d.block_text(0).trim_end(),
        "a <script> </script> and more text"
    );
    assert_eq!(d.spans[0].kind, SpanKind::Code);

    let d = parse("a <embed> \\</embed> visible?");
    assert_eq!(d.block_text(0).trim_end(), "a </embed> visible?");

    // A real closer still drops the element with its content, whatever Markdown is inside it.
    assert_eq!(
        parse("a <script>x</script> b").block_text(0).trim_end(),
        "a b"
    );
    assert_eq!(
        parse("a <script>*x* `y`</script> b")
            .block_text(0)
            .trim_end(),
        "a b"
    );
    let d = parse("- <object>gone</object> after\n\nnext\n");
    assert!(d.text.contains("after") && d.text.contains("next") && !d.text.contains("gone"));
    // Emphasis that opens inside the dropped part and ends after it stays balanced.
    let d = parse("a <script> *b </script> c* d");
    assert_eq!(d.block_text(0).trim_end(), "a c d");
    // A closer in the next block does not count.
    let d = parse("a <script> b\n\nc </script> d");
    assert!(
        d.text.contains("<script> b") && d.text.contains("c </script> d"),
        "{:?}",
        d.text
    );

    // Many tags whose closer never really comes, in one block.
    let src = format!("x {}`</script>`", "<script> a ".repeat(50_000));
    let d = assert_quick("waiting", Duration::from_secs(2), &src);
    assert_eq!(d.text.matches("<script>").count(), 50_000);
}

// Review findings: presentation attributes

#[test]
fn html_presentation_attributes_reach_the_model() {
    let d = parse("<div dir=\"rtl\">\n\nمرحبا\n\n</div>\n");
    assert_eq!(d.attrs_of(0).unwrap().dir, Some(Dir::Rtl));
    assert!(d.attrs_of(1).is_none());

    let d = parse("<p dir=\"rtl\" align=\"justify\">نص Latin first</p>\n");
    assert_eq!(
        d.blocks[0].kind,
        BlockKind::Paragraph {
            align: Align::Justify
        }
    );
    assert_eq!(d.attrs_of(0).unwrap().dir, Some(Dir::Rtl));

    let d = parse(
        "<table align=\"center\" width=\"100%\"><tr><td width=\"50%\" height=\"20\" valign=\"top\" dir=\"auto\">x</td></tr></table>\n",
    );
    let table = d.attrs_of(0).unwrap();
    assert_eq!(
        (table.align, table.width),
        (Align::Center, Some(Length::Percent(100)))
    );
    let cell = d.attrs_of(2).unwrap();
    assert!(matches!(d.blocks[2].kind, BlockKind::TableCell { .. }));
    assert_eq!(cell.width, Some(Length::Percent(50)));
    assert_eq!(cell.height, Some(Length::Px(20)));
    assert_eq!(cell.valign, Some(VAlign::Top));
    assert_eq!(cell.dir, Some(Dir::Auto));

    let d = parse("<hr width=\"50%\" align=\"left\">\n");
    assert_eq!(d.blocks[0].kind, BlockKind::ThematicBreak);
    assert_eq!(d.attrs_of(0).unwrap().width, Some(Length::Percent(50)));
    assert_eq!(d.attrs_of(0).unwrap().align, Align::Left);

    let d = parse(
        "<ol type=\"a\"><li>x</li></ol>\n\n<ol type=\"A\"><li>y</li></ol>\n\n<ul type=\"SQUARE\"><li>z</li></ul>\n",
    );
    let markers: Vec<_> = d.block_attrs.iter().map(|a| a.marker).collect();
    assert_eq!(
        markers,
        [
            Some(ListMarker::LowerAlpha),
            Some(ListMarker::UpperAlpha),
            Some(ListMarker::Square)
        ]
    );

    let d = parse("<img src=\"logo.png\" align=\"right\">\n");
    assert_eq!(d.images[0].align, ImageAlign::Right);
    let d = parse("![x](y.png)\n");
    assert_eq!(d.images[0].align, ImageAlign::None);

    // A dropped empty paragraph takes its attributes with it, so the table stays sorted and true.
    let d = parse("<p dir=\"rtl\"></p>\n\n<div dir=\"ltr\">x</div>\n");
    assert_eq!(d.block_attrs.len(), 1);
    assert_eq!(d.attrs_of(0).unwrap().dir, Some(Dir::Ltr));

    // Attributes inline in a paragraph, and on tags with nothing to record.
    let d = parse("a <div dir=\"rtl\">b</div>\n");
    assert_eq!(d.block_attrs.len(), 1);
    let d = parse("<div class=\"x\">b</div>\n");
    assert!(d.block_attrs.is_empty());
}

#[test]
fn image_without_a_usable_source_keeps_its_alt() {
    let d = parse("<img alt=\"no src\">\n");
    let img = d.images[0];
    assert_eq!(d.str(img.alt), "no src");
    assert_eq!(img.src.len, 0);
    assert!(d.spans.iter().any(|s| matches!(s.kind, SpanKind::Image(0))));

    let d = parse("<img src=\"javascript:alert(1)\" alt=\"bad\">\n");
    assert_eq!(d.str(d.images[0].alt), "bad");
    assert_eq!(d.images[0].src.len, 0);
    assert!(!d.strings.contains("javascript"));

    // With nothing to show there is still nothing to add.
    assert!(parse("<img>\n").images.is_empty());
}

// Review findings: links

#[test]
fn urls_naming_a_host_never_reach_the_filesystem() {
    let dir = Path::new("/docs");
    for src in [
        "//evil.example/share/x.md",
        "\\\\evil.example\\share\\x.md",
        "/\\evil.example\\share\\x.md",
        "\\/evil.example/share/x.md",
        "%2F%2Fevil.example/share/x.md",
        "%5C%5Cevil.example%5Cshare%5Cx.md",
        "/%5Cevil.example/x.md",
        "\\\\.\\pipe\\x.md",
        "\\\\?\\UNC\\evil\\share\\x.md",
        "//evil.example:445/share/x.md",
        "file:////evil.example/share/x.md",
        "file:\\\\evil.example\\share\\x.md",
        "file:%5C%5Cevil.example%5Cx.md",
        "file://evil.example/share/x.md",
    ] {
        assert_eq!(classify_link(src, dir), LinkAction::Ignored, "{src}");
        assert_eq!(classify_image(src, dir), ImageSource::Unsupported, "{src}");
    }
    // Ordinary local forms are unaffected.
    assert_eq!(
        classify_image("/abs/a.png", dir),
        ImageSource::Local(PathBuf::from("/abs/a.png"))
    );
    assert_eq!(
        classify_image("sub//a.png", dir),
        ImageSource::Local(PathBuf::from("/docs/sub/a.png"))
    );
    assert_eq!(
        classify_link("file:///abs/a.md", dir),
        LinkAction::OpenMarkdown {
            path: PathBuf::from("/abs/a.md"),
            fragment: None
        }
    );
}

// Review findings: reload position

#[test]
fn reload_does_not_jump_to_a_repeated_block_elsewhere() {
    let build = |first: &str| {
        (0..50)
            .map(|i| match i {
                11 => first.to_string(),
                42 => "Fixed".to_string(),
                _ => format!("para {i}"),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let old = parse(&build("Fixed"));
    let new = parse(&build("Changed"));
    let pos = ScrollPos {
        block: 11,
        offset: 5.0,
    };
    assert_eq!(new.remap_scroll(&old, pos), pos);

    // Before the edit stays, after it shifts by the change in length.
    let old = parse("a\n\nb\n\nc\n\nd\n\ne");
    let added = parse("a\n\nb\n\nnew\n\nnew\n\nc\n\nd\n\ne");
    let at = |block| ScrollPos { block, offset: 1.0 };
    assert_eq!(added.remap_scroll(&old, at(1)).block, 1);
    assert_eq!(added.remap_scroll(&old, at(3)).block, 5);
    let removed = parse("a\n\nd\n\ne");
    assert_eq!(removed.remap_scroll(&old, at(4)).block, 2);
    assert_eq!(removed.remap_scroll(&old, at(0)).block, 0);
    // Inside the removed part, and past the end, it stays in range.
    assert_eq!(removed.remap_scroll(&old, at(2)).block, 2);
    assert_eq!(removed.remap_scroll(&old, at(99)).block, 2);
    // Repeated blocks at the edit point cannot make the two ends overlap.
    let old = parse("x\n\nx\n\nx");
    let new = parse("x\n\nx");
    assert!(new.remap_scroll(&old, at(2)).block <= 1);
    assert_eq!(parse("").remap_scroll(&old, at(2)), ScrollPos::default());
}

// Review findings: find and closed details

#[test]
fn find_skips_closed_details_bodies() {
    let d =
        parse("<details><summary>More needle</summary>\n\nSecret needle\n\n</details>\n\nneedle");
    let details = d
        .blocks
        .iter()
        .position(|b| matches!(b.kind, BlockKind::Details { .. }))
        .unwrap() as u32;
    assert_eq!(find(&d, "needle").hits.len(), 2);
    assert_eq!(find(&d, "Secret").hits.len(), 0);
    assert_eq!(find_visible(&d, "needle", |_| true).hits.len(), 3);
    assert_eq!(find_visible(&d, "Secret", |b| b == details).hits.len(), 1);
    assert_eq!(find_visible(&d, "needle", |_| false).hits.len(), 2);

    let d = parse("<details open>\n\nSecret\n\n</details>\n");
    assert_eq!(find(&d, "Secret").hits.len(), 1);
    assert_eq!(find_visible(&d, "Secret", |_| false).hits.len(), 0);

    // No summary hides everything, nesting stays ordered, and math in a summary is still hidden.
    let d = parse(
        "<details>\n<summary>\n\n$x$ s1\n\n</summary>\n\nb1\n\n<details>\n<summary>s2</summary>\n\nb2\n\n</details>\n\nb3\n\n</details>\n\nb4\n\n$$q$$\n",
    );
    for (query, hits) in [
        ("s1", 1),
        ("b1", 0),
        ("s2", 0),
        ("b2", 0),
        ("b3", 0),
        ("b4", 1),
        ("x", 0),
        ("q", 0),
    ] {
        assert_eq!(find(&d, query).hits.len(), hits, "{query}");
    }
    let hidden = d.hidden_ranges(&|_| false);
    assert!(
        hidden.windows(2).all(|w| w[0].end <= w[1].start),
        "{hidden:?}"
    );
    // Open outer with closed inner.
    let outer = 0;
    assert_eq!(find_visible(&d, "b1", |b| b == outer).hits.len(), 1);
    assert_eq!(find_visible(&d, "s2", |b| b == outer).hits.len(), 1);
    assert_eq!(find_visible(&d, "b2", |b| b == outer).hits.len(), 0);
}

// Review findings: Windows paths

#[test]
fn verbatim_windows_paths_become_ordinary() {
    for (from, to) in [
        (r"\\?\C:\Users\kaan\a.md", Some(r"C:\Users\kaan\a.md")),
        (r"\\?\UNC\server\share\a.md", Some(r"\\server\share\a.md")),
        (r"C:\Users\kaan\a.md", None),
        (r"\\server\share\a.md", None),
        (r"\\?\Volume{1234}\a.md", None),
        (r"\\?\C:\Users\NUL.txt", None),
        (r"\\?\C:\Users\com1", None),
        (r"\\?\C:\Users\trailing.", None),
        (r"\\?\C:\Users\space ", None),
        (r"\\?\C:\Users\a:b", None),
    ] {
        assert_eq!(files::strip_verbatim(from).as_deref(), to, "{from}");
    }
    let long = format!(r"\\?\C:\{}", "a".repeat(300));
    assert_eq!(files::strip_verbatim(&long), None);
}

#[cfg(windows)]
#[test]
fn canonical_paths_have_one_identity_on_windows() {
    let dir = temp_dir("verbatim");
    let path = dir.join("real.md");
    std::fs::write(&path, "x").unwrap();
    let present = files::canonical_or_absolute(&path);
    assert!(
        !present.to_string_lossy().starts_with(r"\\?\"),
        "{present:?}"
    );
    std::fs::remove_file(&path).unwrap();
    assert_eq!(files::canonical_or_absolute(&path), present);
}
