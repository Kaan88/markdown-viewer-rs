//! Renders a `Document` as HTML using only the public model. Test-only: the app never writes HTML.

use mdview_core::model::*;

pub fn html(doc: &Document) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < doc.blocks.len() as u32 {
        i = block(doc, i, &mut out);
    }
    out
}

fn children(doc: &Document, b: u32, out: &mut String) {
    let end = doc.blocks[b as usize].end;
    let mut i = b + 1;
    while i < end {
        i = block(doc, i, out);
    }
}

fn align_attr(a: Align) -> &'static str {
    match a {
        Align::None => "",
        Align::Left => " align=\"left\"",
        Align::Center => " align=\"center\"",
        Align::Right => " align=\"right\"",
        Align::Justify => " align=\"justify\"",
    }
}

fn block(doc: &Document, b: u32, out: &mut String) -> u32 {
    let bl = doc.blocks[b as usize];
    let wrap =
        |open: String, close: &str, out: &mut String, content: &mut dyn FnMut(&mut String)| {
            out.push_str(&open);
            content(out);
            out.push_str(close);
            out.push('\n');
        };
    match bl.kind {
        BlockKind::Paragraph { align } => {
            wrap(format!("<p{}>", align_attr(align)), "</p>", out, &mut |o| {
                inline(doc, b, o)
            })
        }
        BlockKind::Plain => {
            inline(doc, b, out);
            out.push('\n');
        }
        BlockKind::Heading { level, align, .. } => wrap(
            format!("<h{level}{}>", align_attr(align)),
            &format!("</h{level}>"),
            out,
            &mut |o| inline(doc, b, o),
        ),
        BlockKind::BlockQuote | BlockKind::Alert(_) => {
            wrap("<blockquote>\n".into(), "</blockquote>", out, &mut |o| {
                children(doc, b, o)
            })
        }
        BlockKind::List { start: None } => {
            wrap("<ul>\n".into(), "</ul>", out, &mut |o| children(doc, b, o))
        }
        BlockKind::List { start: Some(1) } => {
            wrap("<ol>\n".into(), "</ol>", out, &mut |o| children(doc, b, o))
        }
        BlockKind::List { start: Some(n) } => {
            wrap(format!("<ol start=\"{n}\">\n"), "</ol>", out, &mut |o| {
                children(doc, b, o)
            })
        }
        BlockKind::ListItem { .. } => {
            wrap("<li>".into(), "</li>", out, &mut |o| children(doc, b, o))
        }
        BlockKind::CodeBlock { lang } => code(doc, doc.str(lang), bl.text, out),
        BlockKind::Diagram { lang, .. } => code(doc, doc.str(lang), bl.text, out),
        BlockKind::Math { .. } => code(doc, "math", bl.text, out),
        BlockKind::ThematicBreak => out.push_str("<hr />\n"),
        BlockKind::Table => wrap("<table>\n".into(), "</table>", out, &mut |o| {
            children(doc, b, o)
        }),
        BlockKind::TableRow { .. } => {
            wrap("<tr>\n".into(), "</tr>", out, &mut |o| children(doc, b, o))
        }
        BlockKind::TableCell {
            align,
            header,
            colspan,
            rowspan,
        } => {
            let t = if header { "th" } else { "td" };
            let mut attrs = align_attr(align).to_string();
            if colspan > 1 {
                attrs.push_str(&format!(" colspan=\"{colspan}\""));
            }
            if rowspan > 1 {
                attrs.push_str(&format!(" rowspan=\"{rowspan}\""));
            }
            out.push_str(&format!("<{t}{attrs}>"));
            inline(doc, b, out);
            children(doc, b, out);
            out.push_str(&format!("</{t}>\n"));
        }
        BlockKind::Details { open } => {
            let attr = if open { " open" } else { "" };
            wrap(format!("<details{attr}>\n"), "</details>", out, &mut |o| {
                children(doc, b, o)
            });
        }
        BlockKind::Summary => wrap("<summary>".into(), "</summary>", out, &mut |o| {
            children(doc, b, o)
        }),
        BlockKind::FootnoteDefinition { .. } => wrap("<div>\n".into(), "</div>", out, &mut |o| {
            children(doc, b, o)
        }),
        BlockKind::Div { align } => wrap(
            format!("<div{}>", align_attr(align)),
            "</div>",
            out,
            &mut |o| children(doc, b, o),
        ),
    }
    bl.end
}

