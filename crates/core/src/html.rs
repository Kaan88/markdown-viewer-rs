//! Allow-list HTML tokenizer. It only recognises tags; nothing here executes or loads anything.

use memchr::{memchr, memmem};
use pulldown_cmark::{Event, Parser};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token<'a> {
    Text(&'a str),
    Open {
        name: &'a str,
        attrs: &'a str,
        self_closing: bool,
        raw: &'a str,
    },
    Close {
        name: &'a str,
        raw: &'a str,
    },
    /// Comment, processing instruction, declaration or CDATA.
    Ignored(&'a str),
}

impl<'a> Token<'a> {
    pub fn raw(&self) -> &'a str {
        match *self {
            Token::Text(s) | Token::Ignored(s) => s,
            Token::Open { raw, .. } | Token::Close { raw, .. } => raw,
        }
    }
}

/// Linear in its input: a construct with no terminator consumes the rest in one token.
pub struct Tokenizer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(src: &'a str) -> Self {
        Tokenizer { src, pos: 0 }
    }

    fn take(&mut self, len: usize) -> &'a str {
        let s = &self.src[self.pos..self.pos + len];
        self.pos += len;
        s
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        let rest = &self.src[self.pos..];
        if rest.is_empty() {
            return None;
        }
        let b = rest.as_bytes();
        if b[0] != b'<' {
            let len = memchr(b'<', b).unwrap_or(b.len());
            return Some(Token::Text(self.take(len)));
        }
        let terminated = |from: usize, end: &[u8]| {
            memmem::find(&b[from.min(b.len())..], end).map_or(b.len(), |i| from + i + end.len())
        };
        match b.get(1).copied() {
            Some(b'!') if b[2..].starts_with(b"--") => {
                let len = terminated(2, b"-->");
                Some(Token::Ignored(self.take(len)))
            }
            Some(b'!') if b[2..].starts_with(b"[CDATA[") => {
                let len = terminated(9, b"]]>");
                Some(Token::Ignored(self.take(len)))
            }
            Some(b'!') if b.get(2).is_some_and(u8::is_ascii_alphabetic) => {
                let len = terminated(2, b">");
                Some(Token::Ignored(self.take(len)))
            }
            Some(b'?') => {
                let len = terminated(2, b"?>");
                Some(Token::Ignored(self.take(len)))
            }
            Some(b'/') if b.get(2).is_some_and(u8::is_ascii_alphabetic) => {
                let name_end = name_end(b, 2);
                match memchr(b'>', &b[name_end..]) {
                    Some(i) => {
                        let name = &rest[2..name_end];
                        let raw = self.take(name_end + i + 1);
                        Some(Token::Close { name, raw })
                    }
                    None => Some(Token::Text(self.take(b.len()))),
                }
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let name_end = name_end(b, 1);
                match tag_end(b, name_end) {
                    Some(gt) => {
                        let name = &rest[1..name_end];
                        let self_closing = b[gt - 1] == b'/';
                        let attrs = &rest[name_end..if self_closing { gt - 1 } else { gt }];
                        let raw = self.take(gt + 1);
                        Some(Token::Open {
                            name,
                            attrs,
                            self_closing,
                            raw,
                        })
                    }
                    None => Some(Token::Text(self.take(b.len()))),
                }
            }
            _ => Some(Token::Text(self.take(1))),
        }
    }
}

fn name_end(b: &[u8], from: usize) -> usize {
    b[from..]
        .iter()
        .position(|&c| c.is_ascii_whitespace() || c == b'/' || c == b'>')
        .map_or(b.len(), |i| from + i)
}

/// Index of the `>` ending a start tag, honouring quoted attribute values.
fn tag_end(b: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    let mut quote = None;
    let mut after_equals = false;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'>' => return Some(i),
                b'"' | b'\'' if after_equals => quote = Some(c),
                _ => {}
            },
        }
        if quote.is_none() && !c.is_ascii_whitespace() {
            after_equals = c == b'=';
        }
        i += 1;
    }
    None
}

