//! The engine's rasterizers. The viewer talks to this through `mdview-engine-proto`, never by
//! linking it, so a malformed document cannot take the viewer down and the memory goes back when
//! the process exits. resvg draws SVG; RaTeX lays out TeX and hands the display list to the same
//! resvg path. Mermaid and highlighting join them in later Phase 4 slices.

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
    let tree =
        resvg::usvg::Tree::from_data(bytes, options).map_err(|e| format!("invalid SVG: {e}"))?;
    let size = tree.size();
    let (width, height) = (size.width(), size.height());
    if !(width.is_finite() && height.is_finite()) || width <= 0.0 || height <= 0.0 {
        return Err("SVG has no size".to_string());
    }
    let natural = width as f64 * height as f64;
    let scale = if natural > max_pixels as f64 {
        (max_pixels as f64 / natural).sqrt()
    } else {
        1.0
    };
    let raster_width = (width as f64 * scale).round().max(1.0) as u32;
    let raster_height = (height as f64 * scale).round().max(1.0) as u32;
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
