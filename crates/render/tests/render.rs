//! Pixel-level smoke tests for the headless renderer: a crash, a blank page, a lost theme, a lost
//! clamp, and the subpixel channel order. `golden.rs` compares whole pages against `tests/golden/`.

use mdview_render::{DARK, DocumentRenderer, LIGHT, Options, render, render_slice, render_view};
use std::path::PathBuf;

fn fixture(name: &str) -> mdview_core::Document {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    mdview_core::parse_bytes(&std::fs::read(path).unwrap())
}

fn background(r: &mdview_render::Rendered, _theme: mdview_render::Theme) -> (u8, u8, u8) {
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
fn a_table_cell_draws_its_inline_image() {
    // `cell_inline` rebuilds a cell from its leaves; it used to drop their image boxes, which
    // blanked math and images inside table cells.
    let dir = temp_dir("table-image");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/assets/badge.png"),
        dir.join("badge.png"),
    )
    .unwrap();
    let doc = mdview_core::parse("| Header |\n|---|\n| ![badge](badge.png) |\n");
    let rendered = render(
        &doc,
        &Options {
            width: 780.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: None,
            base_dir: Some(dir.clone()),
        },
    );
    let blue = rendered
        .bitmap
        .pixels
        .chunks_exact(4)
        // The frame is BGRA.
        .filter(|p| p[0] > 140 && p[2] < 40 && p[1] > 80)
        .count();
    assert!(blue > 1000, "badge pixels in the table: {blue}");
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
fn an_html_image_honours_its_width_and_height() {
    let dir = temp_dir("image-attrs");
    write_red_png(&dir.join("red.png"));
    let red_pixels = |markdown: &str| {
        let doc = mdview_core::parse(markdown);
        let options = Options {
            width: 400.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: None,
            base_dir: Some(dir.clone()),
        };
        let mut renderer = DocumentRenderer::new();
        let page = renderer.layout(&doc, &options);
        let view = renderer.view(&doc, &options, &page, 0.0, 400.0);
        view.bitmap
            .pixels
            .chunks_exact(4)
            .filter(|p| p[2] > 180 && p[0] < 80 && p[1] < 80)
            .count() as f32
    };
    let natural = red_pixels("<img src=\"red.png\">\n");
    let half = red_pixels("<img src=\"red.png\" width=\"32\">\n");
    let wide = red_pixels("<img src=\"red.png\" width=\"128\" height=\"32\">\n");
    let huge = red_pixels("<img src=\"red.png\" width=\"5000\">\n");
    assert!(natural > 3500.0, "natural {natural}");
    assert!((half / natural - 0.25).abs() < 0.05, "half {half} vs {natural}");
    assert!((wide / natural - 1.0).abs() < 0.1, "128x32 is as big as 64x64: {wide}");
    // Clamped to the content width (about 400 minus margins), aspect kept, so far under 5000^2.
    assert!(huge > natural * 10.0 && huge < 400.0 * 400.0, "huge {huge}");
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
fn a_click_maps_back_to_the_word_under_it() {
    let doc = mdview_core::parse("# Title\n\nHello **world** after.\n");
    let options = Options {
        width: 500.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    };
    let mut renderer = DocumentRenderer::new();
    let page = renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, &page, 0.0, 400.0);
    let world = doc.text.find("world").expect("the word is in the document") as u32;
    let rects = renderer.selection_rects(&doc, &options, &page, world, world + 5);
    assert_eq!(rects.len(), 1, "one line, one rect");
    let rect = rects[0];
    assert!(rect.w > 10.0, "{rect:?}");
    let hit = renderer
        .hit_test(&doc, &options, &page, rect.x + 1.0, rect.y + rect.h / 2.0)
        .expect("a click in the text");
    assert_eq!(hit.offset, world, "the click is on the first letter");
    assert_eq!(
        &doc.text[hit.offset as usize..hit.offset as usize + 5],
        "world"
    );
    assert_eq!(hit.link, None);
    // A click past the end of the line still lands on it, at the last caret stop.
    let hit = renderer
        .hit_test(&doc, &options, &page, rect.x + 400.0, rect.y + rect.h / 2.0)
        .expect("a click to the right");
    assert_eq!(hit.block, page.blocks()[1].block);
    let after = doc.text.find("after").unwrap() as u32;
    assert!(hit.offset >= after, "offset {}", hit.offset);
}

#[test]
fn a_click_finds_a_link_and_a_footnote_reference() {
    let doc = mdview_core::parse(
        "A [link](https://example.com) and a reference[^n].\n\n[^n]: The note.\n",
    );
    let options = Options {
        width: 640.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    };
    let mut renderer = DocumentRenderer::new();
    let page = renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, &page, 0.0, 400.0);
    let link = doc.text.find("link").expect("link text") as u32;
    let rect = renderer.selection_rects(&doc, &options, &page, link, link + 4)[0];
    let hit = renderer
        .hit_test(
            &doc,
            &options,
            &page,
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0,
        )
        .expect("a click on the link");
    let index = hit.link.expect("a link under the pointer");
    assert_eq!(
        doc.str(doc.links[index as usize].dest),
        "https://example.com"
    );
    // The footnote mark has no text of its own in the document, so sweep the line for it.
    let text = &doc.blocks[page.blocks()[0].block as usize].text;
    let row = renderer.selection_rects(&doc, &options, &page, text.start, text.end)[0];
    let mut found = None;
    let mut x = row.x;
    while x < row.x + row.w {
        if let Some(hit) = renderer.hit_test(&doc, &options, &page, x, row.y + row.h / 2.0)
            && let Some(note) = hit.footnote
        {
            found = Some((note, hit.offset));
            break;
        }
        x += 1.0;
    }
    let (note, offset) = found.expect("the footnote mark is hittable");
    assert_eq!(doc.footnotes[note as usize].number, 1);
    assert_eq!(
        offset,
        doc.spans
            .iter()
            .find(|s| matches!(s.kind, mdview_core::model::SpanKind::FootnoteRef(_)))
            .unwrap()
            .start
    );
    assert!(
        page.footnote_y(1).is_some(),
        "the footnote list has a position"
    );
}

