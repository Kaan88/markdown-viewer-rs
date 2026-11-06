//! GFM extended autolinks: `www.`, `http(s)://`, `ftp://`, emails, `mailto:` and `xmpp:`.

use memchr::memmem;
use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LinkKind {
    Www,
    Url,
    Email,
    /// The text already carries its `mailto:` or `xmpp:` scheme.
    SchemedEmail,
}

impl LinkKind {
    pub fn destination(self, text: &str) -> String {
        match self {
            LinkKind::Www => format!("http://{text}"),
            LinkKind::Url | LinkKind::SchemedEmail => text.to_string(),
            LinkKind::Email => format!("mailto:{text}"),
        }
    }
}

const MAX_DOMAIN: usize = 255;

/// Byte length of a valid domain at the head of `s`. The cap keeps repeated failing scans linear.
fn domain_len(s: &str, allow_short: bool) -> Option<usize> {
    let (mut dots, mut us1, mut us2, mut end) = (0, false, false, 0);
    for (i, c) in s.char_indices() {
        if i > MAX_DOMAIN {
            return None;
        }
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

/// Named entities in a run of text that has been decoded, as text ranges. GitHub autolinks the raw
/// source, so such a URL still ends where the entity's `&name;` spelling would have been trimmed.
#[derive(Clone, Copy)]
pub(crate) struct Entities<'a> {
    ranges: &'a [(usize, usize)],
    /// Where the slice being trimmed starts in the same coordinates.
    base: usize,
}

impl<'a> Entities<'a> {
    const NONE: Entities<'static> = Entities {
        ranges: &[],
        base: 0,
    };

    pub(crate) fn new(ranges: &'a [(usize, usize)], base: usize) -> Self {
        Entities { ranges, base }
    }

    fn shifted(self, by: usize) -> Self {
        Entities {
            base: self.base + by,
            ..self
        }
    }

    /// The start of an entity that ends at `end`.
    fn ending_at(self, end: usize) -> Option<usize> {
        if self.ranges.is_empty() {
            return None;
        }
        let at = end + self.base;
        let i = self.ranges.binary_search_by_key(&at, |r| r.1).ok()?;
        self.ranges[i].0.checked_sub(self.base)
    }
}

