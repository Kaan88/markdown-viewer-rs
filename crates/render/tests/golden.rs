//! Pixel comparison against the approved showcase images in `tests/golden/`.
//!
//! The goldens use this host's system fonts (Segoe UI, Consolas), so they are compared only on
//! Windows; elsewhere the test passes with a note until the renderer bundles its fonts. The
//! tolerance absorbs anti-aliasing noise between driver and font-cache states, not layout changes:
//! a moved line or a lost image changes far more pixels than it allows.
//!
//! After an intended visual change, regenerate with `MDV_BLESS=1 cargo test --release -p
//! mdview-render --test golden` and review the new images before committing them.

use mdview_render::{DARK, LIGHT, Options, Theme, render};
use std::path::{Path, PathBuf};

/// A pixel counts as different when any channel differs by more than this.
const CHANNEL_TOLERANCE: u8 = 24;
/// At most this share of pixels may differ.
const MAX_DIFFERENT: f64 = 0.001;

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

fn check(name: &str, theme: Theme, scale: f32) {
    let showcase = root().join("fixtures/showcase.md");
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
    let golden = root().join(format!("tests/golden/showcase-{name}.png"));
    if std::env::var_os("MDV_BLESS").is_some() {
        rendered.bitmap.write_png(&golden).unwrap();
        return;
    }
    let (width, height, expected) = read_rgb(&golden);
    let bitmap = &rendered.bitmap;
    assert_eq!(
        (bitmap.width, bitmap.height),
        (width, height),
        "{name}: page size changed"
    );
    let mut different = 0usize;
    let mut first = None;
    for (i, (actual, expected)) in bitmap
        .pixels
        .chunks_exact(4)
        .zip(expected.chunks_exact(3))
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
    if share > MAX_DIFFERENT {
        let out = root().join(format!("target/golden-actual-{name}.png"));
        _ = bitmap.write_png(&out);
        panic!(
            "{name}: {different} pixels ({:.3}%) differ, first at {first:?}; actual written to {}",
            share * 100.0,
            out.display()
        );
    }
}

#[test]
fn showcase_matches_the_goldens() {
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
        check(name, theme, scale);
    }
}
