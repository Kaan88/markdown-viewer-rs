//! A document as one self-contained HTML file: what "Export as HTML" saves and what "Print" hands
//! to the browser.
//!
//! The HTML is made from the parsed document model, never from the source, so it holds what the
//! page shows. It carries its own style sheet (the page's light colours, font stacks and reading
//! width; a print style sheet turns the page white and avoids breaking code and tables), and no
//! scripts of the document's own.
//!
//! - **Images.** A local image is embedded as a `data:` URI when it is at most [`MAX_EMBED`] and
//!   the file stays under [`MAX_TOTAL`]; a larger one is linked by its `file://` address, which
//!   works on this machine only. A remote image is linked when the caller says the reader loaded
//!   it, and is its alt text otherwise. Nothing is downloaded here.
//! - **Code** keeps the page's highlight colours as inline styles.
//! - **Formulas and diagrams** are drawn by the engine process, as the page does, and embedded as
//!   PNG pictures at twice their size (so they stay sharp), with the source as their alt text.
//!   When the engine is missing or rejects the source, the source stays in a code box, labelled.
//! - Everything written from the document is escaped, links keep only `http`, `https`, `mailto`
//!   and relative addresses, and a `data:` image must be an image.

use crate::highlight;
use crate::image::{self, DecodedImage};
use crate::theme::{Color, Theme};
use mdview_core::links::{ImageSource, classify_image};
use mdview_core::model::{
    Align, BlockKind, Dir, Document, ImageAlign, ImageInfo, Length, ListMarker, NONE, SpanKind,
};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The largest local image that is embedded in the file.
pub const MAX_EMBED: u64 = 4 * 1024 * 1024;
/// The most image bytes embedded in one file; later images are linked.
pub const MAX_TOTAL: u64 = 24 * 1024 * 1024;

pub struct ExportOptions<'a> {
    /// The `<title>`. Empty takes the first heading, then "Document".
    pub title: &'a str,
    /// The page's colours and font lists; the reading width comes from it too.
    pub theme: Theme,
    /// Where relative image paths start. `None` leaves them as their alt text.
    pub base_dir: Option<&'a Path>,
    /// The reader has loaded this document's remote images, so they are linked.
    pub remote_images: bool,
    /// For the browser to print: every `<details>` is open, and the page asks the browser to open
    /// its print dialog once it has loaded.
    pub print: bool,
}

/// The whole document as an HTML page.
pub fn export_html(doc: &Document, opts: &ExportOptions) -> String {
    let mut w = Writer {
        doc,
        opts,
        out: String::with_capacity(doc.text.len() * 2 + 4096),
        embedded: 0,
        pictures: HashMap::new(),
        anchors: HashMap::new(),
    };
    for a in &doc.anchors {
        w.anchors.entry(a.block).or_default().push(doc.str(a.name));
    }
    w.blocks(NONE);
    w.footnotes();
    let body = std::mem::take(&mut w.out);
    let title = if opts.title.trim().is_empty() {
        doc.headings
            .first()
            .map(|h| doc.str(h.title).trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Document".to_string())
    } else {
        opts.title.trim().to_string()
    };
    let mut page = String::with_capacity(body.len() + 6000);
    page.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    page.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    page.push_str("<meta name=\"generator\" content=\"mdview\">\n<title>");
    escape_into(&mut page, &title, false);
    page.push_str("</title>\n<style>\n");
    page.push_str(&stylesheet(&opts.theme));
    page.push_str("</style>\n</head>\n<body>\n<main>\n");
    page.push_str(&body);
    page.push_str("</main>\n");
    if opts.print {
        // The browser's own print dialog, once the images are in. Without scripts (a locked-down
        // browser) the page is still there to print with Ctrl+P.
        page.push_str(
            "<script>window.addEventListener('load',function(){setTimeout(function(){window.print()},300)})</script>\n",
        );
    }
    page.push_str("</body>\n</html>\n");
    page
}

struct Writer<'a> {
    doc: &'a Document,
    opts: &'a ExportOptions<'a>,
    out: String,
    /// Image bytes embedded so far.
    embedded: u64,
    /// Formulas and diagrams already drawn, by what they were drawn from.
    pictures: HashMap<(u8, String), Option<Picture>>,
    /// Explicit anchors by block.
    anchors: HashMap<u32, Vec<&'a str>>,
}

#[derive(Clone)]
struct Picture {
    uri: String,
    /// Logical size: the raster is twice that.
    width: f32,
    height: f32,
    /// From the top to the text baseline.
    baseline: f32,
    /// The same drawn for the dark page, when it differs and could be drawn.
    dark: Option<String>,
}

impl<'a> Writer<'a> {
    fn blocks(&mut self, parent: u32) {
        for c in self.doc.children(parent) {
            self.block(c);
        }
    }

