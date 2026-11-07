//! Pixel-level smoke tests for the headless renderer. The golden images are approved by eye; these
//! catch a crash, a blank page, a lost theme and a lost clamp.

use mdview_render::{DARK, DocumentRenderer, LIGHT, Options, render, render_slice, render_view};
use std::path::PathBuf;

fn fixture(name: &str) -> mdview_core::Document {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    mdview_core::parse_bytes(&std::fs::read(path).unwrap())
}

fn background(r: &mdview_render::Rendered, theme: mdview_render::Theme) -> (u8, u8, u8) {
    let p = &r.bitmap.pixels[0..4];
    (p[0], p[1], p[2])
}

#[test]
fn renders_the_showcase_at_both_themes_and_scales() {
    let doc = fixture("showcase.md");
    for (scale, theme) in [(1.0, LIGHT), (1.5, DARK)] {
        let rendered = render(
            &doc,
            &Options {
                width: 780.0,
                scale,
                theme,
                max_height: None,
                base_dir: None,
            },
        );
        assert!(rendered.bitmap.width >= 780, "{scale}");
        assert!(rendered.bitmap.height > 1000, "{scale}");
        assert!(rendered.content_height > 1000.0, "{scale}");
        assert!(!rendered.blocks.is_empty(), "{scale}");
        // The figure is the theme's background, not black or transparent.
        assert_eq!(
            background(&rendered, theme),
            (theme.bg[2], theme.bg[1], theme.bg[0]),
            "{scale}"
        );
    }
}

#[test]
fn a_checked_task_box_draws_in_the_accent_colour() {
    let doc = mdview_core::parse("- [x] done\n- [ ] open\n");
    let rendered = render(&doc, &Options::light());
    let accent = LIGHT.accent;
    let drawn = rendered
        .bitmap
        .pixels
        .chunks_exact(4)
        .any(|p| p[0] == accent[2] && p[1] == accent[1] && p[2] == accent[0]);
    assert!(drawn, "the checked box is filled with the accent colour");
}

#[test]
fn max_height_clips_the_bitmap() {
    let mut src = String::new();
    for i in 0..200 {
        src.push_str(&format!("paragraph {i}\n\n"));
    }
    let doc = mdview_core::parse(&src);
    let full = render(&doc, &Options::light());
    assert!(full.content_height > 500.0, "the page really was longer");
    let rendered = render(
        &doc,
        &Options {
            width: 780.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: Some(200.0),
            base_dir: None,
        },
    );
    assert!(
        rendered.bitmap.height <= 230,
        "{} px",
        rendered.bitmap.height
    );
    assert!(rendered.content_height <= 230.0);
}

#[test]
fn an_empty_document_is_just_background() {
    let doc = mdview_core::parse("");
    let rendered = render(&doc, &Options::default());
    assert!(rendered.blocks.is_empty());
    assert!(rendered.bitmap.height > 0);
    assert!(
        rendered
            .bitmap
            .pixels
            .chunks_exact(4)
            .all(|p| p[0] == LIGHT.bg[2] && p[1] == LIGHT.bg[1] && p[2] == LIGHT.bg[0])
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tmp")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_red_png(path: &std::path::Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 64, 64);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let mut data = Vec::with_capacity(64 * 64 * 4);
    for _ in 0..64 * 64 {
        data.extend_from_slice(&[220, 30, 30, 255]);
    }
    writer.write_image_data(&data).unwrap();
}

#[test]
fn a_png_on_disk_is_drawn_inline() {
    let dir = temp_dir("image-drawn");
    let path = dir.join("red.png");
    write_red_png(&path);
    let doc = mdview_core::parse("![red](red.png)\n");
    let rendered = render(
        &doc,
        &Options {
            width: 400.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: None,
            base_dir: Some(dir),
        },
    );
    let red = rendered
        .bitmap
        .pixels
        .chunks_exact(4)
        .filter(|p| p[2] > 180 && p[0] < 80 && p[1] < 80)
        .count();
    assert!(red > 1000, "red pixels: {red}");
    assert!(
        rendered.content_height > 60.0,
        "the image should take vertical space, content height {}",
        rendered.content_height
    );
}

#[test]
fn a_cached_frame_still_draws_its_images() {
    let dir = temp_dir("image-cached");
    let path = dir.join("red.png");
    write_red_png(&path);
    let doc = mdview_core::parse("![red](red.png)\n");
    let options = Options {
        width: 400.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: Some(dir),
    };
    let mut renderer = DocumentRenderer::new();
    let page = renderer.layout(&doc, &options);
    let first = renderer.view(&doc, &options, &page, 0.0, 300.0);
    // The second frame reuses the cached block paints, which must still find the decoded image.
    let second = renderer.view(&doc, &options, &page, 0.0, 300.0);
    let red = |pixels: &[u8]| {
        pixels
            .chunks_exact(4)
            .filter(|p| p[2] > 180 && p[0] < 80 && p[1] < 80)
            .count()
    };
    assert!(red(&first.bitmap.pixels) > 1000, "first frame has no image");
    assert!(
        red(&second.bitmap.pixels) > 1000,
        "cached frame lost the image"
    );
    assert_eq!(second.bitmap.pixels, first.bitmap.pixels);
}

#[test]
fn a_shifted_frame_matches_a_fresh_view() {
    let dir = temp_dir("frame-shift");
    let path = dir.join("red.png");
    write_red_png(&path);
    let doc = mdview_core::parse(
        "# Title\n\n![red](red.png)\n\nText under the image.\n\n## Next\n\nMore text here.\n",
    );
    let options = Options {
        width: 420.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: Some(dir),
    };
    let mut renderer = DocumentRenderer::new();
    let page = renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, &page, 0.0, 200.0);
    // Small steps reuse the shifted frame; the long jumps force a full repaint.
    for top in [37.0, 5.0, 300.0, 120.0, 0.0] {
        renderer.view_frame(&doc, &options, &page, top, 200.0);
        let frame = renderer.frame().expect("frame");
        let fresh = render_view(&doc, &options, &page, frame.top, 200.0);
        assert_eq!(frame.bitmap.pixels, fresh.bitmap.pixels, "top {top}");
    }
}

