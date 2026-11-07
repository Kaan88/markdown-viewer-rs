//! Syntax highlighting for fenced code blocks, with syntect and its bundled syntax and theme
//! dumps. The renderer turns the returned byte ranges into text runs; the document offsets stay
//! the source's, so selection and find keep working.

use std::ops::Range;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

/// Larger blocks stay plain: parsing a megabyte of source at once is not worth a page frame.
const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;

pub struct Token {
    pub range: Range<usize>,
    pub color: [u8; 3],
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn themes() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

fn theme(dark: bool) -> &'static Theme {
    let name = if dark {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    };
    themes()
        .themes
        .get(name)
        .or_else(|| themes().themes.values().next())
        .expect("syntect ships at least one theme")
}

fn syntax<'a>(set: &'a SyntaxSet, language: &str) -> Option<&'a SyntaxReference> {
    let language = language.trim();
    if language.is_empty() {
        return None;
    }
    let lower = language.to_ascii_lowercase();
    set.find_syntax_by_token(&lower)
        .or_else(|| set.find_syntax_by_extension(&lower))
        .filter(|syntax| !syntax.hidden)
}

/// Byte ranges and foreground colours covering `source`. `None` for an unknown or missing
/// language, an oversized block, or a parser failure, so the caller keeps its plain code style.
pub fn tokens(source: &str, language: Option<&str>, dark: bool) -> Option<Vec<Token>> {
    if source.len() > MAX_HIGHLIGHT_BYTES {
        return None;
    }
    let set = syntaxes();
    let syntax = syntax(set, language?)?;
    let theme = theme(dark);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut tokens = Vec::new();
    let mut offset = 0;
    for line in LinesWithEndings::from(source) {
        let ranges = highlighter.highlight_line(line, set).ok()?;
        let mut cursor = 0;
        for (style, text) in ranges {
            let start = cursor;
            cursor += text.len();
            if text.is_empty() {
                continue;
            }
            tokens.push(Token {
                range: offset + start..offset + cursor,
                color: [style.foreground.r, style.foreground.g, style.foreground.b],
            });
        }
        // A parser that does not cover the line's newline would leave a gap; keep it readable
        // with the theme's foreground rather than dropping the character.
        if cursor < line.len() {
            let color = theme
                .settings
                .foreground
                .unwrap_or(syntect::highlighting::Color {
                    r: 40,
                    g: 40,
                    b: 40,
                    a: 255,
                });
            tokens.push(Token {
                range: offset + cursor..offset + line.len(),
                color: [color.r, color.g, color.b],
            });
        }
        offset += line.len();
    }
    (offset == source.len()).then_some(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_is_highlighted_with_more_than_one_colour() {
        let source = "fn main() {\n    let n = 1 + 2;\n    println!(\"{n}\");\n}\n";
        let tokens = tokens(source, Some("rust"), false).expect("rust is in the default set");
        assert_eq!(tokens[0].range.start, 0);
        assert_eq!(tokens.last().unwrap().range.end, source.len());
        let mut colours = tokens.iter().map(|t| t.color).collect::<Vec<_>>();
        colours.sort_unstable();
        colours.dedup();
        assert!(colours.len() >= 3, "{colours:?}");
        for pair in tokens.windows(2) {
            assert_eq!(
                pair[0].range.end, pair[1].range.start,
                "ranges are contiguous"
            );
        }
    }

    #[test]
    fn dark_and_light_pick_different_themes() {
        let source = "let x = 1;\n";
        let light = tokens(source, Some("rust"), false).unwrap();
        let dark = tokens(source, Some("rust"), true).unwrap();
        assert_ne!(
            light.first().map(|t| t.color),
            dark.first().map(|t| t.color),
            "the two themes should not produce identical first tokens"
        );
    }

    #[test]
    fn unknown_or_missing_languages_stay_plain() {
        assert!(tokens("hello\n", None, false).is_none());
        assert!(tokens("hello\n", Some("not-a-language"), false).is_none());
    }

    #[test]
    fn an_oversized_block_is_not_parsed() {
        let source = "x".repeat(MAX_HIGHLIGHT_BYTES + 1);
        assert!(tokens(&source, Some("rust"), false).is_none());
    }
}