fn code(doc: &Document, lang: &str, r: TextRange, out: &mut String) {
    out.push_str("<pre><code");
    if !lang.is_empty() {
        out.push_str(" class=\"language-");
        escape_attr(lang, out);
        out.push('"');
    }
    out.push('>');
    for c in doc.text_of(r).chars() {
        escape_char(c, out);
    }
    out.push_str("</code></pre>\n");
}

fn inline(doc: &Document, b: u32, out: &mut String) {
    let bl = doc.blocks[b as usize];
    let mut i = bl.spans.start;
    // Own text excludes the text of child blocks, which only cells can have alongside spans.
    let end = if bl.end > b + 1 {
        doc.blocks[(b + 1) as usize].text.start
    } else {
        bl.text.end
    };
    inline_range(
        doc,
        bl.spans.end,
        bl.text.start,
        end,
        NONE,
        false,
        &mut i,
        out,
    );
}

#[allow(clippy::too_many_arguments)]
fn inline_range(
    doc: &Document,
    hi: u32,
    start: u32,
    end: u32,
    parent: u32,
    raw: bool,
    i: &mut u32,
    out: &mut String,
) {
    let mut pos = start;
    while *i < hi && doc.spans[*i as usize].parent == parent {
        let idx = *i;
        let s = doc.spans[idx as usize];
        text(doc, pos, s.start, raw, out);
        pos = s.end;
        *i += 1;
        let (open, close): (String, &str) = match s.kind {
            SpanKind::Emphasis => ("<em>".into(), "</em>"),
            SpanKind::Strong => ("<strong>".into(), "</strong>"),
            SpanKind::Strike => ("<del>".into(), "</del>"),
            SpanKind::Underline => ("<u>".into(), "</u>"),
            SpanKind::Code => ("<code>".into(), "</code>"),
            SpanKind::Kbd => ("<kbd>".into(), "</kbd>"),
            SpanKind::Sub => ("<sub>".into(), "</sub>"),
            SpanKind::Sup => ("<sup>".into(), "</sup>"),
            SpanKind::Mark => ("<mark>".into(), "</mark>"),
            SpanKind::Math { .. } => ("<code>".into(), "</code>"),
            SpanKind::Link(l) => {
                let info = doc.links[l as usize];
                let mut o = String::from("<a href=\"");
                escape_href(doc.str(info.dest), &mut o);
                o.push('"');
                if info.title.len > 0 {
                    o.push_str(" title=\"");
                    escape_attr(doc.str(info.title), &mut o);
                    o.push('"');
                }
                o.push('>');
                (o, "</a>")
            }
            SpanKind::Image(m) => {
                let info = doc.images[m as usize];
                let mut o = String::from("<img src=\"");
                escape_href(doc.str(info.src), &mut o);
                o.push_str("\" alt=\"");
                escape_attr(doc.str(info.alt), &mut o);
                o.push('"');
                if info.title.len > 0 {
                    o.push_str(" title=\"");
                    escape_attr(doc.str(info.title), &mut o);
                    o.push('"');
                }
                o.push_str(" />");
                (o, "")
            }
            SpanKind::FootnoteRef(f) => (
                format!("<sup>{}</sup>", doc.footnotes[f as usize].number),
                "",
            ),
            SpanKind::Checkbox { checked } => (
                format!(
                    "<input type=\"checkbox\"{}> ",
                    if checked { " checked" } else { "" }
                ),
                "",
            ),
        };
        out.push_str(&open);
        let inner_raw = raw || matches!(s.kind, SpanKind::Code | SpanKind::Math { .. });
        inline_range(doc, hi, s.start, s.end, idx, inner_raw, i, out);
        out.push_str(close);
    }
    text(doc, pos, end, raw, out);
}

fn text(doc: &Document, a: u32, b: u32, raw: bool, out: &mut String) {
    if a >= b {
        return;
    }
    for c in doc.text[a as usize..b as usize].chars() {
        if c == '\n' && !raw {
            out.push_str("<br />\n");
        } else {
            escape_char(c, out);
        }
    }
}

fn escape_char(c: char, out: &mut String) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        c => out.push(c),
    }
}

fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        escape_char(c, out);
    }
}

/// The percent-encoding a CommonMark HTML writer applies to destinations.
fn escape_href(s: &str, out: &mut String) {
    for &b in s.as_bytes() {
        match b {
            b'&' => out.push_str("&amp;"),
            b'\'' => out.push_str("&#x27;"),
            b if b.is_ascii_alphanumeric() || b"!#$%()*+,-./:;=?@_~".contains(&b) => {
                out.push(b as char)
            }
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
}
