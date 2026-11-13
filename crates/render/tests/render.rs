//! Pixel-level smoke tests for the headless renderer: a crash, a blank page, a lost theme, a lost
//! clamp, and the subpixel channel order. `golden.rs` compares whole pages against `tests/golden/`.

use mdview_render::{DARK, DocumentRenderer, LIGHT, Options, render, render_slice};
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
        .as_chunks::<4>()
        .0
        .iter()
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
        .as_chunks::<4>()
        .0
        .iter()
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
            .as_chunks::<4>()
            .0
            .iter()
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
        renderer.layout(&doc, &options);
        let view = renderer.view(&doc, &options, 0.0, 400.0);
        view.bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[2] > 180 && p[0] < 80 && p[1] < 80)
            .count() as f32
    };
    let natural = red_pixels("<img src=\"red.png\">\n");
    let half = red_pixels("<img src=\"red.png\" width=\"32\">\n");
    let wide = red_pixels("<img src=\"red.png\" width=\"128\" height=\"32\">\n");
    let huge = red_pixels("<img src=\"red.png\" width=\"5000\">\n");
    assert!(natural > 3500.0, "natural {natural}");
    assert!(
        (half / natural - 0.25).abs() < 0.05,
        "half {half} vs {natural}"
    );
    assert!(
        (wide / natural - 1.0).abs() < 0.1,
        "128x32 is as big as 64x64: {wide}"
    );
    // Clamped to the content width (about 400 minus margins), aspect kept, so far under 5000^2.
    assert!(huge > natural * 10.0 && huge < 400.0 * 400.0, "huge {huge}");
}

#[test]
fn a_picture_uses_the_source_for_the_colour_scheme() {
    let dir = temp_dir("picture");
    write_red_png(&dir.join("red.png"));
    let md = "<picture>\n<source media=\"(prefers-color-scheme: dark)\" srcset=\"red.png\">\n<img src=\"missing.png\" alt=\"x\">\n</picture>\n";
    let doc = mdview_core::parse(md);
    let red = |theme| {
        let options = Options {
            width: 400.0,
            scale: 1.0,
            theme,
            max_height: None,
            base_dir: Some(dir.clone()),
        };
        let mut renderer = DocumentRenderer::new();
        renderer.layout(&doc, &options);
        let view = renderer.view(&doc, &options, 0.0, 300.0);
        view.bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[2] > 180 && p[0] < 80 && p[1] < 80)
            .count()
    };
    assert!(
        red(mdview_render::DARK) > 1000,
        "dark theme should draw the dark source"
    );
    assert_eq!(
        red(LIGHT),
        0,
        "light theme falls back to the missing default"
    );
}