/// Iterates `name[=value]` pairs of a start tag's attribute text. Values are returned raw.
pub struct Attrs<'a> {
    rest: &'a str,
}

pub fn attrs(text: &str) -> Attrs<'_> {
    Attrs { rest: text }
}

impl<'a> Iterator for Attrs<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<(&'a str, &'a str)> {
        loop {
            self.rest = self
                .rest
                .trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
            if self.rest.is_empty() {
                return None;
            }
            let e = self
                .rest
                .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/')
                .unwrap_or(self.rest.len());
            if e == 0 {
                // A stray '=' with no name. Skip it so the loop always advances.
                self.rest = &self.rest[1..];
                continue;
            }
            let name = &self.rest[..e];
            let after = self.rest[e..].trim_start_matches(|c: char| c.is_ascii_whitespace());
            let Some(v) = after.strip_prefix('=') else {
                self.rest = after;
                return Some((name, ""));
            };
            let v = v.trim_start_matches(|c: char| c.is_ascii_whitespace());
            let (value, rest) = match v.as_bytes().first() {
                Some(&q @ (b'"' | b'\'')) => match memchr(q, &v.as_bytes()[1..]) {
                    Some(close) => (&v[1..1 + close], &v[close + 2..]),
                    None => (&v[1..], ""),
                },
                _ => {
                    let e = v.find(|c: char| c.is_ascii_whitespace()).unwrap_or(v.len());
                    (&v[..e], &v[e..])
                }
            };
            self.rest = rest;
            return Some((name, value));
        }
    }
}

/// First value of a case-insensitive attribute, entities decoded.
pub fn attr<'a>(text: &'a str, name: &str) -> Option<Cow<'a, str>> {
    attrs(text)
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| decode_entities(v))
}

/// Decodes character references. Unknown ones stay literal.
pub fn decode_entities(s: &str) -> Cow<'_, str> {
    let Some(first) = memchr(b'&', s.as_bytes()) else {
        return Cow::Borrowed(s);
    };
    let mut out = String::with_capacity(s.len());
    out.push_str(&s[..first]);
    let mut rest = &s[first..];
    while let Some(tail) = rest.strip_prefix('&') {
        let semi = tail.as_bytes().iter().take(34).position(|&c| c == b';');
        let decoded = semi
            .filter(|&e| e > 0)
            .and_then(|e| decode_reference(&tail[..e], &mut out).then_some(e));
        match decoded {
            Some(e) => rest = &tail[e + 1..],
            None => {
                out.push('&');
                rest = tail;
            }
        }
        let next = memchr(b'&', rest.as_bytes()).unwrap_or(rest.len());
        out.push_str(&rest[..next]);
        rest = &rest[next..];
    }
    Cow::Owned(out)
}

fn decode_reference(body: &str, out: &mut String) -> bool {
    if let Some(num) = body.strip_prefix('#') {
        let cp = match num.strip_prefix(['x', 'X']) {
            Some(hex) if !hex.is_empty() && hex.bytes().all(|c| c.is_ascii_hexdigit()) => {
                u32::from_str_radix(hex, 16).ok()
            }
            None if !num.is_empty() && num.bytes().all(|c| c.is_ascii_digit()) => num.parse().ok(),
            _ => return false,
        };
        let c = cp
            .filter(|&n| n != 0)
            .and_then(char::from_u32)
            .unwrap_or('\u{FFFD}');
        out.push(c);
        return true;
    }
    if !body.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    let simple = match body {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        _ => {
            // pulldown-cmark holds the full entity table; asking it avoids a second copy in the binary.
            let probe = format!("&{body};");
            let mut text = String::new();
            for ev in Parser::new(&probe) {
                if let Event::Text(t) = ev {
                    text.push_str(&t);
                }
            }
            if text == probe {
                return false;
            }
            out.push_str(&text);
            return true;
        }
    };
    out.push_str(simple);
    true
}

