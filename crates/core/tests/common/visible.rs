//! Reduces HTML to what the viewer's allow-list can show, so an expected spec result and the model's
//! rendering are compared on visible content. Written independently of the crate's tokenizer, so a
//! mistake there does not cancel itself out here.
//!
//! The reductions, applied identically to both sides:
//! - comments, declarations and processing instructions vanish
//! - tags outside the allow-list vanish and their text stays
//! - script, style and iframe with a closer vanish with their content; without one, and all other
//!   tagfilter names, show as literal text
//! - synonyms unify (b/strong, i/em, s/del/strike), table sections and redundant nested strong vanish
//! - only attributes the model keeps survive, and unclosed elements are closed

const FILTERED_LITERAL: &[&str] = &[
    "title",
    "textarea",
    "xmp",
    "noembed",
    "noframes",
    "plaintext",
];
const DROP_PAIRED: &[&str] = &["script", "style", "iframe"];
const DROP_PAIRED_SILENT: &[&str] = &["object", "embed", "head", "template"];

fn canonical(name: &str) -> Option<&'static str> {
    Some(match name {
        "p" => "p",
        "div" | "section" | "article" | "aside" | "header" | "footer" | "main" | "nav"
        | "address" | "figure" | "figcaption" | "caption" | "dl" | "dt" | "dd" | "center" => "div",
        "h1" => "h1",
        "h2" => "h2",
        "h3" => "h3",
        "h4" => "h4",
        "h5" => "h5",
        "h6" => "h6",
        "blockquote" => "blockquote",
        "ul" => "ul",
        "ol" => "ol",
        "li" => "li",
        "table" => "table",
        "tr" => "tr",
        "td" => "td",
        "th" => "th",
        "details" => "details",
        "summary" => "summary",
        "pre" => "pre",
        "hr" => "hr",
        "a" => "a",
        "img" => "img",
        "br" => "br",
        "input" => "input",
        "b" | "strong" => "strong",
        "i" | "em" | "var" | "cite" | "dfn" => "em",
        "u" | "ins" => "u",
        "s" | "del" | "strike" => "del",
        "code" | "tt" | "samp" => "code",
        "kbd" => "kbd",
        "sub" => "sub",
        "sup" => "sup",
        "mark" => "mark",
        _ => return None,
    })
}

const VOID: &[&str] = &["hr", "img", "br", "input"];

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    hay.to_ascii_lowercase().find(needle)
}

struct Item {
    name: &'static str,
    silent: bool,
}

pub fn visible(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut stack: Vec<Item> = Vec::new();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        rest = &rest[lt..];
        if let Some(r) = rest
            .strip_prefix("<!-->")
            .or_else(|| rest.strip_prefix("<!--->"))
        {
            rest = r;
        } else if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |e| &r[e + 3..]);
        } else if let Some(r) = rest.strip_prefix("<![CDATA[") {
            rest = r.find("]]>").map_or("", |e| &r[e + 3..]);
        } else if rest.starts_with("<?") {
            rest = rest.find("?>").map_or("", |e| &rest[e + 2..]);
        } else if rest.starts_with("<!")
            && rest.as_bytes().get(2).is_some_and(u8::is_ascii_alphabetic)
        {
            rest = rest.find('>').map_or("", |e| &rest[e + 1..]);
        } else if let Some(r) = rest.strip_prefix("</") {
            let Some(gt) = r
                .find('>')
                .filter(|_| r.starts_with(|c: char| c.is_ascii_alphabetic()))
            else {
                out.push_str("&lt;");
                rest = &rest[1..];
                continue;
            };
            let name = r[..gt].trim().to_ascii_lowercase();
            let chunk = &rest[..gt + 3];
            rest = &r[gt + 1..];
            if FILTERED_LITERAL.contains(&name.as_str()) || DROP_PAIRED.contains(&name.as_str()) {
                push_literal(chunk, &mut out);
            } else if let Some(c) = canonical(&name) {
                close(c, &mut stack, &mut out);
            }
        } else if rest.as_bytes().get(1).is_some_and(u8::is_ascii_alphabetic) {
            let Some(gt) = tag_end(rest) else {
                out.push_str("&lt;");
                rest = &rest[1..];
                continue;
            };
            let chunk = &rest[..=gt];
            let tail = &rest[gt + 1..];
            let body = &chunk[1..chunk.len() - 1];
            let name_end = body
                .find(|c: char| c.is_ascii_whitespace() || c == '/')
                .unwrap_or(body.len());
            let name = body[..name_end].to_ascii_lowercase();
            let attrs = parse_attrs(&body[name_end..]);
            rest = tail;
            if DROP_PAIRED.contains(&name.as_str()) || DROP_PAIRED_SILENT.contains(&name.as_str()) {
                let closer = format!("</{name}");
                match find_ci(tail, &closer) {
                    Some(at) => rest = tail[at..].find('>').map_or("", |e| &tail[at + e + 1..]),
                    None if DROP_PAIRED.contains(&name.as_str()) => push_literal(chunk, &mut out),
                    None => {}
                }
            } else if FILTERED_LITERAL.contains(&name.as_str()) {
                push_literal(chunk, &mut out);
            } else if let Some(c) = canonical(&name) {
                open(c, &attrs, &mut stack, &mut out);
            }
        } else {
            out.push_str("&lt;");
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    while let Some(item) = stack.pop() {
        if !item.silent {
            out.push_str("</");
            out.push_str(item.name);
            out.push('>');
        }
    }
    out
}

