//! Emoji shortcodes such as `:rocket:`, replaced in running text the way GitHub does. The table is
//! GitHub's full gemoji list (`emoji_table.rs`, generated), sorted so a lookup is a binary
//! search; an unknown name stays as typed. Code spans, code blocks and link destinations never
//! reach this.

use std::borrow::Cow;

#[path = "emoji_table.rs"]
mod table;
use table::TABLE;

fn lookup(name: &str) -> Option<&'static str> {
    TABLE
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| TABLE[i].1)
}

/// Replaces every known `:name:` in `text`. Borrows when there is nothing to replace.
pub fn replace(text: &str) -> Cow<'_, str> {
    if !text.contains(':') {
        return Cow::Borrowed(text);
    }
    let mut out: Option<String> = None;
    let mut last = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'+' | b'-'))
            {
                end += 1;
            }
            if end > start
                && end < bytes.len()
                && bytes[end] == b':'
                && let Some(emoji) = lookup(&text[start..end])
            {
                let buf = out.get_or_insert_with(String::new);
                buf.push_str(&text[last..i]);
                buf.push_str(emoji);
                last = end + 1;
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    match out {
        Some(mut buf) => {
            buf.push_str(&text[last..]);
            Cow::Owned(buf)
        }
        None => Cow::Borrowed(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_unique() {
        assert!(
            TABLE
                .windows(2)
                .all(|w| w[0].0.as_bytes() < w[1].0.as_bytes())
        );
        assert!(TABLE.iter().all(|(n, e)| !n.is_empty() && !e.is_empty()));
    }

    #[test]
    fn known_names_are_replaced_and_unknown_ones_kept() {
        assert_eq!(replace("ship it :rocket:!"), "ship it 🚀!");
        assert_eq!(replace(":+1: and :-1:"), "👍 and 👎");
        assert_eq!(
            replace("12:30:45 and :nope: and a:b"),
            "12:30:45 and :nope: and a:b"
        );
        assert_eq!(replace(":tada::tada:"), "🎉🎉");
        assert!(matches!(replace("no colons"), Cow::Borrowed(_)));
    }

    #[test]
    fn the_full_github_list_is_present() {
        assert!(TABLE.len() > 1800);
        assert_eq!(replace(":+1: :100: :e-mail:"), "👍 💯 📧");
        assert_eq!(replace(":tr: :us:"), "🇹🇷 🇺🇸");
        assert_eq!(
            replace(":family_man_woman_girl_boy:"),
            "👨\u{200d}👩\u{200d}👧\u{200d}👦"
        );
        assert_eq!(replace(":man_technologist:"), "👨\u{200d}💻");
        assert_eq!(replace(":airplane:"), "✈\u{fe0f}");
        // Aliases of one entry all map to it.
        assert_eq!(replace(":thumbsup:"), replace(":+1:"));
        assert_eq!(replace(":smile:"), "😄");
    }

    #[test]
    fn unknown_and_custom_names_stay_literal_and_matching_is_case_sensitive() {
        assert_eq!(
            replace(":octocat: :shipit: :notanemoji:"),
            ":octocat: :shipit: :notanemoji:"
        );
        assert_eq!(replace(":ROCKET: :Tada: :+1:"), ":ROCKET: :Tada: 👍");
        assert_eq!(replace("::rocket::"), ":🚀:");
    }
}
