//! Correcting a layout in place: a toggled `<details>` or an arrived image re-measures one block,
//! the result equals a fresh layout byte for byte, and a correction above the viewport keeps what
//! is on screen still.

use mdview_render::{DecodedImage, DocumentRenderer, LIGHT, Options, PageLayout};
use std::path::PathBuf;

fn plain(width: f32) -> Options {
    Options {
        width,
        scale: 1.0,
        theme: LIGHT,
        max_height: None,
        base_dir: None,
    }
}

/// A `<details>` near the top, a remote image with no declared size, one with a declared size,
/// and enough paragraphs (and a footnote) to scroll.
fn long_doc() -> mdview_core::Document {
    let mut md = String::from(
        "# Top\n\n<details>\n<summary>More</summary>\n\nHidden body text that takes a line or two \
         when the details is open.\n\nAnd a second paragraph.\n\n</details>\n\n\
         ![remote](https://example.com/a.png)\n\n\
         <img src=\"https://example.com/b.png\" width=\"40\" height=\"30\">\n\n",
    );
    for i in 0..80 {
        md.push_str(&format!(
            "Paragraph {i} has enough words in it to wrap onto a second line at a narrow width, \
             which keeps the page long[^n].\n\n"
        ));
    }
    md.push_str("[^n]: The footnote.\n");
    mdview_core::parse(&md)
}

fn details_of(doc: &mdview_core::Document) -> u32 {
    (0..doc.blocks.len() as u32)
        .find(|&b| {
            matches!(
                doc.blocks[b as usize].kind,
                mdview_core::model::BlockKind::Details { .. }
            )
        })
        .unwrap()
}

/// The exact layout a new renderer makes, with the same toggles and remote images.
fn fresh(
    doc: &mdview_core::Document,
    options: &Options,
    setup: impl Fn(&mut DocumentRenderer),
) -> DocumentRenderer {
    let mut renderer = DocumentRenderer::new();
    setup(&mut renderer);
    renderer.layout_exact(doc, options);
    renderer
}

fn same_positions(a: &PageLayout, b: &PageLayout) {
    assert_eq!(a.blocks().len(), b.blocks().len());
    for (x, y) in a.blocks().iter().zip(b.blocks()) {
        assert_eq!(x.block, y.block);
        assert_eq!(x.y.to_bits(), y.y.to_bits(), "block {} y", x.block);
        assert_eq!(
            x.height.to_bits(),
            y.height.to_bits(),
            "block {} height",
            x.block
        );
    }
    assert_eq!(a.total_height().to_bits(), b.total_height().to_bits());
    assert_eq!(a.footnote_y(1), b.footnote_y(1));
}

fn red(size: u32) -> DecodedImage {
    DecodedImage {
        width: size,
        height: size,
        baseline: size,
        rgba: (0..size * size).flat_map(|_| [220, 30, 30, 255]).collect(),
    }
}

