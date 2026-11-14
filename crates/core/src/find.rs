//! Plain case-insensitive find over the document text.

use crate::limits::{FIND_CAP, FIND_WORK_PER_BYTE};
use crate::model::{Document, TextRange};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindResult {
    pub hits: Vec<TextRange>,
    /// True when a limit stopped the search, so the real count may be higher: `FIND_CAP` hits, or
    /// `FIND_WORK_PER_BYTE` spent on a very long query against text that nearly matches it everywhere.
    pub capped: bool,
}

/// `find_visible` with every `<details>` as the document wrote it.
pub fn find(doc: &Document, query: &str) -> FindResult {
    find_visible(doc, query, |b| doc.details_open_as_written(b))
}

/// Finds non-overlapping matches, up to `FIND_CAP`, in the text the reader can see: not in source the
/// engine draws over, and not in the body of a closed `<details>`, which is what the old app counted
/// too. `is_open` is the shell's view state, so toggling a details block means running find again.
pub fn find_visible(doc: &Document, query: &str, is_open: impl Fn(u32) -> bool) -> FindResult {
    find_shown(doc, query, is_open, &[])
}

/// [`find_visible`], told which formulas and diagrams turned out not to be drawn: `shown` is
/// their sorted `(start, end)` ranges in `Document::text`, as the renderer reports them. Their
/// source is on the page, so it is searched like any other text.
pub fn find_shown(
    doc: &Document,
    query: &str,
    is_open: impl Fn(u32) -> bool,
    shown: &[(u32, u32)],
) -> FindResult {
    let mut hidden = doc.hidden_ranges(&is_open);
    if !shown.is_empty() {
        hidden.retain(|r| shown.binary_search(&(r.start, r.end)).is_err());
    }
    find_with(&doc.text, query, FIND_CAP, |start| {
        let i = hidden.partition_point(|r| r.end <= start);
        hidden.get(i).is_some_and(|r| r.start <= start)
    })
}

pub fn find_in_text(text: &str, query: &str, cap: usize) -> FindResult {
    find_with(text, query, cap, |_| false)
}

fn find_with(text: &str, query: &str, cap: usize, hidden: impl Fn(u32) -> bool) -> FindResult {
    find_folded(text, &fold_query(query), cap, hidden)
}

/// A finder for one query to run on many short pieces of text (the lines of the files of a folder):
/// the byte ranges of its matches in a line, at most `cap`, found as the find in the page finds
/// them. The query is folded once, here.
pub fn line_matcher(
    query: &str,
) -> impl Fn(&str, usize) -> Vec<std::ops::Range<usize>> + Send + use<> {
    let query = fold_query(query);
    move |line: &str, cap: usize| {
        find_folded(line, &query, cap, |_| false)
            .hits
            .into_iter()
            .map(TextRange::usize)
            .collect()
    }
}

/// The query as the characters it is compared as.
fn fold_query(query: &str) -> Vec<char> {
    // Blocks are separated by newlines in the text, so a line break typed or pasted into the query
    // must look for a space to be able to match wrapped text at all.
    // A CRLF pair is one break, not two: Windows clipboard text would otherwise become two spaces
    // and never match wrapped text.
    let mut chars = query.chars().peekable();
    let mut folded = Vec::new();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                folded.push(' ');
            }
            '\n' => folded.push(' '),
            _ => folded.push(fold(c)),
        }
    }
    folded
}

fn find_folded(text: &str, query: &[char], cap: usize, hidden: impl Fn(u32) -> bool) -> FindResult {
    let mut result = FindResult::default();
    let Some(&first) = query.first() else {
        return result;
    };
    let needles = ascii_needles(first);
    let bytes = text.as_bytes();
    let budget = bytes.len().saturating_mul(FIND_WORK_PER_BYTE);
    let mut work = 0;
    let mut pos = 0;
    while pos < bytes.len() {
        if let Some((a, b)) = needles {
            match memchr::memchr2(a, b, &bytes[pos..]) {
                Some(i) => pos += i,
                None => break,
            }
        }
        let end = match match_at(text, pos, query) {
            Ok(end) => end,
            Err(compared) => {
                work += compared;
                if work > budget {
                    result.capped = true;
                    break;
                }
                pos += utf8_len(bytes[pos]);
                continue;
            }
        };
        if !hidden(pos as u32) {
            if result.hits.len() == cap {
                result.capped = true;
                break;
            }
            result.hits.push(TextRange::new(pos, end));
        }
        pos = end;
    }
    result
}

/// Byte values a query's first character can start with, when that is a safe shortcut. Letters with
/// non-ASCII case partners (Kelvin sign, long s, Turkish i forms) have to walk every character. A
/// space also matches U+00A0, whose UTF-8 starts with 0xC2.
fn ascii_needles(first: char) -> Option<(u8, u8)> {
    if first == ' ' {
        return Some((b' ', 0xC2));
    }
    if !first.is_ascii() || matches!(first, 'i' | 'I' | 'k' | 's' | 'K' | 'S') {
        return None;
    }
    let lower = first as u8;
    Some((lower, lower.to_ascii_uppercase()))
}

fn utf8_len(b: u8) -> usize {
    match b {
        0..=0xBF => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// The end of a match at `pos`, or else how many characters were compared to find there is none.
fn match_at(text: &str, pos: usize, query: &[char]) -> Result<usize, usize> {
    let mut chars = text[pos..].char_indices();
    let mut end = pos;
    for (n, &q) in query.iter().enumerate() {
        let Some((i, c)) = chars.next() else {
            return Err(n);
        };
        if !chars_match(q, fold(c)) {
            return Err(n + 1);
        }
        end = pos + i + c.len_utf8();
    }
    Ok(end)
}

/// Simple case folding, leaving the four Turkish i forms distinct so `chars_match` can compare them.
/// A non-breaking space reads as a space, since nobody can tell them apart when typing a query.
fn fold(c: char) -> char {
    if c.is_ascii() {
        return if c == 'I' {
            'I'
        } else {
            c.to_ascii_lowercase()
        };
    }
    match c {
        'İ' | 'ı' => c,
        'ς' => 'σ',
        'ſ' => 's',
        '\u{A0}' => ' ',
        _ => {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) => l,
                _ => c,
            }
        }
    }
}

/// Two letters match when they are case partners under either root or Turkish casing: I/i in English,
/// I/ı and İ/i in Turkish. The relation is deliberately not transitive, so `ı` does not find `i`.
fn chars_match(a: char, b: char) -> bool {
    a == b
        || matches!(
            (a, b),
            ('i', 'I') | ('I', 'i') | ('i', 'İ') | ('İ', 'i') | ('I', 'ı') | ('ı', 'I')
        )
}

#[cfg(test)]
mod line_matcher_tests {
    use super::*;

    #[test]
    fn a_line_matcher_finds_what_the_page_find_finds() {
        let m = line_matcher("Needle");
        assert_eq!(m("a needle, a NEEDLE", 16), vec![2..8, 12..18]);
        assert_eq!(m("a needle, a NEEDLE", 1), vec![2..8]);
        assert!(m("nothing", 16).is_empty());
        // Turkish dotted and dotless i as in the page's find.
        assert_eq!(line_matcher("İş")("bir iş", 4), vec![4..7]);
        assert!(line_matcher("")("text", 4).is_empty());
        let page = find_in_text("a needle, a NEEDLE", "Needle", 16);
        let ranges: Vec<_> = page.hits.iter().map(|h| h.usize()).collect();
        assert_eq!(ranges, m("a needle, a NEEDLE", 16));
    }
}
