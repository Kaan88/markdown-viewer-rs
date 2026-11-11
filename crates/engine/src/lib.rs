//! The engine's rasterizers. The viewer talks to this through `mdview-engine-proto`, never by
//! linking it, so a malformed document cannot take the viewer down and the memory goes back when
//! the process exits. resvg draws SVG; RaTeX lays out TeX and hands the display list to the same
//! resvg path. Syntax highlighting joins them in a later Phase 4 slice.

/// Straight-alpha RGBA, row-major, the shape the renderer's image cache takes. `baseline` is the
/// distance from the top of the raster to the text baseline, in pixels.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub baseline: u32,
    pub rgba: Vec<u8>,
}

/// The options every job shares: system fonts, so SVG text is not blank. Loading fontconfig is the
/// expensive part of an engine start, so the engine does it once, not once per job.
pub fn options() -> resvg::usvg::Options<'static> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    // usvg's default selector falls back to a serif family, which a minimal Linux install may not
    // have. The viewer's own stack is sans, so retry a missing family with a sans face.
    options.font_resolver.select_font = Box::new(|font, database| {
        let default = resvg::usvg::FontResolver::default_font_selector();
        if let Some(id) = default(font, database) {
            return Some(id);
        }
        let families = [
            resvg::usvg::fontdb::Family::Name("Noto Sans"),
            resvg::usvg::fontdb::Family::Name("DejaVu Sans"),
            resvg::usvg::fontdb::Family::Name("Cantarell"),
            resvg::usvg::fontdb::Family::Name("Liberation Sans"),
            resvg::usvg::fontdb::Family::SansSerif,
        ];
        let query = resvg::usvg::fontdb::Query {
            families: &families,
            weight: resvg::usvg::fontdb::Weight(font.weight()),
            stretch: resvg::usvg::fontdb::Stretch::Normal,
            style: resvg::usvg::fontdb::Style::Normal,
        };
        database.query(&query)
    });
    options
}

/// Renders SVG bytes at their natural size, shrinking to stay under `max_pixels`. A document's
/// `width`/`height` attributes are honoured by usvg; a viewBox-only document gets its default.
/// `options` carries the font database, which the engine loads once per process.
pub fn render_svg(
    bytes: &[u8],
    max_pixels: u64,
    options: &resvg::usvg::Options,
) -> Result<Image, String> {
    render_svg_at(bytes, 1.0, max_pixels, options)
}

/// The same rasterizer at a requested scale, for diagrams that should stay crisp when the viewer
/// draws them at device pixel ratios above 1. The pixel cap still wins.
pub fn render_svg_at(
    bytes: &[u8],
    requested_scale: f32,
    max_pixels: u64,
    options: &resvg::usvg::Options,
) -> Result<Image, String> {
    if !requested_scale.is_finite() || requested_scale <= 0.0 {
        return Err("bad render scale".to_string());
    }
    let requested_scale = f64::from(requested_scale.clamp(0.05, 8.0));
    let tree =
        resvg::usvg::Tree::from_data(bytes, options).map_err(|e| format!("invalid SVG: {e}"))?;
    let size = tree.size();
    let (width, height) = (size.width(), size.height());
    if !(width.is_finite() && height.is_finite()) || width <= 0.0 || height <= 0.0 {
        return Err("SVG has no size".to_string());
    }
    let natural = width as f64 * height as f64 * requested_scale * requested_scale;
    let shrink = if natural > max_pixels as f64 {
        (max_pixels as f64 / natural).sqrt()
    } else {
        1.0
    };
    let scale = requested_scale * shrink;
    let raster_width = (width as f64 * scale).floor().max(1.0) as u32;
    let raster_height = (height as f64 * scale).floor().max(1.0) as u32;
    if raster_width as u64 * raster_height as u64 > max_pixels {
        return Err("SVG is too large".to_string());
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(raster_width, raster_height)
        .ok_or_else(|| "could not allocate the SVG raster".to_string())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32),
        &mut pixmap.as_mut(),
    );
    // tiny-skia keeps premultiplied pixels; the renderer blends straight alpha.
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    Ok(Image {
        width: raster_width,
        height: raster_height,
        baseline: raster_height,
        rgba,
    })
}

/// Draws a Mermaid diagram through merman and the resvg rasterizer. `scale` is device pixels per
/// SVG unit; the viewer asks for twice its scale and halves the box, like math.
pub fn render_mermaid(
    source: &str,
    scale: f32,
    dark: bool,
    max_pixels: u64,
    options: &resvg::usvg::Options,
) -> Result<Image, String> {
    let svg = mermaid_page_svg(source, dark)?;
    render_svg_at(svg.as_bytes(), scale, max_pixels, options)
}