#[test]
fn table_spans_lay_out_and_hostile_spans_are_clamped() {
    let height = |markdown: &str| {
        let doc = mdview_core::parse(markdown);
        let options = Options {
            width: 500.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: None,
            base_dir: None,
        };
        let mut renderer = DocumentRenderer::new();
        renderer.layout(&doc, &options);
        let slice = renderer.view(&doc, &options, 0.0, 600.0);
        slice.total_height
    };
    let plain = height("<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>");
    let colspan = height(
        "<table><tr><td colspan=\"2\">a wide cell</td></tr><tr><td>c</td><td>d</td></tr></table>",
    );
    assert_eq!(plain, colspan, "a colspan row is as tall as a plain one");
    // A tall cell spanning two short rows makes them taller than the unspanned table.
    let tall = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen";
    let narrow = height(&format!(
        "<table><tr><td rowspan=\"2\" width=\"10%\">{tall}</td><td>b</td></tr><tr><td>d</td></tr></table>"
    ));
    assert!(
        narrow > plain,
        "rowspan grows the rows it covers: {narrow} vs {plain}"
    );
    // Hostile values neither hang nor panic.
    let hostile = height(
        "<table><tr><td colspan=\"1000\" rowspan=\"65534\">x</td><td>y</td></tr><tr><td>z</td></tr></table>",
    );
    assert!(hostile > 0.0);
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
        .as_chunks::<4>()
        .0
        .iter()
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
    renderer.layout(&doc, &options);
    let first = renderer.view(&doc, &options, 0.0, 300.0);
    // The second frame reuses the cached block paints, which must still find the decoded image.
    let second = renderer.view(&doc, &options, 0.0, 300.0);
    let red = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
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
    renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 200.0);
    // Small steps reuse the shifted frame; the long jumps force a full repaint.
    for top in [37.0, 5.0, 300.0, 120.0, 0.0] {
        renderer.view_frame(&doc, &options, top, 200.0);
        let frame = renderer.frame().expect("frame");
        let fresh = render_slice(&doc, &options, frame.top, 200.0);
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
    renderer.layout(&doc, &options);
    // The first frame is whole: a caller with no previous frame has nothing to reuse.
    let frame = renderer.view_frame(&doc, &options, 0.0, rows);
    assert_eq!(frame.band, None);
    assert_eq!(frame.shift, 0);
    let mut last = frame.bitmap.pixels.clone();
    let stride = frame.bitmap.width as usize * 4;
    let count = frame.bitmap.height as usize;
    for top in [40.0, 75.0, 30.0, 0.0, 120.0] {
        let frame = renderer.view_frame(&doc, &options, top, rows);
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
    let frame = renderer.view_frame(&doc, &options, 900.0, rows);
    assert_eq!(frame.band, None);
    assert_eq!(frame.shift, 0);
    // The same view twice changed nothing, which an empty band says.
    let top = frame.top;
    let frame = renderer.view_frame(&doc, &options, top, rows);
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
    renderer.layout(&doc, &options);
    renderer.view(&doc, &options, 100.0, 300.0);
    let second = renderer.view(&doc, &options, 400.0, 300.0);
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
    renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 400.0);
    let world = doc.text.find("world").expect("the word is in the document") as u32;
    let rects = renderer.selection_rects(&doc, &options, world, world + 5);
    assert_eq!(rects.len(), 1, "one line, one rect");
    let rect = rects[0];
    assert!(rect.w > 10.0, "{rect:?}");
    let hit = renderer
        .hit_test(&doc, &options, rect.x + 1.0, rect.y + rect.h / 2.0)
        .expect("a click in the text");
    assert_eq!(hit.offset, world, "the click is on the first letter");
    assert_eq!(
        &doc.text[hit.offset as usize..hit.offset as usize + 5],
        "world"
    );
    assert_eq!(hit.link, None);
    // A click past the end of the line still lands on it, at the last caret stop.
    let hit = renderer
        .hit_test(&doc, &options, rect.x + 400.0, rect.y + rect.h / 2.0)
        .expect("a click to the right");
    assert_eq!(hit.block, renderer.page_layout().unwrap().blocks()[1].block);
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
    renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 400.0);
    let link = doc.text.find("link").expect("link text") as u32;
    let rect = renderer.selection_rects(&doc, &options, link, link + 4)[0];
    let hit = renderer
        .hit_test(&doc, &options, rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
        .expect("a click on the link");
    let index = hit.link.expect("a link under the pointer");
    assert_eq!(
        doc.str(doc.links[index as usize].dest),
        "https://example.com"
    );
    // The footnote mark has no text of its own in the document, so sweep the line for it.
    let text = &doc.blocks[renderer.page_layout().unwrap().blocks()[0].block as usize].text;
    let row = renderer.selection_rects(&doc, &options, text.start, text.end)[0];
    let mut found = None;
    let mut x = row.x;
    while x < row.x + row.w {
        if let Some(hit) = renderer.hit_test(&doc, &options, x, row.y + row.h / 2.0)
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
        renderer.page_layout().unwrap().footnote_y(1).is_some(),
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
    renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 400.0);
    // Ask for the same view once more so the delta is explicitly empty, then use hit testing.
    let before = renderer
        .view_frame(&doc, &options, 0.0, 400.0)
        .bitmap
        .pixels
        .clone();
    let text = &doc.blocks[renderer.page_layout().unwrap().blocks()[0].block as usize].text;
    let _ = renderer.selection_rects(&doc, &options, text.start, text.end);
    let heading = doc.headings.last().expect("a heading").block;
    let last = *renderer
        .page_layout()
        .unwrap()
        .blocks()
        .iter()
        .find(|block| block.block == heading)
        .expect("the heading is a top-level block");
    let hit = renderer
        .hit_test(&doc, &options, 30.0, last.y + 5.0)
        .expect("a click on a block far below the viewport");
    assert_eq!(hit.block, last.block, "the block was laid out on demand");
    let frame = renderer.view_frame(&doc, &options, 0.0, 400.0);
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
    renderer.layout(&doc, &options);
    let text = &doc.blocks[renderer.page_layout().unwrap().blocks()[0].block as usize].text;
    let rects = renderer.selection_rects(&doc, &options, text.start, text.end);
    assert!(rects.len() > 1, "{rects:?}");
    // Every rect is on its own row band and inside the page.
    for rect in &rects {
        assert!(rect.x >= 0.0 && rect.w > 0.0 && rect.h > 0.0);
        assert!(rect.y >= 0.0 && rect.y + rect.h <= renderer.page_layout().unwrap().total_height());
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
    renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 400.0);
    let closed = renderer.page_layout().unwrap().clone();
    let text = doc.blocks[summary as usize].text;
    let rect = renderer.selection_rects(&doc, &options, text.start, text.end)[0];
    let hit = renderer
        .hit_test(&doc, &options, rect.x + 2.0, rect.y + rect.h / 2.0)
        .expect("a click on the summary");
    assert_eq!(hit.details, Some(details), "the summary names its details");
    assert!(!renderer.details_open(&doc, details), "closed as written");
    renderer.set_details(&doc, details, true);
    assert!(renderer.details_open(&doc, details));
    // The toggled block is measured again by the next frame; nothing else is.
    renderer.view_frame(&doc, &options, 0.0, 400.0);
    let open = renderer.page_layout().unwrap().clone();
    assert!(
        open.total_height() > closed.total_height() + 20.0,
        "opening shows the body: {} vs {}",
        open.total_height(),
        closed.total_height()
    );
    renderer.set_details(&doc, details, false);
    renderer.view_frame(&doc, &options, 0.0, 400.0);
    let again = renderer.page_layout().unwrap().clone();
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
        .as_chunks::<4>()
        .0
        .iter()
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

fn narrow(width: f32) -> Options {
    Options {
        width,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    }
}

/// The page rectangles of one word of the document, found by its text.
fn rects_of(
    renderer: &mut DocumentRenderer,
    doc: &mdview_core::Document,
    options: &Options,
    word: &str,
) -> Vec<mdview_render::Rect> {
    let at = doc.text.find(word).expect("the word is in the document") as u32;
    renderer.selection_rects(doc, options, at, at + word.len() as u32)
}

#[test]
fn a_ruby_annotation_is_drawn_small_above_its_base_and_the_line_grows() {
    let with = mdview_core::parse(
        "Lead line of text that wraps before the word <ruby>base<rt>note</rt></ruby> and more \
         text to fill another full line of the paragraph.",
    );
    let without = mdview_core::parse(
        "Lead line of text that wraps before the word base and more \
         text to fill another full line of the paragraph.",
    );
    let options = narrow(330.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&with, &options);
    // `layout` estimates; the exact pass measures every block.
    let grown = mdview_render::layout(&with, &options).total_height()
        - mdview_render::layout(&without, &options).total_height();
    assert!(grown >= 8.0, "the line makes room for the note: {grown}");

    let base = rects_of(&mut renderer, &with, &options, "base")[0];
    let note = rects_of(&mut renderer, &with, &options, "note")[0];
    // The note is above the base, smaller, and centred on it.
    assert!(
        note.y + note.h <= base.y + base.h / 2.0,
        "{note:?} {base:?}"
    );
    assert!(note.h < base.h, "{note:?} {base:?}");
    let centre = |r: mdview_render::Rect| r.x + r.w / 2.0;
    assert!(
        (centre(note) - centre(base)).abs() < 3.0,
        "{note:?} {base:?}"
    );
    // Nothing on the line above reaches into the band the note sits in.
    let lead = rects_of(&mut renderer, &with, &options, "Lead")[0];
    assert!(lead.y + lead.h <= note.y + 1.0, "{lead:?} {note:?}");
}

#[test]
fn selecting_across_a_ruby_covers_base_annotation_and_the_text_after_it() {
    let doc = mdview_core::parse("before <ruby>漢字<rt>kanji</rt></ruby> after");
    let options = narrow(500.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    let from = doc.text.find("before").unwrap() as u32;
    let to = doc.text.len() as u32;
    let rects = renderer.selection_rects(&doc, &options, from, to);
    assert_eq!(rects.len(), 2, "the line and the annotation: {rects:?}");
    // The copied text is the document's: base, annotation and the rest, in order.
    assert_eq!(
        doc.text[from as usize..].trim_end(),
        "before 漢字kanji after"
    );
    // A selection that starts inside the annotation starts at the text after it.
    let inside = doc.text.find("anji").unwrap() as u32;
    let rects = renderer.selection_rects(&doc, &options, inside, to);
    let after = rects_of(&mut renderer, &doc, &options, "after")[0];
    assert!(
        rects.iter().any(|r| r.x >= after.x - 8.0),
        "{rects:?} {after:?}"
    );
    // A click on the base lands in the base, and one on the annotation resolves too.
    let base = rects_of(&mut renderer, &doc, &options, "漢字")[0];
    let hit = renderer
        .hit_test(&doc, &options, base.x + 2.0, base.y + base.h / 2.0)
        .unwrap();
    assert_eq!(hit.offset, doc.text.find("漢字").unwrap() as u32);
    let note = rects_of(&mut renderer, &doc, &options, "kanji")[0];
    assert!(
        renderer
            .hit_test(&doc, &options, note.x + 2.0, note.y + note.h / 2.0)
            .is_some()
    );
}

#[test]
fn a_description_list_bolds_terms_and_indents_definitions() {
    let list = mdview_core::parse("<dl><dt>Wwwwww</dt><dd>Mmmmmm</dd></dl>");
    let div = mdview_core::parse("<div>Wwwwww</div>");
    let options = narrow(500.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&list, &options);
    let term = rects_of(&mut renderer, &list, &options, "Wwwwww")[0];
    let definition = rects_of(&mut renderer, &list, &options, "Mmmmmm")[0];
    let mut other = DocumentRenderer::new();
    other.layout(&div, &options);
    let regular = rects_of(&mut other, &div, &options, "Wwwwww")[0];
    assert!(
        term.w > regular.w * 1.03,
        "bold is wider: {term:?} {regular:?}"
    );
    assert!(
        (definition.x - term.x - 16.0).abs() < 1.5,
        "the definition is 16 px in: {term:?} {definition:?}"
    );
    assert!(definition.y > term.y + term.h - 1.0);
    // The gap after a definition before the next term is wider than the one inside a group.
    let two = mdview_core::parse("<dl><dt>T1</dt><dd>D1</dd><dt>T2</dt><dd>D2</dd></dl>");
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&two, &options);
    let y = |w: &str, r: &mut DocumentRenderer| rects_of(r, &two, &options, w)[0].y;
    let inside = y("D1", &mut renderer) - y("T1", &mut renderer);
    let between = y("T2", &mut renderer) - y("D1", &mut renderer);
    assert!(between > inside + 8.0, "{inside} {between}");
}

#[test]
fn a_figure_caption_is_smaller_and_follows_the_figure_alignment() {
    let doc = mdview_core::parse(
        "<figure><figcaption>Caption words here</figcaption></figure>\n\n\
         <figure align=\"center\"><figcaption>Caption words here</figcaption></figure>\n\n\
         Caption words here\n",
    );
    let options = narrow(500.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    let rect = |renderer: &mut DocumentRenderer, nth: usize| {
        let from = doc.text.match_indices("Caption").nth(nth).unwrap().0 as u32;
        renderer.selection_rects(&doc, &options, from, from + 18)[0]
    };
    let (left, centred, body) = (
        rect(&mut renderer, 0),
        rect(&mut renderer, 1),
        rect(&mut renderer, 2),
    );
    assert!(
        left.w < body.w * 0.95,
        "smaller than the body text: {left:?} {body:?}"
    );
    assert!(
        (left.x - body.x).abs() < 1.0,
        "left aligned by default: {left:?} {body:?}"
    );
    assert!(
        centred.x > left.x + 40.0,
        "align=center centres it: {centred:?} {left:?}"
    );
}

#[test]
fn a_figure_with_a_local_image_draws_it_above_its_caption() {
    let doc = mdview_core::parse(
        "<figure align=\"center\"><img src=\"badge.png\" alt=\"badge\"><figcaption>Caption</figcaption></figure>",
    );
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/assets");
    let options = Options {
        base_dir: Some(dir),
        ..narrow(500.0)
    };
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    let caption = rects_of(&mut renderer, &doc, &options, "Caption")[0];
    let frame = renderer.view_frame(&doc, &options, 0.0, 400.0);
    // The badge is blue, centred over the caption, and entirely above it.
    let (mut left, mut right, mut bottom) = (u32::MAX, 0, 0);
    for (i, p) in frame.bitmap.pixels.as_chunks::<4>().0.iter().enumerate() {
        if p[0] > 140 && p[2] < 40 && p[1] > 80 {
            let (x, y) = (i as u32 % frame.bitmap.width, i as u32 / frame.bitmap.width);
            left = left.min(x);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    assert!(right > left, "the badge is drawn");
    assert!(bottom as f32 <= caption.y + 1.0, "{bottom} {caption:?}");
    let image_centre = (left + right) as f32 / 2.0;
    assert!((image_centre - (caption.x + caption.w / 2.0)).abs() < 3.0);
}

/// Whether the engine process that draws math and diagrams can be found, as the renderer looks.
fn engine_available() -> bool {
    if let Some(path) = std::env::var_os("MDV_ENGINE") {
        return PathBuf::from(path).is_file();
    }
    let name = if cfg!(windows) {
        "mdview-engine.exe"
    } else {
        "mdview-engine"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/release")
        .join(name)
        .is_file()
}

#[test]
fn a_point_inside_a_formula_or_diagram_reports_its_source() {
    if !engine_available() {
        eprintln!("skipping: no mdview-engine in target/release");
        return;
    }
    use mdview_render::SourceKind;
    let doc = mdview_core::parse(
        "Inline $x^2 + 1$ in a sentence.\n\n$$\nE = mc^2\n$$\n\n```mermaid\ngraph TD\n  A --> B\n```\n",
    );
    let options = narrow(500.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 600.0);
    let text = |s: mdview_render::Source| doc.text[s.start as usize..s.end as usize].trim();
    let mut found: Vec<(SourceKind, String)> = Vec::new();
    let mut beside = 0;
    let total = renderer.page_layout().unwrap().total_height();
    let mut y = 2.0;
    while y < total {
        let mut x = 2.0;
        while x < 500.0 {
            if let Some(hit) = renderer.hit_test(&doc, &options, x, y) {
                match hit.source {
                    Some(s) => {
                        let item = (s.kind, text(s).to_string());
                        if !found.contains(&item) {
                            found.push(item);
                        }
                    }
                    None => beside += 1,
                }
            }
            x += 5.0;
        }
        y += 3.0;
    }
    assert!(
        found.contains(&(SourceKind::Formula, "x^2 + 1".to_string())),
        "{found:?}"
    );
    assert!(
        found.contains(&(SourceKind::Formula, "E = mc^2".to_string())),
        "{found:?}"
    );
    assert!(
        found.contains(&(SourceKind::Diagram, "graph TD\n  A --> B".to_string())),
        "{found:?}"
    );
    assert_eq!(found.len(), 3, "{found:?}");
    // The text of the sentence, outside the formula, is not a source.
    assert!(beside > 0);
    let word = doc.text.find("sentence").unwrap() as u32;
    let rect = renderer.selection_rects(&doc, &options, word, word + 8)[0];
    let hit = renderer
        .hit_test(&doc, &options, rect.x + 2.0, rect.y + rect.h / 2.0)
        .unwrap();
    assert_eq!(hit.source, None);
}

/// The page for `markdown` in the light theme, as BGRA rows.
fn page(markdown: &str) -> mdview_render::Rendered {
    render(&mdview_core::parse(markdown), &Options::light())
}

/// Columns holding a pixel darker than the page, as `(first, last)`.
fn ink_columns(r: &mdview_render::Rendered) -> (usize, usize) {
    let width = r.bitmap.width as usize;
    let mut span = (usize::MAX, 0);
    for (i, p) in r.bitmap.pixels.as_chunks::<4>().0.iter().enumerate() {
        if p[1] < 0x80 {
            span = (span.0.min(i % width), span.1.max(i % width));
        }
    }
    span
}

fn count(r: &mdview_render::Rendered, bgr: [u8; 3]) -> usize {
    r.bitmap
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] == bgr)
        .count()
}

#[test]
fn a_decoration_inside_a_run_is_drawn_where_it_is() {
    // Strikethrough does not change the font, so the struck words share a shaped run with the
    // text before them; the line used to be looked up at the run's start and never drawn.
    let struck = page("plain ~~struck~~ plain\n");
    let plain = page("plain struck plain\n");
    assert_ne!(struck.bitmap.pixels, plain.bitmap.pixels);
    // And a link's underline stops at the link.
    let link = page("[link](#a) and text that is not a link at all\n");
    let accent = count(&link, [LIGHT.accent[2], LIGHT.accent[1], LIGHT.accent[0]]);
    let long = page("[link and text that is not a link at all](#a)\n");
    let all = count(&long, [LIGHT.accent[2], LIGHT.accent[1], LIGHT.accent[0]]);
    assert!(accent > 0 && accent * 3 < all, "{accent} of {all}");
}

#[test]
fn sub_and_superscript_leave_the_baseline() {
    let rows = |markdown: &str| {
        let r = page(markdown);
        let width = r.bitmap.width as usize;
        let ink: Vec<usize> = r
            .bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| p[1] < 0x80)
            .map(|(i, _)| i / width)
            .collect();
        (ink[0], ink[ink.len() - 1])
    };
    let (top, bottom) = rows("xxx\n");
    assert!(rows("x<sup>x</sup>x\n").0 < top, "superscript rises");
    assert!(rows("x<sub>x</sub>x\n").1 > bottom, "subscript drops");
}

#[test]
fn each_alert_kind_has_its_own_colour() {
    let bar = |markdown: &str| {
        let r = page(markdown);
        // The bar is the first column of the block, a few rows in.
        let at = (60 * r.bitmap.width as usize + LIGHT.margin as usize + 1) * 4;
        [
            r.bitmap.pixels[at],
            r.bitmap.pixels[at + 1],
            r.bitmap.pixels[at + 2],
        ]
    };
    let kinds = ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];
    let colours: Vec<[u8; 3]> = kinds
        .iter()
        .map(|kind| bar(&format!("> [!{kind}]\n> Body text.\n")))
        .collect();
    for (i, a) in colours.iter().enumerate() {
        assert_ne!(*a, [LIGHT.bg[2], LIGHT.bg[1], LIGHT.bg[0]], "{}", kinds[i]);
        for b in &colours[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn html_presentation_attributes_are_honoured() {
    // `<ol type>` sets the marker for the whole list.
    let lettered = page("<ol type=\"a\"><li>one</li><li>two</li></ol>\n");
    let numbered = page("<ol><li>one</li><li>two</li></ol>\n");
    assert_ne!(lettered.bitmap.pixels, numbered.bitmap.pixels);

    // `<div align>` and `<center>` reach the Markdown inside them.
    let left = ink_columns(&page("Short.\n")).0;
    for centred in [
        "<div align=\"center\">\n\nShort.\n\n</div>\n",
        "<center>Short.</center>\n",
    ] {
        let (first, last) = ink_columns(&page(centred));
        assert!(first > left + 200, "{centred:?} starts at {first}");
        let middle = (first + last) / 2;
        assert!(
            (middle as i32 - 390).abs() < 4,
            "{centred:?} is centred on {middle}"
        );
    }

    // `<hr width>` shortens the rule.
    let border = [LIGHT.border[2], LIGHT.border[1], LIGHT.border[0]];
    let full = count(&page("<hr>\n"), border);
    let half = count(&page("<hr width=\"50%\">\n"), border);
    assert!(full > 0 && half * 2 == full, "{half} of {full}");

    // `align="justify"` stretches every line but the last to the column.
    let text = "word ".repeat(60);
    let ragged = ink_columns(&page(&format!("<p>{text}</p>\n"))).1;
    let justified = ink_columns(&page(&format!("<p align=\"justify\">{text}</p>\n"))).1;
    assert!(justified > ragged, "{justified} against {ragged}");
    assert!(justified + 4 >= 780 - LIGHT.margin as usize, "{justified}");
}

#[test]
fn a_code_label_is_not_drawn_through_a_long_first_line() {
    // The label sits at the right of the first line; code that reaches it stops short of it.
    let long = format!("```json\n{}\n```\n", "\"key\": 1, ".repeat(12));
    let wide = page(&long);
    let short = page("```json\n\"key\": 1,\n```\n");
    let width = wide.bitmap.width as usize;
    // The label's own pixels are the same in both: the columns it occupies, on its rows.
    let label = |r: &mdview_render::Rendered| -> Vec<u8> {
        (16..40)
            .flat_map(|row| {
                let at = (row * width + width - LIGHT.margin as usize - 46) * 4;
                r.bitmap.pixels[at..at + 34 * 4].to_vec()
            })
            .collect()
    };
    assert_eq!(label(&wide), label(&short));
}

#[test]
fn mark_is_highlighted() {
    let marked = page("some <mark>marked</mark> text\n");
    let yellow = count(&marked, [0xc5, 0xf8, 0xff]);
    assert!(yellow > 200, "highlight pixels: {yellow}");
    assert_eq!(count(&page("some marked text\n"), [0xc5, 0xf8, 0xff]), 0);
}

#[test]
fn dir_moves_a_paragraph_to_its_side() {
    let (first, last) = ink_columns(&page("<p dir=\"rtl\">Short.</p>\n"));
    assert!(
        first > 600 && last + 4 >= 780 - LIGHT.margin as usize,
        "{first}..{last}"
    );
    // Arabic starts at the right by itself, and at the left when its block says `ltr`.
    let arabic = "\u{645}\u{631}\u{62d}\u{628}\u{627}";
    assert!(ink_columns(&page(&format!("{arabic}\n"))).0 > 600);
    let forced = format!("<div dir=\"ltr\">\n\n{arabic}\n\n</div>\n");
    assert!(ink_columns(&page(&forced)).0 < LIGHT.margin as usize + 4);
}

#[test]
fn table_attributes_are_honoured() {
    let border = [LIGHT.border[2], LIGHT.border[1], LIGHT.border[0]];
    let columns_of = |r: &mdview_render::Rendered| {
        let width = r.bitmap.width as usize;
        let mut span = (usize::MAX, 0);
        for (i, p) in r.bitmap.pixels.as_chunks::<4>().0.iter().enumerate() {
            if p[..3] == border {
                span = (span.0.min(i % width), span.1.max(i % width));
            }
        }
        span
    };
    // `width` sets the table's width and `align` centres it.
    let full = columns_of(&page(
        "<table width=\"100%\"><tr><td>a</td><td>b</td></tr></table>\n",
    ));
    assert_eq!(full, (40, 739));
    let half = columns_of(&page(
        "<table width=\"50%\" align=\"center\"><tr><td>a</td><td>b</td></tr></table>\n",
    ));
    assert_eq!(half.1 - half.0 + 1, 350);
    assert!((half.0 as i32 - 215).abs() <= 1, "{half:?}");

    // A row's `height` makes it taller, and `valign` moves the text down inside it.
    let plain = page("<table><tr><td>a</td></tr></table>\n");
    let tall = page("<table><tr height=\"120\"><td>a</td></tr></table>\n");
    assert!(tall.bitmap.height >= plain.bitmap.height + 80);
    let ink_rows = |markdown: &str| {
        let r = page(markdown);
        let width = r.bitmap.width as usize;
        r.bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .position(|p| p[1] < 0x60)
            .map(|i| i / width)
            .unwrap()
    };
    let top = ink_rows("<table><tr height=\"120\"><td>a</td></tr></table>\n");
    let middle = ink_rows("<table><tr height=\"120\"><td valign=\"middle\">a</td></tr></table>\n");
    let bottom = ink_rows("<table><tr height=\"120\" valign=\"bottom\"><td>a</td></tr></table>\n");
    assert!(
        top + 30 < middle && middle + 30 < bottom,
        "{top} {middle} {bottom}"
    );

    // A row's `align` reaches its cells, and a cell's `width` sets its column.
    let left = ink_columns(&page("<table width=\"100%\"><tr><td>a</td></tr></table>\n")).0;
    let right = ink_columns(&page(
        "<table width=\"100%\"><tr align=\"right\"><td>a</td></tr></table>\n",
    ))
    .0;
    assert!(right > left + 500, "{left} {right}");
    let second = |markdown: &str| {
        let r = page(markdown);
        let width = r.bitmap.width as usize;
        // The x of the `b`, the last ink on the row.
        r.bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| p[1] < 0x60)
            .map(|(i, _)| i % width)
            .max()
            .unwrap()
    };
    let even = second("<table width=\"100%\"><tr><td>a</td><td>b</td></tr></table>\n");
    let wide =
        second("<table width=\"100%\"><tr><td width=\"80%\">a</td><td>b</td></tr></table>\n");
    assert!(wide > even + 150, "{even} {wide}");

    // A caption is centred above the grid, not a row inside it.
    let caption_ink = |markdown: &str| {
        let captioned = page(markdown);
        let width = captioned.bitmap.width as usize;
        let mut span = (usize::MAX, 0);
        // Ink above the table's top border only.
        let top = captioned
            .bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .position(|p| p[..3] == border)
            .unwrap()
            / width;
        for (i, p) in captioned
            .bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
        {
            if i / width < top && p[1] < 0x60 {
                span = (span.0.min(i % width), span.1.max(i % width));
            }
        }
        (span, columns_of(&captioned))
    };
    let ((first, last), _) =
        caption_ink("<table width=\"100%\"><caption>Cap</caption><tr><td>a</td></tr></table>\n");
    assert!(
        ((first + last) as i32 / 2 - 390).abs() < 4,
        "{first}..{last}"
    );
    // Over a table narrower than the column it is centred on the table,
    let ((first, last), (left, right)) = caption_ink(
        "<table><caption>Cap</caption><tr><td>a cell wider than its caption</td></tr></table>\n",
    );
    assert!(right < 400, "{right}");
    assert!(
        ((first + last) as i32 - (left + right) as i32).abs() < 8,
        "{first}..{last} over {left}..{right}"
    );
    // and one wider than its table starts at the column's edge, on one line.
    let ((first, last), (_, right)) = caption_ink(
        "<table><caption>A caption wider than its table</caption><tr><td>a</td></tr></table>\n",
    );
    assert!((40..44).contains(&first), "{first}");
    assert!(last > right + 100, "{last} past {right}");
}

#[test]
fn an_aligned_image_stands_beside_its_text() {
    let dir = temp_dir("image-align");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/assets/badge.png"),
        dir.join("badge.png"),
    )
    .unwrap();
    let draw = |markdown: &str| {
        render(
            &mdview_core::parse(markdown),
            &Options {
                base_dir: Some(dir.clone()),
                ..Options::light()
            },
        )
    };
    // The badge is blue; where its pixels are says where the image went.
    let blue_columns = |r: &mdview_render::Rendered| {
        let width = r.bitmap.width as usize;
        // Columns with a run of badge blue; subpixel text leaves the odd blue pixel too.
        let mut per_column = vec![0u32; width];
        for (i, p) in r.bitmap.pixels.as_chunks::<4>().0.iter().enumerate() {
            if p[0] > 140 && p[2] < 40 && p[1] > 80 {
                per_column[i % width] += 1;
            }
        }
        let solid = |n: &u32| *n >= 8;
        (
            per_column.iter().position(solid).unwrap(),
            per_column.iter().rposition(solid).unwrap(),
        )
    };
    let text = "word ".repeat(40);
    let right = draw(&format!(
        "<img src=\"badge.png\" width=\"100\" align=\"right\"> {text}\n"
    ));
    let (first, last) = blue_columns(&right);
    assert!(first >= 640 && last <= 740, "{first}..{last}");
    let inline = draw(&format!("<img src=\"badge.png\" width=\"100\"> {text}\n"));
    assert!(blue_columns(&inline).0 < 60);
    // Beside the image the text is one block, not pushed under it.
    assert!(right.bitmap.height < inline.bitmap.height + 20);
}

#[test]
fn a_block_that_cannot_be_drawn_says_why() {
    let named = page("```mermaid\narchitecture-beta\n    group api(cloud)[API]\n```\n");
    let other = page("```text\narchitecture-beta\n    group api(cloud)[API]\n```\n");
    // The reason is longer than a language name: more muted ink at the top right.
    let ink = |r: &mdview_render::Rendered| {
        let width = r.bitmap.width as usize;
        r.bitmap
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, p)| i % width > 400 && p[1] < 0xc0)
            .count()
    };
    assert!(
        ink(&named) > ink(&other) * 3,
        "{} {}",
        ink(&named),
        ink(&other)
    );
}