    fn block(&mut self, i: u32) {
        let doc = self.doc;
        let b = doc.blocks[i as usize];
        if let Some(names) = self.anchors.remove(&i) {
            for name in names {
                self.out.push_str("<a id=\"");
                escape_into(&mut self.out, name, false);
                self.out.push_str("\"></a>");
            }
        }
        let attrs = self.attrs(i, None);
        match b.kind {
            BlockKind::Paragraph { align } => {
                let attrs = self.attrs(i, Some(align));
                self.holder(i, &format!("<p{attrs}>"), "</p>\n", true);
            }
            BlockKind::Plain => {
                let inline = matches!(
                    doc.blocks.get(b.parent as usize).map(|p| p.kind),
                    Some(BlockKind::ListItem { .. } | BlockKind::TableCell { .. })
                );
                if inline {
                    self.holder(i, "", "", false);
                } else {
                    self.holder(i, &format!("<div{attrs}>"), "</div>\n", true);
                }
            }
            BlockKind::Heading { level, align, slug } => {
                let n = level.clamp(1, 6);
                let mut open = format!("<h{n}");
                if slug.len > 0 {
                    open.push_str(" id=\"");
                    escape_into(&mut open, doc.str(slug), false);
                    open.push('"');
                }
                open.push_str(&self.attrs(i, Some(align)));
                open.push('>');
                self.holder(i, &open, &format!("</h{n}>\n"), true);
            }
            BlockKind::BlockQuote => {
                self.holder(
                    i,
                    &format!("<blockquote{attrs}>\n"),
                    "</blockquote>\n",
                    false,
                );
            }
            BlockKind::Alert(kind) => {
                let name = kind.label();
                let open = format!(
                    "<div class=\"alert alert-{}\"{attrs}>\n<p class=\"alert-title\">{name}</p>\n",
                    name.to_ascii_lowercase()
                );
                self.holder(i, &open, "</div>\n", false);
            }
            BlockKind::List { start } => {
                let marker = doc.attrs_of(i).and_then(|a| a.marker);
                let style = marker_style(marker);
                let open = match start {
                    None => format!("<ul{attrs}{style}>\n"),
                    Some(n) if n != 1 => format!("<ol start=\"{n}\"{attrs}{style}>\n"),
                    Some(_) => format!("<ol{attrs}{style}>\n"),
                };
                let close = if start.is_some() {
                    "</ol>\n"
                } else {
                    "</ul>\n"
                };
                self.holder(i, &open, close, false);
            }
            BlockKind::ListItem { task } => {
                let marker = doc.attrs_of(i).and_then(|a| a.marker);
                let style = marker_style(marker);
                let open = if task {
                    format!("<li class=\"task\"{style}>")
                } else {
                    format!("<li{style}>")
                };
                self.holder(i, &open, "</li>\n", false);
            }
            BlockKind::CodeBlock { lang } => {
                let lang = doc.str(lang).trim().to_string();
                self.code(i, &lang, None);
            }
            BlockKind::Diagram { lang, oversize } => {
                let lang = doc.str(lang).trim().to_string();
                let label = if lang.is_empty() { "diagram" } else { &lang };
                let source = doc.block_text(i).trim();
                let picture = match oversize {
                    None if !source.is_empty() => self.diagram(source),
                    _ => None,
                };
                match picture {
                    Some(p) => {
                        self.figure_picture(&p, source, "diagram");
                    }
                    None => {
                        let why =
                            oversize.map_or("Could not be drawn".to_string(), |o| o.to_string());
                        let label = label.to_string();
                        self.code(i, &label, Some(&why));
                    }
                }
            }
            BlockKind::Math { oversize } => {
                let source = doc.block_text(i).trim();
                let picture = match oversize {
                    None if !source.is_empty() => self.formula(source, true),
                    _ => None,
                };
                match picture {
                    Some(p) => self.figure_picture(&p, source, "math"),
                    None => {
                        let why =
                            oversize.map_or("Could not be drawn".to_string(), |o| o.to_string());
                        self.code(i, "math", Some(&why));
                    }
                }
            }
            BlockKind::ThematicBreak => self.out.push_str("<hr>\n"),
            BlockKind::Table => {
                // A table is placed by `align` (its own, or that of the `<div>`, `<center>` or
                // `<figure>` around it), which a browser does with auto margins.
                let own = doc.attrs_of(i).map_or(Align::None, |a| a.align);
                let align = if own == Align::None {
                    self.inherited_align(i)
                } else {
                    own
                };
                let margin = match align {
                    Align::Center => "margin-left:auto;margin-right:auto;",
                    Align::Right => "margin-left:auto;margin-right:0;",
                    _ => "",
                };
                let attrs = self.attrs_styled(i, None, margin);
                self.holder(i, &format!("<table{attrs}>\n"), "</table>\n", false);
            }
            BlockKind::TableRow { .. } => self.holder(i, "<tr>", "</tr>\n", false),
            BlockKind::TableCell {
                align,
                header,
                colspan,
                rowspan,
            } => {
                let tag = if header { "th" } else { "td" };
                let mut open = format!("<{tag}");
                if colspan > 1 {
                    let _ = write!(open, " colspan=\"{colspan}\"");
                }
                if rowspan > 1 {
                    let _ = write!(open, " rowspan=\"{rowspan}\"");
                }
                // A cell that states no `align` or `valign` takes its row's, and `width` is the
                // column's asked width.
                let own = doc.attrs_of(i);
                let row = doc.attrs_of(b.parent);
                let align = if align == Align::None {
                    row.map_or(Align::None, |a| a.align)
                } else {
                    align
                };
                let mut style = String::new();
                if let Some(css) = align_css(align) {
                    style.push_str(css);
                }
                match own.and_then(|a| a.width) {
                    Some(Length::Px(p)) => {
                        let _ = write!(style, "width:{p}px;");
                    }
                    Some(Length::Percent(p)) => {
                        let _ = write!(style, "width:{p}%;");
                    }
                    None => {}
                }
                if let Some(v) = own.and_then(|a| a.valign).or(row.and_then(|a| a.valign)) {
                    style.push_str(match v {
                        mdview_core::model::VAlign::Top => "vertical-align:top;",
                        mdview_core::model::VAlign::Middle => "vertical-align:middle;",
                        mdview_core::model::VAlign::Bottom => "vertical-align:bottom;",
                        mdview_core::model::VAlign::Baseline => "vertical-align:baseline;",
                    });
                }
                if !style.is_empty() {
                    let _ = write!(open, " style=\"{style}\"");
                }
                open.push('>');
                self.holder(i, &open, &format!("</{tag}>"), false);
            }
            BlockKind::Details { open } => {
                let open_attr = if open || self.opts.print { " open" } else { "" };
                self.holder(
                    i,
                    &format!("<details{open_attr}{attrs}>\n"),
                    "</details>\n",
                    false,
                );
            }
            BlockKind::Summary => self.holder(i, "<summary>", "</summary>\n", false),
            // Written in the list at the end, where the page puts them.
            BlockKind::FootnoteDefinition { .. } => {}
            BlockKind::Div { align } => {
                let attrs = self.attrs(i, Some(align));
                self.holder(i, &format!("<div{attrs}>\n"), "</div>\n", false);
            }
            BlockKind::DefTerm => self.holder(i, "<div class=\"dt\">", "</div>\n", false),
            BlockKind::DefDesc => self.holder(i, "<div class=\"dd\">", "</div>\n", false),
            BlockKind::Figure { align } => {
                let attrs = self.attrs(i, Some(align));
                self.holder(i, &format!("<figure{attrs}>\n"), "</figure>\n", false);
            }
            BlockKind::FigCaption => self.holder(i, "<figcaption>", "</figcaption>\n", false),
        }
    }

    /// A block's tags around what it holds: its own inline text, then its child blocks. A block
    /// with nothing in it writes nothing when `skip_empty`.
    fn holder(&mut self, i: u32, open: &str, close: &str, skip_empty: bool) {
        let doc = self.doc;
        let b = doc.blocks[i as usize];
        let mark = self.out.len();
        self.out.push_str(open);
        let body = self.out.len();
        // Only a leaf has inline text of its own; a container's text is its children's.
        if b.end > i + 1 {
            self.blocks(i);
        } else {
            self.inline(i);
        }
        if skip_empty && self.out.len() == body {
            self.out.truncate(mark);
            return;
        }
        self.out.push_str(close);
    }

    /// The `align` of the nearest `<div>`, `<center>` or `<figure>` around block `i` that states
    /// one: what a table with no `align` of its own is placed by.
    fn inherited_align(&self, i: u32) -> Align {
        let mut b = i;
        loop {
            let block = &self.doc.blocks[b as usize];
            if let BlockKind::Figure { align } | BlockKind::Div { align } = block.kind
                && align != Align::None
            {
                return align;
            }
            if block.parent == NONE {
                return Align::None;
            }
            b = block.parent;
        }
    }

    /// ` dir`, ` style` of a block from its alignment and its HTML attributes.
    fn attrs(&self, i: u32, align: Option<Align>) -> String {
        self.attrs_styled(i, align, "")
    }

    /// `attrs` with `extra` CSS declarations added to the style.
    fn attrs_styled(&self, i: u32, align: Option<Align>, extra: &str) -> String {
        let mut s = String::new();
        let mut style = String::from(extra);
        if let Some(a) = self.doc.attrs_of(i) {
            match a.dir {
                Some(Dir::Rtl) => s.push_str(" dir=\"rtl\""),
                Some(Dir::Ltr) => s.push_str(" dir=\"ltr\""),
                Some(Dir::Auto) => s.push_str(" dir=\"auto\""),
                None => {}
            }
            // A table is placed by margins (see `BlockKind::Table`), not by `text-align`.
            let table = matches!(self.doc.blocks[i as usize].kind, BlockKind::Table);
            if let Some(css) = align_css(a.align).filter(|_| !table) {
                style.push_str(css);
            }
            if let Some(Length::Px(p)) = a.width {
                let _ = write!(style, "width:{p}px;");
            } else if let Some(Length::Percent(p)) = a.width {
                let _ = write!(style, "width:{p}%;");
            }
        }
        if let Some(css) = align.and_then(align_css) {
            style.push_str(css);
        }
        if !style.is_empty() {
            let _ = write!(s, " style=\"{style}\"");
        }
        s
    }