#[test]
fn blocks_are_found_by_page_row() {
    let doc = mdview_core::parse("first paragraph\n\nsecond paragraph\n");
    let page = mdview_render::layout(&doc, &Options::light());
    let blocks = page.blocks();
    assert_eq!(blocks.len(), 2);
    assert_eq!(
        page.block_at(blocks[0].y + 1.0).unwrap().block,
        blocks[0].block
    );
    assert_eq!(
        page.block_at(blocks[1].y + 1.0).unwrap().block,
        blocks[1].block
    );
    // A row above the content and one in the bottom padding both clamp to a block.
    assert_eq!(page.block_at(0.0).unwrap().block, blocks[0].block);
    assert_eq!(
        page.block_at(page.total_height() - 1.0).unwrap().block,
        blocks[1].block
    );
    // A nested block reports the row of the top-level block that holds it.
    let list = mdview_core::parse("- item\n");
    let list_page = mdview_render::layout(&list, &Options::light());
    let top = list_page.blocks()[0].block;
    let child = list.children(top).next().expect("a list item");
    assert_eq!(list_page.y_of(&list, child), Some(list_page.blocks()[0].y));
    assert_eq!(list_page.y_of(&list, 0), Some(list_page.blocks()[0].y));
}

#[test]
fn hit_testing_and_selection_do_not_change_the_frame() {
    let doc = fixture("showcase.md");
    let options = Options {
        width: 520.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    };
    let mut renderer = DocumentRenderer::new();
    let page = renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, &page, 0.0, 400.0);
    // Ask for the same view once more so the delta is explicitly empty, then use hit testing.
    let before = renderer
        .view_frame(&doc, &options, &page, 0.0, 400.0)
        .bitmap
        .pixels
        .clone();
    let text = &doc.blocks[page.blocks()[0].block as usize].text;
    let _ = renderer.selection_rects(&doc, &options, &page, text.start, text.end);
    let heading = doc.headings.last().expect("a heading").block;
    let last = *page
        .blocks()
        .iter()
        .find(|block| block.block == heading)
        .expect("the heading is a top-level block");
    let hit = renderer
        .hit_test(&doc, &options, &page, 30.0, last.y + 5.0)
        .expect("a click on a block far below the viewport");
    assert_eq!(hit.block, last.block, "the block was laid out on demand");
    let frame = renderer.view_frame(&doc, &options, &page, 0.0, 400.0);
    assert_eq!(
        frame.band,
        Some((0, 0)),
        "the frame delta is about the document only"
    );
    assert_eq!(frame.bitmap.pixels, before, "the frame is byte-identical");
}