#[test]
fn text_runs_beside_a_floated_image_and_then_under_it() {
    let dir = temp_dir("image-float");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/assets/badge.png"),
        dir.join("badge.png"),
    )
    .unwrap();
    let text = "word ".repeat(120);
    let doc = mdview_core::parse(&format!(
        "<img src=\"badge.png\" width=\"200\" align=\"right\"> {text}\n"
    ));
    let rendered = render(
        &doc,
        &Options {
            base_dir: Some(dir.clone()),
            ..Options::light()
        },
    );
    let width = rendered.bitmap.width as usize;
    // The last dark-text column of each row: short beside the image, the full column under it.
    let reach = |row: usize| {
        (0..width)
            .rev()
            .find(|x| {
                let p = &rendered.bitmap.pixels[(row * width + x) * 4..][..4];
                p[0] < 0x60 && p[1] < 0x60 && p[2] < 0x60
            })
            .unwrap_or(0)
    };
    let first_lines = (44..60).map(reach).max().unwrap();
    let last_lines = (rendered.bitmap.height as usize - 90..rendered.bitmap.height as usize - 50)
        .map(reach)
        .max()
        .unwrap();
    assert!(first_lines < 540, "beside the image: {first_lines}");
    assert!(last_lines > 640, "under the image: {last_lines}");
}

