//! Syntax highlighting for fenced code blocks, with syntect and its bundled syntax and theme
//! dumps. The renderer turns the returned byte ranges into text runs; the document offsets stay
//! the source's, so selection and find keep working.
//!
//! The bundled set has no TypeScript and no TOML. TypeScript and JSX are read as JavaScript,
//! which colours their keywords, strings, numbers and comments; TOML has a small scanner here.
//! Both cost nothing in the binary, which a second syntax dump would.

use std::ops::Range;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Highlighter, Theme, ThemeSet};
use syntect::parsing::{Scope, SyntaxReference, SyntaxSet};
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
    // Names GitHub accepts that the bundled set knows under another one.
    let lower = match lower.as_str() {
        "typescript" | "ts" | "tsx" | "jsx" | "mjs" | "cjs" | "javascript" => "js",
        "jsonc" | "json5" => "json",
        "shell" | "zsh" | "console" => "bash",
        "c++" => "cpp",
        "golang" => "go",
        other => other,
    };
    set.find_syntax_by_token(lower)
        .or_else(|| set.find_syntax_by_extension(lower))
        .filter(|syntax| !syntax.hidden)
}

/// Byte ranges and foreground colours covering `source`. `None` for an unknown or missing
/// language, an oversized block, or a parser failure, so the caller keeps its plain code style.
pub fn tokens(source: &str, language: Option<&str>, dark: bool) -> Option<Vec<Token>> {
    if source.len() > MAX_HIGHLIGHT_BYTES {
        return None;
    }
    let set = syntaxes();
    let language = language?;
    let theme = theme(dark);
    if language.trim().eq_ignore_ascii_case("toml") {
        return Some(toml(source, theme));
    }
    let syntax = syntax(set, language)?;
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

/// The theme's colour for a scope such as `comment` or `string`.
fn scope_color(theme: &Theme, scope: &str) -> [u8; 3] {
    let style = match Scope::new(scope) {
        Ok(scope) => Highlighter::new(theme).style_for_stack(&[scope]),
        Err(_) => Highlighter::new(theme).get_default(),
    };
    [style.foreground.r, style.foreground.g, style.foreground.b]
}

/// TOML, line by line: comments, `[table]` headers, keys, strings, and numbers, booleans and
/// dates. A multi-line string is coloured to its closing quotes.
fn toml(source: &str, theme: &Theme) -> Vec<Token> {
    let plain = scope_color(theme, "source");
    let comment = scope_color(theme, "comment.line");
    let string = scope_color(theme, "string.quoted");
    let constant = scope_color(theme, "constant.numeric");
    let key = scope_color(theme, "entity.name.tag");
    let table = scope_color(theme, "keyword.control");
    let mut tokens: Vec<Token> = Vec::new();
    let mut push = |range: Range<usize>, color: [u8; 3]| {
        if range.is_empty() {
            return;
        }
        match tokens.last_mut() {
            Some(last) if last.color == color && last.range.end == range.start => {
                last.range.end = range.end;
            }
            _ => tokens.push(Token { range, color }),
        }
    };
    let bytes = source.as_bytes();
    // The delimiter of a multi-line string still open from an earlier line.
    let mut open: Option<&str> = None;
    let mut offset = 0;
    for line in LinesWithEndings::from(source) {
        let end = offset + line.len();
        let mut at = offset;
        let mut value = false;
        if open.is_none() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('[') {
                // A table header, up to a trailing comment.
                let stop = line.find('#').map_or(end, |i| offset + i);
                push(offset..stop, table);
                push(stop..end, comment);
                offset = end;
                continue;
            }
        } else {
            value = true;
        }
        while at < end {
            if let Some(delimiter) = open {
                match source[at..end].find(delimiter) {
                    Some(i) => {
                        push(at..at + i + delimiter.len(), string);
                        at += i + delimiter.len();
                        open = None;
                    }
                    None => {
                        push(at..end, string);
                        at = end;
                    }
                }
                continue;
            }
            let c = bytes[at];
            if c == b'#' {
                push(at..end, comment);
                break;
            }
            if c == b'"' || c == b'\'' {
                let quote = c as char;
                let triple = if quote == '"' { "\"\"\"" } else { "\'\'\'" };
                if source[at..end].starts_with(triple) {
                    push(at..at + 3, string);
                    at += 3;
                    open = Some(triple);
                    continue;
                }
                // To the closing quote on this line; a basic string may escape one.
                let mut stop = at + 1;
                while stop < end && bytes[stop] != c {
                    stop += if c == b'"' && bytes[stop] == b'\\' {
                        2
                    } else {
                        1
                    };
                }
                let stop = (stop + 1).min(end);
                push(at..stop, if value { string } else { key });
                at = stop;
                continue;
            }
            if c == b'=' && !value {
                value = true;
                push(at..at + 1, plain);
                at += 1;
                continue;
            }
            let word =
                |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+' | b'.' | b':');
            if word(c) {
                let mut stop = at;
                while stop < end && word(bytes[stop]) {
                    stop += 1;
                }
                let text = &source[at..stop];
                let color = if !value {
                    key
                } else if matches!(text, "true" | "false" | "inf" | "nan")
                    || text.as_bytes()[0].is_ascii_digit()
                    || matches!(text.as_bytes()[0], b'+' | b'-')
                {
                    constant
                } else {
                    plain
                };
                push(at..stop, color);
                at = stop;
                continue;
            }
            // Punctuation and spaces, one character at a time so UTF-8 stays whole.
            let len = source[at..end].chars().next().map_or(1, char::len_utf8);
            push(at..at + len, plain);
            at += len;
        }
        offset = end;
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_and_toml_are_highlighted() {
        let ts = "export async function load(url: string): Promise<string> {\n  return \"x\";\n}\n";
        for name in ["typescript", "ts", "tsx"] {
            let tokens = tokens(ts, Some(name), false).expect(name);
            let mut colours: Vec<_> = tokens.iter().map(|t| t.color).collect();
            colours.sort_unstable();
            colours.dedup();
            assert!(colours.len() >= 3, "{name}: {colours:?}");
        }
        let source = "# comment\n[package]\nname = \"mdview\" # trailing\nedition = 2024\ntext = \"\"\"\nmany # not a comment\n\"\"\"\nön = true\n";
        let tokens = tokens(source, Some("toml"), false).expect("toml");
        assert_eq!(tokens[0].range.start, 0);
        assert_eq!(tokens.last().unwrap().range.end, source.len());
        for pair in tokens.windows(2) {
            assert_eq!(pair[0].range.end, pair[1].range.start);
        }
        let colour_of = |needle: &str| {
            let at = source.find(needle).unwrap();
            tokens.iter().find(|t| t.range.contains(&at)).unwrap().color
        };
        assert_ne!(colour_of("# comment"), colour_of("name"));
        assert_ne!(colour_of("\"mdview\""), colour_of("name"));
        assert_eq!(colour_of("# trailing"), colour_of("# comment"));
        assert_eq!(colour_of("many # not"), colour_of("\"mdview\""));
        assert_ne!(colour_of("2024"), colour_of("\"mdview\""));
    }

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
