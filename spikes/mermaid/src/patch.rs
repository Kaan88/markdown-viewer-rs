//! SVG repair pass over merman's output, run before resvg.
//!
//! Two faults, both independent of the diagram's content:
//!
//! 1. merman's foreignObject fallback wraps each label in a nested
//!    `<svg style="overflow:hidden">`. Anything wider than the measured box is cut off.
//! 2. merman's mindmap CSS computes `.edge-depth-N{stroke-width:17-3N}`, which goes negative at
//!    depth 6. resvg treats a negative stroke width as no stroke, so the edge disappears.
//!
//! This only touches `style="..."` on `<svg>` elements, `style="..."`/`stroke-width="..."` on any
//! element, and `<style>` bodies. Text content is copied through untouched, so a label that
//! literally says `overflow:hidden` or `stroke-width:-1` survives.
//!
//! The scan is one forward pass with no backtracking, so it is linear in the input. It is not a
//! general XML parser; on malformed markup it copies the rest of the input through unchanged.

use std::borrow::Cow;

/// The last positive value in merman's `17 - 3 * depth` series, so deep edges match depth 5.
pub const MIN_EDGE_STROKE: &str = "2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchOptions {
    pub overflow_visible: bool,
    pub fix_negative_stroke: bool,
}

impl PatchOptions {
    pub const FULL: Self = Self { overflow_visible: true, fix_negative_stroke: true };
    /// Stroke fix only. Labels stay clipped, which exposes any measurement that came out short.
    pub const STROKE_ONLY: Self = Self { overflow_visible: false, fix_negative_stroke: true };
}

pub fn patch_svg(svg: &str) -> Cow<'_, str> {
    patch_svg_with(svg, PatchOptions::FULL)
}

pub fn patch_svg_with(svg: &str, opt: PatchOptions) -> Cow<'_, str> {
    // Cheap exit: most families have neither pattern.
    if !svg.contains("overflow") && !svg.contains("stroke-width") {
        return Cow::Borrowed(svg);
    }
    let mut out = String::with_capacity(svg.len());
    let mut i = 0;
    while let Some(rel) = svg[i..].find('<') {
        let lt = i + rel;
        out.push_str(&svg[i..lt]);
        let rest = &svg[lt..];
        if rest.starts_with("<!--") {
            i = copy_until(svg, lt, "-->", &mut out);
        } else if rest.starts_with("<![CDATA[") {
            i = copy_until(svg, lt, "]]>", &mut out);
        } else if rest.starts_with("<?") {
            i = copy_until(svg, lt, "?>", &mut out);
        } else if rest.starts_with("</") || rest.starts_with("<!") {
            i = copy_until(svg, lt, ">", &mut out);
        } else {
            let Some(end) = tag_end(svg, lt) else {
                out.push_str(rest);
                return Cow::Owned(out);
            };
            let tag = &svg[lt..=end];
            let name = tag_name(tag);
            patch_tag(tag, name, opt, &mut out);
            i = end + 1;
            if name.eq_ignore_ascii_case("style") && !tag.ends_with("/>") {
                let body_end = find_ci(&svg[i..], "</style").map_or(svg.len(), |r| i + r);
                let body = &svg[i..body_end];
                if opt.fix_negative_stroke {
                    push_fixed_css(body, false, &mut out);
                } else {
                    out.push_str(body);
                }
                i = body_end;
            }
        }
        if i >= svg.len() {
            break;
        }
    }
    if i < svg.len() {
        out.push_str(&svg[i..]);
    }
    Cow::Owned(out)
}

fn copy_until(svg: &str, from: usize, end: &str, out: &mut String) -> usize {
    let stop = svg[from..].find(end).map_or(svg.len(), |r| from + r + end.len());
    out.push_str(&svg[from..stop]);
    stop
}

/// Index of the closing `>` of the tag at `lt`, skipping quoted attribute values.
fn tag_end(svg: &str, lt: usize) -> Option<usize> {
    let mut quote = 0u8;
    for (k, &b) in svg.as_bytes()[lt..].iter().enumerate() {
        match (quote, b) {
            (0, b'"' | b'\'') => quote = b,
            (0, b'>') => return Some(lt + k),
            (q, c) if q == c => quote = 0,
            _ => {}
        }
    }
    None
}

fn tag_name(tag: &str) -> &str {
    let t = &tag[1..];
    let n = t.find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/').unwrap_or(t.len());
    &t[..n]
}