/// The first and last column holding the table border's colour.
fn border_columns(r: &mdview_render::Rendered) -> (usize, usize) {
    let border = [LIGHT.border[2], LIGHT.border[1], LIGHT.border[0]];
    let width = r.bitmap.width as usize;
    let mut span = (usize::MAX, 0);
    for (i, p) in r.bitmap.pixels.as_chunks::<4>().0.iter().enumerate() {
        if p[..3] == border {
            span = (span.0.min(i % width), span.1.max(i % width));
        }
    }
    span
}

#[test]
fn a_table_is_as_wide_as_its_content() {
    let width_of = |markdown: &str| {
        let (first, last) = border_columns(&page(markdown));
        (first, last + 1 - first)
    };
    // A small table stays small, at the left of the column.
    let small = "| Key | Value |\n| --- | ---: |\n| a | 1 |\n| bb | 22 |\n";
    let (left, small_width) = width_of(small);
    assert_eq!(left, 40);
    assert!((80..200).contains(&small_width), "{small_width}");
    // One line longer than the page takes the page's width, no more.
    let long = "word ".repeat(60);
    let wide = format!("| Key | Value |\n| --- | --- |\n| a | {long} |\n");
    assert_eq!(width_of(&wide), (40, 700));
    // `align`, on the table or on a `<div>` around it, places a table narrower than the page.
    let cells = "<tr><td>one</td><td>two</td></tr>";
    let (plain_left, plain_width) = width_of(&format!("<table>{cells}</table>\n"));
    assert_eq!(plain_left, 40);
    for centred in [
        format!("<table align=\"center\">{cells}</table>\n"),
        format!("<div align=\"center\">\n<table>{cells}</table>\n</div>\n"),
    ] {
        let (left, width) = width_of(&centred);
        assert_eq!(width, plain_width, "{centred}");
        assert!(
            (left as i32 * 2 + width as i32 - 780).abs() <= 2,
            "{centred}: {left} {width}"
        );
    }
    let (left, width) = width_of(&format!("<table align=\"right\">{cells}</table>\n"));
    assert_eq!(left + width, 740);
    // A cell across two columns widens them when it needs more than they hold.
    let spanning = format!(
        "<table>{cells}<tr><td colspan=\"2\">a cell that is wider than both columns</td></tr></table>\n"
    );
    let (_, spanning_width) = width_of(&spanning);
    assert!(
        spanning_width > plain_width + 100 && spanning_width < 400,
        "{plain_width} {spanning_width}"
    );
}

