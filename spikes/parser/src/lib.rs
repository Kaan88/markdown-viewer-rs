//! Throwaway G4 spike: shared generator, HTML normalizer, bare-URL autolink pass, parser configs.
use comrak::nodes::NodeValue;
use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, TextMergeStream};
use std::collections::VecDeque;
use std::hint::black_box;

// ---------- parser configurations ----------

/// The GFM options the task asks for. ENABLE_GFM only adds alert blockquotes.
pub fn pd_gfm() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_GFM
}
/// What the viewer would really run.
pub fn pd_viewer() -> Options {
    pd_gfm() | Options::ENABLE_FOOTNOTES | Options::ENABLE_MATH
}
pub fn cm_base() -> comrak::Options<'static> {
    let mut o = comrak::Options::default();
    o.render.r#unsafe = true;
    o
}
pub fn cm_gfm() -> comrak::Options<'static> {
    let mut o = cm_base();
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.alerts = true;
    o
}
pub fn cm_viewer() -> comrak::Options<'static> {
    let mut o = cm_gfm();
    o.extension.footnotes = true;
    o.extension.math_dollars = true;
    o
}

// ---------- walkers (touch every event / node so nothing is optimized out) ----------

fn pd_tally(e: &Event, n: &mut usize) {
    *n += 1;
    match e {
        Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) | Event::InlineMath(t)
        | Event::DisplayMath(t) => *n += t.len(),
        _ => {}
    }
}
pub fn walk_pulldown(doc: &str) -> usize {
    let mut n = 0;
    for e in Parser::new_ext(doc, pd_viewer()) {
        pd_tally(&e, &mut n);
    }
    black_box(n)
}
pub fn walk_pulldown_autolink(doc: &str) -> usize {
    let mut n = 0;
    for e in autolink(Parser::new_ext(doc, pd_viewer())) {
        pd_tally(&e, &mut n);
    }
    black_box(n)
}
/// Retained form, comparable to holding an AST: all events kept until the walk ends.
pub fn walk_pulldown_retained(doc: &str) -> usize {
    let v: Vec<Event> = autolink(Parser::new_ext(doc, pd_viewer())).collect();
    let mut n = 0;
    for e in &v {
        pd_tally(e, &mut n);
    }
    black_box(n)
}
pub fn walk_comrak(doc: &str) -> usize {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, doc, &cm_viewer());
    let mut n = 0;
    for node in root.descendants() {
        n += 1;
        match &node.data().value {
            NodeValue::Text(t) => n += t.len(),
            NodeValue::Code(c) => n += c.literal.len(),
            NodeValue::CodeBlock(c) => n += c.literal.len(),
            NodeValue::HtmlBlock(h) => n += h.literal.len(),
            NodeValue::HtmlInline(h) => n += h.len(),
            NodeValue::Math(m) => n += m.literal.len(),
            _ => {}
        }
    }
    black_box(n)
}

// ---------- seeded document generator ----------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 33) as usize % n
    }
    fn range(&mut self, a: usize, b: usize) -> usize {
        a + self.below(b - a + 1)
    }
}
const WORDS: &[&str] = &[
    "the", "of", "and", "to", "in", "is", "that", "for", "it", "as", "with", "on", "viewer", "layout", "document",
    "parser", "render", "scroll", "memory", "launch", "window", "theme", "table", "heading", "paragraph", "selection",
    "cache", "budget", "latency", "reading", "glyph", "shaping", "fonts", "engine", "tab", "session", "watcher",
    "güncelleme", "belge", "görünüm", "çalışma", "ışık", "pencere", "önbellek", "değişiklik", "日本語", "文書",
];

fn words(r: &mut Rng, s: &mut String, n: usize) {
    for i in 0..n {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(WORDS[r.below(WORDS.len())]);
    }
}
fn inline(r: &mut Rng, s: &mut String, n_spans: usize) {
    for _ in 0..n_spans {
        let k = r.range(2, 12);
        words(r, s, k);
        s.push(' ');
        let k = r.range(1, 3);
        let mut w = String::new();
        words(r, &mut w, k);
        match r.below(20) {
            0..=4 => s.push_str(&format!("*{w}*")),
            5..=8 => s.push_str(&format!("**{w}**")),
            9 => s.push_str(&format!("***{w}***")),
            10 => s.push_str(&format!("~~{w}~~")),
            11..=13 => s.push_str(&format!("`{w}`")),
            14 | 15 => s.push_str(&format!("[{w}](https://example.com/p/{} \"title\")", r.below(9999))),
            16 => s.push_str(&format!("[{w}][ref{}]", r.below(40))),
            17 => s.push_str(&format!("https://example.org/a/{}?q={}", r.below(9999), r.below(99))),
            18 => s.push_str(&format!("<span class=\"x\">{w}</span>")),
            _ => s.push_str(&format!("![{w}](assets/{}.png)", r.below(99))),
        }
        s.push(' ');
    }
    s.push_str("end.\n");
}