/// The SVG of a Mermaid diagram as the page draws it: themed, with labels resvg can draw.
fn mermaid_page_svg(source: &str, dark: bool) -> Result<String, String> {
    // Mermaid defaults to HTML labels in `foreignObject`, which resvg cannot draw; its SVG text
    // fallback positions some families inside the node but shifts class, ER and mindmap labels off
    // the left edge. Asking for native text labels avoids both problems. The directive comes
    // first, so a document's own `%%{init: ...}%%` can still override it.
    let init = if dark {
        // Mermaid's stock dark theme draws #333 nodes on our #202020 page, so outlines, gantt bars
        // and pie slices nearly vanish. These overrides keep the palette dark but lift the contrast.
        r##"{"theme":"dark","themeVariables":{"fontSize":"11px","background":"#202020","primaryColor":"#3d4654","primaryTextColor":"#e4e4e4","primaryBorderColor":"#9aa4b2","lineColor":"#c8d0da","secondaryColor":"#3a3a44","tertiaryColor":"#2b2b2b","mainBkg":"#3d4654","nodeBorder":"#9aa4b2","clusterBkg":"#2b2b2b","textColor":"#e4e4e4","titleColor":"#f5f5f5","edgeLabelBackground":"#202020","sectionBkgColor":"#2b2b2b","altSectionBkgColor":"#242424","taskBkgColor":"#4b5563","taskBorderColor":"#9aa4b2","taskTextColor":"#ffffff","taskTextOutsideColor":"#e4e4e4","gridColor":"#4a4a4a","pie1":"#5b6b82","pie2":"#7c5c8a","pie3":"#2f6f6f","pie4":"#8a6d3b"},"htmlLabels":false,"flowchart":{"htmlLabels":false},"gantt":{"leftPadding":150,"gridLineStartPadding":35,"useWidth":900}}"##
    } else {
        r##"{"theme":"default","themeVariables":{"fontSize":"11px"},"htmlLabels":false,"flowchart":{"htmlLabels":false},"gantt":{"leftPadding":150,"gridLineStartPadding":35,"useWidth":900}}"##
    };
    let init = family_init(init, diagram_keyword(source), dark);
    let source = format!("%%{{init: {init}}}%%\n{source}");
    let svg = settle_gantt_ticks(&source, mermaid_svg(&source)?)?;
    let svg = inline_label_uses(&svg);
    let svg = family_svg_fix(svg, diagram_keyword(&source), dark);
    // Families that name no font (tree view) would get usvg's serif default. A root attribute
    // loses to any font the diagram's own CSS names, so styled families are untouched.
    Ok(svg.replacen("<svg ", "<svg font-family=\"Segoe UI, sans-serif\" ", 1))
}

/// The first word of the diagram, after blank lines and `%%` directives or comments.
fn diagram_keyword(source: &str) -> &str {
    source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("")
}