#[test]
fn table_columns_follow_their_content_and_stated_widths() {
    let options = Options::light();
    let x_of = |markdown: &str, word: &str| {
        let doc = mdview_core::parse(markdown);
        let mut renderer = DocumentRenderer::new();
        renderer.layout_exact(&doc, &options);
        let rects = rects_of(&mut renderer, &doc, &options, word);
        (rects[0].x, rects.len())
    };
    // The first cell's text starts after the margin, the border and the cell padding.
    let start = 40.0 + 1.0 + 13.0;
    // A column stated in px is that wide, whatever it holds.
    let stated = "<table><tr><td width=\"200\">a</td><td>next</td></tr></table>\n";
    assert_eq!(x_of(stated, "next").0, start + 200.0);
    // A column stated in percent is that share of a table grown to make it so.
    let half = "<table><tr><td width=\"50%\">a</td><td>the rest of it</td></tr></table>\n";
    let rest = x_of(half, "the rest").0;
    let free = x_of(
        "<table><tr><td>a</td><td>the rest of it</td></tr></table>\n",
        "the rest",
    )
    .0;
    assert!(rest > free + 50.0, "{free} {rest}");
    let (first, last) = border_columns(&page(half));
    let inner = (last + 1 - first - 2) as f32;
    assert!(
        (rest - start - inner / 2.0).abs() <= 1.0,
        "{rest} in {inner}"
    );
    // Too wide for the page: the short column keeps its word and the long one wraps.
    let long = "several words that go on ".repeat(8);
    let wrapping = format!("| Name | Text |\n| --- | --- |\n| identifier | {long} |\n");
    assert_eq!(x_of(&wrapping, "identifier").1, 1);
    assert!(x_of(&wrapping, "several words").0 < 200.0);
    // Words too long to stand side by side: the long one breaks, the short one stays whole.
    let hash = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".repeat(2);
    let breaking = format!("| Hash | Note |\n| --- | --- |\n| {hash} | unchanged |\n");
    assert_eq!(x_of(&breaking, "unchanged").1, 1);
    assert!(x_of(&breaking, &hash).1 > 1);
    assert_eq!(border_columns(&page(&breaking)), (40, 739));
}

