//! The lazy layout: estimates first, exact where it is shown, refined in slices. It must converge
//! to the eager layout, land jumps exactly while estimates remain, keep the content on screen
//! still while refinement corrects what is above it, and load nothing for off-screen blocks.

use mdview_render::{DocumentRenderer, Jump, LIGHT, Options, PageLayout};
use std::path::PathBuf;
use std::time::Duration;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn options(width: f32, scale: f32) -> Options {
    Options {
        width,
        scale,
        theme: LIGHT,
        max_height: None,
        base_dir: Some(fixtures()),
    }
}

fn fixture(name: &str) -> mdview_core::Document {
    mdview_core::parse_bytes(&std::fs::read(fixtures().join(name)).unwrap())
}

/// Headings, lists, quotes, code, a table and long paragraphs: text only, so refinement alone
/// makes every block exact.
fn long_text() -> mdview_core::Document {
    let mut md = String::new();
    for i in 0..40 {
        md.push_str(&format!("## Section {i}\n\n"));
        md.push_str(&"A sentence of ordinary length that wraps somewhere. ".repeat(i % 7 + 1));
        md.push_str("\n\n- one item\n- two items, the second a little longer\n\n");
        if i % 5 == 0 {
            md.push_str("```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n");
            md.push_str("> A quote that runs on for a while to wrap.\n\n");
            md.push_str("| a | b |\n|---|---|\n| one | two words |\n\n");
        }
    }
    mdview_core::parse(&md)
}

fn refine_all(renderer: &mut DocumentRenderer, doc: &mdview_core::Document, options: &Options) {
    let mut slices = 0;
    while renderer.refine(doc, options, Duration::from_millis(2)) {
        slices += 1;
        assert!(slices < 100_000, "refinement ends");
    }
}

fn eager(doc: &mdview_core::Document, options: &Options) -> PageLayout {
    let mut renderer = DocumentRenderer::new();
    renderer.layout_exact(doc, options).clone()
}

fn same_positions(a: &PageLayout, b: &PageLayout, what: &str) {
    assert_eq!(a.blocks().len(), b.blocks().len());
    for (x, y) in a.blocks().iter().zip(b.blocks()) {
        assert_eq!(x.y.to_bits(), y.y.to_bits(), "{what}: block {} y", x.block);
        assert_eq!(
            x.height.to_bits(),
            y.height.to_bits(),
            "{what}: block {}",
            x.block
        );
    }
    assert_eq!(
        a.total_height().to_bits(),
        b.total_height().to_bits(),
        "{what}"
    );
}

#[test]
fn refinement_alone_converges_for_text() {
    let doc = long_text();
    for (width, scale) in [(780.0, 1.0), (420.0, 1.0), (780.0, 1.5), (600.0, 1.25)] {
        let options = options(width, scale);
        let mut renderer = DocumentRenderer::new();
        assert!(renderer.layout(&doc, &options).estimated_blocks() > 0);
        refine_all(&mut renderer, &doc, &options);
        let what = format!("{width}@{scale}");
        same_positions(
            renderer.page_layout().unwrap(),
            &eager(&doc, &options),
            &what,
        );
    }
}

/// With images, formulas and diagrams, refinement leaves stand-ins (it loads nothing); showing
/// every part of the page makes them exact, and then frames equal the one-pass page.
#[test]
fn shown_and_refined_converges_to_the_eager_layout_and_pixels() {
    for name in ["showcase.md", "math.md", "images.md", "diagrams.md"] {
        for (width, scale) in [(780.0, 1.0), (520.0, 1.5)] {
            let doc = fixture(name);
            let options = options(width, scale);
            let what = format!("{name} {width}@{scale}");
            let mut renderer = DocumentRenderer::new();
            renderer.layout(&doc, &options);
            refine_all(&mut renderer, &doc, &options);
            let rows = 400.0;
            // Down the page until the frame stops moving (the last one is clamped).
            let mut top = 0.0;
            let mut last = -1.0;
            loop {
                let drawn = renderer.view_frame(&doc, &options, top, rows).top;
                if drawn == last {
                    break;
                }
                last = drawn;
                top = drawn + rows - 50.0;
            }
            let full = mdview_render::render(&doc, &options);
            same_positions(
                renderer.page_layout().unwrap(),
                &eager(&doc, &options),
                &what,
            );
            assert_eq!(
                renderer.page_layout().unwrap().total_height(),
                full.content_height,
                "{what}"
            );
            let width = full.bitmap.width as usize * 4;
            let mut top = 0.0;
            let mut last = usize::MAX;
            loop {
                let frame = renderer.view_frame(&doc, &options, top, rows);
                let at = frame.top as usize;
                if at == last {
                    break;
                }
                last = at;
                let count = (frame.bitmap.height as usize).min(full.bitmap.height as usize - at);
                let differs = (0..count).find(|row| {
                    frame.bitmap.pixels[row * width..(row + 1) * width]
                        != full.bitmap.pixels[(at + row) * width..(at + row + 1) * width]
                });
                assert_eq!(differs, None, "{what}: frame at {at}");
                top = frame.top + rows - 50.0;
            }
        }
    }
}

