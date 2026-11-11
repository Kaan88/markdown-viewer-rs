//! What survives a relayout and what a relayout costs: the image store across width, scale and
//! `<details>` changes. (`render.rs` holds the pixel smoke tests; this file is about the work the
//! renderer does again, or does not, when the view changes.)

use mdview_render::{DocumentRenderer, ImageStats, LIGHT, Options};
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tmp")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_red_png(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 64, 64);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let data: Vec<u8> = (0..64 * 64).flat_map(|_| [220, 30, 30, 255]).collect();
    writer.write_image_data(&data).unwrap();
}

/// A document with every kind of stored raster: a PNG, an SVG file, inline math in body text, in
/// a heading, in a table cell and in a muted quote, display math and a `<details>`.
fn mixed(dir: &Path) -> mdview_core::Document {
    write_red_png(&dir.join("red.png"));
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/assets/badge.svg"),
        dir.join("badge.svg"),
    )
    .unwrap();
    mdview_core::parse(
        "# Title with $x^2$\n\n\
         Body text ![red](red.png) and ![badge](badge.svg) and $a+b$ inline.\n\n\
         > A quote with $q_i$ in it.\n\n\
         | a | b |\n|---|---|\n| $c^3$ | d |\n\n\
         $$\\sum_{i=1}^n i$$\n\n\
         <details>\n<summary>More</summary>\n\nHidden $h$ text.\n\n</details>\n\n\
         Last paragraph.\n",
    )
}

fn options(dir: &Path, width: f32, scale: f32) -> Options {
    Options {
        width,
        scale,
        theme: LIGHT,
        max_height: None,
        base_dir: Some(dir.to_path_buf()),
    }
}

/// Lays out and draws one viewport, the way a shell does after any change.
fn show(renderer: &mut DocumentRenderer, doc: &mdview_core::Document, options: &Options) {
    renderer.layout(doc, options);
    renderer.view_frame(doc, options, 0.0, 2000.0);
}

fn delta(before: ImageStats, after: ImageStats) -> (u64, u64) {
    (
        after.decodes - before.decodes,
        after.engine_jobs - before.engine_jobs,
    )
}

#[test]
fn stored_images_survive_width_scale_and_details_changes() {
    let dir = temp_dir("store-survives");
    let doc = mixed(&dir);
    let details = (0..doc.blocks.len() as u32)
        .find(|&b| {
            matches!(
                doc.blocks[b as usize].kind,
                mdview_core::model::BlockKind::Details { .. }
            )
        })
        .unwrap();
    let mut renderer = DocumentRenderer::new();
    renderer.set_details(&doc, details, true);
    let base = options(&dir, 600.0, 1.0);
    show(&mut renderer, &doc, &base);
    let first = renderer.image_stats();
    assert_eq!(first.decodes, 2, "the PNG and the SVG are read once");
    assert!(
        first.engine_jobs >= 2,
        "the SVG and the formulas went to the engine"
    );

    // The same layout again, as a tab switch or reload would ask: nothing is sent twice. Inline
    // math in a heading, a table cell and a quote used to be rendered again on every layout.
    let before = renderer.image_stats();
    show(&mut renderer, &doc, &base);
    assert_eq!(delta(before, renderer.image_stats()), (0, 0), "relayout");

    // A width change moves text but no raster depends on it.
    let before = renderer.image_stats();
    show(&mut renderer, &doc, &options(&dir, 900.0, 1.0));
    show(&mut renderer, &doc, &options(&dir, 450.0, 1.0));
    assert_eq!(
        delta(before, renderer.image_stats()),
        (0, 0),
        "width change"
    );

    // A details toggle neither.
    let before = renderer.image_stats();
    renderer.set_details(&doc, details, false);
    show(&mut renderer, &doc, &base);
    renderer.set_details(&doc, details, true);
    show(&mut renderer, &doc, &base);
    assert_eq!(
        delta(before, renderer.image_stats()),
        (0, 0),
        "details toggle"
    );

    // A new scale re-rasterizes what is drawn at device resolution (the SVG, the formulas) but
    // decodes no bitmap; going back to the old scale finds everything.
    let before = renderer.image_stats();
    show(&mut renderer, &doc, &options(&dir, 600.0, 1.5));
    let (decodes, jobs) = delta(before, renderer.image_stats());
    assert_eq!(
        decodes, 1,
        "only the SVG file is read again, for its new raster"
    );
    assert!(jobs >= 2, "formulas follow the scale: {jobs}");
    let before = renderer.image_stats();
    show(&mut renderer, &doc, &base);
    show(&mut renderer, &doc, &options(&dir, 600.0, 1.5));
    assert_eq!(
        delta(before, renderer.image_stats()),
        (0, 0),
        "both scales kept"
    );
}

#[test]
fn an_edited_image_file_loads_again() {
    let dir = temp_dir("store-edited");
    let path = dir.join("red.png");
    write_red_png(&path);
    let doc = mdview_core::parse("![red](red.png)\n");
    let options = options(&dir, 400.0, 1.0);
    let mut renderer = DocumentRenderer::new();
    show(&mut renderer, &doc, &options);
    assert_eq!(renderer.image_stats().decodes, 1);
    show(&mut renderer, &doc, &options);
    assert_eq!(renderer.image_stats().decodes, 1);
    // A different length is a different identity, whatever the clock says.
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(&[0; 16]);
    std::fs::write(&path, bytes).unwrap();
    show(&mut renderer, &doc, &options);
    assert_eq!(renderer.image_stats().decodes, 2);
}