#[test]
fn a_scrolling_frame_reports_the_band_and_shift() {
    let doc = fixture("showcase.md");
    let options = Options {
        width: 520.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    };
    let rows = 200.0;
    let mut renderer = DocumentRenderer::new();
    let page = renderer.layout(&doc, &options);
    // The first frame is whole: a caller with no previous frame has nothing to reuse.
    let frame = renderer.view_frame(&doc, &options, &page, 0.0, rows);
    assert_eq!(frame.band, None);
    assert_eq!(frame.shift, 0);
    let mut last = frame.bitmap.pixels.clone();
    let stride = frame.bitmap.width as usize * 4;
    let count = frame.bitmap.height as usize;
    for top in [40.0, 75.0, 30.0, 0.0, 120.0] {
        let frame = renderer.view_frame(&doc, &options, &page, top, rows);
        let (shift, band) = (
            frame.shift,
            frame.band.expect("a small step keeps the frame"),
        );
        let (first, height) = (band.0 as usize, band.1 as usize);
        assert_ne!(shift, 0, "top {top} moved without saying so");
        assert!(
            first + height <= count,
            "top {top}: band {band:?} off the frame"
        );
        // What a window does with the band: move its copy of the previous frame, paste the band.
        let mut rebuilt = last.clone();
        let moved = shift.unsigned_abs() as usize;
        if shift > 0 {
            rebuilt.copy_within(moved * stride.., 0);
        } else {
            rebuilt.copy_within(0..(count - moved) * stride, moved * stride);
        }
        let start = first * stride;
        let end = start + height * stride;
        rebuilt[start..end].copy_from_slice(&frame.bitmap.pixels[start..end]);
        assert_eq!(rebuilt, frame.bitmap.pixels, "top {top}");
        last = frame.bitmap.pixels.clone();
    }
    // A jump of a whole viewport cannot be shifted, so it is reported as a whole frame.
    let frame = renderer.view_frame(&doc, &options, &page, 900.0, rows);
    assert_eq!(frame.band, None);
    assert_eq!(frame.shift, 0);
    // The same view twice changed nothing, which an empty band says.
    let top = frame.top;
    let frame = renderer.view_frame(&doc, &options, &page, top, rows);
    assert_eq!(frame.band, Some((0, 0)));
    assert_eq!(frame.shift, 0);
}

#[test]
fn a_slice_matches_the_same_rows_of_the_full_page() {
    let doc = fixture("showcase.md");
    let options = Options {
        width: 780.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    };
    let full = render(&doc, &options);
    assert!(full.bitmap.height > 700, "{}", full.bitmap.height);
    let slice = render_slice(&doc, &options, 200.0, 300.0);
    assert_eq!(slice.bitmap.width, full.bitmap.width);
    assert_eq!(slice.bitmap.height, 300);
    assert_eq!(slice.top, 200.0);
    assert!(
        (slice.total_height - full.content_height).abs() < 1.0,
        "{} vs {}",
        slice.total_height,
        full.content_height
    );
    let width = full.bitmap.width as usize;
    for row in 0..300usize {
        let full_start = (200 + row) * width * 4;
        let slice_start = row * width * 4;
        assert_eq!(
            &full.bitmap.pixels[full_start..full_start + width * 4],
            &slice.bitmap.pixels[slice_start..slice_start + width * 4],
            "row {row}"
        );
    }
}

#[test]
fn cached_scroll_frames_match_a_fresh_slice() {
    let doc = fixture("showcase.md");
    let options = Options {
        width: 780.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    };
    let mut renderer = DocumentRenderer::new();
    let page = renderer.layout(&doc, &options);
    renderer.view(&doc, &options, &page, 100.0, 300.0);
    let second = renderer.view(&doc, &options, &page, 400.0, 300.0);
    let fresh = render_slice(&doc, &options, 400.0, 300.0);
    assert_eq!(second.bitmap.pixels, fresh.bitmap.pixels);
}

#[test]
fn a_missing_image_falls_back_to_the_placeholder() {
    let dir = temp_dir("image-missing");
    let doc = mdview_core::parse("![alt text](nope.png)\n");
    let rendered = render(
        &doc,
        &Options {
            width: 400.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: None,
            base_dir: Some(dir),
        },
    );
    assert!(rendered.content_height > 0.0);
    // The placeholder is muted text, so the page is not simply empty.
    let ink = rendered
        .bitmap
        .pixels
        .chunks_exact(4)
        .filter(|p| p[0] != LIGHT.bg[2] || p[1] != LIGHT.bg[1] || p[2] != LIGHT.bg[0])
        .count();
    assert!(ink > 100, "placeholder pixels: {ink}");
}
