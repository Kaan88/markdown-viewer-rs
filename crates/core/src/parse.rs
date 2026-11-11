//! Streams pulldown-cmark events into the document model. No event is kept after it is handled.

use crate::autolink::{self, Entities, LinkKind, SWAP_PREFIX};
use crate::html::{self, Tag, Token, Tokenizer};
use crate::limits::{
    MAX_BLOCK_DEPTH, MAX_COLSPAN, MAX_DECODED_BYTES, MAX_DIAGRAM_BYTES, MAX_EQUATION_BYTES,
    MAX_HTML_DEPTH, MAX_HTML_LENGTH_PX, MAX_IMAGE_DIMENSION_PX, MAX_ROWSPAN, Oversize,
};
use crate::model::*;
use crate::slug::Slugger;
use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, LinkType, Options, Parser, Tag as MdTag,
    TagEnd,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use unicase::UniCase;

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
}

/// Parses Markdown into a document. A leading BOM is ignored. Text over what any file within the limit
/// can decode to yields an empty document with a warning, which also keeps every offset within `u32`.
pub fn parse(src: &str) -> Document {
    let src = src.strip_prefix('\u{FEFF}').unwrap_or(src);
    if src.len() > MAX_DECODED_BYTES {
        return Document {
            warnings: vec![Warning::FileTooLarge],
            ..Document::default()
        };
    }
    // YAML front matter becomes a table (or a yaml block) before anything else sees the source.
    let with_front;
    let src = match crate::frontmatter::take(src) {
        Some((head, used)) => {
            with_front = format!("{head}{}", &src[used..]);
            with_front.as_str()
        }
        None => src,
    };
    // CommonMark requires this, and pulldown-cmark leaves NUL in its text events.
    let no_nul;
    let src = if memchr::memchr(0, src.as_bytes()).is_some() {
        no_nul = src.replace('\0', "\u{FFFD}");
        no_nul.as_str()
    } else {
        src
    };
    // GitHub's `$`x`$` inline math is rewritten to `$x$` before parsing.
    let github;
    let src = match github_math(src) {
        Cow::Owned(rewritten) => {
            github = rewritten;
            github.as_str()
        }
        Cow::Borrowed(same) => same,
    };
    let protected = autolink::protect_url_delimiters(src);
    let restore = matches!(protected, Cow::Owned(_));
    if memchr::memchr(b'$', protected.as_bytes()).is_none() {
        let mut b = Builder::new(&protected, restore, false);
        for (event, range) in Parser::new_ext(&protected, options()).into_offset_iter() {
            b.event(event, range);
        }
        if b.rejected_alerts.is_empty() {
            return b.into_document();
        }
        let rejected = std::mem::take(&mut b.rejected_alerts);
        drop(b);
        return parse_with_checks(protected, restore, rejected);
    }
    parse_with_checks(protected, restore, Vec::new())
}

/// GitHub writes inline math as ``$`x`$`` so that Markdown cannot touch the formula. This rewrites
/// each such pair on one line to `$x$`, outside fenced code. A formula containing `$` or a backtick
/// is left alone. Documents without the two-character opener are not copied.
fn github_math(src: &str) -> Cow<'_, str> {
    if memchr::memmem::find(src.as_bytes(), b"$`").is_none() {
        return Cow::Borrowed(src);
    }
    let mut out = String::with_capacity(src.len());
    let mut fence: Option<(char, usize)> = None;
    let mut changed = false;
    for line in src.split_inclusive('\n') {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let rest = line[indent..].trim_end_matches(['\n', '\r']);
        let marker = rest.chars().next().filter(|c| matches!(c, '`' | '~'));
        let run = marker.map_or(0, |m| rest.chars().take_while(|&c| c == m).count());
        match (fence, marker) {
            (Some((m, n)), Some(c))
                if indent < 4 && c == m && run >= n && rest[run..].trim().is_empty() =>
            {
                fence = None;
            }
            (None, Some(c))
                if indent < 4 && run >= 3 && !(c == '`' && rest[run..].contains('`')) =>
            {
                fence = Some((c, run));
            }
            _ => {}
        }
        if fence.is_some() || marker.is_some() && run >= 3 || !line.contains("$`") {
            out.push_str(line);
            continue;
        }
        let mut at = 0;
        while let Some(open) = line[at..].find("$`").map(|i| at + i) {
            let body = open + 2;
            let close = line[body..].find("`$").map(|i| body + i);
            match close {
                Some(close) if close > body && !line[body..close].contains(['`', '$']) => {
                    out.push_str(&line[at..open]);
                    out.push('$');
                    out.push_str(&line[body..close]);
                    out.push('$');
                    at = close + 2;
                    changed = true;
                }
                _ => {
                    out.push_str(&line[at..body]);
                    at = body;
                }
            }
        }
        out.push_str(&line[at..]);
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(src)
    }
}

/// Passes that may hide rejected delimiters before the rest are shown as literal text. The first
/// usually settles everything, so only a hostile input reaches the limit.
const MATH_PASSES: usize = 4;

/// pulldown-cmark pairs `$` with no regard for what surrounds it, and whatever Markdown sits between
/// a bad pair is already lost by the time the event arrives. It also takes an alert marker wherever
/// it finds one, and the line it takes may have belonged to a paragraph. So rejected delimiters are
/// hidden from the parser and the source is parsed again. `rejected` holds those already known.
fn parse_with_checks(mut src: Cow<'_, str>, mut restore: bool, rejected: Vec<usize>) -> Document {
    // Hiding swaps in characters from U+E003 upward. A document that already uses any character
    // from that private-use block would have its own text rewritten by `restore`, so such a
    // document takes the literal pass instead.
    let can_hide = memchr::memmem::find(src.as_bytes(), SWAP_PREFIX).is_none();
    if can_hide && !rejected.is_empty() {
        src = Cow::Owned(hide_delimiters(&src, &rejected));
        restore = true;
    }
    let passes = if can_hide { MATH_PASSES } else { 0 };
    for _ in 0..passes {
        match build_checked(&src, restore, false) {
            Ok(doc) => return doc,
            Err(rejected) => {
                src = Cow::Owned(hide_delimiters(&src, &rejected));
                restore = true;
            }
        }
    }
    match build_checked(&src, restore, true) {
        Ok(doc) => doc,
        Err(_) => unreachable!("the literal pass rejects nothing"),
    }
}

/// With `literal`, a rejected span or alert becomes plain text. Otherwise the first rejected span stops
/// the build, and the rest of the walk only collects the `$` offsets to hide.
fn build_checked(src: &str, restore: bool, literal: bool) -> Result<Document, Vec<usize>> {
    let mut b = Builder::new(src, restore, literal);
    let mut gate = MathGate {
        boundary: true,
        inline: 0..src.len(),
    };
    let mut rejected = Vec::new();
    let mut settled_until = 0;
    for (event, range) in Parser::new_ext(src, options()).into_offset_iter() {
        let is_math = matches!(event, Event::InlineMath(_) | Event::DisplayMath(_));
        let accepted = !is_math || gate.accepts(src, &range);
        gate.note(&event, &range);
        if accepted {
            if rejected.is_empty() {
                b.event(event, range);
            }
        } else if literal {
            b.event(Event::Text(src[range.clone()].into()), range);
        } else if range.start >= settled_until {
            // The rest of this block's pairs are unreliable now, because each came from pairing
            // with whatever the rejected ones used up. They all get decided here, at once.
            rejected.extend(rejected_dollars(src, gate.inline.clone()));
            rejected.push(range.start);
            settled_until = gate.inline.end;
        }
    }
    rejected.append(&mut b.rejected_alerts);
    if rejected.is_empty() || literal {
        Ok(b.into_document())
    } else {
        rejected.sort_unstable();
        rejected.dedup();
        Err(rejected)
    }
}

/// Each offset is at a `$` or at the `[` of an alert marker.
fn hide_delimiters(src: &str, offsets: &[usize]) -> String {
    let mut out = String::with_capacity(src.len() + offsets.len() * 2);
    let mut pos = 0;
    for &at in offsets {
        out.push_str(&src[pos..at]);
        out.push(if src.as_bytes()[at] == b'[' {
            autolink::HIDDEN_BRACKET
        } else {
            autolink::HIDDEN_DOLLAR
        });
        pos = at + 1;
    }
    out.push_str(&src[pos..]);
    out
}

/// GitHub's rule for what may touch a math span, measured against `gh api markdown` since it is not
/// documented. See docs/COVERAGE.md for where this differs.
struct MathGate {
    /// Whether whatever came just before the next event allows a span to open there.
    boundary: bool,
    /// Source of the innermost block that holds inline content.
    inline: Range<usize>,
}

impl MathGate {
    fn note(&mut self, event: &Event<'_>, range: &Range<usize>) {
        self.boundary = match event {
            Event::Text(t) => t
                .as_bytes()
                .last()
                .is_none_or(|c| c.is_ascii_whitespace() || *c == b'('),
            // `**$x$**` and `~~$x$~~` render, yet `*$x$*`, `***$x$***` and `[$x$](u)` do not.
            Event::Start(MdTag::Strong | MdTag::Strikethrough) => self.boundary,
            Event::Start(MdTag::Emphasis | MdTag::Link { .. } | MdTag::Image { .. }) => false,
            Event::InlineMath(_) | Event::DisplayMath(_) => false,
            Event::InlineHtml(t) | Event::Html(t) => match Tokenizer::new(t).next() {
                Some(Token::Close { .. }) => true,
                Some(Token::Open { name, .. }) => html::classify_tag(name) == Tag::Br,
                _ => false,
            },
            _ => true,
        };
        if let Event::Start(
            MdTag::Paragraph | MdTag::Heading { .. } | MdTag::TableCell | MdTag::Item,
        ) = event
        {
            self.inline = range.clone();
        }
    }

    fn accepts(&self, src: &str, range: &Range<usize>) -> bool {
        self.boundary && may_follow_math(src.as_bytes(), range.end)
    }
}

/// `$c$-d` and `$c$é` render, while `$c$d`, `$c$_` and a span ending an emphasis do not.
fn may_follow_math(b: &[u8], at: usize) -> bool {
    match b.get(at) {
        Some(b'_') => false,
        Some(b'*') => b.get(at + 1) == Some(&b'*'),
        Some(c) => !c.is_ascii_alphanumeric(),
        None => true,
    }
}

/// `MathGate`'s rule for the source text before `at`, for use without events.
fn may_precede_math(src: &str, at: usize, start: usize) -> bool {
    let b = src.as_bytes();
    if at <= start {
        return true;
    }
    match b[at - 1] {
        c if c.is_ascii_whitespace() => true,
        b'(' | b'`' => true,
        c @ (b'*' | b'~' | b'_') => {
            let run = b[start..at].iter().rev().take_while(|&&x| x == c).count();
            let closes_a_span = at - run > start && b[at - run - 1].is_ascii_alphanumeric();
            match c {
                b'*' => run == 2 || (run == 1 && closes_a_span),
                b'~' => run <= 2,
                _ => run == 1 && closes_a_span,
            }
        }
        b'>' => {
            // The longest tag that counts is `<br />`, and `</x>` needs only the slash to be seen.
            let from = start.max(at.saturating_sub(16));
            memchr::memrchr(b'<', &b[from..at]).is_some_and(|i| {
                let tag = &b[from + i + 1..at - 1];
                tag.first() == Some(&b'/')
                    || tag.trim_ascii_end().eq_ignore_ascii_case(b"br")
                    || tag
                        .strip_suffix(b"/")
                        .is_some_and(|t| t.trim_ascii_end().eq_ignore_ascii_case(b"br"))
            })
        }
        _ => false,
    }
}