/// Deterministic for a seed. Stops at the first block boundary at or past `target` bytes.
pub fn generate(seed: u64, target: usize) -> String {
    let mut r = Rng(seed | 1);
    let mut s = String::with_capacity(target + 8192);
    for i in 0..40 {
        s.push_str(&format!("[ref{i}]: https://example.net/ref/{i} \"Reference {i}\"\n"));
    }
    s.push('\n');
    let mut fn_id = 0;
    while s.len() < target {
        match r.below(100) {
            0..=9 => {
                s.push_str(&"#".repeat(r.range(1, 4)));
                s.push(' ');
                let k = r.range(2, 7);
                words(&mut r, &mut s, k);
                s.push('\n');
            }
            10..=44 => {
                let k = r.range(2, 6);
                inline(&mut r, &mut s, k);
            }
            45..=56 => {
                for _ in 0..r.range(3, 9) {
                    let d = r.below(3);
                    s.push_str(&"  ".repeat(d));
                    s.push_str("- ");
                    inline(&mut r, &mut s, 1);
                }
            }
            57..=62 => {
                for i in 1..=r.range(3, 8) {
                    s.push_str(&format!("{i}. "));
                    inline(&mut r, &mut s, 1);
                }
            }
            63..=66 => {
                for _ in 0..r.range(2, 7) {
                    s.push_str(if r.below(2) == 0 { "- [ ] " } else { "- [x] " });
                    inline(&mut r, &mut s, 1);
                }
            }
            67..=76 => {
                let cols = r.range(2, 6);
                let rows = r.range(3, 14);
                for c in 0..cols {
                    s.push_str(&format!("| col{c} "));
                }
                s.push_str("|\n");
                for _ in 0..cols {
                    s.push_str(["| --- ", "| :-- ", "| :-: ", "| --: "][r.below(4)]);
                }
                s.push_str("|\n");
                for _ in 0..rows {
                    for _ in 0..cols {
                        s.push_str("| ");
                        let k = r.range(1, 4);
                        match r.below(6) {
                            0 => {
                                s.push('`');
                                words(&mut r, &mut s, k);
                                s.push('`');
                            }
                            1 => {
                                s.push_str("**");
                                words(&mut r, &mut s, k);
                                s.push_str("**");
                            }
                            _ => words(&mut r, &mut s, k),
                        }
                        s.push(' ');
                    }
                    s.push_str("|\n");
                }
            }
            77..=88 => {
                s.push_str(["```rust\n", "```python\n", "```\n", "```json\n"][r.below(4)]);
                for _ in 0..r.range(4, 28) {
                    s.push_str(&" ".repeat(r.below(3) * 4));
                    s.push_str(&format!("let v{} = call_{}(&x, {});\n", r.below(99), r.below(99), r.below(999)));
                }
                s.push_str("```\n");
            }
            89..=92 => {
                s.push_str(&format!(
                    "<details>\n<summary>Item {}</summary>\n\n",
                    r.below(999)
                ));
                inline(&mut r, &mut s, 2);
                s.push_str("\n</details>\n");
            }
            93 | 94 => {
                s.push_str("<div align=\"center\">\n<table><tr><td>raw cell</td></tr></table>\n</div>\n");
            }
            95 | 96 => {
                for _ in 0..r.range(1, 4) {
                    s.push_str("> ");
                    inline(&mut r, &mut s, 1);
                }
            }
            97 => {
                s.push_str(["> [!NOTE]\n", "> [!WARNING]\n", "> [!TIP]\n"][r.below(3)]);
                s.push_str("> ");
                inline(&mut r, &mut s, 1);
            }
            98 => {
                fn_id += 1;
                s.push_str(&format!("Footnoted claim[^f{fn_id}].\n\n[^f{fn_id}]: Footnote body {fn_id}.\n"));
            }
            _ => {
                s.push_str(&format!("Inline $a_{0}^2 + b_{0}$ math.\n\n$$\n\\sum_i x_{0}\n$$\n", r.below(9)));
            }
        }
        s.push('\n');
    }
    s
}