#[test]
fn a_one_block_relayout_equals_a_fresh_layout() {
    let doc = long_doc();
    let details = details_of(&doc);
    let options = plain(420.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout_exact(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 300.0);
    renderer.set_details(&doc, details, true);
    renderer.view_frame(&doc, &options, 0.0, 300.0);
    let mut expected = fresh(&doc, &options, |r| r.set_details(&doc, details, true));
    same_positions(
        renderer.page_layout().unwrap(),
        expected.page_layout().unwrap(),
    );
    let frame = renderer.frame().unwrap().bitmap.pixels.clone();
    let view = expected.view(&doc, &options, 0.0, 300.0);
    assert_eq!(frame, view.bitmap.pixels, "the frame matches a fresh view");

    // An arrived image re-measures the block that shows it, and only that one.
    let before = renderer.page_layout().unwrap().clone();
    assert!(renderer.image_arrived("https://example.com/a.png", Some(red(64))));
    renderer.view_frame(&doc, &options, 0.0, 300.0);
    let after = renderer.page_layout().unwrap().clone();
    let changed: Vec<u32> = before
        .blocks()
        .iter()
        .zip(after.blocks())
        .filter(|(a, b)| a.height != b.height)
        .map(|(a, _)| a.block)
        .collect();
    assert_eq!(changed.len(), 1, "one block grew: {changed:?}");
    let expected = fresh(&doc, &options, |r| {
        r.set_details(&doc, details, true);
        r.image_arrived("https://example.com/a.png", Some(red(64)));
    });
    same_positions(&after, expected.page_layout().unwrap());
}

#[test]
fn a_declared_size_reserves_the_box_and_remote_urls_are_reported() {
    let doc = long_doc();
    let options = plain(420.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    renderer.view_frame(&doc, &options, 0.0, 600.0);
    let mut asked = renderer.take_remote_requests();
    asked.sort();
    assert_eq!(
        asked,
        ["https://example.com/a.png", "https://example.com/b.png"]
    );
    assert!(renderer.take_remote_requests().is_empty(), "each URL once");
    let before = renderer.page_layout().unwrap().clone();
    renderer.image_arrived("https://example.com/b.png", Some(red(80)));
    renderer.view_frame(&doc, &options, 0.0, 600.0);
    same_positions(&before, renderer.page_layout().unwrap());
    // The reserved box now draws the image.
    let reds = renderer
        .frame()
        .unwrap()
        .bitmap
        .pixels
        .chunks_exact(4)
        .filter(|p| p[2] > 180 && p[0] < 80 && p[1] < 80)
        .count();
    assert!(reds > 40 * 30 / 2, "the 40x30 box shows the image: {reds}");
}

#[test]
fn a_correction_above_the_viewport_keeps_the_content_still() {
    let doc = long_doc();
    let details = details_of(&doc);
    let options = plain(420.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout_exact(&doc, &options);
    let start = renderer.page_layout().unwrap().blocks()[40].y + 7.0;
    renderer.view_frame(&doc, &options, start, 300.0);
    let shown = renderer.frame().unwrap().bitmap.pixels.clone();
    // Opening the details and an image arriving, both far above the viewport. A selection over
    // the details text and a hit test measure them now, as find marks or a click would.
    renderer.set_details(&doc, details, true);
    let body = doc.children(details).last().unwrap();
    let text = doc.blocks[body as usize].text;
    renderer.selection_rects(&doc, &options, text.start, text.end);
    renderer.image_arrived("https://example.com/a.png", Some(red(64)));
    renderer.hit_test(&doc, &options, 10.0, 200.0);
    let top = renderer.top().unwrap();
    assert!(
        top > start + 20.0,
        "the anchor moved down with the content: {top}"
    );
    let frame = renderer.view_frame(&doc, &options, top, 300.0);
    assert_eq!(frame.top, top);
    assert_eq!(
        frame.band,
        Some((0, 0)),
        "the kept rows still show the same text"
    );
    assert_eq!(frame.bitmap.pixels, shown, "nothing on screen moved");
    // The frame equals a fresh view of the corrected layout.
    let mut expected = fresh(&doc, &options, |r| {
        r.set_details(&doc, details, true);
        r.image_arrived("https://example.com/a.png", Some(red(64)));
    });
    same_positions(
        renderer.page_layout().unwrap(),
        expected.page_layout().unwrap(),
    );
    let view = expected.view(&doc, &options, top, 300.0);
    assert_eq!(shown, view.bitmap.pixels);
    // Queries agree with the fresh layout after the correction.
    let page = renderer.page_layout().unwrap().clone();
    let fresh_page = expected.page_layout().unwrap().clone();
    for y in [0.0, 100.0, top, top + 150.0, page.total_height() - 2.0] {
        assert_eq!(page.index_at(y), fresh_page.index_at(y), "index_at {y}");
        let a = renderer.hit_test(&doc, &options, 40.0, y);
        let b = expected.hit_test(&doc, &options, 40.0, y);
        assert_eq!(a, b, "hit at {y}");
    }
    for block in [details, body, page.blocks()[60].block] {
        assert_eq!(page.y_of(&doc, block), fresh_page.y_of(&doc, block));
    }
    assert_eq!(
        renderer.selection_rects(&doc, &options, text.start, text.end),
        expected.selection_rects(&doc, &options, text.start, text.end)
    );
}

/// At fractional scales a correction above moves the content by part of a row: the kept frame
/// cannot be reused, and the blocks it showed are drawn again for their new fraction (rules and
/// underlines stay on whole rows) rather than shifted.
#[test]
fn a_fractional_correction_above_redraws_like_a_fresh_view() {
    let doc = long_doc();
    let details = details_of(&doc);
    for scale in [1.0, 1.25, 1.5, 1.75] {
        let options = Options {
            scale,
            ..plain(420.0 * scale)
        };
        let mut renderer = DocumentRenderer::new();
        renderer.layout_exact(&doc, &options);
        let start = renderer.page_layout().unwrap().blocks()[40].y.round();
        renderer.view_frame(&doc, &options, start, 300.0);
        renderer.set_details(&doc, details, true);
        renderer.hit_test(&doc, &options, 10.0, 10.0);
        let top = renderer.top().unwrap();
        let frame = renderer.view_frame(&doc, &options, top, 300.0);
        let (at, band) = (frame.top, frame.band);
        let shown = frame.bitmap.pixels.clone();
        let mut expected = fresh(&doc, &options, |r| r.set_details(&doc, details, true));
        same_positions(
            renderer.page_layout().unwrap(),
            expected.page_layout().unwrap(),
        );
        let view = expected.view(&doc, &options, at, 300.0);
        assert_eq!(shown, view.bitmap.pixels, "@{scale} band {band:?}");
    }
}

#[test]
fn a_frame_matches_the_whole_page_render() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (fixture, scale) in [("showcase.md", 1.0), ("showcase.md", 1.5), ("math.md", 1.0)] {
        let doc = mdview_core::parse_bytes(&std::fs::read(dir.join(fixture)).unwrap());
        let options = Options {
            scale,
            base_dir: Some(dir.clone()),
            ..plain(780.0)
        };
        let full = mdview_render::render(&doc, &options);
        let mut renderer = DocumentRenderer::new();
        renderer.layout(&doc, &options);
        let rows = 500u32;
        let width = full.bitmap.width as usize * 4;
        let mut top = 0u32;
        while top < full.bitmap.height {
            let frame = renderer.view_frame(&doc, &options, top as f32, rows as f32);
            let at = frame.top as usize;
            let count = (frame.bitmap.height as usize).min(full.bitmap.height as usize - at);
            let differs = (0..count).find(|row| {
                frame.bitmap.pixels[row * width..(row + 1) * width]
                    != full.bitmap.pixels[(at + row) * width..(at + row + 1) * width]
            });
            assert_eq!(differs, None, "{fixture}@{scale} frame at {at}");
            top += rows - 37;
        }
        let total = renderer.page_layout().unwrap().total_height();
        assert_eq!(total, full.content_height, "{fixture}@{scale}");
    }
}