fn push_literal(chunk: &str, out: &mut String) {
    out.push_str(
        &chunk
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;"),
    );
}

fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    let mut prev_eq = false;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '>' {
                    return Some(i);
                }
                if (c == '"' || c == '\'') && prev_eq {
                    quote = Some(c);
                }
            }
        }
        if !c.is_ascii_whitespace() {
            prev_eq = c == '=';
        }
    }
    None
}

fn parse_attrs(mut s: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    loop {
        s = s.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        if s.is_empty() {
            return out;
        }
        let e = s
            .find(|c: char| c.is_ascii_whitespace() || c == '=' || c == '/')
            .unwrap_or(s.len());
        if e == 0 {
            s = &s[1..];
            continue;
        }
        let k = s[..e].to_ascii_lowercase();
        s = s[e..].trim_start();
        let mut v = String::new();
        if let Some(r) = s.strip_prefix('=') {
            let r = r.trim_start();
            match r.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let close = r[1..].find(q).unwrap_or(r.len() - 1);
                    v = r[1..1 + close].to_string();
                    s = r.get(close + 2..).unwrap_or("");
                }
                _ => {
                    let e = r.find(|c: char| c.is_ascii_whitespace()).unwrap_or(r.len());
                    v = r[..e].to_string();
                    s = &r[e..];
                }
            }
        }
        if !out.iter().any(|(n, _)| *n == k) {
            out.push((k, v));
        }
    }
}

/// A URL reads the same however it is encoded: raw HTML keeps its spelling while the writer
/// percent-encodes. Entity decoding borrows the crate's table, since the spec's raw attributes use
/// names like `&ouml;` that the normalizer's small table cannot resolve.
fn canonical_url(v: &str) -> String {
    let decoded = mdview_core::links::percent_decode(&mdview_core::html::decode_entities(v));
    decoded
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn close(name: &str, stack: &mut Vec<Item>, out: &mut String) {
    let Some(i) = stack.iter().rposition(|it| it.name == name) else {
        return;
    };
    while stack.len() > i {
        let item = stack.pop().unwrap();
        if !item.silent {
            out.push_str("</");
            out.push_str(item.name);
            out.push('>');
        }
    }
}

fn get<'a>(attrs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

fn safe(url: &str, extra_data: bool) -> bool {
    let cleaned: String = url
        .chars()
        .filter(|c| !c.is_ascii_control() && *c != ' ')
        .collect();
    let Some(colon) = cleaned.find(':') else {
        return true;
    };
    let scheme = cleaned[..colon].to_ascii_lowercase();
    scheme.contains(['/', '?', '#'])
        || matches!(scheme.as_str(), "http" | "https" | "file")
        || (!extra_data && scheme == "mailto")
        || (extra_data && scheme == "data")
}

fn open(name: &'static str, attrs: &[(String, String)], stack: &mut Vec<Item>, out: &mut String) {
    let mut kept = String::new();
    fn add(kept: &mut String, k: &str, attrs: &[(String, String)]) {
        let Some(v) = get(attrs, k) else { return };
        if k == "href" || k == "src" {
            kept.push_str(&format!(" {k}=\"{}\"", canonical_url(v)));
        } else {
            kept.push_str(&format!(" {k}=\"{v}\""));
        }
    }
    match name {
        "a" => {
            if !get(attrs, "href").is_some_and(|h| safe(h, false)) {
                return;
            }
            add(&mut kept, "href", attrs);
            if get(attrs, "title").is_some_and(|t| !t.is_empty()) {
                add(&mut kept, "title", attrs);
            }
        }
        "img" => {
            if !get(attrs, "src").is_some_and(|h| safe(h, true)) {
                return;
            }
            add(&mut kept, "src", attrs);
            match get(attrs, "alt") {
                Some(_) => add(&mut kept, "alt", attrs),
                None => kept.push_str(" alt=\"\""),
            }
            if get(attrs, "title").is_some_and(|t| !t.is_empty()) {
                add(&mut kept, "title", attrs);
            }
        }
        "input" => {
            if !get(attrs, "type").is_some_and(|t| t.eq_ignore_ascii_case("checkbox")) {
                return;
            }
            kept.push_str(" type=\"checkbox\"");
            if get(attrs, "checked").is_some() {
                kept.push_str(" checked");
            }
        }
        "ol" => {
            if get(attrs, "start").is_some_and(|s| s.trim() != "1") {
                add(&mut kept, "start", attrs);
            }
        }
        "td" | "th" | "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            add(&mut kept, "align", attrs)
        }
        "code" => {
            if get(attrs, "class").is_some_and(|c| c.starts_with("language-")) {
                add(&mut kept, "class", attrs);
            }
        }
        "details" if get(attrs, "open").is_some() => kept.push_str(" open"),
        _ => {}
    }
    let in_pre = stack.iter().any(|i| i.name == "pre");
    let collapsed = (name == "strong" && stack.iter().any(|i| i.name == "strong"))
        || (name == "code" && in_pre && kept.is_empty());
    if !collapsed {
        out.push('<');
        out.push_str(name);
        out.push_str(&kept);
        out.push('>');
    }
    if !VOID.contains(&name) {
        stack.push(Item {
            name,
            silent: collapsed,
        });
    }
}