// ---------- bare-URL autolink post-pass over pulldown-cmark events ----------

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LinkKind {
    Www,
    Url,
    Email,
}

fn is_ws(c: char) -> bool {
    c.is_whitespace()
}

/// Returns byte length of a valid domain at the head of `s`.
fn domain_len(s: &str, allow_short: bool) -> Option<usize> {
    let (mut dots, mut us1, mut us2, mut end) = (0, false, false, 0);
    for (i, c) in s.char_indices() {
        if c == '.' {
            dots += 1;
            us2 = us1;
            us1 = false;
        } else if c == '_' {
            us1 = true;
        } else if !(c == '-' || c.is_alphanumeric()) {
            break;
        }
        end = i + c.len_utf8();
    }
    (end > 0 && !us1 && !us2 && (allow_short || dots > 0)).then_some(end)
}

/// GFM trailing-punctuation rules: returns the trimmed byte length.
fn trim_end(s: &str) -> usize {
    let mut end = s.find('<').unwrap_or(s.len());
    while end > 0 {
        let b = s.as_bytes()[end - 1];
        match b {
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => end -= 1,
            b';' => {
                let head = &s.as_bytes()[..end - 1];
                let mut i = head.len();
                while i > 0 && head[i - 1].is_ascii_alphanumeric() {
                    i -= 1;
                }
                if i > 0 && i < head.len() && head[i - 1] == b'&' {
                    end = i - 1;
                } else {
                    end -= 1;
                }
            }
            b')' => {
                let body = &s[..end];
                if body.matches(')').count() > body.matches('(').count() {
                    end -= 1;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    end
}

fn url_tail(s: &str, dom: usize) -> usize {
    let stop = s[dom..].find(is_ws).map_or(s.len(), |i| dom + i);
    trim_end(&s[..stop])
}

/// Finds GFM extended autolinks in plain text: `www.`, `http(s)://`, `ftp://`, and emails.
pub fn find_links(t: &str) -> Vec<(usize, usize, LinkKind)> {
    let b = t.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut floor) = (0, 0);
    while i < b.len() {
        let hit = match b[i] {
            b'w' if t[i..].starts_with("www.") => {
                let ok = i == 0
                    || t[..i].chars().next_back().is_some_and(|c| is_ws(c) || "*_~(".contains(c));
                if ok {
                    domain_len(&t[i..], false).map(|d| (i, i + url_tail(&t[i..], d), LinkKind::Www))
                } else {
                    None
                }
            }
            b':' if t[i..].starts_with("://") => {
                let mut st = i;
                while st > floor && b[st - 1].is_ascii_alphabetic() {
                    st -= 1;
                }
                let scheme = t[st..i].to_ascii_lowercase();
                if matches!(scheme.as_str(), "http" | "https" | "ftp") {
                    domain_len(&t[i + 3..], true).map(|d| (st, i + 3 + url_tail(&t[i + 3..], d), LinkKind::Url))
                } else {
                    None
                }
            }
            b'@' => {
                let mut st = i;
                while st > floor && (b[st - 1].is_ascii_alphanumeric() || b".+-_".contains(&b[st - 1])) {
                    st -= 1;
                }
                let (mut e, mut dots) = (i + 1, 0);
                while e < b.len() {
                    let c = b[e];
                    if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' {
                    } else if c == b'.' && e + 1 < b.len() && b[e + 1].is_ascii_alphanumeric() {
                        dots += 1;
                    } else {
                        break;
                    }
                    e += 1;
                }
                let last_ok = e > i + 1 && b[e - 1].is_ascii_alphabetic();
                (st < i && dots > 0 && last_ok).then_some((st, e, LinkKind::Email))
            }
            _ => None,
        };
        match hit {
            Some((s, e, k)) if e > s => {
                out.push((s, e, k));
                i = e;
                floor = e;
            }
            _ => i += 1,
        }
    }
    out
}

fn slice<'a>(src: &CowStr<'a>, a: usize, b: usize) -> CowStr<'a> {
    match src {
        CowStr::Borrowed(s) => CowStr::Borrowed(&s[a..b]),
        other => CowStr::from(other[a..b].to_string()),
    }
}

pub struct Autolink<'a, I: Iterator<Item = Event<'a>>> {
    inner: TextMergeStream<'a, I>,
    queue: VecDeque<Event<'a>>,
    skip: u32,
}

