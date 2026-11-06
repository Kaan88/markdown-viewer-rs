//! Rust port of cmark-gfm's test/normalize.py, plus the writer-form rewrite applied to both sides.

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
    "article",
    "header",
    "aside",
    "hgroup",
    "blockquote",
    "hr",
    "iframe",
    "body",
    "li",
    "map",
    "button",
    "object",
    "canvas",
    "ol",
    "caption",
    "output",
    "col",
    "p",
    "colgroup",
    "pre",
    "dd",
    "progress",
    "div",
    "section",
    "dl",
    "table",
    "td",
    "dt",
    "tbody",
    "embed",
    "textarea",
    "fieldset",
    "tfoot",
    "figcaption",
    "th",
    "figure",
    "thead",
    "footer",
    "tr",
    "form",
    "ul",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "video",
    "script",
    "style",
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
            d = if self.last == Last::Start {
                d.trim_start_matches(py_ws)
            } else {
                d.trim_matches(py_ws)
            };
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
                e > 0
                    && tail[..e]
                        .chars()
                        .enumerate()
                        .all(|(i, c)| c.is_ascii_alphanumeric() || (i == 0 && c == '#'))
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
        match tail
            .find(';')
            .and_then(|e| decode_ref(&tail[..e]).map(|c| (e, c)))
        {
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
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
/// Parses `<name attrs>`; None means "not a tag", which is then kept as verbatim data.
#[allow(clippy::type_complexity)]
fn parse_start(chunk: &str) -> Option<(String, Vec<(String, Option<String>)>, bool)> {
    let body = &chunk[1..chunk.len() - 1];
    if !body.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let name_end = body
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(body.len());
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
        let e = s
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(s.len());
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
    let mut n = Norm {
        out: String::new(),
        last: Last::Start,
        last_tag: String::new(),
        in_pre: false,
    };
    let mut rest = html;
    while !rest.is_empty() {
        if rest.starts_with("<![CDATA[")
            && let Some(e) = rest.find("]]>")
        {
            n.out.push_str(&rest[..e + 3]);
            rest = &rest[e + 3..];
            continue;
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

const INLINE_TAGS: &[&str] = &[
    "a", "strong", "em", "del", "u", "code", "kbd", "sub", "sup", "mark",
];

/// Differences no reader can see, removed from both sides: `"`, `>` and bare `&` may or may not be
/// entity-encoded in text, whitespace at the start of a block or the document collapses away, and a
/// link or paragraph with no content draws nothing.
pub fn semantic(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut at_block_start = true;
    let mut in_pre = false;
    let mut rest = html;
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let end = rest.find('>').map_or(rest.len(), |e| e + 1);
            let tag = &rest[..end];
            let name_start = if tag.starts_with("</") { 2 } else { 1 };
            let body = &tag[name_start..];
            let name = &body[..body
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(body.len())];
            if name == "pre" {
                in_pre = name_start == 1;
            }
            if INLINE_TAGS.contains(&name) {
                if name_start == 2 {
                    at_block_start = false;
                }
            } else {
                at_block_start = !matches!(name, "img" | "br" | "input");
            }
            out.push_str(tag);
            rest = &rest[end..];
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let mut text = &rest[..end];
            rest = &rest[end..];
            if at_block_start && !in_pre {
                text = text.trim_start();
            }
            if !text.is_empty() {
                at_block_start = false;
            }
            push_text(text, &mut out);
        }
    }
    let mut out = without_empty(&without_empty(&out, "<a ", "</a>"), "<p", "</p>");
    out.truncate(out.trim_end().len());
    out
}

fn without_empty(html: &str, open: &str, close: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find(open) {
        let tag_end = rest[at..].find('>').map(|e| at + e + 1);
        match tag_end.filter(|&e| rest[e..].starts_with(close)) {
            Some(e) => {
                out.push_str(&rest[..at]);
                rest = &rest[e + close.len()..];
            }
            None => {
                out.push_str(&rest[..at + open.len()]);
                rest = &rest[at + open.len()..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn push_text(text: &str, out: &mut String) {
    for (i, c) in text.char_indices() {
        match c {
            '"' => out.push_str("&quot;"),
            '>' => out.push_str("&gt;"),
            '&' if !starts_reference(&text[i + 1..]) => out.push_str("&amp;"),
            c => out.push(c),
        }
    }
}

fn starts_reference(s: &str) -> bool {
    s.find(';').is_some_and(|e| {
        e > 0
            && s[..e]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '#')
    })
}