#[test]
fn layout_and_refinement_load_nothing() {
    let doc = fixture("showcase.md");
    let options = options(780.0, 1.0);
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    refine_all(&mut renderer, &doc, &options);
    let stats = renderer.image_stats();
    assert_eq!((stats.decodes, stats.engine_jobs), (0, 0));
    assert!(renderer.take_remote_requests().is_empty());
    renderer.view_frame(&doc, &options, 0.0, 900.0);
    assert!(renderer.image_stats().decodes + renderer.image_stats().engine_jobs > 0);
}

/// A jump measures the viewport it lands on, so the target sits exactly `margin` below the top
/// although most of the page is still estimated, and the frame equals an eager view there.
#[test]
fn a_jump_is_exact_while_estimates_remain() {
    let doc = long_text();
    let options = options(780.0, 1.0);
    let margin = 24.0;
    let exact = {
        let mut renderer = DocumentRenderer::new();
        renderer.layout_exact(&doc, &options);
        renderer
    };
    let headings: Vec<u32> = doc.headings.iter().map(|h| h.block).collect();
    for &block in [headings[31], headings[3], headings[39]].iter() {
        let mut renderer = DocumentRenderer::new();
        renderer.layout(&doc, &options);
        let top = renderer
            .jump_to(&doc, &options, Jump::Block(block), margin, 600.0)
            .unwrap();
        let page = renderer.page_layout().unwrap();
        assert!(page.estimated_blocks() > 0, "most of the page is a guess");
        let y = page.y_of(&doc, block).unwrap();
        let max = page.total_height() - 600.0;
        if top < max {
            assert_eq!(y - top, margin, "block {block} lands at the margin");
        }
        let shown = renderer
            .view_frame(&doc, &options, top, 600.0)
            .bitmap
            .pixels
            .clone();
        // An eager layout of the same document shows the same rows around the target.
        let mut exact = DocumentRenderer::new();
        exact.layout_exact(&doc, &options);
        let target = exact.page_layout().unwrap().y_of(&doc, block).unwrap();
        if top < max {
            let view = exact.view(&doc, &options, target - margin, 600.0);
            assert_eq!(shown, view.bitmap.pixels, "block {block}");
        }
    }
    // A text jump puts the line holding the offset at the margin.
    let start = doc.text.find("Section 27").unwrap() as u32;
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    let top = renderer
        .jump_to(&doc, &options, Jump::Text(start), margin, 600.0)
        .unwrap();
    let rects = renderer.selection_rects(&doc, &options, start, start + 7);
    assert_eq!(rects[0].y - top, margin);
    let _ = exact;
}

/// Refinement corrects blocks above the viewport while the reader looks at the middle of the
/// page: the anchor keeps the same text on screen, and the frame there equals an eager view.
#[test]
fn refinement_keeps_the_content_on_screen_still() {
    let doc = long_text();
    let options = options(780.0, 1.0);
    let headings: Vec<u32> = doc.headings.iter().map(|h| h.block).collect();
    let mut renderer = DocumentRenderer::new();
    renderer.layout(&doc, &options);
    let top = renderer
        .jump_to(&doc, &options, Jump::Block(headings[25]), 10.0, 500.0)
        .unwrap();
    let shown = renderer
        .view_frame(&doc, &options, top, 500.0)
        .bitmap
        .pixels
        .clone();
    refine_all(&mut renderer, &doc, &options);
    let after = renderer.top().unwrap();
    let frame = renderer.view_frame(&doc, &options, after, 500.0);
    assert_eq!(frame.bitmap.pixels, shown, "same text, same rows");
    let exact = eager(&doc, &options);
    assert_eq!(after, exact.y_of(&doc, headings[25]).unwrap() - 10.0);
}