/// Merges adjacent text, then rewrites bare URLs and emails in text outside links, images and code.
pub fn autolink<'a, I: Iterator<Item = Event<'a>>>(it: I) -> Autolink<'a, I> {
    Autolink { inner: TextMergeStream::new(it), queue: VecDeque::new(), skip: 0 }
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for Autolink<'a, I> {
    type Item = Event<'a>;
    fn next(&mut self) -> Option<Event<'a>> {
        if let Some(e) = self.queue.pop_front() {
            return Some(e);
        }
        let e = self.inner.next()?;
        match &e {
            Event::Start(Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_)) => self.skip += 1,
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock) => self.skip -= 1,
            Event::Text(t) if self.skip == 0 && (t.contains("://") || t.contains("www.") || t.contains('@')) => {
                let links = find_links(t);
                if links.is_empty() {
                    return Some(e);
                }
                let mut pos = 0;
                for (s, end, kind) in links {
                    if s > pos {
                        self.queue.push_back(Event::Text(slice(t, pos, s)));
                    }
                    let txt = &t[s..end];
                    let (dest, lt) = match kind {
                        LinkKind::Www => (format!("http://{txt}"), LinkType::Autolink),
                        LinkKind::Url => (txt.to_string(), LinkType::Autolink),
                        // pulldown's LinkType::Email means dest_url has no scheme; consumers add `mailto:`.
                        LinkKind::Email => (txt.to_string(), LinkType::Email),
                    };
                    self.queue.push_back(Event::Start(Tag::Link {
                        link_type: lt,
                        dest_url: dest.into(),
                        title: "".into(),
                        id: "".into(),
                    }));
                    self.queue.push_back(Event::Text(slice(t, s, end)));
                    self.queue.push_back(Event::End(TagEnd::Link));
                    pos = end;
                }
                if pos < t.len() {
                    self.queue.push_back(Event::Text(slice(t, pos, t.len())));
                }
                return self.queue.pop_front();
            }
            _ => {}
        }
        Some(e)
    }
}

// ---------- rendering helpers ----------

pub fn html_pulldown(md: &str, opts: Options, auto: bool) -> String {
    let mut out = String::new();
    let p = Parser::new_ext(md, opts);
    if auto {
        pulldown_cmark::html::push_html(&mut out, autolink(p));
    } else {
        pulldown_cmark::html::push_html(&mut out, p);
    }
    out
}
pub fn html_comrak(md: &str, opts: &comrak::Options) -> String {
    comrak::markdown_to_html(md, opts)
}

// ---------- HTML normalizer: Rust port of cmark-gfm test/normalize.py ----------

