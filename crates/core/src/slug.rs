//! Heading anchors as GitHub makes them: its filter for which characters stay, and github-slugger's
//! rule for duplicates.

use std::collections::HashMap;

/// Assigns unique slugs in document order.
#[derive(Default)]
pub struct Slugger {
    /// Every slug given out, with the last suffix tried for it as a base. Resuming from there keeps
    /// a long run of equal headings linear; "a", "a", "a-1" still gives a, a-1, a-1-1.
    used: HashMap<String, u32>,
}

impl Slugger {
    /// `title` is the heading's text with its original whitespace.
    pub fn slug(&mut self, title: &str) -> String {
        let base = slug_base(title);
        let Some(&last) = self.used.get(&base) else {
            self.used.insert(base.clone(), 0);
            return base;
        };
        let mut n = last;
        let candidate = loop {
            n += 1;
            let candidate = format!("{base}-{n}");
            if !self.used.contains_key(&candidate) {
                break candidate;
            }
        };
        self.used.insert(base, n);
        self.used.insert(candidate.clone(), 0);
        candidate
    }
}

/// Lowercases, turns each space into `-`, and drops everything but word characters and `-`. So a
/// newline or tab goes away instead of becoming `-`. Lowercasing one character at a time avoids
/// `str::to_lowercase`'s final-sigma rule, which GitHub does not apply.
fn slug_base(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.chars().flat_map(char::to_lowercase) {
        if c == ' ' {
            out.push('-');
        } else if c == '-' || is_word(c) {
            out.push(c);
        }
    }
    out
}

/// GitHub's `\p{Word}`, which per UTS 18 is Alphabetic, marks, decimal digits, connector punctuation
/// and the two joiners. std cannot express that, and scripts like Persian and Myanmar rely on it.
fn is_word(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphanumeric() || c == '_'
    } else {
        regex_syntax::is_word_character(c)
    }
}