/// Goes through one block's `$` delimiters in a single sweep, using the parser's pairing rule plus
/// GitHub's: a `$` can open if a non-space follows, it can close if a non-space precedes, and the next
/// delimiter is the only one it can pair with. Returns the offsets of every `$` not in a good pair.
///
/// An opener that fails takes only itself out, because its partner may still open a good span: in
/// `y$. **$a$**` the first pair would be `$. **$`, and `$a$` has to survive. A closer that fails takes
/// both, since GitHub shows all three of `$x$y$` literally.
fn rejected_dollars(src: &str, block: Range<usize>) -> Vec<usize> {
    let b = src.as_bytes();
    let d = math_delimiters(b, block.clone());
    let double_at = |k: usize| d.get(k + 1).is_some_and(|&next| next == d[k] + 1);
    let mut out = Vec::new();
    let mut k = 0;
    while k < d.len() {
        let width = if double_at(k) { 2 } else { 1 };
        let closer = k + width;
        let pairs = closer < d.len()
            && (width == 1 || double_at(closer))
            && !b[d[k] + width].is_ascii_whitespace()
            && !b[d[closer] - 1].is_ascii_whitespace();
        if pairs && may_precede_math(src, d[k], block.start) {
            if !may_follow_math(b, d[closer] + width) {
                out.extend(d[k..closer + width].iter().copied());
            }
            k = closer + width;
        } else {
            out.extend(d[k..closer].iter().copied());
            k = closer;
        }
    }
    out
}