#[derive(PartialEq, Clone, Copy)]
enum Last {
    Start,
    End,
    Data,
    Other,
}
struct Norm {
    out: String,
    last: Last,
    last_tag: String,
    in_pre: bool,
}
const BLOCK_TAGS: &[&str] = &[
    "article", "header", "aside", "hgroup", "blockquote", "hr", "iframe", "body", "li", "map", "button", "object",
    "canvas", "ol", "caption", "output", "col", "p", "colgroup", "pre", "dd", "progress", "div", "section", "dl",
    "table", "td", "dt", "tbody", "embed", "textarea", "fieldset", "tfoot", "figcaption", "th", "figure", "thead",
    "footer", "tr", "form", "ul", "h1", "h2", "h3", "h4", "h5", "h6", "video", "script", "style",
];
/// Python str.isspace-like, as used by its `\s` and strip().
fn py_ws(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}
fn collapse(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut ws = false;
    for c in s.chars() {
        if py_ws(c) {
            if !ws {
                o.push(' ');
            }
            ws = true;
        } else {
            o.push(c);
            ws = false;
        }
    }
    o
}
fn rstrip(s: &mut String) {
    let n = s.trim_end_matches(py_ws).len();
    s.truncate(n);
}
impl Norm {
    fn data(&mut self, d: &str) {
        let after_tag = self.last == Last::End || self.last == Last::Start;
        let after_block = after_tag && BLOCK_TAGS.contains(&self.last_tag.as_str());
        let mut d = d;
        if after_tag && self.last_tag == "br" {
            d = d.trim_start_matches('\n');
        }
        let c;
        let mut d: &str = if self.in_pre {
            d
        } else {
            c = collapse(d);
            &c
        };
        if after_block && !self.in_pre {
            d = if self.last == Last::Start { d.trim_start_matches(py_ws) } else { d.trim_matches(py_ws) };
        }
        self.out.push_str(d);
        self.last = Last::Data;
    }
    /// Text with character references kept as separate tokens, as HTMLParser does.
    fn text(&mut self, t: &str) {
        let mut rest = t;
        while let Some(a) = rest.find('&') {
            let tail = &rest[a + 1..];
            let refn = tail.find(';').filter(|&e| {
                e > 0 && tail[..e].chars().enumerate().all(|(i, c)| c.is_ascii_alphanumeric() || (i == 0 && c == '#'))
            });
            let Some(e) = refn else {
                self.data(&rest[..=a]);
                rest = tail;
                continue;
            };
            if a > 0 {
                self.data(&rest[..a]);
            }
            match decode_ref(&tail[..e]) {
                Some('<') => self.out.push_str("&lt;"),
                Some('>') => self.out.push_str("&gt;"),
                Some('&') => self.out.push_str("&amp;"),
                Some('"') => self.out.push_str("&quot;"),
                Some(c) => self.out.push(c),
                None => {
                    self.out.push('&');
                    self.out.push_str(&tail[..=e]);
                }
            }
            self.last = Last::Other;
            rest = &tail[e + 1..];
        }
        if !rest.is_empty() {
            self.data(rest);
        }
    }
    fn end_tag(&mut self, tag: &str) {
        if tag == "pre" {
            self.in_pre = false;
        } else if BLOCK_TAGS.contains(&tag) {
            rstrip(&mut self.out);
        }
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push('>');
        self.last_tag = tag.to_string();
        self.last = Last::End;
    }
    fn start_tag(&mut self, tag: &str, mut attrs: Vec<(String, Option<String>)>, self_close: bool) {
        if tag == "pre" {
            self.in_pre = true;
        }
        if BLOCK_TAGS.contains(&tag) {
            rstrip(&mut self.out);
        }
        self.out.push('<');
        self.out.push_str(tag);
        attrs.sort();
        for (k, v) in attrs {
            self.out.push(' ');
            self.out.push_str(&k);
            if let Some(v) = v {
                self.out.push_str("=\"");
                self.out.push_str(&escape_attr(&unescape(&v)));
                self.out.push('"');
            }
        }
        self.out.push('>');
        self.last_tag = tag.to_string();
        self.last = if self_close { Last::End } else { Last::Start };
    }
}
fn decode_ref(r: &str) -> Option<char> {
    if let Some(n) = r.strip_prefix('#') {
        let cp = match n.strip_prefix(['x', 'X']) {
            Some(h) => u32::from_str_radix(h, 16).ok()?,
            None => n.parse().ok()?,
        };
        return char::from_u32(cp);
    }
    match r {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "quot" => Some('"'),
        _ => None,
    }
}
fn unescape(v: &str) -> String {
    let mut o = String::new();
    let mut rest = v;
    while let Some(a) = rest.find('&') {
        o.push_str(&rest[..a]);
        let tail = &rest[a + 1..];
        match tail.find(';').and_then(|e| decode_ref(&tail[..e]).map(|c| (e, c))) {
            Some((e, c)) => {
                o.push(c);
                rest = &tail[e + 1..];
            }
            None => {
                o.push('&');
                rest = tail;
            }
        }
    }
    o.push_str(rest);
    o
}
fn escape_attr(v: &str) -> String {
    v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#x27;")
}
/// Parses `<name attrs>`; None means "not a tag", which is then kept as verbatim data.
fn parse_start(chunk: &str) -> Option<(String, Vec<(String, Option<String>)>, bool)> {
    let body = &chunk[1..chunk.len() - 1];
    if !body.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let name_end = body.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(body.len());
    let name = body[..name_end].to_ascii_lowercase();
    let mut s = &body[name_end..];
    let mut attrs = Vec::new();
    let mut self_close = false;
    loop {
        s = s.trim_start();
        if s.is_empty() {
            break;
        }
        if s == "/" {
            self_close = true;
            break;
        }
        if let Some(r) = s.strip_prefix('/') {
            s = r;
            continue;
        }
        let e = s.find(|c: char| c.is_whitespace() || c == '=' || c == '/').unwrap_or(s.len());
        if e == 0 {
            return None;
        }
        let k = s[..e].to_ascii_lowercase();
        s = s[e..].trim_start();
        if let Some(r) = s.strip_prefix('=') {
            let r = r.trim_start();
            let (v, rest) = match r.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let close = r[1..].find(q)?;
                    (r[1..1 + close].to_string(), &r[close + 2..])
                }
                _ => {
                    let e = r.find(char::is_whitespace).unwrap_or(r.len());
                    (r[..e].to_string(), &r[e..])
                }
            };
            attrs.push((k, Some(v)));
            s = rest;
        } else {
            attrs.push((k, None));
        }
    }
    Some((name, attrs, self_close))
}
pub fn normalize(html: &str) -> String {
    let mut n = Norm { out: String::new(), last: Last::Start, last_tag: String::new(), in_pre: false };
    let mut rest = html;
    while !rest.is_empty() {
        if rest.starts_with("<![CDATA[") {
            if let Some(e) = rest.find("]]>") {
                n.out.push_str(&rest[..e + 3]);
                rest = &rest[e + 3..];
                continue;
            }
        }
        if rest.starts_with('<') {
            let Some(e) = rest.find('>') else {
                n.out.push_str(rest);
                break;
            };
            let chunk = &rest[..=e];
            rest = &rest[e + 1..];
            if chunk.starts_with("<!") || chunk.starts_with("<?") {
                n.out.push_str(chunk);
                n.last = Last::Other;
            } else if let Some(t) = chunk.strip_prefix("</") {
                n.end_tag(&t[..t.len() - 1].trim().to_ascii_lowercase());
            } else if let Some((tag, attrs, sc)) = parse_start(chunk) {
                n.start_tag(&tag, attrs, sc);
            } else {
                n.out.push_str(chunk);
                n.last = Last::Other;
            }
        } else {
            let e = rest.find('<').unwrap_or(rest.len());
            n.text(&rest[..e]);
            rest = &rest[e..];
        }
    }
    n.out
}

