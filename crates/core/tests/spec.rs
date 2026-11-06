//! CommonMark 0.31.2 and cmark-gfm spec suites, run in the viewer's real configuration: every
//! extension and the autolink pass on, comparing `common::neutral` of the expected HTML against
//! `common::neutral` of the document model's rendering.

mod common;

use common::{neutral, normalize, writer};
use std::path::PathBuf;

struct Example {
    number: usize,
    markdown: String,
    html: String,
}

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spikes/parser/data")
        .join(name)
}

fn load_commonmark() -> Vec<Example> {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(data("spec.json")).unwrap()).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| Example {
            number: e["example"].as_u64().unwrap() as usize,
            markdown: e["markdown"].as_str().unwrap().to_string(),
            html: e["html"].as_str().unwrap().to_string(),
        })
        .collect()
}

fn load_gfm() -> Vec<Example> {
    let fence = "`".repeat(32);
    let text = std::fs::read_to_string(data("spec.txt")).unwrap();
    let (mut out, mut state) = (Vec::new(), 0);
    let (mut md, mut html) = (String::new(), String::new());
    for l in text.lines() {
        if state == 0 {
            if l.starts_with(&format!("{fence} example")) {
                state = 1;
                md.clear();
                html.clear();
            }
        } else if l == fence {
            out.push(Example {
                number: out.len() + 1,
                markdown: md.replace('\u{2192}', "\t"),
                html: html.replace('\u{2192}', "\t"),
            });
            state = 0;
        } else if l == "." && state == 1 {
            state = 2;
        } else {
            let b = if state == 1 { &mut md } else { &mut html };
            b.push_str(l);
            b.push('\n');
        }
    }
    out
}

/// Runs a suite and fails if the misses differ from `exceptions` in either direction.
fn run(name: &str, examples: &[Example], exceptions: &[(usize, &str)]) {
    let mut misses = Vec::new();
    let mut report = String::new();
    let mut without_visible = 0;
    for e in examples {
        let rendered = writer::html(&mdview_core::parse(&e.markdown));
        // Disclosure only: how many match with no allow-list reduction at all.
        if normalize::semantic(&normalize::normalize(&rendered))
            == normalize::semantic(&normalize::normalize(&e.html))
        {
            without_visible += 1;
        }
        let got = neutral(&rendered);
        let want = neutral(&e.html);
        if got != want {
            misses.push(e.number);
            if !exceptions.iter().any(|(n, _)| *n == e.number) {
                report.push_str(&format!(
                    "--- example {}\n{}--- want\n{want}\n--- got\n{got}\n",
                    e.number, e.markdown
                ));
            }
        }
    }
    let unlisted: Vec<usize> = misses
        .iter()
        .copied()
        .filter(|m| !exceptions.iter().any(|(n, _)| n == m))
        .collect();
    let stale: Vec<usize> = exceptions
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| !misses.contains(n))
        .collect();
    println!(
        "{name}: {} examples, {} match, {} listed exceptions, {} unlisted misses ({without_visible} match without the allow-list reduction)",
        examples.len(),
        examples.len() - misses.len(),
        misses.len() - unlisted.len(),
        unlisted.len()
    );
    assert!(
        unlisted.is_empty() && stale.is_empty(),
        "{name}: unlisted misses {unlisted:?}, stale exceptions {stale:?}\n{report}"
    );
}

const BARE_URL: &str = "bare URLs stay linked as on GitHub; the example expects plain text";

const PRE_BLANK: &str =
    "a blank line ends the HTML block, so <pre> closes there; the text and its order are unchanged";
const WRAPPER: &str = "looks the same: an inline tag alone in an HTML block wraps the next block's content from inside";

const COMMONMARK_EXCEPTIONS: &[(usize, &str)] = &[
    (148, PRE_BLANK),
    (167, WRAPPER),
    (602, BARE_URL),
    (608, BARE_URL),
    (611, BARE_URL),
    (612, BARE_URL),
];
const GFM_EXCEPTIONS: &[(usize, &str)] = &[
    (118, PRE_BLANK),
    (137, WRAPPER),
    (610, BARE_URL),
    (616, BARE_URL),
    (619, BARE_URL),
    (620, BARE_URL),
];

#[test]
fn commonmark_suite() {
    run(
        "CommonMark 0.31.2",
        &load_commonmark(),
        COMMONMARK_EXCEPTIONS,
    );
}

#[test]
fn gfm_suite() {
    run("cmark-gfm spec", &load_gfm(), GFM_EXCEPTIONS);
}

#[test]
fn comparison_still_sees_differences() {
    for (a, b) in [
        ("<p>a</p>", "<p>b</p>"),
        ("<p>a</p>", "<h1>a</h1>"),
        ("<em>a</em>", "<strong>a</strong>"),
        (
            "<pre><code>a  b</code></pre>",
            "<pre><code>a b</code></pre>",
        ),
        ("<a href=\"x\">y</a>", "<a href=\"z\">y</a>"),
        ("<img src=\"a\" alt=\"b\" />", "<img src=\"a\" alt=\"c\" />"),
        ("<p>a<br />b</p>", "<p>a b</p>"),
        ("<ol start=\"2\"><li>a</li></ol>", "<ol><li>a</li></ol>"),
        ("<td align=\"left\">a</td>", "<td>a</td>"),
    ] {
        assert_ne!(neutral(a), neutral(b), "{a} vs {b}");
    }
}