/// GFM trailing-punctuation rules: returns the trimmed byte length.
fn trim_end(s: &str, entities: Entities<'_>) -> usize {
    let b = s.as_bytes();
    let mut end = b.len();
    let mut excess = None;
    while end > 0 {
        if let Some(start) = entities.ending_at(end) {
            end = start;
            continue;
        }
        match b[end - 1] {
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => end -= 1,
            b';' => {
                let mut i = end - 1;
                while i > 0 && b[i - 1].is_ascii_alphabetic() {
                    i -= 1;
                }
                if i > 0 && i < end - 1 && b[i - 1] == b'&' {
                    end = i - 1;
                } else {
                    end -= 1;
                }
            }
            b')' => {
                let unbalanced = excess.get_or_insert_with(|| {
                    let body = &b[..end];
                    let close = body.iter().filter(|&&c| c == b')').count() as isize;
                    let open = body.iter().filter(|&&c| c == b'(').count() as isize;
                    close - open
                });
                if *unbalanced > 0 {
                    *unbalanced -= 1;
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

fn url_tail(s: &str, domain: usize, entities: Entities<'_>) -> usize {
    let rest = &s[domain..];
    let stop = rest
        .find(|c: char| c.is_whitespace() || c == '<')
        .map_or(s.len(), |i| domain + i);
    trim_end(&s[..stop], entities)
}

/// Finds extended autolinks in one run of text.
pub fn find_links(t: &str) -> Vec<(usize, usize, LinkKind)> {
    let mut out = Vec::new();
    find_links_into(t, 0, Entities::NONE, &mut out);
    out
}

pub(crate) fn find_links_into(
    t: &str,
    base: usize,
    entities: Entities<'_>,
    out: &mut Vec<(usize, usize, LinkKind)>,
) {
    let b = t.as_bytes();
    let (mut i, mut floor) = (0, 0);
    while i < b.len() {
        let hit = match b[i] {
            b'w' if b[i..].starts_with(b"www.") && www_boundary(t, i) => domain_len(&t[i..], false)
                .map(|d| {
                    let tail = url_tail(&t[i..], d, entities.shifted(i));
                    (i, i + tail, LinkKind::Www)
                }),
            b':' if b[i..].starts_with(b"://") => {
                let mut st = i;
                while st > floor && b[st - 1].is_ascii_alphabetic() {
                    st -= 1;
                }
                if is_url_scheme(&b[st..i]) {
                    domain_len(&t[i + 3..], true).map(|d| {
                        let tail = url_tail(&t[i + 3..], d, entities.shifted(i + 3));
                        (st, i + 3 + tail, LinkKind::Url)
                    })
                } else {
                    None
                }
            }
            b'@' => email_at(t, i, floor),
            _ => None,
        };
        match hit {
            Some((s, e, k)) if e > s => {
                out.push((base + s, base + e, k));
                i = e;
                floor = e;
            }
            _ => i += 1,
        }
    }
}

fn www_boundary(t: &str, i: usize) -> bool {
    i == 0
        || t[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_whitespace() || "*_~(".contains(c))
}

fn is_url_scheme(s: &[u8]) -> bool {
    s.eq_ignore_ascii_case(b"http")
        || s.eq_ignore_ascii_case(b"https")
        || s.eq_ignore_ascii_case(b"ftp")
}

fn email_at(t: &str, at: usize, floor: usize) -> Option<(usize, usize, LinkKind)> {
    let b = t.as_bytes();
    let mut st = at;
    while st > floor && (b[st - 1].is_ascii_alphanumeric() || b".+-_".contains(&b[st - 1])) {
        st -= 1;
    }
    let (mut e, mut dots) = (at + 1, 0);
    while e < b.len() {
        let c = b[e];
        if c == b'.' && e + 1 < b.len() && b[e + 1].is_ascii_alphanumeric() {
            dots += 1;
        } else if !(c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
            break;
        }
        e += 1;
    }
    if st >= at || dots == 0 || !b[e - 1].is_ascii_alphabetic() {
        return None;
    }
    let scheme =
        |len: usize, name: &[u8]| st >= floor + len && b[st - len..st].eq_ignore_ascii_case(name);
    if scheme(7, b"mailto:") {
        return Some((st - 7, e, LinkKind::SchemedEmail));
    }
    if scheme(5, b"xmpp:") {
        if e < b.len() && b[e] == b'/' {
            e += 1;
            while e < b.len() && (b[e].is_ascii_alphanumeric() || b[e] == b'@' || b[e] == b'.') {
                e += 1;
            }
            while b[e - 1] == b'.' {
                e -= 1;
            }
        }
        return Some((st - 5, e, LinkKind::SchemedEmail));
    }
    Some((st, e, LinkKind::Email))
}

pub(crate) fn may_contain_link(t: &str) -> bool {
    let b = t.as_bytes();
    memchr::memchr(b'@', b).is_some()
        || memmem::find(b, b"://").is_some()
        || memmem::find(b, b"www.").is_some()
}

// Delimiters inside a bare URL would otherwise start emphasis in the parser and cut the link short.
// Before parsing they are swapped for private-use characters, which CommonMark treats as ordinary
// letters and accepts in link destinations, and swapped back wherever text leaves the parser.
// Offsets stay meaningful because the swapped string is the source of record for the whole parse.
const SWAPS: [(u8, char); 3] = [(b'_', '\u{E000}'), (b'*', '\u{E001}'), (b'~', '\u{E002}')];
const SWAP_PREFIX: &[u8] = b"\xEE\x80";

/// Returns the source with delimiters inside bare URLs protected. Borrowed when there is nothing to
/// do, which includes a document that already uses private-use characters from this block, since
/// `restore` would change those too.
pub(crate) fn protect_url_delimiters(src: &str) -> Cow<'_, str> {
    let b = src.as_bytes();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut note = |start: usize, end: usize| {
        if b[start..end]
            .iter()
            .any(|c| SWAPS.iter().any(|(from, _)| from == c))
        {
            ranges.push((start, end));
        }
    };
    let mut cursor = 0;
    for at in memmem::find_iter(b, b"://") {
        if at < cursor {
            continue;
        }
        let mut st = at;
        while st > cursor && b[st - 1].is_ascii_alphabetic() {
            st -= 1;
        }
        if is_url_scheme(&b[st..at]) && domain_len(&src[at + 3..], true).is_some() {
            let end = at + 3 + url_extent(&src[at + 3..]);
            note(at + 3, end);
            cursor = end;
        }
    }
    cursor = 0;
    for at in memmem::find_iter(b, b"www.") {
        if at >= cursor && www_boundary(src, at) && domain_len(&src[at..], false).is_some() {
            let end = at + url_extent(&src[at..]);
            note(at, end);
            cursor = end;
        }
    }
    if ranges.is_empty() || memmem::find(b, SWAP_PREFIX).is_some() {
        return Cow::Borrowed(src);
    }
    ranges.sort_unstable();
    let mut out = String::with_capacity(src.len() + ranges.len() * 8);
    let mut pos = 0;
    for (start, end) in ranges {
        if start < pos {
            continue;
        }
        out.push_str(&src[pos..start]);
        for c in src[start..end].chars() {
            match SWAPS.iter().find(|(from, _)| u32::from(*from) == c as u32) {
                Some(&(_, to)) => out.push(to),
                None => out.push(c),
            }
        }
        pos = end;
    }
    out.push_str(&src[pos..]);
    Cow::Owned(out)
}

/// How far a URL runs in raw source. Stops at whatever Markdown syntax would end it first: an
/// unbalanced closing bracket ends a link destination, a pipe a table cell, a backslash is an escape.
fn url_extent(s: &str) -> usize {
    let b = s.as_bytes();
    let (mut parens, mut brackets) = (0i32, 0i32);
    let mut stop = b.len();
    for (i, &c) in b.iter().enumerate() {
        match c {
            b' ' | b'\t' | b'\n' | b'\r' | b'<' | b'>' | b'|' | b'`' | b'\\' | 0x0b | 0x0c => {
                stop = i;
                break;
            }
            b'(' => parens += 1,
            b'[' => brackets += 1,
            b')' => {
                if parens == 0 {
                    stop = i;
                    break;
                }
                parens -= 1;
            }
            b']' => {
                if brackets == 0 {
                    stop = i;
                    break;
                }
                brackets -= 1;
            }
            _ => {}
        }
    }
    trim_end(&s[..stop], Entities::NONE)
}

/// Stands in for a `$` the parser must not take as a math delimiter. It shares the swaps' prefix, so
/// `restore` brings it back along with them.
pub(crate) const HIDDEN_DOLLAR: char = '\u{E003}';
pub(crate) const HIDDEN_DOLLAR_UTF8: &[u8] = "\u{E003}".as_bytes();
/// The same for the `[` of an alert marker that GitHub would not treat as one.
pub(crate) const HIDDEN_BRACKET: char = '\u{E004}';
pub(crate) const HIDDEN_BRACKET_UTF8: &[u8] = "\u{E004}".as_bytes();

/// Undoes `protect_url_delimiters` and the hidden delimiters on text coming back from the parser.
pub(crate) fn restore(s: &str) -> Cow<'_, str> {
    if memmem::find(s.as_bytes(), SWAP_PREFIX).is_none() {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.chars()
            .map(|c| {
                if c == HIDDEN_DOLLAR {
                    return '$';
                }
                if c == HIDDEN_BRACKET {
                    return '[';
                }
                SWAPS
                    .iter()
                    .find(|(_, to)| *to == c)
                    .map_or(c, |&(from, _)| char::from(from))
            })
            .collect(),
    )
}