#[test]
fn a_selection_that_wraps_gives_one_rect_per_line() {
    let doc = mdview_core::parse(
        "This paragraph is long enough that a narrow page breaks it over several lines, which is \
         what the selection needs in order to produce more than one rectangle on the page.\n",
    );
    let options = Options {
        width: 300.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    };
    let mut renderer = DocumentRenderer::new();
    let page = renderer.layout(&doc, &options);
    let text = &doc.blocks[page.blocks()[0].block as usize].text;
    let rects = renderer.selection_rects(&doc, &options, &page, text.start, text.end);
    assert!(rects.len() > 1, "{rects:?}");
    // Every rect is on its own row band and inside the page.
    for rect in &rects {
        assert!(rect.x >= 0.0 && rect.w > 0.0 && rect.h > 0.0);
        assert!(rect.y >= 0.0 && rect.y + rect.h <= page.total_height());
    }
}

#[test]
fn a_details_summary_toggles_and_the_page_remeasures() {
    let doc = mdview_core::parse(
        "# Title\n\n<details>\n<summary>More</summary>\n\nHidden body text.\n\n</details>\n",
    );
    let details = doc
        .blocks
        .iter()
        .position(|block| matches!(block.kind, mdview_core::model::BlockKind::Details { .. }))
        .expect("a details block") as u32;
    let summary = doc
        .children(details)
        .find(|child| {
            matches!(
                doc.blocks[*child as usize].kind,
                mdview_core::model::BlockKind::Summary
            )
        })
        .expect("a summary");
    let options = Options {
        width: 500.0,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    };
    let mut renderer = DocumentRenderer::new();
    let closed = renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, &closed, 0.0, 400.0);
    let text = doc.blocks[summary as usize].text;
    let rect = renderer.selection_rects(&doc, &options, &closed, text.start, text.end)[0];
    let hit = renderer
        .hit_test(&doc, &options, &closed, rect.x + 2.0, rect.y + rect.h / 2.0)
        .expect("a click on the summary");
    assert_eq!(hit.details, Some(details), "the summary names its details");
    assert!(!renderer.details_open(&doc, details), "closed as written");
    renderer.set_details(details, true);
    assert!(renderer.details_open(&doc, details));
    let open = renderer.layout(&doc, &options);
    assert!(
        open.total_height() > closed.total_height() + 20.0,
        "opening shows the body: {} vs {}",
        open.total_height(),
        closed.total_height()
    );
    renderer.set_details(details, false);
    let again = renderer.layout(&doc, &options);
    assert!(
        (again.total_height() - closed.total_height()).abs() < 0.5,
        "closing restores the height: {} vs {}",
        again.total_height(),
        closed.total_height()
    );
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

#[test]
fn subpixel_text_uses_rgb_stripe_order() {
    // Phase 0 found zeno's `Format::Subpixel` swapping red and blue, which put coloured fringes on
    // the wrong side of every stem. On an RGB-stripe panel red is the left third of a pixel and
    // blue the right, so dark ink starting partway into a pixel covers blue more than red: the left
    // edge of a dark stem on a light page is red-tinted (R > B) and its right edge blue-tinted. A
    // swap in the mask format or in the blit flips both.
    let doc = mdview_core::parse("# llllllll IIIIIIII\n");
    let rendered = render(
        &doc,
        &Options {
            width: 780.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: None,
            base_dir: None,
        },
    );
    let bitmap = &rendered.bitmap;
    let bg = &bitmap.pixels[0..3];
    let (mut left, mut right) = ((0, 0), (0, 0)); // (red-tinted, blue-tinted)
    for y in 0..bitmap.height {
        let row = &bitmap.pixels[(y * bitmap.width * 4) as usize..][..(bitmap.width * 4) as usize];
        let inked = |x: u32| row[(x * 4) as usize..][..3] != *bg;
        for x in 1..bitmap.width - 1 {
            let p = &row[(x * 4) as usize..][..3];
            let (b, r) = (p[0] as i32, p[2] as i32);
            if (b - r).abs() < 8 {
                continue;
            }
            let tally = if inked(x) && !inked(x - 1) {
                &mut left
            } else if inked(x) && !inked(x + 1) {
                &mut right
            } else {
                continue;
            };
            if r > b { tally.0 += 1 } else { tally.1 += 1 }
        }
    }
    assert!(
        left.0 + left.1 > 20 && right.0 + right.1 > 20,
        "too few edges: {left:?} {right:?}"
    );
    assert!(
        left.0 > left.1 * 3,
        "left edges should be red-tinted: {left:?}"
    );
    assert!(
        right.1 > right.0 * 3,
        "right edges should be blue-tinted: {right:?}"
    );
}
