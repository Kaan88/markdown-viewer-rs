//! A selection as HTML, for the clipboard: what a paste into a word processor or mail keeps.
//!
//! The HTML is made from the document model already parsed, never from the source, so it holds
//! exactly what the page shows of the selected range: headings, emphasis, links, lists, tables and
//! code. It has no external resources (an image becomes its alt text) and every piece of text is
//! escaped. Links are kept only when they are http, https or mailto addresses. The plain text
//! that goes with it is the selected slice of `Document::text`, untouched.

use crate::model::{BlockKind, Document, NONE, SpanKind};

/// The selected range `from..to` (offsets into `Document::text`) as an HTML fragment, or `None`
/// when the range holds nothing to copy.
pub fn selection_html(doc: &Document, from: u32, to: u32) -> Option<String> {
    if from >= to || to as usize > doc.text.len() {
        return None;
    }
    let mut w = Writer {
        doc,
        from,
        to,
        out: String::new(),
    };
    for top in doc.children(NONE) {
        w.block(top);
    }
    (!w.out.is_empty()).then_some(w.out)
}

struct Writer<'a> {
    doc: &'a Document,
    from: u32,
    to: u32,
    out: String,
}

impl Writer<'_> {
    /// Writes one block if any of it is selected; returns whether it wrote something.
    fn block(&mut self, i: u32) -> bool {
        let doc = self.doc;
        let b = doc.blocks[i as usize];
        let leaf = b.end == i + 1;
        let before = self.out.len();
        match b.kind {
            BlockKind::Paragraph { align } => {
                self.leaf(i, &format!("<p{}>", align_style(align)), "</p>");
            }
            BlockKind::Plain => {
                let inline = matches!(
                    doc.blocks.get(b.parent as usize).map(|p| p.kind),
                    Some(BlockKind::ListItem { .. } | BlockKind::TableCell { .. })
                );
                if inline {
                    self.leaf(i, "", "");
                } else {
                    self.leaf(i, "<div>", "</div>");
                }
            }
            BlockKind::Heading { level, align, .. } => {
                let n = level.clamp(1, 6);
                self.leaf(
                    i,
                    &format!("<h{n}{}>", align_style(align)),
                    &format!("</h{n}>"),
                );
            }
            BlockKind::CodeBlock { .. } | BlockKind::Diagram { .. } | BlockKind::Math { .. } => {
                self.leaf(i, "<pre><code>", "</code></pre>");
            }
            BlockKind::ThematicBreak => {
                if self.holds_point(b.text.start) {
                    self.out.push_str("<hr>");
                }
            }
            BlockKind::TableCell { .. } if leaf => {}
            BlockKind::TableRow { .. } => self.row(i),
            _ => self.container(i),
        }
        self.out.len() > before
    }

    /// A block that holds other blocks: its tags are written only if a child wrote something.
    fn container(&mut self, i: u32) {
        let doc = self.doc;
        let b = doc.blocks[i as usize];
        let (open, close) = match b.kind {
            BlockKind::BlockQuote => ("<blockquote>".to_string(), "</blockquote>".to_string()),
            BlockKind::Alert(kind) => (
                format!("<blockquote><p><strong>{}</strong></p>", kind.label()),
                "</blockquote>".to_string(),
            ),
            BlockKind::List { start: None } => ("<ul>".to_string(), "</ul>".to_string()),
            BlockKind::List { start: Some(n) } if n != 1 => {
                (format!("<ol start=\"{n}\">"), "</ol>".to_string())
            }
            BlockKind::List { start: Some(_) } => ("<ol>".to_string(), "</ol>".to_string()),
            BlockKind::ListItem { .. } => ("<li>".to_string(), "</li>".to_string()),
            BlockKind::Table => (
                "<table border=\"1\" cellspacing=\"0\" cellpadding=\"4\" style=\"border-collapse:collapse\">"
                    .to_string(),
                "</table>".to_string(),
            ),
            BlockKind::TableCell { .. } => cell_tags(b.kind),
            BlockKind::Summary => ("<div><strong>".to_string(), "</strong></div>".to_string()),
            BlockKind::DefTerm => ("<div><strong>".to_string(), "</strong></div>".to_string()),
            BlockKind::DefDesc => (
                "<div style=\"margin-left:2em\">".to_string(),
                "</div>".to_string(),
            ),
            BlockKind::Figure { .. } => ("<figure>".to_string(), "</figure>".to_string()),
            BlockKind::FigCaption => ("<figcaption>".to_string(), "</figcaption>".to_string()),
            BlockKind::Div { align } => (format!("<div{}>", align_style(align)), "</div>".to_string()),
            BlockKind::FootnoteDefinition { footnote } => {
                let number = doc.footnotes.get(footnote as usize).map_or(0, |f| f.number);
                (
                    format!("<div><sup>{number}</sup> "),
                    "</div>".to_string(),
                )
            }
            _ => ("<div>".to_string(), "</div>".to_string()),
        };
        let mark = self.out.len();
        self.out.push_str(&open);
        let body = self.out.len();
        for c in doc.children(i) {
            self.block(c);
        }
        if self.out.len() == body {
            self.out.truncate(mark);
        } else {
            self.out.push_str(&close);
        }
    }

    /// A table row: when anything in it is selected every cell is written, those outside the
    /// selection empty, so the columns stay where they were.
    fn row(&mut self, i: u32) {
        let doc = self.doc;
        let mark = self.out.len();
        self.out.push_str("<tr>");
        let mut any = false;
        for c in doc.children(i) {
            let cell = doc.blocks[c as usize];
            let (open, close) = cell_tags(cell.kind);
            self.out.push_str(&open);
            let body = self.out.len();
            if cell.end == c + 1 {
                self.leaf(c, "", "");
            } else {
                for k in doc.children(c) {
                    self.block(k);
                }
            }
            any |= self.out.len() > body;
            self.out.push_str(&close);
        }
        if any {
            self.out.push_str("</tr>");
        } else {
            self.out.truncate(mark);
        }
    }

    /// A block with inline content of its own, clipped to the selection.
    fn leaf(&mut self, i: u32, open: &str, close: &str) {
        let doc = self.doc;
        let b = doc.blocks[i as usize];
        let pre = matches!(
            b.kind,
            BlockKind::CodeBlock { .. } | BlockKind::Diagram { .. } | BlockKind::Math { .. }
        );
        let (start, end) = (b.text.start.max(self.from), b.text.end.min(self.to));
        let empty = b.text.is_empty();
        if empty {
            // Nothing but atoms (an image, a checkbox): selected when it lies inside the range.
            if !self.holds_point(b.text.start) {
                return;
            }
        } else if start >= end {
            return;
        }
        let mark = self.out.len();
        self.out.push_str(open);
        let body = self.out.len();
        if pre {
            escape_into(
                &mut self.out,
                &doc.text[start as usize..end as usize],
                false,
            );
        } else {
            self.inline(b.spans.start, b.spans.end, NONE, start, end);
        }
        if self.out.len() == body {
            self.out.truncate(mark);
        } else {
            self.out.push_str(close);
        }
    }

    /// Whether `at` is strictly inside the selection, for blocks with no text of their own.
    fn holds_point(&self, at: u32) -> bool {
        (at > self.from || (at == 0 && self.from == 0)) && at < self.to
    }

    /// Text `from..to` of a block, with the spans among `spans` whose parent is `parent`.
    fn inline(&mut self, spans_start: u32, spans_end: u32, parent: u32, from: u32, to: u32) {
        let doc = self.doc;
        let mut at = from;
        for s in spans_start..spans_end {
            let span = doc.spans[s as usize];
            // A span with no parent in this block is at its top, whatever it was carried from.
            let top = parent == NONE && !(spans_start..spans_end).contains(&span.parent);
            if span.parent != parent && !top {
                continue;
            }
            let atom = matches!(
                span.kind,
                SpanKind::Image(_) | SpanKind::Checkbox { .. } | SpanKind::FootnoteRef(_)
            );
            if atom {
                // At the end of a selection that stops before it, an atom is left out.
                let inside = (span.start >= from && span.start < to) || from == to;
                if span.start < at || !inside {
                    continue;
                }
                self.text(at, span.start);
                at = span.start;
                self.atom(span.kind);
                continue;
            }
            let (cs, ce) = (span.start.max(from), span.end.min(to));
            if cs >= ce || cs < at {
                continue;
            }
            self.text(at, cs);
            let mark = self.out.len();
            let (open, close) = self.span_tags(span.kind);
            self.out.push_str(&open);
            let body = self.out.len();
            match span.kind {
                // The source of a formula is shown as code; its text has no spans inside.
                SpanKind::Math { .. } => self.text(cs, ce),
                _ => self.inline(spans_start, spans_end, s, cs, ce),
            }
            if self.out.len() == body {
                self.out.truncate(mark);
            } else {
                self.out.push_str(&close);
            }
            at = ce;
        }
        self.text(at, to);
    }

    fn text(&mut self, from: u32, to: u32) {
        if from < to {
            escape_into(
                &mut self.out,
                &self.doc.text[from as usize..to as usize],
                true,
            );
        }
    }

    fn atom(&mut self, kind: SpanKind) {
        match kind {
            SpanKind::Checkbox { checked } => {
                self.out
                    .push_str(if checked { "&#9745; " } else { "&#9744; " });
            }
            SpanKind::FootnoteRef(n) => {
                let number = self.doc.footnotes.get(n as usize).map_or(0, |f| f.number);
                self.out.push_str(&format!("<sup>{number}</sup>"));
            }
            SpanKind::Image(n) => {
                // No picture: pasting must not fetch anything. The alt text stands in for it.
                if let Some(image) = self.doc.images.get(n as usize) {
                    escape_into(&mut self.out, self.doc.str(image.alt), true);
                }
            }
            _ => {}
        }
    }

    fn span_tags(&self, kind: SpanKind) -> (String, String) {
        let tag = |t: &str| (format!("<{t}>"), format!("</{t}>"));
        match kind {
            SpanKind::Emphasis => tag("em"),
            SpanKind::Strong => tag("strong"),
            SpanKind::Strike => tag("del"),
            SpanKind::Underline => tag("u"),
            SpanKind::Code | SpanKind::Math { .. } => tag("code"),
            SpanKind::Kbd => tag("kbd"),
            SpanKind::Sub => tag("sub"),
            SpanKind::Sup => tag("sup"),
            SpanKind::Mark => tag("mark"),
            SpanKind::Small => tag("small"),
            SpanKind::Quote => tag("q"),
            SpanKind::Ruby => tag("ruby"),
            SpanKind::RubyNote => tag("rt"),
            SpanKind::Link(n) => match self.doc.links.get(n as usize) {
                Some(link) if !link.wiki && safe_href(self.doc.str(link.dest)) => {
                    let mut open = String::from("<a href=\"");
                    escape_into(&mut open, self.doc.str(link.dest), false);
                    open.push('"');
                    let title = self.doc.str(link.title);
                    if !title.is_empty() {
                        open.push_str(" title=\"");
                        escape_into(&mut open, title, false);
                        open.push('"');
                    }
                    open.push('>');
                    (open, "</a>".to_string())
                }
                _ => (String::new(), String::new()),
            },
            SpanKind::Image(_) | SpanKind::FootnoteRef(_) | SpanKind::Checkbox { .. } => {
                (String::new(), String::new())
            }
        }
    }
}