/// Dark-theme colours that merman hard-codes and no theme variable reaches: the byte numbers and
/// title of a packet diagram, the journey title and the event-modeling lane titles (all black), and
/// the relationship lines and labels of a C4 diagram (dark grey) would vanish on the dark page.
fn family_svg_fix(svg: String, keyword: &str, dark: bool) -> String {
    // The tree view sizes its canvas from merman's text metrics, and the real font's longest
    // name ("Cargo.toml") runs past the right edge. Leave room for it.
    if keyword == "treeView-beta" {
        return widen_viewbox(svg, 30.0);
    }
    let svg = match keyword {
        "usecase-beta" | "usecase" | "useCase" => centre_label_groups(svg),
        // Participant boxes are measured for a narrower font than the one that draws them.
        "zenuml" => svg.replace(
            ".participant-label{font-family:Helvetica,Verdana,serif;font-size:16px",
            ".participant-label{font-family:Helvetica,Verdana,serif;font-size:14px",
        ),
        _ => svg,
    };
    if !dark {
        return svg;
    }
    match keyword {
        // The rings are a light grey wash, which turns the dark page muddy.
        "radar-beta" | "radar" => svg
            .replace(
                ".radarGraticule{fill:#DEDEDE;fill-opacity:0.3;stroke:#DEDEDE;",
                ".radarGraticule{fill:#c8d0da;fill-opacity:0.04;stroke:#5b6470;",
            )
            .replace(
                ".radarAxisLine{stroke:lightgrey;stroke-width:2;",
                ".radarAxisLine{stroke:#5b6470;stroke-width:1;",
            ),
        // ZenUML carries one fixed light stylesheet and takes no theme variables.
        "zenuml" => svg
            .replace("fill:#fff}", "fill:#2b2b2b}")
            .replace("fill:#fff;", "fill:#2b2b2b;")
            .replace("fill:#222", "fill:#e4e4e4")
            .replace("color:#222", "color:#e4e4e4")
            .replace("fill:#333", "fill:#e4e4e4")
            .replace("fill:#000", "fill:#c8d0da")
            .replace("stroke=\"#000\"", "stroke=\"#c8d0da\"")
            .replace("stroke:#000", "stroke:#c8d0da")
            .replace("fill:#666", "fill:#9aa4b2")
            .replace("stroke:#666", "stroke:#9aa4b2")
            .replace("fill:#dedede", "fill:#4b5563")
            .replace("fill:#fff5ad", "fill:#4a4630"),
        "packet-beta" | "packet" => svg
            .replace(
                ".packetByte.start{fill:black;}",
                ".packetByte.start{fill:#c8d0da;}",
            )
            .replace(
                ".packetByte.end{fill:black;}",
                ".packetByte.end{fill:#c8d0da;}",
            )
            .replace(".packetTitle{fill:black;", ".packetTitle{fill:#e4e4e4;"),
        // The title carries an empty `fill`, which paints black.
        "journey" => svg.replace("fill=\"\" font-family", "fill=\"#f5f5f5\" font-family"),
        // Lane titles carry no fill at all.
        "eventmodeling" => svg.replace(
            "<text font-weight=\"bold\" ",
            "<text fill=\"#e4e4e4\" font-weight=\"bold\" ",
        ),
        "C4Context" | "C4Container" | "C4Component" | "C4Dynamic" | "C4Deployment" => svg
            .replace("fill=\"#444444\"", "fill=\"#c8d0da\"")
            .replace("stroke=\"#444444\"", "stroke=\"#c8d0da\""),
        _ => svg,
    }
}

/// Use-case labels sit in a group shifted left by half the label box, as for a start-anchored
/// HTML label, while the text inside is centred as well, which leaves it hanging off the left of
/// its shape. This drops the group's horizontal shift so the centred text lands on the shape.
fn centre_label_groups(svg: String) -> String {
    const OPEN: &str = "<g class=\"label";
    const SHIFT: &str = "transform=\"translate(";
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg.as_str();
    while let Some(at) = rest.find(OPEN) {
        let tag_end = at + rest[at..].find('>').unwrap_or(rest.len() - at);
        let tag = &rest[at..tag_end];
        match tag.find(SHIFT).map(|i| i + SHIFT.len()) {
            Some(x) if tag[x..].contains(',') => {
                let comma = x + tag[x..].find(',').unwrap_or(0);
                out.push_str(&rest[..at + x]);
                out.push('0');
                out.push_str(&rest[at + comma..tag_end]);
            }
            _ => out.push_str(&rest[..tag_end]),
        }
        rest = &rest[tag_end..];
    }
    out.push_str(rest);
    out
}

/// Grows the root `viewBox` width by `extra` user units.
fn widen_viewbox(svg: String, extra: f32) -> String {
    let Some(at) = svg.find("viewBox=\"") else {
        return svg;
    };
    let start = at + "viewBox=\"".len();
    let Some(len) = svg[start..].find('"') else {
        return svg;
    };
    let numbers: Vec<f32> = svg[start..start + len]
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    let [x, y, w, h] = numbers[..] else {
        return svg;
    };
    format!(
        "{}{x} {y} {} {h}{}",
        &svg[..start],
        w + extra,
        &svg[start + len..]
    )
}