    // ---- code, formulas, diagrams ----------------------------------------------------------

    /// A code box; `note` follows the language in the corner, for source shown in place of a
    /// picture.
    fn code(&mut self, i: u32, lang: &str, note: Option<&str>) {
        let source = self.doc.block_text(i).trim_end_matches('\n');
        let label = match (lang.is_empty(), note) {
            (false, Some(n)) => format!("{lang} \u{b7} {n}"),
            (false, None) => lang.to_string(),
            (true, Some(n)) => n.to_string(),
            (true, None) => String::new(),
        };
        self.out.push_str("<pre");
        if !label.is_empty() {
            self.out.push_str(" data-lang=\"");
            escape_into(&mut self.out, &label, false);
            self.out.push('"');
        }
        self.out.push_str("><code>");
        // Coloured for the light page, and for the dark one too where the two lists agree: the
        // dark colour rides along as `--d`, which the dark style rules put to use.
        let tokens = if lang.is_empty() || note.is_some() {
            None
        } else {
            highlight::tokens(source, Some(lang), false)
        };
        match tokens {
            Some(tokens) => {
                let dark = highlight::tokens(source, Some(lang), true);
                highlighted(&mut self.out, source, &tokens, dark.as_deref());
            }
            None => escape_into(&mut self.out, source, false),
        }
        self.out.push_str("</code></pre>\n");
    }

    fn figure_picture(&mut self, p: &Picture, alt: &str, class: &str) {
        let _ = write!(self.out, "<p class=\"{class}\">");
        self.picture_imgs(p, "", "", alt);
        self.out.push_str("</p>\n");
    }

    /// The `<img>` of a drawn picture: one, or a light one and a dark one of which the page
    /// shows the one the reader's browser asks for.
    fn picture_imgs(&mut self, p: &Picture, class: &str, style: &str, alt: &str) {
        let both = [("lt", Some(&p.uri)), ("dk", p.dark.as_ref())];
        for (scheme, uri) in both {
            let Some(uri) = uri else {
                continue;
            };
            self.out.push_str("<img");
            let scheme = if p.dark.is_some() { scheme } else { "" };
            let classes = [class, scheme].join(" ");
            if !classes.trim().is_empty() {
                let _ = write!(self.out, " class=\"{}\"", classes.trim());
            }
            let _ = write!(
                self.out,
                " src=\"{uri}\" width=\"{}\" height=\"{}\"",
                px(p.width),
                px(p.height)
            );
            if !style.is_empty() {
                let _ = write!(self.out, " style=\"{style}\"");
            }
            self.out.push_str(" alt=\"");
            escape_into(&mut self.out, alt, false);
            self.out.push_str("\">");
        }
    }

    /// A formula drawn by the engine, once per source and style.
    fn formula(&mut self, source: &str, display: bool) -> Option<Picture> {
        let key = (u8::from(display), source.to_string());
        if let Some(found) = self.pictures.get(&key) {
            return found.clone();
        }
        let size = self.opts.theme.body_size * if display { 1.15 } else { 1.0 } * 2.0;
        let drawn = crate::engine::render_math(source, display, size, crate::theme::LIGHT.text);
        let dark = crate::engine::render_math(source, display, size, crate::theme::DARK.text);
        let picture = with_dark(drawn.and_then(|d| picture_of(&d)), dark);
        self.pictures.insert(key, picture.clone());
        picture
    }

    fn diagram(&mut self, source: &str) -> Option<Picture> {
        let key = (2, source.to_string());
        if let Some(found) = self.pictures.get(&key) {
            return found.clone();
        }
        let drawn = crate::engine::render_mermaid(source, 2.0, false);
        let dark = crate::engine::render_mermaid(source, 2.0, true);
        let picture = with_dark(drawn.and_then(|d| picture_of(&d)), dark);
        self.pictures.insert(key, picture.clone());
        picture
    }

    // ---- inline content --------------------------------------------------------------------

    fn inline(&mut self, i: u32) {
        let b = self.doc.blocks[i as usize];
        self.run(b.spans.start, b.spans.end, NONE, b.text.start, b.text.end);
    }

    /// Text `from..to` of a block with the spans among `ss..se` whose parent is `parent`.
    fn run(&mut self, ss: u32, se: u32, parent: u32, from: u32, to: u32) {
        let doc = self.doc;
        let mut at = from;
        for s in ss..se {
            let span = doc.spans[s as usize];
            // A span with no parent in this block is at its top, whatever it was carried from.
            let top = parent == NONE && !(ss..se).contains(&span.parent);
            if span.parent != parent && !top {
                continue;
            }
            let atom = matches!(
                span.kind,
                SpanKind::Image(_) | SpanKind::Checkbox { .. } | SpanKind::FootnoteRef(_)
            );
            if atom {
                if span.start < at || span.start > to {
                    continue;
                }
                self.text(at, span.start);
                at = span.start;
                self.atom(span.kind);
                continue;
            }
            let (cs, ce) = (span.start.max(from), span.end.min(to));
            // A span with no text of its own is written when it holds a picture: a link around
            // an image alone.
            let holds_atom = cs == ce
                && (ss..se).any(|k| {
                    let inner = doc.spans[k as usize];
                    inner.parent == s && matches!(inner.kind, SpanKind::Image(_))
                });
            if (cs >= ce && !holds_atom) || cs < at {
                continue;
            }
            self.text(at, cs);
            at = ce;
            if let SpanKind::Math {
                display,
                oversize: None,
            } = span.kind
            {
                let source = doc.text[cs as usize..ce as usize].trim();
                if let Some(p) = self.formula(source, display) {
                    // The baseline sits on the text's: the part of the picture below it hangs down.
                    let style =
                        format!("vertical-align:-{}px", px((p.height - p.baseline).max(0.0)));
                    self.picture_imgs(&p, "math", &style, source);
                    continue;
                }
            }
            let (open, close) = self.span_tags(span.kind);
            self.out.push_str(&open);
            match span.kind {
                // A formula that is not drawn is its source, as code.
                SpanKind::Math { .. } => self.text(cs, ce),
                _ => self.run(ss, se, s, cs, ce),
            }
            self.out.push_str(&close);
        }
        self.text(at, to);
    }

    fn text(&mut self, from: u32, to: u32) {
        if from < to
            && let Some(t) = self.doc.text.get(from as usize..to as usize)
        {
            escape_into(&mut self.out, t, true);
        }
    }