/// Writer-form neutral comparison, applied to both sides after `normalize`:
/// - pulldown-cmark's HTML writer emits `style="text-align: x"` where cmark emits `align="x"`.
/// - It writes `"` literally in text where cmark writes `&quot;`. A browser sees the same text.
/// - It writes `<tbody></tbody>` for a header-only table, where the events hold no body rows.
/// The parsed events are identical in these cases, so none is a parse difference.
pub fn semantic(html: &str) -> String {
    let h = html
        .replace("style=\"text-align: left\"", "align=\"left\"")
        .replace("style=\"text-align: center\"", "align=\"center\"")
        .replace("style=\"text-align: right\"", "align=\"right\"")
        .replace("<tbody></tbody>", "");
    let mut out = String::with_capacity(h.len());
    let mut in_tag = false;
    for c in h.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {}
        }
        if c == '"' && !in_tag {
            out.push_str("&quot;");
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalize_doc_examples() {
        assert_eq!(normalize("<p>a  \t b</p>"), "<p>a b</p>");
        assert_eq!(normalize("<p>a  \t\nb</p>"), "<p>a b</p>");
        assert_eq!(normalize(" <p>a  b</p>"), "<p>a b</p>");
        assert_eq!(normalize("<p>a  b</p> "), "<p>a b</p>");
        assert_eq!(normalize("\n\t<p>\n\t\ta  b\t\t</p>\n\t"), "<p>a b</p>");
        assert_eq!(normalize("<i>a  b</i> "), "<i>a b</i> ");
        assert_eq!(normalize("<br />"), "<br>");
        assert_eq!(normalize("<a title=\"bar\" HREF=\"foo\">x</a>"), "<a href=\"foo\" title=\"bar\">x</a>");
        assert_eq!(normalize("&amp;&gt;&lt;&quot;"), "&amp;&gt;&lt;&quot;");
    }
    #[test]
    fn normalize_keeps_differences() {
        assert_ne!(normalize("<p>a</p>"), normalize("<p>b</p>"));
        assert_ne!(normalize("<pre>a  b</pre>"), normalize("<pre>a b</pre>"));
        assert_ne!(normalize("<a href=\"x\">y</a>"), normalize("<a href=\"z\">y</a>"));
    }
    #[test]
    fn generator_is_deterministic() {
        assert_eq!(generate(7, 20_000), generate(7, 20_000));
    }
    #[test]
    fn autolink_skips_code_and_links() {
        let h = html_pulldown("`http://a.b` [http://c.d](x) http://e.f\n", Options::empty(), true);
        assert_eq!(h, "<p><code>http://a.b</code> <a href=\"x\">http://c.d</a> <a href=\"http://e.f\">http://e.f</a></p>\n");
    }
}