fn patch_tag(tag: &str, name: &str, opt: PatchOptions, out: &mut String) {
    let b = tag.as_bytes();
    let mut i = 1 + name.len();
    out.push_str(&tag[..i]);
    let is_svg = name.eq_ignore_ascii_case("svg");
    while i < b.len() {
        // Attribute name.
        let name_start = i;
        while i < b.len() && !matches!(b[i], b'=' | b'>') && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let attr = &tag[name_start..i];
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() || b[j] != b'=' {
            let stop = if i == name_start { i + 1 } else { i };
            let stop = stop.min(b.len());
            out.push_str(&tag[name_start..stop]);
            i = stop;
            continue;
        }
        j += 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() || !matches!(b[j], b'"' | b'\'') {
            out.push_str(&tag[name_start..j]);
            i = j;
            continue;
        }
        let q = b[j];
        let val_start = j + 1;
        let Some(val_len) = b[val_start..].iter().position(|&c| c == q) else {
            out.push_str(&tag[name_start..]);
            return;
        };
        let val_end = val_start + val_len;
        let value = &tag[val_start..val_end];
        out.push_str(&tag[name_start..val_start]);
        if attr.eq_ignore_ascii_case("style") {
            let mut v = Cow::Borrowed(value);
            if opt.fix_negative_stroke {
                let mut s = String::with_capacity(value.len());
                push_fixed_css(&v, false, &mut s);
                v = Cow::Owned(s);
            }
            if opt.overflow_visible && is_svg {
                let mut s = String::with_capacity(v.len());
                push_fixed_css(&v, true, &mut s);
                v = Cow::Owned(s);
            }
            out.push_str(&v);
        } else if attr.eq_ignore_ascii_case("stroke-width")
            && opt.fix_negative_stroke
            && is_negative_number(value)
        {
            out.push_str(MIN_EDGE_STROKE);
        } else {
            out.push_str(value);
        }
        i = val_end;
    }
}

fn is_negative_number(v: &str) -> bool {
    let mut c = v.trim_start().chars();
    c.next() == Some('-') && c.next().is_some_and(|d| d.is_ascii_digit() || d == '.')
}

