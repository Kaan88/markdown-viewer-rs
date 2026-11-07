//! The engine's SVG rasterizer. The viewer talks to this through `mdview-engine-proto`, never by
//! linking it, so a malformed document cannot take the viewer down and the memory goes back when
//! the process exits. resvg is the only dependency of this crate; math, Mermaid and highlighting
//! join it in later Phase 4 slices.

/// Straight-alpha RGBA, row-major, the shape the renderer's image cache takes.
pub struct Image {
    pub width: u32,
    pub height: u32,
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
        rgba,
    })
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
