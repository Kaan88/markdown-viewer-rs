//! Presentation mode's slides: where a document is cut. Slides are cut at the horizontal rules
//! (`---`, `***`, `___`) when the document has any at its top level, otherwise before every
//! level 1 and level 2 heading. Front matter is skipped. Each slide comes back as Markdown source
//! of its own, ready to parse; the link reference definitions of the whole document ride along
//! with each, so `[text][ref]` still finds its target when the definition sits on another slide.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};
use std::ops::Range;

// Keep block recognition in slides consistent with the shared Markdown parser.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
}

/// The slides of `src`, in order. Empty for a document without anything to show.
pub fn split(src: &str) -> Vec<String> {
    let src = src.strip_prefix('\u{FEFF}').unwrap_or(src);
    let skip = mdview_core::frontmatter::take(src).map_or(0, |(_, used)| used);
    let body = &src[skip..];

    let mut parser = Parser::new_ext(body, options()).into_offset_iter();
    let mut depth = 0usize;
    let mut rules: Vec<Range<usize>> = Vec::new();
    let mut headings: Vec<usize> = Vec::new();
    for (event, range) in parser.by_ref() {
        match event {
            Event::Start(tag) => {
                if depth == 0
                    && matches!(tag, Tag::Heading { level, .. } if level <= HeadingLevel::H2)
                {
                    headings.push(range.start);
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Rule if depth == 0 => rules.push(range),
            _ => {}
        }
    }
    let mut definitions: Vec<Range<usize>> = parser
        .reference_definitions()
        .iter()
        .map(|(_, def)| def.span.clone())
        .collect();
    definitions.sort_by_key(|span| span.start);

    // (where a slide ends, where the next one starts)
    let cuts: Vec<(usize, usize)> = if rules.is_empty() {
        headings.iter().map(|&at| (at, at)).collect()
    } else {
        rules.iter().map(|r| (r.start, r.end)).collect()
    };
    let mut segments = Vec::with_capacity(cuts.len() + 1);
    let mut start = 0;
    for (end, next) in cuts {
        segments.push(start..end.max(start));
        start = next.max(start);
    }
    segments.push(start..body.len());

    let tail: String = definitions
        .iter()
        .map(|span| body[span.clone()].trim())
        .collect::<Vec<_>>()
        .join("\n");
    segments
        .into_iter()
        .filter_map(|seg| {
            // What the slide shows: its text without the definitions inside it.
            let mut shown = String::new();
            let mut at = seg.start;
            for span in definitions
                .iter()
                .filter(|s| s.start >= seg.start && s.end <= seg.end)
            {
                shown.push_str(&body[at..span.start]);
                at = span.end;
            }
            shown.push_str(&body[at..seg.end]);
            if shown.trim().is_empty() {
                return None;
            }
            let text = body[seg].trim_matches(['\r', '\n']);
            Some(if tail.is_empty() {
                text.to_string()
            } else {
                format!("{text}\n\n{tail}\n")
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str) -> Vec<String> {
        split(src).iter().map(|s| s.trim().to_string()).collect()
    }

    #[test]
    fn rules_cut_slides() {
        let src = "# One\n\ntext\n\n---\n\n# Two\n\n## Sub\n\n***\n\nThree\n";
        assert_eq!(
            texts(src),
            ["# One\n\ntext", "# Two\n\n## Sub", "Three"],
            "headings inside a slide do not cut when rules are there"
        );
    }

    #[test]
    fn headings_cut_slides_without_rules() {
        let src = "intro\n\n# One\n\ntext\n\n## Two\n\n### Three stays\n\nmore\n\n# Four\n";
        assert_eq!(
            texts(src),
            [
                "intro",
                "# One\n\ntext",
                "## Two\n\n### Three stays\n\nmore",
                "# Four"
            ]
        );
    }

    #[test]
    fn setext_headings_cut_and_underlines_are_not_rules() {
        let src = "Title\n=====\n\nbody\n\nNext\n----\n\nmore\n";
        assert_eq!(texts(src), ["Title\n=====\n\nbody", "Next\n----\n\nmore"]);
    }

    #[test]
    fn fences_quotes_and_lists_do_not_cut() {
        let src = "# A\n\n```md\n---\n# not a slide\n```\n\n> ---\n\n- one\n\n  ---\n\n# B\n";
        let s = texts(src);
        assert_eq!(s.len(), 2, "{s:?}");
        assert!(s[0].contains("# not a slide") && s[1] == "# B");
    }

    #[test]
    fn front_matter_is_skipped() {
        let src = "---\ntitle: Deck\n---\n\n# One\n\n---\n\n# Two\n";
        assert_eq!(texts(src), ["# One", "# Two"]);
        let src = "---\ntitle: Deck\n---\n# One\n\ntext\n\n# Two\n";
        assert_eq!(texts(src), ["# One\n\ntext", "# Two"]);
    }

    #[test]
    fn empty_slides_are_dropped_and_a_plain_document_is_one_slide() {
        assert_eq!(texts("---\n\n---\n\nonly\n\n---\n"), ["only"]);
        assert_eq!(texts("just text\n\nmore\n"), ["just text\n\nmore"]);
        assert!(split("").is_empty());
        assert!(split("\n\n  \n").is_empty());
        assert!(split("---\ntitle: x\n---\n").is_empty());
    }

    #[test]
    fn link_definitions_follow_every_slide() {
        let src = "# One\n\n[a][x]\n\n---\n\n# Two\n\n[b][x]\n\n[x]: https://example.com/\n";
        let s = split(src);
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|t| t.contains("[x]: https://example.com/")));
        // A slide of nothing but definitions is not a slide.
        let src = "# One\n\n---\n\n[x]: https://example.com/\n";
        assert_eq!(split(src).len(), 1);
    }

    #[test]
    fn crlf_and_bom() {
        let src = "\u{FEFF}# One\r\n\r\n---\r\n\r\n# Two\r\n";
        assert_eq!(texts(src), ["# One", "# Two"]);
    }
}