/// Adds the per-family settings that the shared init does not cover. A fixed-grid layout
/// (timeline) needs Mermaid's own 16 px text, and several newer families take their colours from
/// their own theme sections, which the shared dark palette does not reach. Families not named
/// here, including the original eight, keep the shared init unchanged.
fn family_init(init: &str, keyword: &str, dark: bool) -> String {
    let (font, theme, root): (&str, &str, &str) = match (keyword, dark) {
        ("timeline", false) => ("16px", r##","scaleLabelColor":"#1a1a1a""##, ""),
        ("timeline", true) => (
            "16px",
            r##","scaleLabelColor":"#f2f2f2","cScale0":"#4b5d7a","cScale1":"#6b4f8a","cScale2":"#3f7f7f","cScale3":"#8a6d3b","cScale4":"#7a4b4b","cScale5":"#4b7a52","cScale6":"#5b6b82","cScale7":"#7c5c8a","cScale8":"#2f6f6f","cScale9":"#8a6d3b","cScale10":"#7a4b4b","cScale11":"#4b7a52""##,
            "",
        ),
        // Journey's boxes are a fixed 150 units wide and the whole chart is scaled to the page,
        // so text measured at 11 px ends up about 5 px tall.
        ("journey", _) => ("20px", "", ""),
        ("treeView-beta", true) => (
            "11px",
            r##","treeView":{"labelColor":"#e4e4e4","lineColor":"#9aa4b2","descriptionColor":"#9bc785"}"##,
            "",
        ),
        ("quadrantChart", true) => ("11px", r##","quadrantPointFill":"#e4e4e4""##, ""),
        // The stock dark scale starts near black, so sections vanish into the page.
        ("treemap-beta" | "treemap", true) => (
            "11px",
            r##","cScale0":"#4b5d7a","cScale1":"#6b4f8a","cScale2":"#3f7f7f","cScale3":"#8a6d3b","cScale4":"#7a4b4b","cScale5":"#4b7a52","cScale6":"#5b6b82","cScale7":"#7c5c8a","cScale8":"#2f6f6f","cScale9":"#8a6d3b","cScale10":"#7a4b4b","cScale11":"#4b7a52""##,
            "",
        ),
        // Curves are drawn half transparent over each other, so they need bright, distinct hues.
        ("radar-beta" | "radar", true) => (
            "11px",
            r##","cScale0":"#6aa9ff","cScale1":"#f59e6b","cScale2":"#7bd389","cScale3":"#d58cf0","cScale4":"#f2d16b","cScale5":"#6fd3d3","cScale6":"#f28b9b","cScale7":"#a5b4fc","cScale8":"#b7e07a","cScale9":"#e0a36b","cScale10":"#8fd0f0","cScale11":"#e6a8d7""##,
            "",
        ),
        _ => return init.to_string(),
    };
    let mut out = init.replacen(
        r#""fontSize":"11px""#,
        &format!(r#""fontSize":"{font}"{theme}"#),
        1,
    );
    if !root.is_empty() {
        out.pop();
        out.push_str(root);
        out.push('}');
    }
    out
}

/// Mermaid source to SVG text.
fn mermaid_svg(source: &str) -> Result<String, String> {
    use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
    let output = Renderer::new()
        .render(RenderRequest::svg(
            source,
            OperationControl::new(),
            SvgRequest {
                // Families that still emit HTML labels get a `<text>` overlay from this pipeline.
                // `inline_label_uses` then moves that text out of the nested `<defs>` SVG, which
                // resvg does not draw.
                pipeline: Some(merman::svg::SvgPipeline::readable()),
                ..Default::default()
            },
        ))
        .map_err(|e| format!("mermaid: {e}"))?;
    let RenderOutput::Svg(Some(svg)) = output else {
        return Err("mermaid found no diagram".to_string());
    };
    Ok(svg.svg().to_string())
}

/// Mermaid picks gantt ticks for about ten per axis, so a plan of a few days gets 12-hour ticks
/// under a date-only label: every date shows twice and the labels crowd together. While
/// neighbouring labels repeat, this steps up to the next calendar interval. A document's own
/// `tickInterval` is left alone, and other diagrams pass through.
fn settle_gantt_ticks(source: &str, mut svg: String) -> Result<String, String> {
    if is_gantt(source) && !source.contains("tickInterval") {
        for interval in ["1day", "1week", "1month"] {
            if !has_repeated_ticks(&svg) {
                break;
            }
            svg = mermaid_svg(&format!("{source}\ntickInterval {interval}"))?;
        }
    }
    Ok(svg)
}

/// True when the first diagram line, after `%%` directives and comments, is `gantt`.
fn is_gantt(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .is_some_and(|line| line == "gantt" || line.starts_with("gantt "))
}

/// True when two neighbouring gantt axis tick labels read the same.
fn has_repeated_ticks(svg: &str) -> bool {
    let mut previous: Option<&str> = None;
    for group in svg.split(r#"<g class="tick""#).skip(1) {
        let label = group
            .split_once("<text")
            .and_then(|(_, rest)| rest.split_once('>'))
            .and_then(|(_, rest)| rest.split_once("</text>"))
            .map(|(label, _)| label);
        if let Some(label) = label {
            if previous == Some(label) {
                return true;
            }
            previous = Some(label);
        }
    }
    false
}

/// `name="number"` in an SVG tag fragment.
fn attr_f32(fragment: &str, name: &str) -> Option<f32> {
    let at = fragment.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = &fragment[at..];
    let end = rest.find('"')?;
    rest[..end].parse().ok()
}

/// One fallback `<text>` line, sized and squeezed for the real font.
fn fallback_text(raw: &str) -> String {
    // The fallback text keeps Mermaid's default 16 px, but the nodes were measured at our
    // 11 px init size; at 16 the label overflows the circle and boxes.
    let mut text = raw.replace("font-size: 16px", "font-size: 11px");
    // The fallback font is wider than merman's deterministic measurement (notably DejaVu on
    // Linux), and the node shape clips the first and last letters. Compress the label
    // horizontally around its own anchor so it stays inside the circle or box.
    if let Some(open) = text.find('>')
        && let Some(cx) = attr_f32(&text[..open], "x")
    {
        text.insert_str(
            open,
            &format!(" transform=\"translate({cx} 0) scale(0.85 1) translate(-{cx} 0)\""),
        );
    }
    text
}

/// Replaces `<use href="#mindmap-foreignobject-label-N">` with the fallback `<text>` that the
/// readable pipeline put inside the referenced `<defs>` group. resvg does not draw labels through a
/// `<use>` of a nested `<svg>`; inlining the text at the use site keeps the label group's transform.
fn inline_label_uses(svg: &str) -> String {
    const PREFIX: &str = "foreignobject-label-";
    let mut labels: Vec<(String, String)> = Vec::new();
    let mut rest = svg;
    while let Some(found) = rest.find("<g id=\"") {
        let after = &rest[found + 7..];
        let Some(quote) = after.find('"') else { break };
        let id = &after[..quote];
        if !id.contains(PREFIX) {
            rest = &after[quote..];
            continue;
        }
        let Some(text_start) = after.find("<text ") else {
            break;
        };
        // A wrapped label is several consecutive `<text>` lines; take them all.
        let mut end = text_start;
        let mut text = String::new();
        while after[end..].starts_with("<text ") {
            let Some(text_end) = after[end..].find("</text>") else {
                break;
            };
            let next = end + text_end + "</text>".len();
            text.push_str(&fallback_text(&after[end..next]));
            end = next;
        }
        if text.is_empty() {
            break;
        }
        labels.push((id.to_string(), text));
        rest = &after[end..];
    }
    if labels.is_empty() {
        return svg.to_string();
    }
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(found) = rest.find("<use ") {
        out.push_str(&rest[..found]);
        let after = &rest[found..];
        let Some(end) = after.find("/>") else {
            out.push_str(after);
            return out;
        };
        let tag = &after[..end + 2];
        let mut replaced = false;
        for (id, text) in &labels {
            if tag.contains(&format!("href=\"#{id}\"")) {
                // `<use x= y=>` offsets the label; mindmap uses none, journey and kanban do.
                let (x, y) = (
                    attr_f32(tag, "x").unwrap_or(0.0),
                    attr_f32(tag, "y").unwrap_or(0.0),
                );
                if x != 0.0 || y != 0.0 {
                    out.push_str(&format!("<g transform=\"translate({x} {y})\">{text}</g>"));
                } else {
                    out.push_str(text);
                }
                replaced = true;
                break;
            }
        }
        if !replaced {
            out.push_str(tag);
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// Draws a TeX formula through RaTeX and the same resvg rasterizer. `font_size` is device pixels
/// per em; `display` picks the display style (otherwise text style, for inline math). The returned
/// `baseline` lets the renderer sit the image on the text baseline.
pub fn render_math(
    source: &str,
    display: bool,
    font_size: f32,
    color: [u8; 3],
    max_pixels: u64,
    options: &resvg::usvg::Options,
) -> Result<Image, String> {
    use ratex_layout::{LayoutOptions, layout, to_display_list, validate_layout};
    use ratex_types::color::Color;

    if source.is_empty() {
        return Err("empty formula".to_string());
    }
    let ast = ratex_parser::parser::parse(source).map_err(|e| format!("parse: {e}"))?;
    let style = if display {
        ratex_types::math_style::MathStyle::Display
    } else {
        ratex_types::math_style::MathStyle::Text
    };
    let colour = Color {
        r: f32::from(color[0]) / 255.0,
        g: f32::from(color[1]) / 255.0,
        b: f32::from(color[2]) / 255.0,
        a: 1.0,
    };
    let layout_options = LayoutOptions::default()
        .with_style(style)
        .with_color(colour);
    let lbox = layout(&ast, &layout_options);
    validate_layout(&lbox)?;
    let list = to_display_list(&lbox);

    let padding = 1.0;
    let svg_options = ratex_svg::SvgOptions {
        font_size: f64::from(font_size),
        padding,
        stroke_width: 1.0,
        embed_glyphs: true,
        font_dir: String::new(),
    };
    // RGB paint syntax keeps the embedded glyphs valid SVG 1.1; RaTeX's default `rgba()` is not.
    let svg = ratex_svg::render_to_svg_with_color_syntax(
        &list,
        &svg_options,
        ratex_svg::SvgColorSyntax::Rgb,
    );
    // RaTeX sizes its root in points while its viewBox is in pixels. resvg would apply 96/72 and
    // make every formula a third larger than the font size the layout asked for.
    let svg = px_sizes(svg);
    let mut image = render_svg(svg.as_bytes(), max_pixels, options)?;
    image.baseline = (padding + lbox.height * f64::from(font_size))
        .round()
        .clamp(0.0, f64::from(image.height)) as u32;
    Ok(image)
}

/// Removes the `pt` unit from the root `width`/`height` of a RaTeX SVG, leaving the viewBox's
/// pixel numbers in charge.
fn px_sizes(mut svg: String) -> String {
    for attribute in [" width=\"", " height=\""] {
        let Some(start) = svg.find(attribute).map(|i| i + attribute.len()) else {
            continue;
        };
        let Some(len) = svg[start..].find('"') else {
            continue;
        };
        let end = start + len;
        if svg[start..end].ends_with("pt") {
            svg.replace_range(end - 2..end, "");
        }
    }
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    const BADGE: &str = include_str!("../../../fixtures/assets/badge.svg");

    #[test]
    fn a_label_use_is_inlined_and_compressed() {
        let svg = concat!(
            r##"<defs><g id="merman-foreignobject-label-1"><svg width="33" height="16">"##,
            r##"<text x="16.5" y="8" style="font-size: 16px; transform: none;">Reading</text>"##,
            r##"</svg></g></defs><use x="0" y="0" href="#merman-foreignobject-label-1"/>"##,
        );
        let out = inline_label_uses(svg);
        assert!(!out.contains("<use"), "{out}");
        assert!(out.contains("font-size: 11px"), "{out}");
        assert!(out.contains("scale(0.85 1)"), "{out}");
    }

    #[test]
    fn the_badge_renders_at_its_natural_size() {
        let image = render_svg(
            BADGE.as_bytes(),
            24_000_000,
            &resvg::usvg::Options::default(),
        )
        .expect("the badge renders");
        assert_eq!((image.width, image.height), (180, 28));
        assert_eq!(image.rgba.len(), 180 * 28 * 4);
        // #0369a1, the bar's fill, somewhere inside the left edge.
        let blue = image
            .rgba
            .chunks_exact(4)
            .filter(|p| p[2] > 140 && p[0] < 40 && p[1] > 80)
            .count();
        assert!(blue > 1000, "blue pixels: {blue}");
        assert!(image.rgba.chunks_exact(4).any(|p| p[3] > 0));
    }

    #[test]
    fn an_oversize_svg_is_scaled_under_the_pixel_cap() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4000" height="4000"><rect width="4000" height="4000" fill="#000"/></svg>"##;
        let image = render_svg(
            source.as_bytes(),
            1_000_000,
            &resvg::usvg::Options::default(),
        )
        .expect("it renders smaller");
        assert!(image.width as u64 * image.height as u64 <= 1_000_000);
        assert!(image.width >= 900 && image.width <= 1001, "{}", image.width);
    }

    const READING_PLAN: &str = "gantt\n    title Reading plan\n    dateFormat YYYY-MM-DD\n    \
        section Documentation\n    Read README :read, 2026-11-02, 2d\n    Explore docs :after read, 3d";

    #[test]
    fn a_short_gantt_gets_one_tick_label_per_date() {
        // Stock Mermaid labels this five-day plan every 12 hours with dates only, so each date
        // appears twice. The settled chart must not repeat a label.
        let raw = super::mermaid_svg(READING_PLAN).unwrap();
        assert!(super::has_repeated_ticks(&raw), "the stock ticks repeat");
        let settled = super::settle_gantt_ticks(READING_PLAN, raw).unwrap();
        assert!(
            !super::has_repeated_ticks(&settled),
            "settled ticks still repeat"
        );
        assert!(settled.contains(">2026-11-03<") && settled.contains(">2026-11-06<"));
    }

    #[test]
    fn gantt_ticks_are_left_alone_when_they_do_not_repeat_or_are_set() {
        let year = "gantt\n dateFormat YYYY-MM-DD\n section A\n Long :a, 2026-01-01, 300d";
        let raw = super::mermaid_svg(year).unwrap();
        assert!(!super::has_repeated_ticks(&raw));
        assert_eq!(super::settle_gantt_ticks(year, raw.clone()).unwrap(), raw);
        // A document that chose its interval keeps it, repeats and all.
        let chosen = format!("{READING_PLAN}\n    tickInterval 12hour");
        let raw = super::mermaid_svg(&chosen).unwrap();
        assert_eq!(
            super::settle_gantt_ticks(&chosen, raw.clone()).unwrap(),
            raw
        );
    }

    #[test]
    fn the_eight_showcase_diagram_families_render() {
        let options = options();
        let sources = [
            ("flowchart", "flowchart LR\n A[Start] --> B[Done]"),
            ("sequence", "sequenceDiagram\n A->>B: Hi"),
            ("class", "classDiagram\n class A\n A : +run()"),
            ("state", "stateDiagram-v2\n [*] --> Idle\n Idle --> [*]"),
            ("er", "erDiagram\n CUSTOMER ||--o{ ORDER : places"),
            (
                "gantt",
                "gantt\n title Plan\n dateFormat YYYY-MM-DD\n section One\n Task :a1, 2026-01-01, 2d",
            ),
            ("pie", "pie title Files\n \"Rust\" : 70\n \"Docs\" : 30"),
            ("mindmap", "mindmap\n root((Plan))\n  Docs\n  Code"),
        ];
        for (name, source) in sources {
            let image = render_mermaid(source, 1.0, false, 24_000_000, &options)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(
                image.width > 20 && image.height > 20,
                "{name}: {}x{}",
                image.width,
                image.height
            );
            let inked = image.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
            assert!(inked > 100, "{name}: {inked} inked pixels");
        }
    }

    /// The ```mermaid blocks of `fixtures/diagrams-more.md`, as (heading, source).
    fn more_diagrams() -> Vec<(String, String)> {
        let text = include_str!("../../../fixtures/diagrams-more.md").replace("\r\n", "\n");
        let mut found = Vec::new();
        let mut heading = String::new();
        let mut source: Option<String> = None;
        for line in text.lines() {
            if let Some(title) = line.strip_prefix("## ") {
                heading = title.to_string();
            } else if line == "```mermaid" {
                source = Some(String::new());
            } else if line == "```" {
                if let Some(done) = source.take() {
                    found.push((heading.clone(), done));
                }
            } else if let Some(open) = source.as_mut() {
                open.push_str(line);
                open.push('\n');
            }
        }
        found
    }

    /// Not a test: writes each fixture diagram's final SVG to `target/tmp/mermaid-svg`, for
    /// working on the per-family fixes. `cargo test -p mdview-engine dump_svgs -- --ignored`
    #[test]
    #[ignore]
    fn dump_svgs() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/mermaid-svg");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, source) in more_diagrams() {
            for dark in [false, true] {
                if let Ok(svg) = super::mermaid_page_svg(&source, dark) {
                    let theme = if dark { "dark" } else { "light" };
                    let name = name.replace([' ', ':', '(', ')'], "");
                    std::fs::write(dir.join(format!("{name}-{theme}.svg")), svg).unwrap();
                }
            }
        }
    }

    #[test]
    fn every_extra_family_in_the_fixture_renders_in_both_themes() {
        let options = options();
        let all = more_diagrams();
        assert_eq!(all.len(), 24, "23 enabled families plus the disabled one");
        for (name, source) in &all {
            if name.starts_with("Not enabled") {
                continue;
            }
            for dark in [false, true] {
                let started = std::time::Instant::now();
                let image = render_mermaid(source, 2.0, dark, 24_000_000, &options)
                    .unwrap_or_else(|e| panic!("{name} (dark {dark}): {e}"));
                assert!(
                    image.width > 100 && image.height > 40 && image.width < 6000,
                    "{name}: {}x{}",
                    image.width,
                    image.height
                );
                let inked = image.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
                assert!(inked > 500, "{name}: {inked} inked pixels");
                // Loose bound: the target is 300 ms on a quiet machine; a busy test run needs room.
                assert!(
                    started.elapsed().as_millis() < 3000,
                    "{name}: {:?}",
                    started.elapsed()
                );
            }
        }
    }

    #[test]
    fn a_disabled_family_is_an_error_so_the_viewer_shows_its_source() {
        let options = options();
        let (_, source) = more_diagrams()
            .into_iter()
            .find(|(name, _)| name.starts_with("Not enabled"))
            .unwrap();
        assert!(render_mermaid(&source, 1.0, false, 24_000_000, &options).is_err());
    }

    #[test]
    fn the_original_eight_families_keep_the_shared_init() {
        for keyword in [
            "flowchart",
            "graph",
            "sequenceDiagram",
            "classDiagram",
            "stateDiagram-v2",
            "erDiagram",
            "gantt",
            "pie",
            "mindmap",
        ] {
            for dark in [false, true] {
                assert_eq!(super::family_init("{init}", keyword, dark), "{init}");
                assert_eq!(
                    super::family_svg_fix("<svg/>".to_string(), keyword, dark),
                    "<svg/>"
                );
            }
        }
    }

    #[test]
    fn the_dark_palette_differs_from_light() {
        let options = options();
        let light =
            render_mermaid("flowchart LR\n A-->B", 1.0, false, 24_000_000, &options).unwrap();
        let dark = render_mermaid("flowchart LR\n A-->B", 1.0, true, 24_000_000, &options).unwrap();
        assert_eq!((light.width, light.height), (dark.width, dark.height));
        assert_ne!(
            light.rgba, dark.rgba,
            "the dark theme should change the palette"
        );
    }

    #[test]
    fn a_scaled_diagram_is_larger_but_under_the_cap() {
        let options = options();
        let one = render_mermaid("flowchart LR\n A-->B", 1.0, false, 24_000_000, &options).unwrap();
        let two = render_mermaid("flowchart LR\n A-->B", 2.0, false, 24_000_000, &options).unwrap();
        assert!(two.width > one.width && two.height > one.height);
        let capped =
            render_mermaid("flowchart LR\n A-->B", 8.0, false, 1_000_000, &options).unwrap();
        assert!(capped.width as u64 * capped.height as u64 <= 1_000_000);
    }

    #[test]
    fn text_that_is_not_a_diagram_is_an_error() {
        let options = options();
        assert!(render_mermaid("hello, not a diagram", 1.0, false, 24_000_000, &options).is_err());
    }

    #[test]
    fn a_formula_renders_with_a_baseline() {
        let options = options();
        let image = render_math(
            "x^2 + y^2 = z^2",
            false,
            32.0,
            [20, 20, 20],
            24_000_000,
            &options,
        )
        .expect("the formula renders");
        assert!(
            image.width > 20 && image.height > 10,
            "{image:?}",
            image = (image.width, image.height)
        );
        assert!(image.baseline > 0 && image.baseline < image.height);
        let inked = image.rgba.chunks_exact(4).filter(|p| p[3] > 0).count();
        assert!(inked > 200, "inked pixels: {inked}");
        assert_eq!(
            image.rgba.len(),
            image.width as usize * image.height as usize * 4
        );
    }

    #[test]
    fn math_takes_the_requested_colour() {
        let options = options();
        let image = render_math(
            "\\frac{a}{b}",
            true,
            40.0,
            [255, 0, 0],
            24_000_000,
            &options,
        )
        .expect("the fraction renders");
        let red = image
            .rgba
            .chunks_exact(4)
            .filter(|p| p[0] > 180 && p[1] < 80 && p[2] < 80 && p[3] > 0)
            .count();
        assert!(red > 100, "red pixels: {red}");
    }

    #[test]
    fn malformed_tex_is_an_error_not_a_crash() {
        let options = options();
        assert!(render_math("\\frac{1", false, 32.0, [0, 0, 0], 24_000_000, &options).is_err());
        assert!(render_math("", false, 32.0, [0, 0, 0], 24_000_000, &options).is_err());
    }

    #[test]
    fn malformed_input_is_an_error_not_a_crash() {
        assert!(render_svg(b"<svg", 24_000_000, &resvg::usvg::Options::default()).is_err());
        assert!(
            render_svg(
                b"not an svg at all",
                24_000_000,
                &resvg::usvg::Options::default()
            )
            .is_err()
        );
    }

    #[test]
    fn system_fonts_render_svg_text() {
        // The badge's label is white on a blue bar. With the engine's font database it is drawn; a
        // missing family falls back through the same sans stack the renderer uses.
        let image =
            render_svg(BADGE.as_bytes(), 24_000_000, &options()).expect("the badge renders");
        let white = image
            .rgba
            .chunks_exact(4)
            .filter(|p| p[0] > 180 && p[1] > 180 && p[2] > 180 && p[3] > 0)
            .count();
        assert!(white > 100, "white text pixels: {white}");
    }
}