    fn atom(&mut self, kind: SpanKind) {
        match kind {
            SpanKind::Checkbox { checked } => {
                self.out.push_str(if checked {
                    "<input type=\"checkbox\" checked disabled> "
                } else {
                    "<input type=\"checkbox\" disabled> "
                });
            }
            SpanKind::FootnoteRef(n) => {
                let number = self.doc.footnotes.get(n as usize).map_or(0, |f| f.number);
                let _ = write!(
                    self.out,
                    "<sup class=\"fnref\"><a href=\"#fn-{number}\" id=\"fnref-{number}\">{number}</a></sup>"
                );
            }
            SpanKind::Image(n) => {
                if let Some(info) = self.doc.images.get(n as usize).copied() {
                    self.image(&info);
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
            SpanKind::Abbr(title) => {
                let mut open = String::from("<abbr title=\"");
                escape_into(&mut open, self.doc.str(title), false);
                open.push_str("\">");
                (open, "</abbr>".to_string())
            }
            SpanKind::Link(n) => match self.doc.links.get(n as usize) {
                Some(link) if !link.wiki && safe_href(self.doc.str(link.dest)) => {
                    let mut open = String::from("<a href=\"");
                    escape_into(&mut open, self.doc.str(link.dest).trim(), false);
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

    // ---- images ----------------------------------------------------------------------------

    fn image(&mut self, info: &ImageInfo) {
        let doc = self.doc;
        let alt = doc.str(info.alt);
        let (light, dark) = (doc.str(info.light_src), doc.str(info.dark_src));
        let plain = doc.str(info.src);
        // A picture made for each scheme is written twice, and the page shows the one the
        // reader's browser asks for; any other image once.
        let pick = |themed: &'a str| if themed.is_empty() { plain } else { themed };
        let Some(uri) = self.image_uri(pick(light).trim()) else {
            escape_into(&mut self.out, alt, true);
            return;
        };
        let dark_uri = if light.is_empty() && dark.is_empty() {
            None
        } else {
            self.image_uri(pick(dark).trim()).filter(|d| *d != uri)
        };
        match dark_uri {
            Some(dark_uri) => {
                self.image_tag(info, &uri, "lt");
                self.image_tag(info, &dark_uri, "dk");
            }
            None => self.image_tag(info, &uri, ""),
        }
    }

    /// One `<img>` for `info` with the address `uri`; `scheme` is its class when there is a
    /// twin for the other colour scheme.
    fn image_tag(&mut self, info: &ImageInfo, uri: &str, scheme: &str) {
        let doc = self.doc;
        let alt = doc.str(info.alt);
        self.out.push_str("<img");
        if !scheme.is_empty() {
            let _ = write!(self.out, " class=\"{scheme}\"");
        }
        self.out.push_str(" src=\"");
        escape_into(&mut self.out, uri, false);
        self.out.push_str("\" alt=\"");
        escape_into(&mut self.out, alt, false);
        self.out.push('"');
        let title = doc.str(info.title);
        if !title.is_empty() {
            self.out.push_str(" title=\"");
            escape_into(&mut self.out, title, false);
            self.out.push('"');
        }
        let mut style = String::new();
        match info.width {
            Some(Length::Px(p)) => {
                let _ = write!(self.out, " width=\"{p}\"");
            }
            Some(Length::Percent(p)) => {
                let _ = write!(style, "width:{p}%;");
            }
            None => {}
        }
        match info.height {
            Some(Length::Px(p)) => {
                let _ = write!(self.out, " height=\"{p}\"");
            }
            Some(Length::Percent(_)) | None => {}
        }
        style.push_str(match info.align {
            ImageAlign::None => "",
            ImageAlign::Left => "float:left;margin:0 12px 8px 0;",
            ImageAlign::Right => "float:right;margin:0 0 8px 12px;",
            ImageAlign::Top => "vertical-align:top;",
            ImageAlign::Middle => "vertical-align:middle;",
            ImageAlign::Bottom => "vertical-align:bottom;",
        });
        if !style.is_empty() {
            let _ = write!(self.out, " style=\"{style}\"");
        }
        self.out.push('>');
    }

    /// What the `src` of an `<img>` is for this image, or `None` when it has no picture.
    fn image_uri(&mut self, src: &str) -> Option<String> {
        if src.is_empty() {
            return None;
        }
        let lower = src.get(..11).unwrap_or(src).to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return self.opts.remote_images.then(|| src.to_string());
        }
        if lower.starts_with("data:image/") {
            return Some(src.to_string());
        }
        let path = self.local_path(src)?;
        let bytes = image::read(&path)?;
        let mime = image_mime(&bytes)?;
        let len = bytes.len() as u64;
        if len <= MAX_EMBED && self.embedded + len <= MAX_TOTAL {
            self.embedded += len;
            return Some(format!("data:{mime};base64,{}", base64(&bytes)));
        }
        Some(file_url(&path))
    }

    /// The file a document's image `src` names, as the page looks for it.
    fn local_path(&self, src: &str) -> Option<PathBuf> {
        let base = self.opts.base_dir;
        let raw = PathBuf::from(src);
        let joined = match base {
            Some(dir) if raw.is_relative() => dir.join(&raw),
            _ => raw,
        };
        if joined.is_absolute() && joined.is_file() {
            return Some(joined);
        }
        match classify_image(src, base.unwrap_or(Path::new(""))) {
            ImageSource::Local(p) if p.is_absolute() && p.is_file() => Some(p),
            _ => None,
        }
    }

    // ---- footnotes -------------------------------------------------------------------------

    /// The footnotes the text refers to, numbered, after the page, as the viewer lists them.
    fn footnotes(&mut self) {
        let doc = self.doc;
        let mut notes: Vec<_> = doc
            .footnotes
            .iter()
            .filter(|f| f.number > 0 && f.defined)
            .collect();
        if notes.is_empty() {
            return;
        }
        notes.sort_by_key(|f| f.number);
        self.out
            .push_str("<section class=\"footnotes\">\n<hr>\n<ol>\n");
        for note in notes {
            let n = note.number;
            let _ = write!(self.out, "<li id=\"fn-{n}\">");
            if note.block != NONE {
                if doc.blocks[note.block as usize].end > note.block + 1 {
                    self.blocks(note.block);
                } else {
                    self.inline(note.block);
                }
            }
            let _ = writeln!(
                self.out,
                " <a class=\"back\" href=\"#fnref-{n}\">\u{21a9}</a></li>"
            );
        }
        self.out.push_str("</ol>\n</section>\n");
    }
}

// ---- pieces ------------------------------------------------------------------------------

fn picture_of(d: &DecodedImage) -> Option<Picture> {
    let png = png_bytes(d)?;
    Some(Picture {
        uri: format!("data:image/png;base64,{}", base64(&png)),
        width: d.width as f32 / 2.0,
        height: d.height as f32 / 2.0,
        baseline: d.baseline as f32 / 2.0,
        dark: None,
    })
}

/// `picture` with the drawing for the dark page, if there is one and it is not the same pixels.
fn with_dark(picture: Option<Picture>, dark: Option<DecodedImage>) -> Option<Picture> {
    let mut picture = picture?;
    if let Some(dark) = dark.and_then(|d| picture_of(&d))
        && dark.uri != picture.uri
    {
        picture.dark = Some(dark.uri);
    }
    Some(picture)
}

fn png_bytes(d: &DecodedImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, d.width, d.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&d.rgba).ok()?;
    }
    Some(out)
}

/// Rounded to a quarter px, without a trailing `.0`.
fn px(v: f32) -> String {
    let q = (v * 4.0).round() / 4.0;
    if q.fract() == 0.0 {
        format!("{}", q as i64)
    } else {
        format!("{q}")
    }
}

fn align_css(a: Align) -> Option<&'static str> {
    match a {
        Align::None => None,
        Align::Left => Some("text-align:left;"),
        Align::Center => Some("text-align:center;"),
        Align::Right => Some("text-align:right;"),
        Align::Justify => Some("text-align:justify;"),
    }
}

fn marker_style(m: Option<ListMarker>) -> String {
    let t = match m {
        None => return String::new(),
        Some(ListMarker::Decimal) => "decimal",
        Some(ListMarker::LowerAlpha) => "lower-alpha",
        Some(ListMarker::UpperAlpha) => "upper-alpha",
        Some(ListMarker::LowerRoman) => "lower-roman",
        Some(ListMarker::UpperRoman) => "upper-roman",
        Some(ListMarker::Disc) => "disc",
        Some(ListMarker::Circle) => "circle",
        Some(ListMarker::Square) => "square",
        Some(ListMarker::None) => "none",
    };
    format!(" style=\"list-style-type:{t}\"")
}

/// Whether a link may be written: a web or mail address, or one with no scheme (a relative file
/// or a `#heading`). Anything else (`javascript:`, `data:`, `file:`) is its text only.
fn safe_href(href: &str) -> bool {
    let href = href.trim();
    if href.is_empty() {
        return false;
    }
    let head: String = href
        .chars()
        .take_while(|c| !matches!(c, ':' | '/' | '?' | '#'))
        .collect();
    if head.len() == href.len() || !href[head.len()..].starts_with(':') {
        // No scheme: a relative reference.
        return true;
    }
    matches!(
        head.to_ascii_lowercase().as_str(),
        "http" | "https" | "mailto"
    )
}

/// Appends `text` escaped for HTML; `breaks` turns a newline (a hard break) into `<br>`.
fn escape_into(out: &mut String, text: &str, breaks: bool) {
    // The clipboard writer's escaping (`mdview_core::rich`), so both agree on it.
    mdview_core::rich::escape_html(out, text, if breaks { Some("<br>\n") } else { None });
}

/// The source with each run of one colour in a `<span>`. `dark` is the same source coloured for
/// the dark page: when its tokens cover the same ranges, each run also carries its dark colour as
/// `--d`, which the page's dark style puts to use.
fn highlighted(
    out: &mut String,
    source: &str,
    tokens: &[highlight::Token],
    dark: Option<&[highlight::Token]>,
) {
    let dark = dark.filter(|d| {
        d.len() == tokens.len() && d.iter().zip(tokens).all(|(a, b)| a.range == b.range)
    });
    type Run = ((Color, Option<Color>), usize);
    let mut at = 0;
    let mut run: Option<Run> = None;
    let flush = |out: &mut String, run: &mut Option<Run>, end: usize| {
        if let Some(((c, d), start)) = run.take() {
            let _ = write!(
                out,
                "<span style=\"color:#{:02x}{:02x}{:02x}",
                c[0], c[1], c[2]
            );
            if let Some(d) = d {
                let _ = write!(out, ";--d:#{:02x}{:02x}{:02x}", d[0], d[1], d[2]);
            }
            out.push_str("\">");
            escape_into(out, &source[start..end], false);
            out.push_str("</span>");
        }
    };
    for (i, t) in tokens.iter().enumerate() {
        let (s, e) = (t.range.start, t.range.end);
        if s < at
            || e > source.len()
            || s >= e
            || !source.is_char_boundary(s)
            || !source.is_char_boundary(e)
        {
            continue;
        }
        if s > at {
            flush(out, &mut run, at);
            escape_into(out, &source[at..s], false);
        }
        let colors = (t.color, dark.map(|d| d[i].color));
        match run {
            Some((c, _)) if c == colors => {}
            _ => {
                flush(out, &mut run, s);
                run = Some((colors, s));
            }
        }
        at = e;
    }
    flush(out, &mut run, at);
    if at < source.len() {
        escape_into(out, &source[at..], false);
    }
}

/// The type of a picture by its first bytes: the formats a browser shows that the page shows.
fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if image::looks_like_svg(bytes) {
        Some("image/svg+xml")
    } else {
        None
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A `file://` address for a path: it works on this machine only, which is what an image too
/// large to embed gets.
fn file_url(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A font list as CSS: each family quoted, the generic ones left as keywords.
fn css_families(list: &str) -> String {
    const GENERIC: &[&str] = &[
        "serif",
        "sans-serif",
        "monospace",
        "cursive",
        "fantasy",
        "system-ui",
    ];
    list.split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|f| {
            if GENERIC.contains(&f) {
                f.to_string()
            } else {
                format!("\"{}\"", f.replace(['"', '\\'], ""))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The colour variables of one palette: the page's colours and the five alerts'.
fn palette(theme: &Theme) -> String {
    use mdview_core::model::AlertKind::*;
    let mut css = String::new();
    let vars = [
        ("bg", hex(theme.bg)),
        ("text", hex(theme.text)),
        ("heading", hex(theme.heading)),
        ("muted", hex(theme.muted)),
        ("accent", hex(theme.accent)),
        ("border", hex(theme.border)),
        ("code-bg", hex(theme.code_bg)),
        ("head-bg", hex(theme.head_bg)),
        ("tint", hex(theme.tint)),
        ("mark", hex(theme.mark())),
    ];
    for (k, v) in &vars {
        let _ = write!(css, "--{k}:{v};");
    }
    for (name, kind) in [
        ("note", Note),
        ("tip", Tip),
        ("important", Important),
        ("warning", Warning),
        ("caution", Caution),
    ] {
        let (c, bg) = theme.alert(kind);
        let _ = write!(css, "--{name}:{};--{name}-bg:{};", hex(c), hex(bg));
    }
    css
}

/// The page's style sheet. The colours are the viewer's light palette, and under
/// `prefers-color-scheme: dark` its dark palette, whichever of the two the viewer showed when
/// the file was made: the page follows the reader's browser. Fonts and widths are the viewer's.
/// Printing keeps the light palette.
fn stylesheet(theme: &Theme) -> String {
    let mut css = String::new();
    let width = if theme.reading_width >= 100_000.0 {
        "none".to_string()
    } else {
        format!("{}px", (theme.reading_width + 2.0 * theme.margin) as i64)
    };
    let _ = writeln!(
        css,
        ":root{{color-scheme:light;{}--sans:{};--mono:{};--width:{width};--margin:{}px}}\n",
        palette(&crate::theme::LIGHT),
        css_families(theme.sans),
        css_families(theme.mono),
        theme.margin as i64
    );
    css.push_str(STYLE);
    let _ = write!(
        css,
        "@media screen and (prefers-color-scheme:dark){{\n:root{{color-scheme:dark;{}}}\n\
         span[style*=\"--d:\"]{{color:var(--d)!important}}\n\
         .lt{{display:none}}.dk{{display:inline}}\n}}\n",
        palette(&crate::theme::DARK)
    );
    css
}

const STYLE: &str = r##"html{background:var(--bg);-webkit-text-size-adjust:100%}
body{margin:0;color:var(--text);font:16px/1.6 var(--sans);overflow-wrap:break-word}
main{box-sizing:border-box;max-width:var(--width);margin:0 auto;padding:32px var(--margin) 48px}
h1,h2,h3,h4,h5,h6{color:var(--heading);font-weight:600;line-height:1.25;margin:0}
h1{font-size:34px;margin-bottom:14px}
h2{font-size:24px;margin:22px 0 12px}
h3{font-size:18px;margin:12px 0 8px}
h4{font-size:17px;margin:10px 0 8px}
h5{font-size:16px;margin:8px 0 6px}
h6{font-size:15px;margin:8px 0 6px}
p,ul,ol,pre,table,blockquote,figure,details,.alert,.dt,.dd{margin:0 0 12px}
p.math,p.diagram{text-align:center;margin:10px 0 18px}
img{max-width:100%;height:auto}
.dk{display:none}
a{color:var(--accent)}
hr{border:0;border-top:1px solid var(--border);margin:16px 0}
ul,ol{padding-left:2em}
li>ul,li>ol{margin:0}
li.task{list-style:none}
li.task>input{margin:0 .4em 0 -1.4em;vertical-align:middle}
code,kbd{font-family:var(--mono);font-size:.9em}
:not(pre)>code{background:var(--code-bg);border-radius:4px;padding:.1em .35em}
kbd{border:1px solid var(--border);border-bottom-width:2px;border-radius:4px;padding:.05em .4em;background:var(--code-bg)}
mark{background:var(--mark);color:inherit}
pre{position:relative;background:var(--code-bg);border:1px solid var(--border);border-radius:6px;padding:13px 16px;overflow:auto;font:14px/1.5 var(--mono);white-space:pre}
pre code{font:inherit;background:none;padding:0}
pre[data-lang]::after{content:attr(data-lang);position:absolute;top:6px;right:12px;font:12px var(--sans);color:var(--muted)}
blockquote{border-left:3px solid var(--border);margin-left:0;padding:2px 0 2px 16px;color:var(--muted)}
.alert{border-left:3px solid var(--note);background:var(--note-bg);padding:12px 16px;border-radius:0 4px 4px 0}
.alert>:last-child{margin-bottom:0}
.alert-title{font-weight:600;color:var(--note)}
.alert-tip{border-color:var(--tip);background:var(--tip-bg)}.alert-tip .alert-title{color:var(--tip)}
.alert-important{border-color:var(--important);background:var(--important-bg)}.alert-important .alert-title{color:var(--important)}
.alert-warning{border-color:var(--warning);background:var(--warning-bg)}.alert-warning .alert-title{color:var(--warning)}
.alert-caution{border-color:var(--caution);background:var(--caution-bg)}.alert-caution .alert-title{color:var(--caution)}
table{border-collapse:collapse;max-width:100%}
th,td{border:1px solid var(--border);padding:6px 13px;text-align:left;vertical-align:top}
th{background:var(--head-bg);font-weight:600}
summary{cursor:pointer;font-weight:600}
summary>div,summary>p{display:inline}
figure{margin-left:40px;margin-right:40px}
abbr[title]{text-decoration:underline dotted}
figcaption{font-size:.875em;color:var(--muted)}
.dt{font-weight:600;margin-bottom:2px}
.dd{margin-left:2em}
sup.fnref{font-size:.75em;line-height:0}
.footnotes{font-size:.9em;color:var(--muted)}
.footnotes ol{margin:0}
.footnotes p{margin:0;display:inline}
.footnotes .back{text-decoration:none}
@media print{
@page{margin:16mm}
html{background:#fff}
body{color:#000;font-size:11pt}
main{max-width:none;padding:0}
*{-webkit-print-color-adjust:exact;print-color-adjust:exact}
pre{white-space:pre-wrap;overflow:visible;break-inside:avoid}
pre[data-lang]::after{position:static;display:block;text-align:right;margin-bottom:4px}
table,figure,blockquote,.alert,img{break-inside:avoid}
tr{break-inside:avoid}
h1,h2,h3,h4,h5,h6{break-after:avoid}
a{color:inherit}
a[href^="http"]:not([href*="#"])::after{content:" (" attr(href) ")";font-size:.85em;color:var(--muted);word-break:break-all}
}
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::LIGHT;
    use mdview_core::parse;

    fn export(md: &str) -> String {
        export_with(md, None, false)
    }

    fn export_with(md: &str, base: Option<&Path>, remote: bool) -> String {
        // A formula or a diagram goes to the engine, which the engine's own tests count on
        // having to themselves.
        let _engine = crate::engine::TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let doc = parse(md);
        export_html(
            &doc,
            &ExportOptions {
                title: "",
                theme: LIGHT,
                base_dir: base,
                remote_images: remote,
                print: false,
            },
        )
    }

    fn body(html: &str) -> &str {
        let from = html.find("<main>").unwrap() + 7;
        let to = html.rfind("</main>").unwrap();
        &html[from..to]
    }

    #[test]
    fn base64_matches_the_standard_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn the_page_is_complete_and_self_contained() {
        let html = export("# Title\n\nHello *there* **you**.\n");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>Title</title>"));
        assert!(html.contains("<style>"));
        assert!(html.contains("<h1 id=\"title\">Title</h1>"));
        assert!(html.contains("<p>Hello <em>there</em> <strong>you</strong>.</p>"));
        assert!(!html.contains("<script"), "no script outside print");
        assert!(!html.contains("<link"), "no external style sheet");
        assert!(html.trim_end().ends_with("</html>"));
    }

    #[test]
    fn the_title_option_wins_and_is_escaped() {
        let doc = parse("# A\n");
        let html = export_html(
            &doc,
            &ExportOptions {
                title: "a <b> & c",
                theme: LIGHT,
                base_dir: None,
                remote_images: false,
                print: false,
            },
        );
        assert!(html.contains("<title>a &lt;b&gt; &amp; c</title>"));
    }

    #[test]
    fn print_adds_the_dialog_script_and_opens_details() {
        let doc = parse("<details><summary>S</summary>\n\nbody\n\n</details>\n");
        let make = |print| {
            export_html(
                &doc,
                &ExportOptions {
                    title: "t",
                    theme: LIGHT,
                    base_dir: None,
                    remote_images: false,
                    print,
                },
            )
        };
        let plain = make(false);
        let printed = make(true);
        assert!(!plain.contains("window.print"));
        assert!(!plain.contains("<details open"));
        assert!(printed.contains("window.print"));
        assert!(printed.contains("<details open"));
    }

    #[test]
    fn text_from_the_document_is_escaped() {
        let html = export("a <b>x</b> & \"q\" `<i>`\n");
        let b = body(&html);
        assert!(!b.contains("<script"));
        assert!(b.contains("&amp;"));
        assert!(b.contains("&lt;i&gt;"));
    }

    #[test]
    fn unsafe_links_lose_their_address() {
        let html = export(
            "[a](javascript:alert(1)) [b](https://example.com/x?y=1&z=2) [c](notes/a.md) [d](#top) [e](mailto:x@y.z) [f](data:text/html,hi)\n",
        );
        let b = body(&html);
        assert!(!b.contains("javascript"));
        assert!(!b.contains("data:text"));
        assert!(b.contains("<a href=\"https://example.com/x?y=1&amp;z=2\">b</a>"));
        assert!(b.contains("<a href=\"notes/a.md\">c</a>"));
        assert!(b.contains("<a href=\"#top\">d</a>"));
        assert!(b.contains("<a href=\"mailto:x@y.z\">e</a>"));
        assert!(safe_href("a/b:c"), "a colon after a slash is not a scheme");
    }

    #[test]
    fn lists_tasks_and_tables() {
        let html = export(
            "- [x] done\n- [ ] todo\n\n3. three\n4. four\n\n| a | b |\n|:-:|--:|\n| 1 | 2 |\n",
        );
        let b = body(&html);
        assert!(
            b.contains("<li class=\"task\"><input type=\"checkbox\" checked disabled> done</li>"),
            "{b}"
        );
        assert!(b.contains("<input type=\"checkbox\" disabled> todo"));
        assert!(b.contains("<ol start=\"3\">"));
        assert!(b.contains("<th style=\"text-align:center;\">a</th>"), "{b}");
        assert!(b.contains("<td style=\"text-align:right;\">2</td>"));
    }

    #[test]
    fn code_is_highlighted_with_inline_colours_and_escaped() {
        let html = export("```rust\nfn main() { let a = \"<x>\"; }\n```\n\n```\nplain <b>\n```\n");
        let b = body(&html);
        assert!(b.contains("<pre data-lang=\"rust\"><code>"), "{b}");
        assert!(b.contains("<span style=\"color:#"));
        assert!(b.contains("&lt;x&gt;"));
        assert!(b.contains("<pre><code>plain &lt;b&gt;</code></pre>"), "{b}");
    }

    #[test]
    fn vue_and_svelte_code_blocks_are_highlighted() {
        let vue = "```vue\n<template><p :id=\"id\">{{ msg }}</p></template>\n<script setup lang=\"ts\">\nconst msg = 'hi'\n</script>\n```\n";
        let svelte = "```svelte\n<script>\n  let n = 0;\n</script>\n{#if n > 1}<b on:click={() => n++}>{n}</b>{/if}\n```\n";
        for (md, lang) in [(vue, "vue"), (svelte, "svelte")] {
            let html = export(md);
            let b = body(&html);
            assert!(b.contains(&format!("<pre data-lang=\"{lang}\">")), "{b}");
            // More than one colour: markup, script and expressions are told apart.
            let colours: std::collections::HashSet<&str> = b
                .split("color:#")
                .skip(1)
                .filter_map(|s| s.get(..6))
                .collect();
            assert!(colours.len() >= 3, "{lang}: {colours:?}");
        }
    }

    #[test]
    fn a_formula_or_diagram_without_the_engine_keeps_its_source() {
        // Whether or not the engine is installed where the tests run, the source is not lost.
        let html = export("$$\nx^2\n$$\n\n```mermaid\ngraph TD; A-->B\n```\n");
        let b = body(&html);
        assert!(b.contains("x^2"), "{b}");
        assert!(
            b.contains("A--&gt;B") || b.contains("alt=\"graph TD; A--&gt;B\""),
            "{b}"
        );
    }

    #[test]
    fn footnotes_are_listed_at_the_end() {
        let html = export("Text[^1].\n\n[^1]: The note.\n");
        let b = body(&html);
        assert!(
            b.contains("<sup class=\"fnref\"><a href=\"#fn-1\" id=\"fnref-1\">1</a></sup>"),
            "{b}"
        );
        assert!(b.contains("<li id=\"fn-1\">"));
        assert!(b.contains("The note."));
        assert!(b.find("Text").unwrap() < b.find("The note.").unwrap());
    }

    #[test]
    fn alerts_and_quotes() {
        let html = export("> [!WARNING]\n> careful\n\n> quoted\n");
        let b = body(&html);
        assert!(b.contains("class=\"alert alert-warning\""), "{b}");
        assert!(b.contains("<p class=\"alert-title\">Warning</p>"));
        assert!(b.contains("<blockquote>"));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdview-export-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 1x1 PNG.
    const PIXEL: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0,
        0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44,
        0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f, 0, 5, 0xfe, 2, 0xfe, 0xa7, 0x35,
        0x81, 0x84, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn a_local_image_is_embedded_and_a_missing_one_is_its_alt_text() {
        let dir = scratch("img");
        std::fs::write(dir.join("p.png"), PIXEL).unwrap();
        std::fs::write(dir.join("notes.txt"), "not an image").unwrap();
        let md = "![pic](p.png \"T\") ![gone](nope.png) ![txt](notes.txt) ![up](../x.png)\n";
        let html = export_with(md, Some(&dir), false);
        let b = body(&html);
        assert!(b.contains("src=\"data:image/png;base64,iVBOR"), "{b}");
        assert!(b.contains("alt=\"pic\" title=\"T\""));
        assert!(b.contains("gone"));
        assert!(!b.contains("nope.png"));
        assert!(!b.contains("notes.txt"));
        // Without a base folder a relative path has nothing to be relative to.
        let html = export_with("![pic](p.png)\n", None, false);
        assert!(!body(&html).contains("<img"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_images_follow_the_readers_choice() {
        let md = "![r](https://example.com/a.png)\n";
        let off = export_with(md, None, false);
        assert!(!body(&off).contains("example.com"));
        assert!(body(&off).contains('r'));
        let on = export_with(md, None, true);
        assert!(body(&on).contains("<img src=\"https://example.com/a.png\""));
    }

    #[test]
    fn a_data_image_is_kept_and_other_data_is_not() {
        let md = "![a](data:image/png;base64,AAAA) ![b](data:text/html;base64,AAAA)\n";
        let html = export(md);
        let b = body(&html);
        assert!(b.contains("data:image/png;base64,AAAA"));
        assert!(!b.contains("text/html"));
    }

    #[test]
    fn file_urls_are_escaped_and_windows_paths_get_a_slash() {
        assert_eq!(
            file_url(Path::new("/a b/c#d.png")),
            "file:///a%20b/c%23d.png"
        );
        assert_eq!(
            file_url(Path::new("C:\\x\\y z.png")),
            "file:///C:/x/y%20z.png"
        );
    }

    #[test]
    fn the_style_sheet_follows_the_theme() {
        let mut theme = LIGHT.with_fonts("Noto Serif", "");
        theme.reading_width = 1000.0;
        let css = stylesheet(&theme);
        assert!(
            css.contains("--sans:\"Noto Serif\", \"Segoe UI\""),
            "{}",
            &css[..300]
        );
        assert!(css.contains("sans-serif"));
        assert!(!css.contains("\"sans-serif\""));
        assert!(css.contains("--width:1080px"));
        theme.reading_width = f32::MAX;
        assert!(stylesheet(&theme).contains("--width:none"));
        assert!(css.contains("@media print"));
    }

    #[test]
    fn highlight_runs_merge_and_cover_the_source() {
        let source = "ab cd";
        let tokens = vec![
            highlight::Token {
                range: 0..1,
                color: [1, 2, 3],
            },
            highlight::Token {
                range: 1..2,
                color: [1, 2, 3],
            },
            highlight::Token {
                range: 3..5,
                color: [9, 9, 9],
            },
        ];
        let mut out = String::new();
        highlighted(&mut out, source, &tokens, None);
        assert_eq!(
            out,
            "<span style=\"color:#010203\">ab</span> <span style=\"color:#090909\">cd</span>"
        );
    }

    #[test]
    fn a_hard_break_is_a_br_and_headings_carry_ids() {
        let html = export("line one  \nline two\n\n## Two words\n");
        let b = body(&html);
        assert!(b.contains("line one<br>"), "{b}");
        assert!(b.contains("<h2 id=\"two-words\">Two words</h2>"));
    }

    #[test]
    fn an_empty_document_still_makes_a_page() {
        let html = export("");
        assert!(html.contains("<title>Document</title>"));
        assert!(body(&html).trim().is_empty());
    }

    #[test]
    fn a_link_around_a_picture_alone_keeps_both() {
        let dir = scratch("linked-image");
        std::fs::write(dir.join("p.png"), PIXEL).unwrap();
        let html = export_with(
            "<p align=\"center\"><b>Head</b><br><a href=\"https://example.com\"><img src=\"p.png\" alt=\"badge\" width=\"18\"></a></p>\n\n[![alt](p.png)](https://example.org)\n",
            Some(&dir),
            false,
        );
        let b = body(&html);
        assert!(
            b.contains("<a href=\"https://example.com\"><img src=\"data:image/png;base64,"),
            "{b}"
        );
        assert!(
            b.contains("<a href=\"https://example.org\"><img src=\"data:image/png;base64,"),
            "{b}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_summary_stays_on_the_line_of_its_marker() {
        let html = export("<details>\n<summary>More</summary>\n\nBody.\n\n</details>\n");
        assert!(html.contains("summary>div,summary>p{display:inline}"));
        assert!(body(&html).contains("<summary>"));
    }

    #[test]
    fn an_abbreviation_keeps_its_title_and_a_bare_one_is_plain_text() {
        let html = export(
            "The <abbr title=\"A &amp; B\">AB</abbr> and <abbr>CD</abbr> and <time>now</time>.",
        );
        assert!(
            body(&html).contains("<abbr title=\"A &amp; B\">AB</abbr> and CD and now."),
            "{}",
            body(&html)
        );
        assert!(html.contains("abbr[title]{text-decoration:underline dotted}"));
    }

    #[test]
    fn a_table_is_placed_and_its_columns_sized_by_the_html_attributes() {
        let html = export(
            "<table width=\"50%\" align=\"center\"><tr align=\"center\"><td width=\"30\">a</td>\
             <td align=\"right\" valign=\"bottom\">b</td><td width=\"25%\">c</td></tr>\
             <tr valign=\"middle\"><td>d</td></tr></table>\n",
        );
        let b = body(&html);
        assert!(
            b.contains("<table style=\"margin-left:auto;margin-right:auto;width:50%;\">"),
            "{b}"
        );
        // A cell that states no `align` takes its row's; `width` is the column's.
        assert!(
            b.contains("<td style=\"text-align:center;width:30px;\">a</td>"),
            "{b}"
        );
        assert!(
            b.contains("<td style=\"text-align:right;vertical-align:bottom;\">b</td>"),
            "{b}"
        );
        assert!(
            b.contains("<td style=\"text-align:center;width:25%;\">c</td>"),
            "{b}"
        );
        assert!(
            b.contains("<td style=\"vertical-align:middle;\">d</td>"),
            "{b}"
        );
    }

    #[test]
    fn a_table_in_center_or_an_aligned_div_is_placed_as_the_viewer_does() {
        let b = export("<center>\n\n| a |\n|---|\n| b |\n\n</center>\n");
        assert!(
            body(&b).contains("<table style=\"margin-left:auto;margin-right:auto;\">"),
            "{}",
            body(&b)
        );
        let b = export("<div align=\"right\">\n\n| a |\n|---|\n| b |\n\n</div>\n");
        assert!(
            body(&b).contains("<table style=\"margin-left:auto;margin-right:0;\">"),
            "{}",
            body(&b)
        );
        // Its own `align` wins over the one around it; a plain table has no margins of its own.
        let b = export(
            "<div align=\"right\"><table align=\"left\"><tr><td>x</td></tr></table></div>\n",
        );
        assert!(
            body(&b).contains("<table>") && !body(&b).contains("margin"),
            "{}",
            body(&b)
        );
        let b = export("| a |\n|---|\n| b |\n");
        assert!(body(&b).contains("<table>"), "{}", body(&b));
    }

    #[test]
    fn the_page_follows_the_readers_colour_scheme_whichever_theme_made_it() {
        for theme in [LIGHT, crate::theme::DARK] {
            let _engine = crate::engine::TEST_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let doc = parse("```rust\nfn main() {}\n```\n");
            let html = export_html(
                &doc,
                &ExportOptions {
                    title: "",
                    theme,
                    base_dir: None,
                    remote_images: false,
                    print: false,
                },
            );
            let light = html
                .find(":root{color-scheme:light;--bg:#fbfbfb;")
                .expect("light root");
            let dark = html
                .find("@media screen and (prefers-color-scheme:dark){")
                .expect("a dark block");
            assert!(light < dark);
            let block = &html[dark..html[dark..]
                .find("</style>")
                .map_or(html.len(), |e| dark + e)];
            assert!(
                block.contains(":root{color-scheme:dark;--bg:#202020;--text:#e4e4e4;"),
                "{block}"
            );
            assert!(block.contains("--note:#60cdff;") && block.contains("--caution:#f85149;"));
            assert!(block.contains("span[style*=\"--d:\"]{color:var(--d)!important}"));
            // The code carries both colours, the page being one file for both.
            assert!(body(&html).contains(";--d:#"), "{}", body(&html));
        }
    }

    #[test]
    fn printing_keeps_the_light_palette_and_a_themed_picture_is_written_for_each_scheme() {
        let dir = scratch("themed");
        std::fs::write(dir.join("light.png"), PIXEL).unwrap();
        let mut other = PIXEL.to_vec();
        *other.last_mut().unwrap() ^= 1;
        std::fs::write(dir.join("dark.png"), &other).unwrap();
        let md = "<picture><source media=\"(prefers-color-scheme: dark)\" srcset=\"dark.png\">\
                  <source media=\"(prefers-color-scheme: light)\" srcset=\"light.png\">\
                  <img src=\"light.png\" alt=\"themed\"></picture>\n\n![plain](light.png)\n";
        let html = export_with(md, Some(&dir), false);
        let b = body(&html);
        assert!(
            b.contains("<img class=\"lt\" src=\"data:image/png;base64,"),
            "{b}"
        );
        assert!(
            b.contains("<img class=\"dk\" src=\"data:image/png;base64,"),
            "{b}"
        );
        // An image with no twin is written once and carries no scheme class.
        assert!(b.contains("<img src=\"data:image/png;base64,"), "{b}");
        assert_eq!(b.matches("alt=\"themed\"").count(), 2);
        assert!(html.contains(".dk{display:none}"));
        assert!(html.contains(".lt{display:none}.dk{display:inline}"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn highlight_runs_carry_the_dark_colour_when_both_lists_agree() {
        let source = "ab cd";
        let token = |range, color| highlight::Token { range, color };
        let light = vec![token(0..2, [1, 2, 3]), token(3..5, [4, 5, 6])];
        let dark = vec![token(0..2, [7, 8, 9]), token(3..5, [7, 8, 9])];
        let mut out = String::new();
        highlighted(&mut out, source, &light, Some(&dark));
        assert_eq!(
            out,
            "<span style=\"color:#010203;--d:#070809\">ab</span> \
             <span style=\"color:#040506;--d:#070809\">cd</span>"
        );
        // Lists that do not cover the same ranges leave the light colours alone.
        let other = vec![token(0..1, [7, 8, 9])];
        let mut out = String::new();
        highlighted(&mut out, source, &light, Some(&other));
        assert!(!out.contains("--d"), "{out}");
    }
}
