//! Pixel comparison against the approved showcase, tables and floats images in `tests/golden/`.
//!
//! The goldens use this host's system fonts (Segoe UI, Consolas), so they are compared only on
//! Windows; elsewhere the test passes with a note until the renderer bundles its fonts. Rendering
//! is on the CPU and deterministic: with nothing changed, no pixel differs. The small allowance is
//! for a font or library update nudging an edge, not for real changes. A share of the page would
//! be too loose: a gantt axis that lost half its labels changed 646 pixels, 0.02% of a 4071 px page.
//!
//! Cargo stages the pinned engine. After an intended visual change, regenerate with
//! `MDV_BLESS=1 cargo test --release -p mdview --test golden` and review the images.

use mdview_render::{DARK, DocumentRenderer, LIGHT, Options, Theme, render};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[path = "support/engine.rs"]
mod test_engine;

/// A pixel counts as different when any channel differs by more than this.
const CHANNEL_TOLERANCE: u8 = 24;
/// At most this many pixels may differ.
const MAX_DIFFERENT: usize = 64;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Decodes an 8-bit RGB PNG into (width, height, RGB bytes).
fn read_rgb(path: &Path) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    ));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb, "{}", path.display());
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "{}", path.display());
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

fn engine_raster() -> bool {
    static WORKS: OnceLock<bool> = OnceLock::new();
    *WORKS.get_or_init(|| {
        let source = "graph TD\n  A --> B";
        let doc = mdview_core::parse(&format!("```mermaid\n{source}\n```\n"));
        let mut renderer = DocumentRenderer::new();
        renderer.layout_exact(
            &doc,
            &Options {
                width: 780.0,
                scale: 1.0,
                theme: LIGHT,
                max_height: None,
                base_dir: None,
            },
        );
        renderer.diagram_raster(source).is_some_and(|image| {
            image.width > 0
                && image.height > 0
                && image.rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 0)
        })
    })
}

fn bless(probe: impl FnOnce() -> bool, write: impl FnOnce()) {
    assert!(probe(), "golden bless: engine raster probe failed");
    write();
}

#[test]
fn a_failed_engine_probe_never_writes_goldens() {
    let wrote = std::cell::Cell::new(false);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bless(|| false, || wrote.set(true));
    }));
    assert!(result.is_err());
    assert!(!wrote.get());
}

#[test]
fn the_staged_engine_produces_a_real_raster() {
    test_engine::require_engine();
    assert!(engine_raster(), "test engine: diagram raster missing");
}

fn check(fixture: &str, name: &str, theme: Theme, scale: f32) {
    test_engine::require_engine();
    let showcase = root().join(format!("fixtures/{fixture}.md"));
    let doc = mdview_core::parse_bytes(&std::fs::read(&showcase).unwrap());
    let rendered = render(
        &doc,
        &Options {
            width: 780.0,
            scale,
            theme,
            max_height: None,
            base_dir: showcase.parent().map(Path::to_path_buf),
        },
    );
    let golden = root().join(format!("tests/golden/{fixture}-{name}.png"));
    if std::env::var_os("MDV_BLESS").is_some() {
        bless(engine_raster, || {
            rendered.bitmap.write_png(&golden).unwrap()
        });
        return;
    }
    let (width, height, expected) = read_rgb(&golden);
    let bitmap = &rendered.bitmap;
    assert_eq!(
        (bitmap.width, bitmap.height),
        (width, height),
        "{fixture}-{name}: page size changed"
    );
    let mut different = 0usize;
    let mut first = None;
    for (i, (actual, expected)) in bitmap
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.as_chunks::<3>().0.iter())
        .enumerate()
    {
        // The bitmap is BGRA, the PNG RGB.
        let actual = [actual[2], actual[1], actual[0]];
        if actual
            .iter()
            .zip(expected)
            .any(|(a, e)| a.abs_diff(*e) > CHANNEL_TOLERANCE)
        {
            different += 1;
            first.get_or_insert((i as u32 % width, i as u32 / width));
        }
    }
    let share = different as f64 / (width as f64 * height as f64);
    if different > MAX_DIFFERENT {
        let out = root().join(format!("target/golden-actual-{fixture}-{name}.png"));
        _ = bitmap.write_png(&out);
        panic!(
            "{fixture}-{name}: {different} pixels ({:.3}%) differ, first at {first:?}; actual written to {}",
            share * 100.0,
            out.display()
        );
    }
}

#[test]
fn fixtures_match_the_goldens() {
    if !cfg!(windows) {
        eprintln!("golden comparison skipped: the goldens use Windows system fonts");
        return;
    }
    for (name, theme, scale) in [
        ("light@1.0", LIGHT, 1.0),
        ("light@1.5", LIGHT, 1.5),
        ("dark@1.0", DARK, 1.0),
        ("dark@1.5", DARK, 1.5),
    ] {
        check("showcase", name, theme, scale);
    }
    // Table widths: narrow, wrapping, spans, stated widths, inside a list and a quote.
    check("tables", "light@1.0", LIGHT, 1.0);
    check("tables", "dark@1.5", DARK, 1.5);
}

/// `fixtures/floats.md`: floated images with paragraphs, a heading, lists, a quote, code, a table
/// and a rule beside or below them, a float inside a list item and one at the end of the page.
#[test]
fn floats_match_the_golden() {
    if !cfg!(windows) {
        eprintln!("golden comparison skipped: the goldens use Windows system fonts");
        return;
    }
    check("floats", "light@1.0", LIGHT, 1.0);
    check("floats", "dark@1.5", DARK, 1.5);
}