/// Schemes allowed on raw HTML `href`. Anything else, `javascript:` included, loses its destination.
const LINK_SCHEMES: &[&str] = &["http", "https", "mailto", "file"];
/// As above for `src`, which may also be inline data.
const IMAGE_SCHEMES: &[&str] = &["http", "https", "file", "data"];

pub fn safe_link_url(url: &str) -> bool {
    scheme_allowed(url, LINK_SCHEMES)
}

pub fn safe_image_url(url: &str) -> bool {
    scheme_allowed(url, IMAGE_SCHEMES)
}

fn scheme_allowed(url: &str, allowed: &[&str]) -> bool {
    // Browsers strip these before reading a scheme, so "java\nscript:" must be caught too.
    let cleaned: String = url
        .chars()
        .filter(|c| !c.is_ascii_control() && *c != ' ')
        .take(32)
        .collect();
    let Some(colon) = cleaned.find(':') else {
        return true;
    };
    if cleaned[..colon].contains(['/', '?', '#']) {
        return true;
    }
    allowed
        .iter()
        .any(|s| cleaned[..colon].eq_ignore_ascii_case(s))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tag {
    P,
    Section,
    Center,
    Div,
    H(u8),
    Blockquote,
    Ul,
    Ol,
    Li,
    Table,
    TableSection,
    Tr,
    Td,
    Th,
    Details,
    Summary,
    Pre,
    Hr,
    A,
    Img,
    Br,
    Input,
    Picture,
    Source,
    Strong,
    Em,
    Underline,
    Strike,
    Code,
    Kbd,
    Sub,
    Sup,
    Mark,
    Small,
    /// `<q>`: a short quotation, drawn between curly quotes as browsers do.
    Q,
    /// `<ruby>` and `<rt>` are spans; `<rp>` is the fallback for a browser with no ruby and is
    /// dropped along with its text.
    Ruby,
    Rp,
    Rt,
    Dt,
    Dd,
    Figure,
    FigCaption,
    /// GFM tagfilter names that show as literal text.
    Literal,
    Unknown,
}

impl Tag {
    pub(crate) const SLOTS: usize = 49;

    /// A distinct small index per element the builder keeps open.
    pub(crate) fn slot(self) -> usize {
        match self {
            Tag::P => 0,
            Tag::Section => 1,
            Tag::Center => 2,
            Tag::Div => 3,
            Tag::H(level) => 3 + usize::from(level.min(6)),
            Tag::Blockquote => 10,
            Tag::Ul => 11,
            Tag::Ol => 12,
            Tag::Li => 13,
            Tag::Table => 14,
            Tag::TableSection => 15,
            Tag::Tr => 16,
            Tag::Td => 17,
            Tag::Th => 18,
            Tag::Details => 19,
            Tag::Summary => 20,
            Tag::Pre => 21,
            Tag::Hr => 22,
            Tag::A => 23,
            Tag::Img => 24,
            Tag::Br => 25,
            Tag::Input => 26,
            Tag::Picture => 27,
            Tag::Source => 28,
            Tag::Strong => 29,
            Tag::Em => 30,
            Tag::Underline => 31,
            Tag::Strike => 32,
            Tag::Code => 33,
            Tag::Kbd => 34,
            Tag::Sub => 35,
            Tag::Sup => 36,
            Tag::Mark => 37,
            Tag::Literal => 38,
            Tag::Unknown => 39,
            Tag::Small => 40,
            Tag::Q => 41,
            Tag::Ruby => 42,
            Tag::Rp => 43,
            Tag::Rt => 44,
            Tag::Dt => 45,
            Tag::Dd => 46,
            Tag::Figure => 47,
            Tag::FigCaption => 48,
        }
    }

    /// Whether this element stays open and so adds a level of nesting.
    pub(crate) fn nests(self) -> bool {
        !matches!(
            self,
            Tag::Hr
                | Tag::Img
                | Tag::Br
                | Tag::Input
                | Tag::Picture
                | Tag::Source
                | Tag::TableSection
                | Tag::Unknown
                | Tag::Rp
        )
    }
}

/// Tags GitHub's sanitizer keeps but this model has no element for. They draw nothing, yet an alert
/// whose body holds one still counts as an alert on GitHub, so this is only asked about there.
pub(crate) fn is_inert_tag(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "span"
            | "font"
            | "abbr"
            | "bdo"
            | "q"
            | "rp"
            | "rt"
            | "ruby"
            | "small"
            | "time"
            | "wbr"
            | "hgroup"
            | "col"
            | "colgroup"
            | "data"
    )
}

