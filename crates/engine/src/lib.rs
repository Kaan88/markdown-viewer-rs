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
    use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};

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
    let source = format!("%%{{init: {init}}}%%\n{source}");
    let output = Renderer::new()
        .render(RenderRequest::svg(
            &source,
            OperationControl::new(),
            SvgRequest {
                // Families that still emit HTML labels get a `<text>` overlay from this pipeline.
                // `inline_label_uses` below then moves that text out of the nested `<defs>` SVG,
                // which resvg does not draw.
                pipeline: Some(merman::svg::SvgPipeline::readable()),
                ..Default::default()
            },
        ))
        .map_err(|e| format!("mermaid: {e}"))?;
    let RenderOutput::Svg(Some(svg)) = output else {
        return Err("mermaid found no diagram".to_string());
    };
    let svg = inline_label_uses(svg.svg());
    render_svg_at(svg.as_bytes(), scale, max_pixels, options)
}

/// `name="number"` in an SVG tag fragment.
fn attr_f32(fragment: &str, name: &str) -> Option<f32> {
    let at = fragment.find(&format!("{name}=\""))? + name.len() + 2;
    let rest = &fragment[at..];
    let end = rest.find('"')?;
    rest[..end].parse().ok()
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
        let Some(text_end) = after[text_start..].find("</text>") else {
            break;
        };
        let end = text_start + text_end + "</text>".len();
        // The fallback text keeps Mermaid's default 16 px, but the nodes were measured at our
        // 11 px init size; at 16 the label overflows the circle and boxes.
        let mut text = after[text_start..end].replace("font-size: 16px", "font-size: 11px");
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
                out.push_str(text);
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