/// Offsets of each `$`, leaving out escaped ones and those in code spans.
fn math_delimiters(b: &[u8], block: Range<usize>) -> Vec<usize> {
    const MAX_FENCE: usize = 32;
    let mut out = Vec::new();
    let mut no_closer = [false; MAX_FENCE];
    let mut i = block.start;
    while i < block.end {
        match b[i] {
            b'\\' => i += 2,
            b'`' => {
                let run = b[i..block.end].iter().take_while(|&&c| c == b'`').count();
                i += run;
                // Remembering a length with no closer keeps this linear on unmatched runs.
                if run < MAX_FENCE && !no_closer[run] {
                    match closing_backticks(&b[i..block.end], run) {
                        Some(end) => i += end,
                        None => no_closer[run] = true,
                    }
                }
            }
            b'$' => {
                out.push(i);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn closing_backticks(b: &[u8], run: usize) -> Option<usize> {
    let mut i = 0;
    while let Some(at) = memchr::memchr(b'`', &b[i..]) {
        let start = i + at;
        let len = b[start..].iter().take_while(|&&c| c == b'`').count();
        if len == run {
            return Some(start + len);
        }
        i = start + len;
    }
    None
}

#[derive(Clone, Copy, PartialEq)]
enum Origin {
    Md,
    /// Opened implicitly around inline content.
    Auto,
    Html(Tag),
}

#[derive(Clone, Copy, PartialEq)]
enum Node {
    Block(u32),
    Span(u32),
    /// Balances an event or tag that produced nothing, such as a collapsed nested strong.
    Nothing,
}

#[derive(Clone, Copy)]
struct Open {
    node: Node,
    origin: Origin,
}

/// An inline HTML tag left open at the end of an HTML block. It wraps the next blocks' inline
/// content until closed, which is how `<a href=x>`, a blank line, then an image is meant to work.
struct Ambient {
    tag: Tag,
    kind: SpanKind,
    depth: usize,
}

struct Builder<'a> {
    src: &'a str,
    restore: bool,
    doc: Document,
    stack: Vec<Open>,
    cur_block: u32,
    cur_span: u32,
    inline_open: bool,
    inline_block: u32,
    inline_start: usize,
    atom_end: usize,
    // Start of the text run still waiting for the autolink pass, and its escaped characters.
    run_start: Option<usize>,
    escapes: Vec<(usize, usize)>,
    link_hits: Vec<(usize, usize, LinkKind)>,
    link_depth: u32,
    strong_depth: u32,
    in_code_block: bool,
    html_buf: Option<String>,
    html_store: String,
    pre_open: bool,
    pre_first_newline: bool,
    ambient: Vec<Ambient>,
    ambient_spans_left: usize,
    open_html: [u32; Tag::SLOTS],
    /// Inside a `<rp>`, whose text only a browser with no ruby support shows. It ends with its
    /// closer, the next `<rt>` or `</ruby>`, or the block, so a stray `<rp>` cannot hide the rest.
    in_rp: bool,
    html_depth: usize,
    image_depth: u32,
    image_alt_start: usize,
    image_idx: u32,
    in_picture: bool,
    // The block the picture opened in, and how deep the stack was: a source counts only as a direct
    // child, and the picture ends when its block does.
    picture_block: u32,
    picture_depth: usize,
    picture_dark: StrRef,
    picture_light: StrRef,
    aligns: Vec<Alignment>,
    col: usize,
    header_row: bool,
    inline_limit: usize,
    slugger: Slugger,
    in_heading: bool,
    heading_text: String,
    // End of the last text event: the backslashes before it belong to that event.
    text_floor: usize,
    footnote_ids: HashMap<UniCase<String>, u32>,
    next_footnote_number: u32,
    // Events to ignore until a repeated footnote definition ends. The first definition wins.
    skip_depth: u32,
    // Text ranges of named entities in the current run, which a bare URL can't end on.
    entities: Vec<(usize, usize)>,
    block_depth: usize,
    // Implied table rows, cells and list items, innermost last. They exist only to give stray
    // content a legal home, so an empty one is dropped when it closes instead of drawing a gap.
    implied: Vec<u32>,
    // At most one: an alert can't nest. The block, and where its marker's `[` is in the source.
    open_alert: Option<(u32, usize)>,
    // Whether anything visible was added inside the open alert. GitHub keeps an alert whose body is
    // only empty inline HTML (`<i></i>`, `<a href=x></a>`, `<p></p>`, `<img>`), though nothing of it
    // reaches the model. A stray `<foo>` or a raw `<input>` adds nothing there either.
    alert_content: bool,
    rejected_alerts: Vec<usize>,
    // The last pass, or a document that can't hide a marker: a rejected one is written out instead.
    literal_alerts: bool,
}

impl<'a> Builder<'a> {
    fn new(src: &'a str, restore: bool, literal_alerts: bool) -> Self {
        Builder {
            src,
            restore,
            doc: Document {
                text: String::with_capacity(src.len()),
                blocks: Vec::with_capacity(src.len() / 64),
                spans: Vec::with_capacity(src.len() / 64),
                ..Document::default()
            },
            stack: Vec::with_capacity(32),
            cur_block: NONE,
            cur_span: NONE,
            inline_open: false,
            inline_block: NONE,
            inline_start: 0,
            atom_end: usize::MAX,
            run_start: None,
            escapes: Vec::new(),
            link_hits: Vec::new(),
            link_depth: 0,
            strong_depth: 0,
            in_code_block: false,
            html_buf: None,
            html_store: String::new(),
            pre_open: false,
            pre_first_newline: false,
            ambient: Vec::new(),
            ambient_spans_left: src.len() / 4 + 64,
            open_html: [0; Tag::SLOTS],
            in_rp: false,
            html_depth: 0,
            image_depth: 0,
            image_alt_start: 0,
            image_idx: 0,
            in_picture: false,
            picture_block: NONE,
            picture_depth: 0,
            picture_dark: StrRef::default(),
            picture_light: StrRef::default(),
            aligns: Vec::new(),
            col: 0,
            header_row: false,
            inline_limit: 0,
            slugger: Slugger::default(),
            in_heading: false,
            heading_text: String::new(),
            text_floor: 0,
            footnote_ids: HashMap::new(),
            next_footnote_number: 0,
            skip_depth: 0,
            entities: Vec::new(),
            block_depth: 0,
            implied: Vec::new(),
            open_alert: None,
            alert_content: false,
            rejected_alerts: Vec::new(),
            literal_alerts,
        }
    }

    fn event(&mut self, event: Event<'a>, range: Range<usize>) {
        if self.image_depth > 0 {
            return self.image_event(event);
        }
        if self.skip_depth > 0 {
            match event {
                Event::Start(_) => self.skip_depth += 1,
                Event::End(_) => self.skip_depth -= 1,
                // A reference inside a duplicate definition still counts on GitHub: it numbers the
                // note and puts it in the list, even though this definition's text is dropped.
                Event::FootnoteReference(label) => {
                    self.number_footnote(&label);
                }
                _ => {}
            }
            return;
        }
        if self.pre_open {
            return self.pre_event(event);
        }
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(TagEnd::HtmlBlock) => self.end_html_block(),
            Event::End(_) => self.end(),
            Event::Text(t) if self.in_code_block => self.push_str_restored(&t),
            Event::Text(t) => self.md_text(&t, range),
            Event::Code(t) => {
                self.begin_inline();
                self.flush_run();
                self.note_heading_text(&t);
                let start = self.doc.text.len();
                self.push_str_restored(&t);
                self.push_atom_like(SpanKind::Code, start);
            }
            Event::InlineMath(t) => self.math(&t, false),
            Event::DisplayMath(t) => self.math(&t, true),
            Event::Html(t) => match &mut self.html_buf {
                Some(buf) => {
                    if self.restore {
                        buf.push_str(&autolink::restore(&t));
                    } else {
                        buf.push_str(&t);
                    }
                }
                None => self.html_tokens(&autolink::restore(&t)),
            },
            Event::InlineHtml(t) => {
                if self.restore {
                    self.html_tokens(&autolink::restore(&t));
                } else {
                    self.html_tokens(&t);
                }
            }
            Event::FootnoteReference(label) => {
                self.begin_inline();
                let id = self.number_footnote(&label);
                if self.in_heading {
                    let number = self.doc.footnotes[id as usize].number.to_string();
                    self.heading_text.push_str(&number);
                }
                self.push_atom(SpanKind::FootnoteRef(id));
            }
            Event::SoftBreak => self.line_break(' '),
            Event::HardBreak => self.line_break('\n'),
            Event::Rule => {
                self.close_inline_for_block();
                // Under a marker line, `---` makes that line a setext heading, so no alert. Only
                // when no blank line separates them: after a blank line it is an ordinary rule.
                if self.first_in_open_alert()
                    && self.underlines_open_alert(range.start)
                    && self.src[range].trim_end().bytes().all(|c| c == b'-')
                {
                    self.reject_open_alert();
                }
                let b = self.open_block(BlockKind::ThematicBreak, Origin::Md);
                self.pop_open();
                self.finish_block(b);
            }
            Event::TaskListMarker(checked) => {
                self.begin_inline();
                self.mark_task_item();
                self.push_atom(SpanKind::Checkbox { checked });
            }
        }
    }

    fn image_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(_) => self.image_depth += 1,
            Event::End(_) => {
                self.image_depth -= 1;
                if self.image_depth == 0 {
                    let alt = StrRef {
                        start: self.image_alt_start as u32,
                        len: (self.doc.strings.len() - self.image_alt_start) as u32,
                    };
                    self.doc.images[self.image_idx as usize].alt = alt;
                }
            }
            Event::Text(t) | Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => {
                if self.restore {
                    self.doc.strings.push_str(&autolink::restore(&t));
                } else {
                    self.doc.strings.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => self.doc.strings.push(' '),
            _ => {}
        }
    }

    /// Markdown inside a `<pre>` opened by an inline tag: a code block holds plain text only.
    fn pre_event(&mut self, event: Event<'a>) {
        match event {
            Event::Start(_) => self.push_open(Open {
                node: Node::Nothing,
                origin: Origin::Md,
            }),
            Event::End(_) => self.end(),
            Event::Text(t) | Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => {
                let restored = if self.restore {
                    autolink::restore(&t)
                } else {
                    Cow::Borrowed(&*t)
                };
                self.pre_text(&restored);
            }
            Event::SoftBreak | Event::HardBreak => self.pre_text("\n"),
            Event::Html(t) | Event::InlineHtml(t) => self.html_tokens(&autolink::restore(&t)),
            _ => {}
        }
    }

    fn start(&mut self, tag: MdTag<'_>, range: Range<usize>) {
        match tag {
            MdTag::Emphasis => self.open_md_span(SpanKind::Emphasis),
            MdTag::Strong => self.open_strong(Origin::Md),
            MdTag::Strikethrough => self.open_md_span(SpanKind::Strike),
            MdTag::Link {
                link_type,
                dest_url,
                title,
                ..
            } => {
                let dest = if matches!(link_type, LinkType::Email) {
                    self.pool_restored(&format!("mailto:{dest_url}"))
                } else {
                    self.pool_restored(&dest_url)
                };
                let title = self.pool_restored(&title);
                let idx = self.doc.links.len() as u32;
                self.doc.links.push(LinkInfo { dest, title });
                self.open_md_span(SpanKind::Link(idx));
            }
            MdTag::Image {
                dest_url, title, ..
            } => {
                self.begin_inline();
                let src = self.pool_restored(&dest_url);
                let title = self.pool_restored(&title);
                self.image_idx = self.doc.images.len() as u32;
                self.doc.images.push(ImageInfo {
                    src,
                    title,
                    ..ImageInfo::default()
                });
                self.image_alt_start = self.doc.strings.len();
                self.image_depth = 1;
                self.push_atom(SpanKind::Image(self.image_idx));
            }
            MdTag::Paragraph => {
                // `--` and `-` are setext underlines to GitHub, though pulldown-cmark reads them
                // as paragraph text. They underline the marker line, so this is not an alert.
                if self.first_in_open_alert()
                    && self.underlines_open_alert(range.start)
                    && is_dash_run(&self.src[range.clone()])
                {
                    self.reject_open_alert();
                }
                self.md_block(BlockKind::Paragraph { align: Align::None }, true, range)
            }
            MdTag::Heading { level, .. } => {
                // A setext underline turns what is above it into the heading, marker line included.
                // An ATX heading is a child instead, and so is a setext heading after a blank line.
                if self.first_in_open_alert()
                    && self.underlines_open_alert(range.start)
                    && !is_atx_heading(&self.src[range.start..])
                {
                    self.reject_open_alert();
                }
                self.md_block(
                    BlockKind::Heading {
                        level: level as u8,
                        align: Align::None,
                        slug: StrRef::default(),
                    },
                    true,
                    range,
                );
            }
            MdTag::BlockQuote(kind) => {
                let kind = kind.map(|k| match k {
                    BlockQuoteKind::Note => AlertKind::Note,
                    BlockQuoteKind::Tip => AlertKind::Tip,
                    BlockQuoteKind::Important => AlertKind::Important,
                    BlockQuoteKind::Warning => AlertKind::Warning,
                    BlockQuoteKind::Caution => AlertKind::Caution,
                });
                self.block_quote(kind, range);
            }
            MdTag::CodeBlock(kind) => {
                let info = match &kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or(""),
                    CodeBlockKind::Indented => "",
                };
                let lang = self.pool_restored(info);
                let kind = match info {
                    "mermaid" => BlockKind::Diagram {
                        lang,
                        oversize: None,
                    },
                    "math" => BlockKind::Math { oversize: None },
                    _ => BlockKind::CodeBlock { lang },
                };
                self.md_block(kind, false, range);
                self.in_code_block = true;
            }
            MdTag::HtmlBlock => {
                self.flush_run();
                let mut buf = std::mem::take(&mut self.html_store);
                buf.clear();
                self.html_buf = Some(buf);
            }
            MdTag::List(start) => {
                self.md_block(
                    BlockKind::List {
                        start: start.map(|n| n.min(u64::from(u32::MAX)) as u32),
                    },
                    false,
                    range,
                );
            }
            MdTag::Item => self.md_block(BlockKind::ListItem { task: false }, false, range),
            MdTag::FootnoteDefinition(label) => {
                // GitHub draws a footnote definition as a list item, so a marker line immediately
                // above one makes a plain quote, not an alert.
                if self.first_in_open_alert() && self.underlines_open_alert(range.start) {
                    self.reject_open_alert();
                }
                let id = self.footnote(&label);
                if self.doc.footnotes[id as usize].defined {
                    self.close_inline_for_block();
                    self.skip_depth = 1;
                    return;
                }
                // First definition wins. At the depth limit it gets no block of its own, so the
                // content joins an ancestor; `block` must then stay NONE rather than name that
                // ancestor, and `defined` is what keeps a later definition from replacing it.
                self.doc.footnotes[id as usize].defined = true;
                self.md_block(BlockKind::FootnoteDefinition { footnote: id }, false, range);
                let b = self.cur_block;
                if b != NONE
                    && matches!(
                        self.doc.blocks[b as usize].kind,
                        BlockKind::FootnoteDefinition { footnote } if footnote == id
                    )
                {
                    self.doc.footnotes[id as usize].block = b;
                }
            }
            MdTag::Table(aligns) => {
                self.aligns = aligns;
                self.md_block(BlockKind::Table, false, range);
            }
            MdTag::TableHead => {
                self.col = 0;
                self.header_row = true;
                self.md_block(BlockKind::TableRow { header: true }, false, range);
            }
            MdTag::TableRow => {
                self.col = 0;
                self.header_row = false;
                self.md_block(BlockKind::TableRow { header: false }, false, range);
            }
            MdTag::TableCell => {
                let align = match self.aligns.get(self.col) {
                    Some(Alignment::Left) => Align::Left,
                    Some(Alignment::Center) => Align::Center,
                    Some(Alignment::Right) => Align::Right,
                    _ => Align::None,
                };
                self.col += 1;
                let kind = BlockKind::TableCell {
                    align,
                    header: self.header_row,
                    colspan: 1,
                    rowspan: 1,
                };
                self.md_block(kind, true, range);
            }
            _ => {
                self.flush_run();
                self.push_open(Open {
                    node: Node::Nothing,
                    origin: Origin::Md,
                });
            }
        }
    }

    fn push_open(&mut self, open: Open) {
        if let Origin::Html(tag) = open.origin {
            self.open_html[tag.slot()] += 1;
            self.html_depth += 1;
        }
        self.stack.push(open);
    }

    fn pop_open(&mut self) -> Option<Open> {
        let open = self.stack.pop()?;
        if let Origin::Html(tag) = open.origin {
            self.open_html[tag.slot()] -= 1;
            self.html_depth -= 1;
        }
        Some(open)
    }

    fn md_block(&mut self, kind: BlockKind, inline: bool, range: Range<usize>) {
        self.close_inline_for_block();
        self.inline_limit = range.end;
        // A cell whose table was flattened away still has inline content to hold.
        let kind = match kind {
            BlockKind::TableCell { .. } if !self.parent_is(Tag::Tr) => BlockKind::Plain,
            _ => kind,
        };
        if self.flattened(kind) {
            return self.push_open(Open {
                node: Node::Nothing,
                origin: Origin::Md,
            });
        }
        self.open_block(kind, Origin::Md);
        if inline {
            self.enter_inline();
        }
    }

    /// GitHub turns a marked quote into an alert only if it holds something, and only outside a
    /// quote, list item, `<div>` or `<details>`, as measured with `gh api markdown`. That includes a
    /// footnote, since GitHub draws one as a list item.
    fn block_quote(&mut self, alert: Option<AlertKind>, range: Range<usize>) {
        let marker = alert.and_then(|_| {
            let b = self.src.as_bytes();
            let at = range.start + memchr::memchr(b'[', &b[range.start..range.end])?;
            b[at..].starts_with(b"[!").then_some(at)
        });
        let (Some(alert), Some(marker)) = (alert, marker) else {
            let kind = alert.map_or(BlockKind::BlockQuote, BlockKind::Alert);
            return self.md_block(kind, false, range);
        };
        self.close_inline_for_block();
        if self.alert_allowed_here() {
            self.alert_content = false;
            self.md_block(BlockKind::Alert(alert), false, range);
            // Unless it was flattened, deep inside tags that don't rule out an alert.
            if matches!(self.doc.blocks.last(), Some(b) if matches!(b.kind, BlockKind::Alert(_))) {
                self.open_alert = Some((self.cur_block, marker));
            }
        } else {
            self.rejected_alerts.push(marker);
            self.md_block(BlockKind::BlockQuote, false, range);
            if self.literal_alerts {
                self.push_marker_paragraph(marker);
            }
        }
    }

    /// Walked from the innermost entry: the first Markdown container or rejecting HTML tag ends it,
    /// and a table cell resets the `<div>`, list and quote ancestors outside it. Only `<details>`
    /// still rejects inside a cell. That is how GitHub behaves, measured with `gh api markdown`.
    fn alert_allowed_here(&self) -> bool {
        if self
            .stack
            .iter()
            .any(|o| matches!((o.origin, o.node), (Origin::Html(Tag::Details), _)))
        {
            return false;
        }
        for o in self.stack.iter().rev() {
            match (o.origin, o.node) {
                (Origin::Html(Tag::Td | Tag::Th), _) => return true,
                (Origin::Html(tag), _) => {
                    if matches!(tag, Tag::Div | Tag::Blockquote | Tag::Li) {
                        return false;
                    }
                }
                (Origin::Md, Node::Block(b)) => {
                    if matches!(
                        self.doc.blocks[b as usize].kind,
                        BlockKind::BlockQuote
                            | BlockKind::Alert(_)
                            | BlockKind::ListItem { .. }
                            | BlockKind::FootnoteDefinition { .. }
                    ) {
                        return false;
                    }
                }
                (Origin::Md, Node::Nothing) => {}
                _ => {}
            }
        }
        true
    }

    fn first_in_open_alert(&self) -> bool {
        matches!(self.open_alert, Some((b, _)) if b as usize + 1 == self.doc.blocks.len())
    }

    /// Whether a block starting at `start` touches the open alert's marker line. A blank quote line
    /// between them means it is an ordinary child block; otherwise a setext underline or thematic
    /// break rewrites the marker line itself, which leaves a plain quote on GitHub.
    fn underlines_open_alert(&self, start: usize) -> bool {
        let Some((_, marker)) = self.open_alert else {
            return false;
        };
        let from = self.src[marker..]
            .find('\n')
            .map_or(self.src.len(), |i| marker + i + 1);
        if from >= start {
            return true;
        }
        let region = &self.src[from..start];
        let parts: Vec<&str> = region.split('\n').collect();
        // The last piece is only the prefix of the line `start` sits on, not a complete line.
        !parts[..parts.len().saturating_sub(1)]
            .iter()
            .any(|line| is_blank_quote_line(line))
    }

    fn reject_open_alert(&mut self) {
        let Some((b, marker)) = self.open_alert.take() else {
            return;
        };
        self.rejected_alerts.push(marker);
        if self.literal_alerts {
            self.doc.blocks[b as usize].kind = BlockKind::BlockQuote;
            self.push_marker_paragraph(marker);
        }
    }

    /// Only for a document that is keeping its result, since hiding the marker reads better: this
    /// can't join the marker to a paragraph that follows it.
    fn push_marker_paragraph(&mut self, marker: usize) {
        let tail = &self.src[marker..];
        let text = &tail[..tail.find(']').map_or(0, |i| i + 1)];
        let p = self.open_block(BlockKind::Paragraph { align: Align::None }, Origin::Md);
        self.doc.text.push_str(text);
        self.pop_open();
        self.finish_block(p);
    }

    fn parent_is(&self, tag: Tag) -> bool {
        self.cur_block != NONE
            && match self.doc.blocks[self.cur_block as usize].kind {
                BlockKind::Table => tag == Tag::Table,
                BlockKind::TableRow { .. } => tag == Tag::Tr,
                BlockKind::List { .. } => tag == Tag::Ul,
                _ => false,
            }
    }

    /// Whether a container goes without a block of its own. Rows, cells and items follow their
    /// parent, so a table or list either fits whole or leaves no part of itself behind.
    fn flattened(&self, kind: BlockKind) -> bool {
        match kind {
            BlockKind::Paragraph { .. }
            | BlockKind::Plain
            | BlockKind::Heading { .. }
            | BlockKind::CodeBlock { .. }
            | BlockKind::Diagram { .. }
            | BlockKind::Math { .. }
            | BlockKind::ThematicBreak => false,
            BlockKind::ListItem { .. } => !self.parent_is(Tag::Ul),
            BlockKind::TableRow { .. } => !self.parent_is(Tag::Table),
            BlockKind::TableCell { .. } => !self.parent_is(Tag::Tr),
            _ => self.block_depth >= MAX_BLOCK_DEPTH,
        }
    }

    /// Keeps the shape `BlockKind` promises when content arrives where a table or list has no room
    /// for it. The implied parents count as real tags, so a later `<tr>` or `</table>` ends them.
    fn fit_parent(&mut self, kind: BlockKind) {
        if self.cur_block == NONE
            || matches!(
                kind,
                BlockKind::TableRow { .. }
                    | BlockKind::TableCell { .. }
                    | BlockKind::ListItem { .. }
            )
        {
            return;
        }
        match self.doc.blocks[self.cur_block as usize].kind {
            BlockKind::Table => {
                self.open_implied_row();
                self.open_implied_cell();
            }
            BlockKind::TableRow { .. } => self.open_implied_cell(),
            BlockKind::List { .. } => {
                let list = Node::Block(self.cur_block);
                let at = self.stack.iter().rposition(|o| o.node == list);
                match at {
                    // A list that only stood in for a missing one ends at the first thing that isn't
                    // an item, because in a browser there was no list to stay inside.
                    Some(i) if self.stack[i].origin == Origin::Auto => self.pop_to(i),
                    _ => {
                        let b = self
                            .open_block(BlockKind::ListItem { task: false }, Origin::Html(Tag::Li));
                        self.implied.push(b);
                        self.doc.block_attrs.push(BlockAttrs {
                            block: b,
                            marker: Some(ListMarker::None),
                            ..BlockAttrs::default()
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn open_implied_row(&mut self) {
        let b = self.open_block(BlockKind::TableRow { header: false }, Origin::Html(Tag::Tr));
        self.implied.push(b);
    }

    fn open_implied_cell(&mut self) {
        let cell = BlockKind::TableCell {
            align: Align::None,
            header: false,
            colspan: 1,
            rowspan: 1,
        };
        let b = self.open_block(cell, Origin::Html(Tag::Td));
        self.implied.push(b);
    }

    /// Closes everything down to and including the innermost entry a Markdown event opened. A
    /// placeholder left by `split_md_inline` stands for something already closed, so HTML blocks
    /// opened after it stay open for what follows, as they would in a browser.
    fn end(&mut self) {
        self.flush_run();
        self.in_code_block = false;
        let md = self.stack.iter().rposition(|o| o.origin == Origin::Md);
        let keep_from = md
            .filter(|&i| self.stack[i].node == Node::Nothing)
            .and_then(|i| {
                let above = &self.stack[i + 1..];
                let html_block = above.iter().rposition(|o| match o.node {
                    Node::Block(b) => {
                        matches!(o.origin, Origin::Html(_))
                            && !matches!(
                                self.doc.blocks[b as usize].kind,
                                BlockKind::CodeBlock { .. }
                            )
                    }
                    _ => false,
                })?;
                Some((i, i + 1 + html_block + 1))
            });
        if let Some((placeholder, keep)) = keep_from {
            while self.stack.len() > keep {
                if let Some(open) = self.pop_open() {
                    self.close(open);
                }
            }
            self.stack.remove(placeholder);
        } else {
            while let Some(open) = self.pop_open() {
                self.close(open);
                if open.origin == Origin::Md {
                    break;
                }
            }
        }
        let depth = self.stack.len();
        self.ambient.retain(|a| a.depth <= depth);
    }

    fn close(&mut self, open: Open) {
        match open.node {
            Node::Block(b) => self.finish_block(b),
            Node::Span(s) => self.finish_span(s),
            Node::Nothing => {}
        }
    }

    fn finish_span(&mut self, s: u32) {
        let end = self.doc.text.len() as u32;
        let span = &mut self.doc.spans[s as usize];
        span.end = end;
        self.cur_span = span.parent;
        match span.kind {
            SpanKind::Link(_) => self.link_depth -= 1,
            SpanKind::Strong => self.strong_depth -= 1,
            _ => {}
        }
    }

    fn open_block(&mut self, kind: BlockKind, origin: Origin) -> u32 {
        self.in_rp = false;
        // A block an HTML tag opened is content even when it ends empty, as GitHub keeps the alert.
        if self.open_alert.is_some() && !matches!(origin, Origin::Md) {
            self.alert_content = true;
        }
        self.fit_parent(kind);
        self.block_depth += 1;
        let idx = self.doc.blocks.len() as u32;
        let text = self.doc.text.len() as u32;
        let spans = self.doc.spans.len() as u32;
        self.doc.blocks.push(Block {
            kind,
            parent: self.cur_block,
            end: idx + 1,
            text: TextRange {
                start: text,
                end: text,
            },
            spans: TextRange {
                start: spans,
                end: spans,
            },
        });
        self.push_open(Open {
            node: Node::Block(idx),
            origin,
        });
        self.cur_block = idx;
        idx
    }

    fn finish_block(&mut self, b: u32) {
        if matches!(self.open_alert, Some((a, _)) if a == b) {
            if self.doc.blocks.len() == b as usize + 1 && !self.alert_content {
                self.reject_open_alert();
            }
            self.open_alert = None;
            self.alert_content = false;
        }
        self.block_depth -= 1;
        self.in_rp = false;
        if self.in_picture && self.picture_block == b {
            self.leave_picture();
        }
        let text_end = self.doc.text.len();
        let spans_end = self.doc.spans.len() as u32;
        let blocks_end = self.doc.blocks.len() as u32;
        let block = &mut self.doc.blocks[b as usize];
        block.end = blocks_end;
        block.text.end = text_end as u32;
        let has_children = blocks_end > b + 1;
        // A container's range would otherwise cover its children's spans too.
        block.spans.end = if has_children {
            block.spans.start
        } else {
            spans_end
        };
        self.cur_block = block.parent;
        let kind = block.kind;
        let start = block.text.start as usize;
        match kind {
            BlockKind::Paragraph { .. } => self.promote_display_math(b),
            BlockKind::Heading { .. } => self.finish_heading(b),
            BlockKind::Diagram { lang, .. } => {
                let oversize = (text_end - start > MAX_DIAGRAM_BYTES).then_some(Oversize::Diagram);
                self.doc.blocks[b as usize].kind = BlockKind::Diagram { lang, oversize };
                self.warn_oversize(oversize);
            }
            BlockKind::Math { .. } => {
                let oversize =
                    (text_end - start > MAX_EQUATION_BYTES).then_some(Oversize::Equation);
                self.doc.blocks[b as usize].kind = BlockKind::Math { oversize };
                self.warn_oversize(oversize);
            }
            _ => {}
        }
        let leaf = matches!(
            kind,
            BlockKind::Paragraph { .. }
                | BlockKind::Plain
                | BlockKind::Heading { .. }
                | BlockKind::CodeBlock { .. }
                | BlockKind::Diagram { .. }
                | BlockKind::Math { .. }
        ) || (matches!(kind, BlockKind::TableCell { .. }) && !has_children);
        if leaf && text_end > start && !self.doc.text.ends_with('\n') {
            // Keeps find from matching across block boundaries. Not part of any block's own range.
            self.doc.text.push('\n');
        }
        if self.inline_open && self.inline_block == b {
            self.inline_open = false;
        }
        if matches!(kind, BlockKind::CodeBlock { .. }) {
            self.pre_open = false;
        }
        // An implied row, cell or item that never got content would draw an empty line or a phantom
        // column. GitHub moves such stray content out of the table; keeping the shape without the
        // empty node is the closest this model can do. Empty inline tags such as `<b></b>` leave an
        // empty container child, so the whole subtree goes: no text of its own and no atom anywhere.
        if self.implied.last() == Some(&b) {
            self.implied.pop();
            let block = self.doc.blocks[b as usize];
            let empty = block.text.is_empty()
                && !self.doc.spans[block.spans.start as usize..]
                    .iter()
                    .any(is_atom);
            if empty {
                self.drop_last_block(b);
                return;
            }
        }
        // `<a id="x"></a>` alone on a line parses as a paragraph that would only add a gap. A browser
        // gives an empty paragraph no height. Anchors noted against it land on the next block.
        let block = self.doc.blocks[b as usize];
        if matches!(block.kind, BlockKind::Paragraph { .. })
            && !has_children
            && block.text.is_empty()
            && !self.doc.spans[block.spans.usize()].iter().any(is_atom)
        {
            self.drop_last_block(b);
        }
    }

    fn drop_last_block(&mut self, b: u32) {
        let block = self.doc.blocks[b as usize];
        self.doc.spans.truncate(block.spans.start as usize);
        self.doc.blocks.truncate(b as usize);
        while self.doc.block_attrs.last().is_some_and(|a| a.block >= b) {
            self.doc.block_attrs.pop();
        }
        // An empty implied container can hold an empty heading or an anchor; both would point past
        // the end of the tree once it goes.
        while self.doc.headings.last().is_some_and(|h| h.block >= b) {
            self.doc.headings.pop();
        }
        // An anchor noted for the dropped container, or for the block that would have followed it,
        // belongs to whatever block takes its place.
        for a in &mut self.doc.anchors {
            a.block = a.block.min(b);
        }
        // A definition with no text can itself have been what the empty container held.
        for f in &mut self.doc.footnotes {
            if f.block != NONE && f.block >= b {
                f.block = NONE;
            }
        }
    }

    fn warn_oversize(&mut self, oversize: Option<Oversize>) {
        if let Some(o) = oversize {
            self.doc.warnings.push(Warning::Oversize(o));
        }
    }

    fn promote_display_math(&mut self, b: u32) {
        let block = self.doc.blocks[b as usize];
        if block.spans.len() != 1 {
            return;
        }
        let span = self.doc.spans[block.spans.start as usize];
        if let SpanKind::Math {
            display: true,
            oversize,
        } = span.kind
            && span.start == block.text.start
            && span.end == block.text.end
        {
            self.doc.spans.pop();
            let block = &mut self.doc.blocks[b as usize];
            block.spans.end = block.spans.start;
            block.kind = BlockKind::Math { oversize };
        }
    }

    fn finish_heading(&mut self, b: u32) {
        let block = self.doc.blocks[b as usize];
        let BlockKind::Heading { level, align, .. } = block.kind else {
            return;
        };
        // The outline shows one line, so a hard break in the heading reads as a space there.
        let raw = self.doc.text[block.text.usize()].trim();
        let title = if raw.contains('\n') {
            let flat = raw.replace('\n', " ");
            self.pool(&flat)
        } else {
            let start = self.doc.strings.len() as u32;
            self.doc.strings.push_str(raw);
            StrRef {
                start,
                len: raw.len() as u32,
            }
        };
        let slug = self.slugger.slug(&self.heading_text);
        self.in_heading = false;
        let slug = self.pool(&slug);
        self.doc.blocks[b as usize].kind = BlockKind::Heading { level, align, slug };
        self.doc.headings.push(Heading {
            level,
            block: b,
            title,
            slug,
        });
    }

    /// Slugs come from the heading's text before whitespace collapses, as GitHub's do.
    fn note_heading_text(&mut self, t: &str) {
        if !self.in_heading {
            return;
        }
        if self.restore {
            self.heading_text.push_str(&autolink::restore(t));
        } else {
            self.heading_text.push_str(t);
        }
    }

    // Inline content

    fn enter_inline(&mut self) {
        self.inline_open = true;
        self.inline_block = self.cur_block;
        self.inline_start = self.doc.text.len();
        self.cur_span = NONE;
        self.in_heading = matches!(
            self.doc.blocks[self.cur_block as usize].kind,
            BlockKind::Heading { .. }
        );
        if self.in_heading {
            self.heading_text.clear();
        }
        // Every later block re-opens these, so a short file could otherwise ask for a span count
        // of tags times blocks. Real documents stay far under this budget.
        if self.ambient.len() > self.ambient_spans_left {
            self.ambient.clear();
            if !self.doc.warnings.contains(&Warning::UnclosedHtmlDropped) {
                self.doc.warnings.push(Warning::UnclosedHtmlDropped);
            }
        }
        self.ambient_spans_left -= self.ambient.len();
        for i in 0..self.ambient.len() {
            let (tag, kind) = (self.ambient[i].tag, self.ambient[i].kind);
            self.push_span(kind, Origin::Html(tag));
        }
    }

    /// One entry per tag: `<i><i>` looks the same as `<i>`, and a browser ends an open `<a>` when the
    /// next one starts. That bounds the list by the number of inline tags.
    fn carry_ambient(&mut self, tag: Tag, kind: SpanKind, depth: usize) {
        if let Some(i) = self.ambient.iter().position(|a| a.tag == tag) {
            self.ambient.remove(i);
        }
        self.ambient.push(Ambient { tag, kind, depth });
    }

    /// Makes sure text has somewhere to go: opens an anonymous block when no inline container is open.
    fn begin_inline(&mut self) {
        if !self.inline_open {
            self.flush_run();
            self.open_block(BlockKind::Plain, Origin::Auto);
            self.enter_inline();
        }
    }

    fn close_inline_for_block(&mut self) {
        self.flush_run();
        if !self.inline_open {
            return;
        }
        let closable = self
            .stack
            .iter()
            .rev()
            .find(|o| matches!(o.node, Node::Block(_)))
            .is_some_and(|o| o.origin != Origin::Md);
        if closable {
            self.close_inline_container();
        }
    }

    /// Pops through the open anonymous or HTML inline container, carrying its open HTML spans over.
    fn close_inline_container(&mut self) {
        self.flush_run();
        let mut carried: Vec<(Tag, SpanKind)> = Vec::new();
        while let Some(open) = self.pop_open() {
            match open.node {
                Node::Span(s) => {
                    if let Origin::Html(tag) = open.origin {
                        carried.push((tag, self.doc.spans[s as usize].kind));
                    }
                    self.finish_span(s);
                }
                Node::Block(b) => {
                    self.finish_block(b);
                    self.discard_if_only_carried(b, carried.len());
                    break;
                }
                Node::Nothing => {}
            }
        }
        let depth = self.stack.len();
        self.ambient.clear();
        for (tag, kind) in carried.into_iter().rev() {
            self.carry_ambient(tag, kind, depth);
        }
    }

    /// A block tag inside Markdown inline content. The Markdown block's end event is still to come,
    /// so placeholders stay on the stack for it and for any Markdown span open here. The formatting
    /// open at this point carries on inside the new blocks, until its own end event or closer.
    ///
    /// A paragraph closes as it would in a browser. A table cell stays open and takes block
    /// children, with what it holds so far moved into the first of them.
    fn split_md_inline(&mut self, is_cell: bool) {
        self.flush_run();
        let mut spans: Vec<(Origin, SpanKind)> = Vec::new();
        let mut placeholders = 0;
        let mut block = None;
        while let Some(open) = self.pop_open() {
            match open.node {
                Node::Span(s) => {
                    spans.push((open.origin, self.doc.spans[s as usize].kind));
                    self.finish_span(s);
                }
                Node::Block(b) => {
                    block = Some((open, b));
                    break;
                }
                Node::Nothing if open.origin == Origin::Md => placeholders += 1,
                Node::Nothing => {}
            }
        }
        let Some((open, b)) = block else {
            return;
        };
        if is_cell {
            self.move_cell_inline_to_child(b);
            self.inline_open = false;
            self.push_open(open);
        } else {
            self.finish_block(b);
            self.push_open(Open {
                node: Node::Nothing,
                origin: Origin::Md,
            });
        }
        let mut depth = self.stack.len();
        for _ in 0..placeholders {
            self.push_open(Open {
                node: Node::Nothing,
                origin: Origin::Md,
            });
        }
        for (origin, kind) in spans.into_iter().rev() {
            match origin {
                Origin::Html(tag) => self.carry_ambient(tag, kind, depth),
                Origin::Md => {
                    self.push_open(Open {
                        node: Node::Nothing,
                        origin: Origin::Md,
                    });
                    depth += 1;
                    self.carry_ambient(md_span_tag(kind), kind, depth);
                }
                Origin::Auto => {}
            }
        }
    }

    fn move_cell_inline_to_child(&mut self, cell: u32) {
        let block = self.doc.blocks[cell as usize];
        let text_end = self.doc.text.len();
        let spans_end = self.doc.spans.len();
        let own = &self.doc.text[block.text.start as usize..];
        let has_atom = self.doc.spans[block.spans.start as usize..]
            .iter()
            .any(is_atom);
        if own.trim().is_empty() && !has_atom {
            self.doc.text.truncate(block.text.start as usize);
            self.doc.spans.truncate(block.spans.start as usize);
            return;
        }
        let idx = self.doc.blocks.len() as u32;
        self.doc.blocks.push(Block {
            kind: BlockKind::Plain,
            parent: cell,
            end: idx + 1,
            text: TextRange::new(block.text.start as usize, text_end),
            spans: TextRange::new(block.spans.start as usize, spans_end),
        });
        if !self.doc.text.ends_with('\n') {
            self.doc.text.push('\n');
        }
    }

    /// An anonymous block holding nothing but tags carried onward would draw as an empty line.
    fn discard_if_only_carried(&mut self, b: u32, carried: usize) {
        // finish_block may already have dropped an empty paragraph.
        let Some(&block) = self.doc.blocks.get(b as usize) else {
            return;
        };
        if matches!(block.kind, BlockKind::Plain | BlockKind::Paragraph { .. })
            && block.text.is_empty()
            && block.spans.len() == carried
        {
            self.drop_last_block(b);
        }
    }

    fn push_span(&mut self, kind: SpanKind, origin: Origin) -> u32 {
        if self.open_alert.is_some() {
            self.alert_content = true;
        }
        let at = self.doc.text.len() as u32;
        let idx = self.doc.spans.len() as u32;
        self.doc.spans.push(Span {
            start: at,
            end: at,
            parent: self.cur_span,
            kind,
        });
        self.cur_span = idx;
        self.push_open(Open {
            node: Node::Span(idx),
            origin,
        });
        match kind {
            SpanKind::Link(_) => self.link_depth += 1,
            SpanKind::Strong => self.strong_depth += 1,
            _ => {}
        }
        idx
    }

    fn push_atom(&mut self, kind: SpanKind) {
        self.flush_run();
        if self.open_alert.is_some() {
            self.alert_content = true;
        }
        let at = self.doc.text.len() as u32;
        self.atom_end = at as usize;
        self.doc.spans.push(Span {
            start: at,
            end: at,
            parent: self.cur_span,
            kind,
        });
    }

    /// A span whose text was already appended, starting at `start`.
    fn push_atom_like(&mut self, kind: SpanKind, start: usize) {
        if self.open_alert.is_some() {
            self.alert_content = true;
        }
        let end = self.doc.text.len() as u32;
        self.doc.spans.push(Span {
            start: start as u32,
            end,
            parent: self.cur_span,
            kind,
        });
    }

    fn open_md_span(&mut self, kind: SpanKind) {
        self.begin_inline();
        self.flush_run();
        self.push_span(kind, Origin::Md);
    }

    fn open_strong(&mut self, origin: Origin) {
        self.begin_inline();
        self.flush_run();
        if self.strong_depth > 0 {
            self.push_open(Open {
                node: Node::Nothing,
                origin,
            });
        } else {
            self.push_span(SpanKind::Strong, origin);
        }
    }

    fn math(&mut self, t: &str, display: bool) {
        // Unlike code, math reaches here with the file's line endings.
        let normalized;
        let t = if memchr::memchr(b'\r', t.as_bytes()).is_some() {
            normalized = t.replace("\r\n", "\n").replace('\r', "\n");
            normalized.as_str()
        } else {
            t
        };
        self.begin_inline();
        self.flush_run();
        self.note_heading_text(t);
        let start = self.doc.text.len();
        self.push_str_restored(t);
        let oversize =
            (self.doc.text.len() - start > MAX_EQUATION_BYTES).then_some(Oversize::Equation);
        self.warn_oversize(oversize);
        self.push_atom_like(SpanKind::Math { display, oversize }, start);
    }

    fn push_str_restored(&mut self, t: &str) {
        if self.restore {
            self.doc.text.push_str(&autolink::restore(t));
        } else {
            self.doc.text.push_str(t);
        }
    }

    fn md_text(&mut self, t: &str, range: Range<usize>) {
        if self.in_rp {
            return;
        }
        let swapped = crate::emoji::replace(t);
        let t: &str = &swapped;
        let escaped = self.is_escaped(range.start);
        self.text_floor = range.end;
        // Text resuming after a block tag, as in ` done` after `</ul>`, starts a block of its own.
        if !self.inline_open && t.bytes().all(|c| c.is_ascii_whitespace()) {
            return;
        }
        self.begin_inline();
        self.note_heading_text(t);
        let at = self.doc.text.len();
        let t = if at == self.inline_start && at != self.atom_end {
            t.trim_start_matches(|c: char| c.is_ascii_whitespace())
        } else {
            t
        };
        if self.run_start.is_none() {
            self.run_start = Some(at);
        }
        if escaped && let Some(c) = t.chars().next() {
            self.escapes.push((at, at + c.len_utf8()));
        }
        self.push_inline_text(t);
        if self.src.as_bytes()[range.start] == b'&' && is_named_entity(&self.src[range]) {
            self.entities.push((at, self.doc.text.len()));
        }
    }

    /// Appends inline text with whitespace runs collapsed as HTML would show them, so the renderer
    /// and find both see one space. A decoded `&#10;` also ends up here, because a newline in inline
    /// text is reserved to mean a hard break.
    fn push_inline_text(&mut self, t: &str) {
        let b = t.as_bytes();
        let plain = !b.iter().any(|&c| matches!(c, b'\t' | b'\n' | b'\r'))
            && memchr::memmem::find(b, b"  ").is_none();
        let len = self.doc.text.len();
        let after_space =
            len > self.inline_start && len != self.atom_end && self.doc.text.ends_with(' ');
        if plain && !(after_space && b.first() == Some(&b' ')) {
            self.push_str_restored(t);
            return;
        }
        let restored = if self.restore {
            autolink::restore(t)
        } else {
            std::borrow::Cow::Borrowed(t)
        };
        let mut pending = false;
        for c in restored.chars() {
            if matches!(c, ' ' | '\t' | '\n' | '\r') {
                pending = true;
                continue;
            }
            if pending {
                self.push_collapsed_space();
                pending = false;
            }
            self.doc.text.push(c);
        }
        if pending {
            self.push_collapsed_space();
        }
    }

    /// Whether the character at `start` follows an unescaped backslash. The walk stops where the last
    /// text event ended: a backslash before that is already spoken for, and a long run of them
    /// emits an event per pair, so walking further would be quadratic.
    fn is_escaped(&self, start: usize) -> bool {
        let b = self.src.as_bytes();
        let mut i = start;
        let mut n = 0;
        while i > self.text_floor && b[i - 1] == b'\\' {
            n += 1;
            i -= 1;
        }
        n % 2 == 1
    }

    fn line_break(&mut self, c: char) {
        // Between block tags, as in `<ul>` and `<li>` on separate lines of one paragraph.
        if !self.inline_open {
            return;
        }
        if c == ' ' && self.doc.text.ends_with('\n') && self.doc.text.len() > self.inline_start {
            return;
        }
        if self.run_start.is_none() {
            self.run_start = Some(self.doc.text.len());
        }
        self.doc.text.push(c);
    }

    /// The autolink pass, run on each finished text run so that links become ordinary spans in order.
    fn flush_run(&mut self) {
        let Some(start) = self.run_start.take() else {
            return;
        };
        let mut escapes = std::mem::take(&mut self.escapes);
        let mut named = std::mem::take(&mut self.entities);
        if self.link_depth == 0 && autolink::may_contain_link(&self.doc.text[start..]) {
            let mut hits = std::mem::take(&mut self.link_hits);
            hits.clear();
            let mut pos = start;
            // An escaped character is a wall: nothing links across it or through it, as in cmark-gfm
            // where the escape splits the text node.
            for &(a, b) in &escapes {
                let entities = Entities::new(&named, pos);
                autolink::find_links_into(&self.doc.text[pos..a], pos, entities, &mut hits);
                pos = b;
            }
            let entities = Entities::new(&named, pos);
            autolink::find_links_into(&self.doc.text[pos..], pos, entities, &mut hits);
            for &(s, e, kind) in &hits {
                let dest_text = kind.destination(&self.doc.text[s..e]);
                let dest = self.pool(&dest_text);
                let idx = self.doc.links.len() as u32;
                self.doc.links.push(LinkInfo {
                    dest,
                    title: StrRef::default(),
                });
                self.doc.spans.push(Span {
                    start: s as u32,
                    end: e as u32,
                    parent: self.cur_span,
                    kind: SpanKind::Link(idx),
                });
            }
            self.link_hits = hits;
        }
        escapes.clear();
        self.escapes = escapes;
        named.clear();
        self.entities = named;
    }

    fn pool(&mut self, s: &str) -> StrRef {
        if s.is_empty() {
            return StrRef::default();
        }
        let start = self.doc.strings.len() as u32;
        self.doc.strings.push_str(s);
        StrRef {
            start,
            len: s.len() as u32,
        }
    }

    fn pool_restored(&mut self, s: &str) -> StrRef {
        if self.restore {
            let restored = autolink::restore(s);
            self.pool(&restored)
        } else {
            self.pool(s)
        }
    }

    fn footnote(&mut self, label: &str) -> u32 {
        // pulldown-cmark matches a reference to its definition ignoring case, so its labels must be
        // folded the same way here. The first spelling seen is the one kept for display.
        let key = UniCase::new(label.to_string());
        if let Some(&id) = self.footnote_ids.get(&key) {
            return id;
        }
        let label_ref = self.pool_restored(label);
        let id = self.doc.footnotes.len() as u32;
        self.doc.footnotes.push(Footnote {
            label: label_ref,
            number: 0,
            block: NONE,
            defined: false,
        });
        self.footnote_ids.insert(key, id);
        id
    }

    /// Numbers a footnote the first time any reference reaches it, in reference order as GitHub
    /// does, and returns its id. Duplicate definitions share the id, so they share the number.
    fn number_footnote(&mut self, label: &str) -> u32 {
        let id = self.footnote(label);
        let fnote = &mut self.doc.footnotes[id as usize];
        if fnote.number == 0 {
            self.next_footnote_number += 1;
            fnote.number = self.next_footnote_number;
        }
        id
    }

    fn mark_task_item(&mut self) {
        let mut b = self.cur_block;
        while b != NONE {
            let block = &mut self.doc.blocks[b as usize];
            if let BlockKind::ListItem { task } = &mut block.kind {
                *task = true;
                return;
            }
            b = block.parent;
        }
    }

    // Embedded HTML

    fn end_html_block(&mut self) {
        let Some(buf) = self.html_buf.take() else {
            return;
        };
        self.html_tokens(&buf);
        self.html_store = buf;
        self.flush_run();
        // An unclosed `<picture>` ends with the HTML block that held it, as in a browser, so its
        // sources cannot leak onto a later image.
        self.leave_picture();
        if self.pre_open {
            self.close_html(Tag::Pre);
            self.pre_open = false;
        }
        // Only an anonymous container ends with the block. An explicit `<p>` or `<h1>` stays open for
        // the next HTML block, and a following Markdown block closes it the way a browser would.
        let top_block = self
            .stack
            .iter()
            .rev()
            .find(|o| matches!(o.node, Node::Block(_)));
        if self.inline_open && top_block.is_some_and(|o| o.origin == Origin::Auto) {
            self.close_inline_container();
        }
    }

    fn html_tokens(&mut self, html: &str) {
        let mut tk = Tokenizer::new(html);
        while let Some(tok) = tk.next() {
            if self.pre_open {
                self.pre_token(tok);
                continue;
            }
            match tok {
                Token::Text(t) => self.html_text(t),
                Token::Ignored(_) => {}
                Token::Close { name, raw } => self.html_close(name, raw),
                // Self-closing syntax means nothing on HTML elements, so it is ignored as a browser does.
                Token::Open {
                    name, attrs, raw, ..
                } => self.html_open(name, attrs, raw),
            }
        }
    }

    fn pre_token(&mut self, tok: Token<'_>) {
        match tok {
            Token::Text(t) => self.pre_text(&html::decode_entities(t)),
            Token::Close { name, .. } if html::classify_tag(name) == Tag::Pre => {
                self.close_html(Tag::Pre);
                self.pre_open = false;
            }
            _ => self.pre_first_newline = false,
        }
    }

    fn pre_text(&mut self, t: &str) {
        let s = if self.pre_first_newline {
            t.strip_prefix('\n').unwrap_or(t)
        } else {
            t
        };
        self.doc.text.push_str(s);
        self.pre_first_newline = false;
    }

    /// Text inside HTML collapses whitespace as a browser would. Whitespace between block tags
    /// produces nothing.
    fn html_text(&mut self, t: &str) {
        if self.in_rp {
            return;
        }
        let decoded = html::decode_entities(t);
        self.collapsed_text(&decoded);
    }

    fn collapsed_text(&mut self, t: &str) {
        if !self.inline_open && t.bytes().all(|c| c.is_ascii_whitespace()) {
            return;
        }
        self.begin_inline();
        self.flush_run();
        self.note_heading_text(t);
        let mut pending_space = false;
        for c in t.chars() {
            if c.is_ascii_whitespace() {
                pending_space = true;
                continue;
            }
            if pending_space {
                self.push_collapsed_space();
                pending_space = false;
            }
            self.doc.text.push(c);
        }
        if pending_space {
            self.push_collapsed_space();
        }
    }

    fn push_collapsed_space(&mut self) {
        let len = self.doc.text.len();
        // An image or checkbox has no text of its own, yet a space on either side of it still shows.
        let after_atom = len == self.atom_end;
        if after_atom || (len != self.inline_start && !self.doc.text.ends_with([' ', '\n'])) {
            self.doc.text.push(' ');
        }
    }

    fn html_open(&mut self, name: &str, attrs: &str, raw: &str) {
        let tag = html::classify_tag(name);
        // An image, source or break with no text still counts as body for an alert, as on GitHub, and
        // so does a tag the sanitizer keeps with no element here, such as `<span>`.
        if self.open_alert.is_some()
            && (matches!(tag, Tag::Img | Tag::Source | Tag::Br) || html::is_inert_tag(name))
        {
            self.alert_content = true;
        }
        match tag {
            Tag::Literal => return self.collapsed_text(raw),
            // A browser stops nesting too. The text stays, in the element that is already open.
            _ if self.html_depth >= MAX_HTML_DEPTH && tag.nests() => return,
            _ => {}
        }
        let anchor = html::attr(attrs, "id").or_else(|| html::attr(attrs, "name"));
        match tag {
            Tag::Unknown => {}
            Tag::Ruby => {
                self.in_rp = false;
                self.open_html_span(tag, SpanKind::Ruby);
            }
            Tag::Rp => {
                self.flush_run();
                self.in_rp = true;
            }
            Tag::Rt => {
                self.in_rp = false;
                self.open_html_span(tag, SpanKind::RubyNote);
            }
            Tag::Strong => self.open_strong(Origin::Html(tag)),
            Tag::Em => self.open_html_span(tag, SpanKind::Emphasis),
            Tag::Underline => self.open_html_span(tag, SpanKind::Underline),
            Tag::Strike => self.open_html_span(tag, SpanKind::Strike),
            Tag::Code => self.open_html_span(tag, SpanKind::Code),
            Tag::Kbd => self.open_html_span(tag, SpanKind::Kbd),
            Tag::Sub => self.open_html_span(tag, SpanKind::Sub),
            Tag::Sup => self.open_html_span(tag, SpanKind::Sup),
            Tag::Mark => self.open_html_span(tag, SpanKind::Mark),
            Tag::Small => self.open_html_span(tag, SpanKind::Small),
            Tag::Q => {
                self.open_html_span(tag, SpanKind::Quote);
                self.push_inline_text("\u{201c}");
            }
            Tag::A => self.html_link(attrs),
            Tag::Img => self.html_image(attrs),
            Tag::Br => {
                self.begin_inline();
                self.flush_run();
                self.doc.text.push('\n');
            }
            // GitHub's sanitizer removes a raw input outside a task list, so nothing is drawn.
            Tag::Input => {}
            Tag::Picture => {
                self.in_picture = true;
                self.picture_block = self.cur_block;
                self.picture_depth = self.stack.len();
                self.picture_dark = StrRef::default();
                self.picture_light = StrRef::default();
            }
            Tag::Source if self.in_picture && self.stack.len() == self.picture_depth => {
                self.html_source(attrs)
            }
            Tag::Source => {}
            _ => self.html_open_block(tag, attrs),
        }
        if let Some(a) = anchor
            && !a.is_empty()
        {
            let name = self.pool(&a);
            // With no text open, an anchor belongs to whatever block comes next.
            let block = if self.inline_open {
                self.inline_block
            } else {
                self.doc.blocks.len() as u32
            };
            self.doc.anchors.push(Anchor { name, block });
        }
    }

    fn html_open_block(&mut self, tag: Tag, attrs: &str) {
        // A browser drops these with no table open, text kept, and does not end the paragraph.
        if matches!(tag, Tag::Tr | Tag::Td | Tag::Th) && self.open_html[Tag::Table.slot()] == 0 {
            return;
        }
        let align = html_align(attrs);
        self.flush_run();
        if self.inline_open && !self.make_room_for_html_block(tag) {
            return;
        }
        let origin = Origin::Html(tag);
        let kind = match tag {
            Tag::Hr => {
                let b = self.open_block(BlockKind::ThematicBreak, origin);
                self.note_block_attrs(b, tag, attrs);
                self.pop_open();
                self.finish_block(b);
                return;
            }
            Tag::TableSection => return,
            Tag::P => BlockKind::Paragraph { align },
            Tag::H(level) => BlockKind::Heading {
                level,
                align,
                slug: StrRef::default(),
            },
            Tag::Div | Tag::Section => BlockKind::Div { align },
            Tag::Dt | Tag::Dd => {
                // `<dt>a<dd>b<dt>c` is complete HTML: each opener ends the item before it.
                self.close_implied(&[Tag::Dt, Tag::Dd], &[Tag::Section, Tag::Div]);
                if tag == Tag::Dt {
                    BlockKind::DefTerm
                } else {
                    BlockKind::DefDesc
                }
            }
            Tag::Figure => BlockKind::Figure { align },
            Tag::FigCaption => BlockKind::FigCaption,
            Tag::Center => BlockKind::Div {
                align: Align::Center,
            },
            Tag::Blockquote => BlockKind::BlockQuote,
            Tag::Ul => BlockKind::List { start: None },
            Tag::Ol => {
                let start = html::attr(attrs, "start")
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1);
                BlockKind::List { start: Some(start) }
            }
            Tag::Li => {
                self.close_implied(&[Tag::Li], &[Tag::Ul, Tag::Ol]);
                let list = BlockKind::List { start: None };
                if !self.parent_is(Tag::Ul) && !self.flattened(list) {
                    self.open_block(list, Origin::Auto);
                }
                BlockKind::ListItem { task: false }
            }
            Tag::Table => BlockKind::Table,
            Tag::Tr => {
                self.close_implied(&[Tag::Tr], &[Tag::Table]);
                if !self.parent_is(Tag::Table) {
                    return;
                }
                BlockKind::TableRow { header: false }
            }
            Tag::Td | Tag::Th => {
                self.close_implied(&[Tag::Td, Tag::Th], &[Tag::Tr, Tag::Table]);
                if self.parent_is(Tag::Table) {
                    self.open_implied_row();
                }
                if !self.parent_is(Tag::Tr) {
                    return;
                }
                BlockKind::TableCell {
                    align,
                    header: tag == Tag::Th,
                    colspan: parse_span(attrs, "colspan", MAX_COLSPAN),
                    rowspan: parse_span(attrs, "rowspan", MAX_ROWSPAN),
                }
            }
            Tag::Details => BlockKind::Details {
                open: html::attrs(attrs).any(|(k, _)| k.eq_ignore_ascii_case("open")),
            },
            Tag::Summary => BlockKind::Summary,
            Tag::Pre => {
                let lang = html::attr(attrs, "lang").unwrap_or_default();
                let lang = self.pool(&lang);
                let b = self.open_block(BlockKind::CodeBlock { lang }, origin);
                self.note_block_attrs(b, tag, attrs);
                self.pre_open = true;
                self.pre_first_newline = true;
                return;
            }
            _ => return,
        };
        if self.flattened(kind) {
            return self.push_open(Open {
                node: Node::Nothing,
                origin,
            });
        }
        let b = self.open_block(kind, origin);
        self.note_block_attrs(b, tag, attrs);
        if matches!(tag, Tag::P | Tag::H(_)) {
            self.enter_inline();
        }
    }

    /// Ends whatever inline content is open so that a block can start. False when the tag has to be
    /// ignored instead: a Markdown heading stays whole, and GitHub's outline keeps it as one line.
    fn make_room_for_html_block(&mut self, tag: Tag) -> bool {
        let innermost = self.stack.iter().rev().find_map(|o| match o.node {
            Node::Block(b) => Some((b, o.origin)),
            _ => None,
        });
        let Some((b, origin)) = innermost else {
            return true;
        };
        if origin != Origin::Md {
            self.close_inline_container();
            return true;
        }
        match self.doc.blocks[b as usize].kind {
            _ if tag == Tag::TableSection => false,
            BlockKind::Paragraph { .. } => {
                self.split_md_inline(false);
                true
            }
            BlockKind::TableCell { .. } => {
                self.split_md_inline(true);
                true
            }
            _ => false,
        }
    }

    /// Presentation attributes that have no room in `BlockKind`. Nearly every block has none, and
    /// `Block` is the structure that most affects a document's memory.
    fn note_block_attrs(&mut self, block: u32, tag: Tag, attrs: &str) {
        if attrs.trim().is_empty() {
            return;
        }
        let own_align = matches!(
            tag,
            Tag::P
                | Tag::H(_)
                | Tag::Div
                | Tag::Section
                | Tag::Center
                | Tag::Td
                | Tag::Th
                | Tag::Figure
        );
        let length = |key: &str| html::attr(attrs, key).and_then(|v| parse_length(&v));
        let a = BlockAttrs {
            block,
            dir: html::attr(attrs, "dir").and_then(|v| parse_dir(&v)),
            align: if own_align {
                Align::None
            } else {
                html_align(attrs)
            },
            valign: html::attr(attrs, "valign").and_then(|v| parse_valign(&v)),
            width: length("width"),
            height: length("height"),
            marker: match tag {
                Tag::Ul | Tag::Ol | Tag::Li => {
                    html::attr(attrs, "type").and_then(|v| parse_marker(&v))
                }
                _ => None,
            },
        };
        let empty = BlockAttrs {
            block,
            ..BlockAttrs::default()
        };
        if a != empty {
            self.doc.block_attrs.push(a);
        }
    }

    /// Closes an open `targets` element if one is found before a `boundaries` element or Markdown.
    fn close_implied(&mut self, targets: &[Tag], boundaries: &[Tag]) {
        if targets.iter().all(|t| self.open_html[t.slot()] == 0) {
            return;
        }
        let mut found = None;
        for (i, o) in self.stack.iter().enumerate().rev() {
            match o.origin {
                Origin::Md => break,
                Origin::Html(t) if targets.contains(&t) => {
                    found = Some(i);
                    break;
                }
                Origin::Html(t) if boundaries.contains(&t) => break,
                _ => {}
            }
        }
        if let Some(i) = found {
            self.pop_to(i);
        }
    }

    fn pop_to(&mut self, index: usize) {
        self.flush_run();
        while self.stack.len() > index {
            let open = self.pop_open().expect("length checked by the loop");
            self.close(open);
        }
    }

    /// Closes the innermost `tag` opened by HTML, without crossing anything Markdown opened.
    fn close_html(&mut self, tag: Tag) -> bool {
        if self.open_html[tag.slot()] == 0 {
            return false;
        }
        let mut found = None;
        for (i, o) in self.stack.iter().enumerate().rev() {
            match o.origin {
                Origin::Md => break,
                Origin::Html(t) if t == tag => {
                    found = Some(i);
                    break;
                }
                _ => {}
            }
        }
        match found {
            Some(i) => {
                self.pop_to(i);
                true
            }
            None => false,
        }
    }

    fn html_close(&mut self, name: &str, raw: &str) {
        let tag = html::classify_tag(name);
        match tag {
            Tag::Literal => return self.collapsed_text(raw),
            Tag::Picture => {
                self.leave_picture();
                return;
            }
            Tag::Rp => {
                self.in_rp = false;
                return;
            }
            Tag::Unknown
            | Tag::TableSection
            | Tag::Hr
            | Tag::Br
            | Tag::Img
            | Tag::Input
            | Tag::Source => return,
            _ => {}
        }
        // A closer for an inline element never reaches past a block; a closer for a block may close
        // inline containers on the way.
        let is_span = matches!(
            tag,
            Tag::A
                | Tag::Strong
                | Tag::Em
                | Tag::Underline
                | Tag::Strike
                | Tag::Code
                | Tag::Kbd
                | Tag::Sub
                | Tag::Sup
                | Tag::Mark
                | Tag::Small
                | Tag::Q
                | Tag::Ruby
                | Tag::Rt
        );
        if is_span {
            self.close_html_span(tag);
        } else {
            self.close_html(tag);
        }
    }

    fn close_html_span(&mut self, tag: Tag) {
        if tag == Tag::Q && self.open_html[tag.slot()] > 0 {
            self.push_inline_text("\u{201d}");
        }
        if tag == Tag::Ruby {
            self.in_rp = false;
        }
        if self.open_html[tag.slot()] > 0 {
            let mut found = None;
            for (i, o) in self.stack.iter().enumerate().rev() {
                if matches!(o.node, Node::Block(_)) || o.origin == Origin::Md {
                    break;
                }
                if o.origin == Origin::Html(tag) {
                    found = Some(i);
                    break;
                }
            }
            if let Some(i) = found {
                self.pop_to(i);
            }
        }
        if let Some(i) = self.ambient.iter().rposition(|a| a.tag == tag) {
            self.ambient.remove(i);
        }
    }

    fn open_html_span(&mut self, tag: Tag, kind: SpanKind) {
        self.begin_inline();
        self.flush_run();
        self.push_span(kind, Origin::Html(tag));
    }

    fn html_link(&mut self, attrs: &str) {
        let Some(href) = html::attr(attrs, "href").filter(|h| html::safe_link_url(h)) else {
            return;
        };
        let dest = self.pool(&href);
        let title = html::attr(attrs, "title").unwrap_or_default();
        let title = self.pool(&title);
        let idx = self.doc.links.len() as u32;
        self.doc.links.push(LinkInfo { dest, title });
        self.open_html_span(Tag::A, SpanKind::Link(idx));
    }

    fn html_image(&mut self, attrs: &str) {
        let src = html::attr(attrs, "src")
            .filter(|s| html::safe_image_url(s))
            .unwrap_or_default();
        let alt = html::attr(attrs, "alt").unwrap_or_default();
        // GitHub keeps an image whose source is missing or was stripped, and its alt text shows.
        if src.is_empty() && alt.is_empty() {
            return;
        }
        self.begin_inline();
        let src = self.pool(&src);
        let alt = self.pool(&alt);
        let title = html::attr(attrs, "title").unwrap_or_default();
        let title = self.pool(&title);
        let mut width = html::attr(attrs, "width").and_then(|v| parse_length(&v));
        let mut height = html::attr(attrs, "height").and_then(|v| parse_length(&v));
        clamp_image_size(&mut width, &mut height);
        let idx = self.doc.images.len() as u32;
        self.doc.images.push(ImageInfo {
            src,
            alt,
            title,
            width,
            height,
            dark_src: self.picture_dark,
            light_src: self.picture_light,
            align: html::attr(attrs, "align").map_or(ImageAlign::None, |v| parse_image_align(&v)),
        });
        self.push_atom(SpanKind::Image(idx));
        self.leave_picture();
    }

    /// A `<picture>` shows one image, and a `<source>` means nothing outside one.
    fn leave_picture(&mut self) {
        self.in_picture = false;
        self.picture_block = NONE;
        self.picture_depth = 0;
        self.picture_dark = StrRef::default();
        self.picture_light = StrRef::default();
    }

    fn html_source(&mut self, attrs: &str) {
        let Some(media) = html::attr(attrs, "media") else {
            return;
        };
        let Some(srcset) = html::attr(attrs, "srcset") else {
            return;
        };
        let first = srcset
            .split(',')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("");
        if first.is_empty() || !html::safe_image_url(first) {
            return;
        }
        let compact: String = media
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        // A browser takes the first matching source; a later one never replaces it.
        if compact.contains("prefers-color-scheme:dark") && self.picture_dark.len == 0 {
            self.picture_dark = self.pool(first);
        } else if compact.contains("prefers-color-scheme:light") && self.picture_light.len == 0 {
            self.picture_light = self.pool(first);
        }
    }

    fn into_document(mut self) -> Document {
        self.flush_run();
        while let Some(open) = self.pop_open() {
            self.close(open);
        }
        let mut doc = self.doc;
        let last = doc.blocks.len().saturating_sub(1) as u32;
        doc.anchors
            .iter_mut()
            .for_each(|a| a.block = a.block.min(last));
        if doc.blocks.is_empty() {
            doc.anchors.clear();
        }
        doc.text.shrink_to_fit();
        doc.strings.shrink_to_fit();
        doc.blocks.shrink_to_fit();
        doc.spans.shrink_to_fit();
        doc.links.shrink_to_fit();
        doc.images.shrink_to_fit();
        doc.headings.shrink_to_fit();
        doc.anchors.shrink_to_fit();
        doc.footnotes.shrink_to_fit();
        doc.block_attrs.shrink_to_fit();
        doc
    }
}

fn html_align(attrs: &str) -> Align {
    match html::attr(attrs, "align") {
        Some(a) if a.eq_ignore_ascii_case("left") => Align::Left,
        Some(a) if a.eq_ignore_ascii_case("center") => Align::Center,
        Some(a) if a.eq_ignore_ascii_case("right") => Align::Right,
        Some(a) if a.eq_ignore_ascii_case("justify") => Align::Justify,
        _ => Align::None,
    }
}

fn parse_dir(v: &str) -> Option<Dir> {
    match v.trim().to_ascii_lowercase().as_str() {
        "ltr" => Some(Dir::Ltr),
        "rtl" => Some(Dir::Rtl),
        "auto" => Some(Dir::Auto),
        _ => None,
    }
}

fn parse_valign(v: &str) -> Option<VAlign> {
    match v.trim().to_ascii_lowercase().as_str() {
        "top" => Some(VAlign::Top),
        "middle" | "center" => Some(VAlign::Middle),
        "bottom" => Some(VAlign::Bottom),
        "baseline" => Some(VAlign::Baseline),
        _ => None,
    }
}

/// `type` is case-sensitive here, unlike the other attributes: `a` and `A` are different lists.
fn parse_marker(v: &str) -> Option<ListMarker> {
    match v.trim() {
        "1" => Some(ListMarker::Decimal),
        "a" => Some(ListMarker::LowerAlpha),
        "A" => Some(ListMarker::UpperAlpha),
        "i" => Some(ListMarker::LowerRoman),
        "I" => Some(ListMarker::UpperRoman),
        t if t.eq_ignore_ascii_case("disc") => Some(ListMarker::Disc),
        t if t.eq_ignore_ascii_case("circle") => Some(ListMarker::Circle),
        t if t.eq_ignore_ascii_case("square") => Some(ListMarker::Square),
        _ => None,
    }
}

fn parse_image_align(v: &str) -> ImageAlign {
    match v.trim().to_ascii_lowercase().as_str() {
        "left" => ImageAlign::Left,
        "right" => ImageAlign::Right,
        "top" => ImageAlign::Top,
        "middle" | "center" => ImageAlign::Middle,
        "bottom" => ImageAlign::Bottom,
        _ => ImageAlign::None,
    }
}

/// A line inside a quote that shows nothing: just the `>` markers, or a lone HTML comment, which
/// GitHub ignores. Such a line separates a marker from the block after it.
fn is_blank_quote_line(line: &str) -> bool {
    let mut s = line.trim();
    while let Some(rest) = s.strip_prefix('>') {
        s = rest.trim_start();
    }
    s.is_empty() || (s.starts_with("<!--") && s.ends_with("-->"))
}

/// ATX headings need one to six `#` followed by a space or the end of the line. `#x` is a setext
/// heading's text, not an ATX heading, and that distinction picks the right alert rule.
fn is_atx_heading(s: &str) -> bool {
    let indent = s.len() - s.trim_start_matches(' ').len();
    if indent > 3 {
        return false;
    }
    let s = &s[indent..];
    let hashes = s.bytes().take_while(|&c| c == b'#').count();
    (1..=6).contains(&hashes)
        && s.as_bytes()
            .get(hashes)
            .is_none_or(|&c| c == b' ' || c == b'\t')
}

/// `-`, `--` and longer runs are setext underlines to GitHub, though pulldown-cmark only makes a
/// heading of `---` and longer.
fn is_dash_run(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.bytes().all(|c| c == b'-')
}

fn is_atom(s: &Span) -> bool {
    matches!(
        s.kind,
        SpanKind::Image(_) | SpanKind::Checkbox { .. } | SpanKind::FootnoteRef(_)
    )
}

/// The tag a Markdown span counts as once it carries on past a block tag.
fn md_span_tag(kind: SpanKind) -> Tag {
    match kind {
        SpanKind::Strong => Tag::Strong,
        SpanKind::Strike => Tag::Strike,
        SpanKind::Link(_) => Tag::A,
        _ => Tag::Em,
    }
}

/// `colspan` or `rowspan` under the HTML rules: leading digits count, zero or nothing means 1, and a
/// value over the limit is clamped. Layout can then divide by it and size from it.
fn parse_span(attrs: &str, key: &str, max: u16) -> u16 {
    let Some(v) = html::attr(attrs, key) else {
        return 1;
    };
    let v = v.trim();
    // Chromium accepts a leading `+` here, as in `colspan="+2"`.
    let v = v.strip_prefix('+').unwrap_or(v);
    let digits = v.bytes().take_while(u8::is_ascii_digit).count();
    let n = match v[..digits].parse::<u64>() {
        Ok(n) => n,
        Err(_) if digits > 0 => u64::MAX,
        Err(_) => 1,
    };
    n.clamp(1, u64::from(max)) as u16
}

/// A length as Chromium reads one from an HTML attribute: leading digits, an optional fraction,
/// then an optional `%` immediately after; any other suffix is ignored (`10em` is 10). No sign and
/// no exponent, so `+5`, `.5` and `1e3` give no length or just their leading digit. `50 %` is 50px,
/// because the space stops the percent form.
fn parse_length(v: &str) -> Option<Length> {
    let v = v.trim_start();
    let b = v.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    if b.get(i) == Some(&b'.') {
        let mut j = i + 1;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        i = j;
    }
    let number: f64 = v[..i].parse().ok()?;
    if v[i..].starts_with('%') {
        return Some(Length::Percent(number.min(1000.0) as u16));
    }
    Some(Length::Px((number as u32).min(MAX_HTML_LENGTH_PX)))
}

/// GitHub caps an image's pixel width and height at 2000, scaling both together so the picture
/// keeps its shape. A percentage is left alone, as is one dimension with nothing to scale against.
fn clamp_image_size(width: &mut Option<Length>, height: &mut Option<Length>) {
    let side = |l: Option<Length>| match l {
        Some(Length::Px(n)) => Some(f64::from(n)),
        _ => None,
    };
    let max = f64::from(MAX_IMAGE_DIMENSION_PX);
    let (w, h) = (side(*width), side(*height));
    if let (Some(w), Some(h)) = (w, h)
        && (w > max || h > max)
    {
        let scale = (max / w.max(h)).min(1.0);
        *width = Some(Length::Px((w * scale).round() as u32));
        *height = Some(Length::Px((h * scale).round() as u32));
        return;
    }
    if matches!(*width, Some(Length::Px(n)) if n > MAX_IMAGE_DIMENSION_PX) {
        *width = Some(Length::Px(MAX_IMAGE_DIMENSION_PX));
    }
    if matches!(*height, Some(Length::Px(n)) if n > MAX_IMAGE_DIMENSION_PX) {
        *height = Some(Length::Px(MAX_IMAGE_DIMENSION_PX));
    }
}

/// `&name;` with letters only. GitHub's URL trimming, working on the source, stops at anything else.
fn is_named_entity(source: &str) -> bool {
    source
        .strip_prefix('&')
        .and_then(|s| s.strip_suffix(';'))
        .is_some_and(|name| !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphabetic()))
}
