//! Pixel-level smoke tests for the headless renderer. The golden images are approved by eye; these
//! catch a crash, a blank page, a lost theme and a lost clamp.

use mdview_render::{DARK, LIGHT, Options, render};
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