/// The opening and closing tag of a table cell, with its span counts and alignment.
fn cell_tags(kind: BlockKind) -> (String, String) {
    let BlockKind::TableCell {
        align,
        header,
        colspan,
        rowspan,
    } = kind
    else {
        return (String::new(), String::new());
    };
    let tag = if header { "th" } else { "td" };
    let mut open = format!("<{tag}");
    if colspan > 1 {
        open.push_str(&format!(" colspan=\"{colspan}\""));
    }
    if rowspan > 1 {
        open.push_str(&format!(" rowspan=\"{rowspan}\""));
    }
    open.push_str(&align_style(align));
    open.push('>');
    (open, format!("</{tag}>"))
}

fn align_style(align: crate::model::Align) -> String {
    use crate::model::Align;
    let value = match align {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "justify",
        Align::None => return String::new(),
    };
    format!(" style=\"text-align:{value}\"")
}

/// Only addresses a mail or a document can follow from anywhere. A relative path or a note name
/// means nothing once pasted elsewhere, and other schemes are not carried over.
fn safe_href(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p))
}

/// Appends `s` escaped for HTML text or a double-quoted attribute. In text, a line break (a hard
/// break in the page) becomes `<br>`.
fn escape_into(out: &mut String, s: &str, breaks: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' if breaks => out.push_str("<br>"),
            '\0' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn all(src: &str) -> String {
        let doc = parse(src);
        selection_html(&doc, 0, doc.text.len() as u32).unwrap()
    }

    fn range(src: &str, from: &str, to: &str) -> String {
        let doc = parse(src);
        let a = doc.text.find(from).unwrap() as u32;
        let b = (doc.text.find(to).unwrap() + to.len()) as u32;
        selection_html(&doc, a, b).unwrap()
    }

    #[test]
    fn headings_emphasis_and_links_survive() {
        let h = all(
            "# Title\n\nSome *em* and **strong** and `code` and [site](https://example.com \"t\").\n",
        );
        assert_eq!(
            h,
            "<h1>Title</h1><p>Some <em>em</em> and <strong>strong</strong> and <code>code</code> and <a href=\"https://example.com\" title=\"t\">site</a>.</p>"
        );
    }

    #[test]
    fn lists_and_tasks() {
        assert_eq!(
            all("- a\n- b\n  - c\n"),
            "<ul><li>a</li><li>b<ul><li>c</li></ul></li></ul>"
        );
        assert_eq!(
            all("3. x\n4. y\n"),
            "<ol start=\"3\"><li>x</li><li>y</li></ol>"
        );
        assert_eq!(all("1. x\n"), "<ol><li>x</li></ol>");
        let h = all("- [x] done\n- [ ] todo\n");
        assert!(h.starts_with("<ul><li>&#9745; "), "{h}");
        assert!(
            h.contains("done</li><li>&#9744; ") && h.ends_with("todo</li></ul>"),
            "{h}"
        );
    }

    #[test]
    fn tables_keep_their_shape() {
        let h = all("| A | B |\n|:--|--:|\n| 1 | 2 |\n");
        assert!(h.starts_with("<table"));
        assert!(h.contains(
            "<tr><th style=\"text-align:left\">A</th><th style=\"text-align:right\">B</th></tr>"
        ));
        assert!(h.contains(
            "<tr><td style=\"text-align:left\">1</td><td style=\"text-align:right\">2</td></tr>"
        ));
        assert!(h.ends_with("</table>"));
    }

    #[test]
    fn a_partial_row_keeps_every_cell() {
        let h = range("| A | B |\n|---|---|\n| one | two |\n", "two", "two");
        assert!(h.contains("<tr><td></td><td>two</td></tr>"), "{h}");
        assert!(!h.contains("<th"), "{h}");
    }

    #[test]
    fn code_is_preformatted_and_escaped() {
        assert_eq!(
            all("```rust\nlet a = 1 < 2 && \"x\";\n```\n"),
            "<pre><code>let a = 1 &lt; 2 &amp;&amp; &quot;x&quot;;\n</code></pre>"
        );
    }

    #[test]
    fn quotes_and_rules() {
        assert_eq!(all("> quoted\n"), "<blockquote><p>quoted</p></blockquote>");
        assert_eq!(all("a\n\n---\n\nb\n"), "<p>a</p><hr><p>b</p>");
    }

    #[test]
    fn the_selection_clips_text_and_spans() {
        let h = range("Before **bold text** after.\n", "old te", "old te");
        assert_eq!(h, "<p><strong>old te</strong></p>");
        let h = range("one\n\ntwo\n\nthree\n", "e\ntw", "tw");
        assert_eq!(h, "<p>e</p><p>tw</p>");
    }

    #[test]
    fn an_unselected_container_leaves_no_tags() {
        let doc = parse("- a\n- b\n\npara\n");
        let at = doc.text.find("para").unwrap() as u32;
        let h = selection_html(&doc, at, at + 4).unwrap();
        assert_eq!(h, "<p>para</p>");
    }

    #[test]
    fn text_is_escaped_everywhere() {
        let h = all("a < b & c > d \"q\"\n");
        assert_eq!(h, "<p>a &lt; b &amp; c &gt; d &quot;q&quot;</p>");
        let h = all("[x](https://e.com/?a=1&b=\"2\")\n");
        assert!(
            h.contains("href=\"https://e.com/?a=1&amp;b=&quot;2&quot;\""),
            "{h}"
        );
    }

    #[test]
    fn only_safe_links_are_kept() {
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "relative.md",
            "#top",
        ] {
            let h = all(&format!("[x]({url})\n"));
            assert_eq!(h, "<p>x</p>", "{url}");
        }
        assert!(all("[x](mailto:a@b.c)\n").contains("<a href=\"mailto:a@b.c\">"));
    }

    #[test]
    fn images_become_alt_text_and_load_nothing() {
        let h = all("see ![the logo](https://example.com/a.png) here\n");
        assert_eq!(h, "<p>see the logo here</p>");
        assert!(!h.contains("<img") && !h.contains("src"));
    }

    #[test]
    fn a_hard_break_is_a_line_break() {
        assert_eq!(all("a\\\nb\n"), "<p>a<br>b</p>");
        assert_eq!(all("a\nb\n"), "<p>a b</p>");
    }

    #[test]
    fn an_empty_or_out_of_range_selection_gives_nothing() {
        let doc = parse("text\n");
        assert_eq!(selection_html(&doc, 2, 2), None);
        assert_eq!(selection_html(&doc, 3, 1), None);
        assert_eq!(selection_html(&doc, 0, 999), None);
    }

    #[test]
    fn html_in_the_source_cannot_inject_markup() {
        let h = all("<script>alert(1)</script>\n\n<b onclick=\"x\">bold</b>\n");
        assert!(!h.contains("<script") && !h.contains("onclick"), "{h}");
    }
}