#[test]
fn a_click_in_a_table_lands_on_the_text_of_its_cell() {
    // Cells of one row share their rows of pixels, so the column decides which one was meant.
    let rows = "<tr><td>first</td><td>second</td></tr><tr><td>third</td><td>fourth</td></tr>";
    for (table, centred) in [
        (format!("<table width=\"100%\">{rows}</table>\n"), false),
        (format!("<table align=\"center\">{rows}</table>\n"), true),
    ] {
        let doc = mdview_core::parse(&table);
        let options = Options::light();
        let mut renderer = DocumentRenderer::new();
        renderer.layout(&doc, &options);
        renderer.view_frame(&doc, &options, 0.0, 400.0);
        for word in ["first", "second", "third", "fourth"] {
            let at = doc.text.find(word).unwrap() as u32;
            let rect = rects_of(&mut renderer, &doc, &options, word)[0];
            // A centred table is in the middle of the page, and the highlight is where it is.
            assert!(
                !centred || (rect.x > 300.0 && rect.x + rect.w < 480.0),
                "{word}: {rect:?}"
            );
            let hit = renderer
                .hit_test(&doc, &options, rect.x + 1.0, rect.y + rect.h / 2.0)
                .expect("a click in the cell");
            assert_eq!(hit.offset, at, "{word} in {table}");
            assert!(hit.on_text, "{word} in {table}");
        }
    }
}

#[test]
fn the_tables_fixture_keeps_its_tables_inside_what_holds_them() {
    // A narrow table, a wrapping one, spans, stated widths, tables in a list and a quote, and
    // a word that must break: every table starts where its container does and none passes the
    // right edge of the column.
    let doc = fixture("tables.md");
    for width in [780.0, 420.0] {
        let rendered = render(&doc, &narrow(width));
        let (first, last) = border_columns(&rendered);
        assert_eq!(first, 40, "{width}");
        assert!(last < width as usize - 40, "{width}: {last}");
        assert!(rendered.content_height > 1000.0, "{width}");
    }
}