pub(crate) fn classify_tag(name: &str) -> Tag {
    let mut buf = [0u8; 12];
    if name.len() > buf.len() {
        return Tag::Unknown;
    }
    buf[..name.len()].copy_from_slice(name.as_bytes());
    buf.make_ascii_lowercase();
    match &buf[..name.len()] {
        b"p" => Tag::P,
        b"div" => Tag::Div,
        b"center" => Tag::Center,
        b"section" | b"article" | b"aside" | b"header" | b"footer" | b"main" | b"nav"
        | b"address" | b"caption" | b"dl" => Tag::Section,
        b"dt" => Tag::Dt,
        b"dd" => Tag::Dd,
        b"figure" => Tag::Figure,
        b"figcaption" => Tag::FigCaption,
        b"h1" => Tag::H(1),
        b"h2" => Tag::H(2),
        b"h3" => Tag::H(3),
        b"h4" => Tag::H(4),
        b"h5" => Tag::H(5),
        b"h6" => Tag::H(6),
        b"blockquote" => Tag::Blockquote,
        b"ul" => Tag::Ul,
        b"ol" => Tag::Ol,
        b"li" => Tag::Li,
        b"table" => Tag::Table,
        b"thead" | b"tbody" | b"tfoot" => Tag::TableSection,
        b"tr" => Tag::Tr,
        b"td" => Tag::Td,
        b"th" => Tag::Th,
        b"details" => Tag::Details,
        b"summary" => Tag::Summary,
        b"pre" => Tag::Pre,
        b"hr" => Tag::Hr,
        b"a" => Tag::A,
        b"img" => Tag::Img,
        b"br" => Tag::Br,
        b"input" => Tag::Input,
        b"picture" => Tag::Picture,
        b"source" => Tag::Source,
        b"b" | b"strong" => Tag::Strong,
        b"i" | b"em" | b"var" | b"cite" | b"dfn" => Tag::Em,
        b"u" | b"ins" => Tag::Underline,
        b"s" | b"del" | b"strike" => Tag::Strike,
        b"code" | b"tt" | b"samp" => Tag::Code,
        b"kbd" => Tag::Kbd,
        b"sub" => Tag::Sub,
        b"sup" => Tag::Sup,
        b"mark" => Tag::Mark,
        b"small" => Tag::Small,
        b"q" => Tag::Q,
        b"ruby" => Tag::Ruby,
        b"rp" => Tag::Rp,
        b"rt" => Tag::Rt,
        // GitHub's tag filter escapes these tags, so they show as text and their content is ordinary
        // HTML by the time the browser sees it. Nothing here executes either way: the model holds
        // plain text, not markup.
        b"script" | b"style" | b"iframe" | b"title" | b"textarea" | b"xmp" | b"noembed"
        | b"noframes" | b"plaintext" => Tag::Literal,
        // GitHub strips these tags and keeps their children, so they are treated as unknown.
        b"object" | b"embed" | b"head" | b"template" => Tag::Unknown,
        _ => Tag::Unknown,
    }
}