/// Copies CSS, rewriting `stroke-width:<negative>` or, with `overflow`, `overflow:hidden`.
fn push_fixed_css(css: &str, overflow: bool, out: &mut String) {
    let key = if overflow { "overflow" } else { "stroke-width" };
    let mut i = 0;
    while let Some(rel) = find_ci(&css[i..], key) {
        let at = i + rel;
        let after_key = at + key.len();
        let bounded = css[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        let mut v = after_key;
        let b = css.as_bytes();
        while v < b.len() && b[v].is_ascii_whitespace() {
            v += 1;
        }
        let colon = v < b.len() && b[v] == b':';
        if !(bounded && colon) {
            out.push_str(&css[i..after_key]);
            i = after_key;
            continue;
        }
        v += 1;
        while v < b.len() && b[v].is_ascii_whitespace() {
            v += 1;
        }
        let end = v + css[v..].find([';', '}', '!']).unwrap_or(css.len() - v);
        let value = css[v..end].trim_end();
        let hit = if overflow {
            value.eq_ignore_ascii_case("hidden")
        } else {
            is_negative_number(value)
        };
        out.push_str(&css[i..v]);
        if hit {
            out.push_str(if overflow { "visible" } else { MIN_EDGE_STROKE });
            i = v + value.len();
        } else {
            i = v;
        }
    }
    out.push_str(&css[i..]);
}

/// ASCII case-insensitive `find`. `needle` must be ASCII, so any hit is on a char boundary.
fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    let first = n[0].to_ascii_lowercase();
    (0..=h.len() - n.len())
        .find(|&i| h[i].to_ascii_lowercase() == first && h[i..i + n.len()].eq_ignore_ascii_case(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_svg_overflow_becomes_visible() {
        let s = r#"<svg id="r"><svg x="0" style="overflow:hidden;opacity:1"><text>a</text></svg></svg>"#;
        assert_eq!(
            patch_svg(s),
            r#"<svg id="r"><svg x="0" style="overflow:visible;opacity:1"><text>a</text></svg></svg>"#
        );
    }

    #[test]
    fn every_negative_stroke_width_in_a_stylesheet_is_fixed() {
        // A general fix, not just the -1 and -4 the original patch listed.
        let s = "<svg><style>#a .edge-depth-5{stroke-width:-1;}#a .edge-depth-6{stroke-width:-4;}\
                 #a .edge-depth-9{stroke-width: -13.5px }.ok{stroke-width:3}</style></svg>";
        let p = patch_svg(s);
        assert!(!p.contains(":-"), "{p}");
        assert!(!p.contains(": -"), "{p}");
        assert!(p.contains(".edge-depth-5{stroke-width:2;}"));
        assert!(p.contains("#a .edge-depth-9{stroke-width: 2 }"), "{p}");
        assert!(p.contains(".ok{stroke-width:3}"));
    }

    #[test]
    fn inline_and_attribute_strokes_are_fixed() {
        let s = r#"<svg><path style="fill:none;stroke-width:-7" stroke-width="-2.5"/><path stroke-width="3"/></svg>"#;
        assert_eq!(
            patch_svg(s),
            r#"<svg><path style="fill:none;stroke-width:2" stroke-width="2"/><path stroke-width="3"/></svg>"#
        );
    }

    #[test]
    fn label_text_is_never_rewritten() {
        let s = r#"<svg><text x="1">overflow:hidden stroke-width:-1 &lt;svg style="overflow:hidden"&gt;</text></svg>"#;
        assert_eq!(patch_svg(s), s);
        let s = r#"<svg><g aria-label="stroke-width:-1"><title>overflow:hidden</title></g></svg>"#;
        assert_eq!(patch_svg(s), s);
    }

    #[test]
    fn only_svg_elements_get_overflow_changed() {
        let s = r#"<svg><g style="overflow:hidden"/><svg style="overflow: hidden ; x:y"/></svg>"#;
        assert_eq!(patch_svg(s), r#"<svg><g style="overflow:hidden"/><svg style="overflow: visible ; x:y"/></svg>"#);
    }

    #[test]
    fn similar_property_names_are_left_alone() {
        let s = r#"<svg style="text-overflow:hidden;overflow-x:hidden;x-stroke-width:-1"><style>.a{my-stroke-width:-1}</style></svg>"#;
        assert_eq!(patch_svg(s), s);
    }

    #[test]
    fn stroke_only_keeps_clipping() {
        let s = r#"<svg style="overflow:hidden;stroke-width:-1"/>"#;
        assert_eq!(patch_svg_with(s, PatchOptions::STROKE_ONLY), r#"<svg style="overflow:hidden;stroke-width:2"/>"#);
    }

    #[test]
    fn patch_is_idempotent() {
        let s = r#"<svg style="overflow:hidden"><style>.a{stroke-width:-1}</style><text>é</text></svg>"#;
        let once = patch_svg(s).into_owned();
        assert_eq!(patch_svg(&once), once);
    }

    #[test]
    fn untouched_families_borrow() {
        assert!(matches!(patch_svg("<svg><text>pie</text></svg>"), Cow::Borrowed(_)));
    }

    #[test]
    fn malformed_markup_never_panics_or_loses_bytes() {
        let seed = "<svg style=\"overflow:hidden\"><style>.é{stroke-width:-1}</style><!-- c --><a b='é'>ş</a></svg>";
        let chars: Vec<(usize, char)> = seed.char_indices().collect();
        for (at, _) in &chars {
            // Every prefix and suffix on a char boundary, plus odd openers.
            let _ = patch_svg(&seed[..*at]);
            let _ = patch_svg(&seed[*at..]);
        }
        for s in ["<", "<>", "< ", "<a", "<a=", "<a b", "<a b=", "<a b=\"", "<a b=\"c", "<style>", "<style", "</",
                  "<!--", "<![CDATA[", "<?", "<svg style=", "<svg style='overflow:", "<svg style='stroke-width:-",
                  "<a stroke-width=\"-\">", "<a overflow stroke-width>"] {
            let p = patch_svg(s);
            assert_eq!(p, s, "no-change inputs keep their bytes");
        }
    }

    #[test]
    fn pathological_input_stays_linear() {
        // 8 MiB of the hottest path. The scan must not go quadratic.
        let s = format!("<svg><style>{}</style></svg>", ".a{stroke-width:-1}".repeat(420_000));
        let t = std::time::Instant::now();
        let p = patch_svg(&s);
        assert!(!p.contains(":-1"));
        assert!(t.elapsed().as_millis() < 2000, "{:?}", t.elapsed());
    }
}
