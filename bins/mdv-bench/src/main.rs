//! Renders a Markdown file to a PNG with the headless renderer. Phase 2's measurement and golden
//! harness starts here; `--all` writes the four comparison images.

use mdview_render::{DARK, LIGHT, Options, Theme, render};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let mut input = PathBuf::from("fixtures/showcase.md");
    let mut out: Option<PathBuf> = None;
    let mut width = 780.0f32;
    let mut scale = 1.0f32;
    let mut theme = "light".to_string();
    let mut max_height: Option<f32> = None;
    let mut all = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = PathBuf::from(args.next().expect("--input needs a path")),
            "--out" => out = Some(PathBuf::from(args.next().expect("--out needs a path"))),
            "--width" => {
                width = args
                    .next()
                    .expect("--width needs a number")
                    .parse()
                    .unwrap()
            }
            "--scale" => {
                scale = args
                    .next()
                    .expect("--scale needs a number")
                    .parse()
                    .unwrap()
            }
            "--theme" => theme = args.next().expect("--theme needs light or dark"),
            "--max-height" => {
                max_height = Some(
                    args.next()
                        .expect("--max-height needs a number")
                        .parse()
                        .unwrap(),
                )
            }
            "--all" => all = true,
            other => panic!("unknown argument {other}"),
        }
    }
    let bytes = std::fs::read(&input).unwrap_or_else(|e| panic!("{}: {e}", input.display()));
    let start = Instant::now();
    let doc = mdview_core::parse_bytes(&bytes);
    let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{}: {} blocks, {} spans, text {} bytes, parsed in {:.2} ms, {} warnings",
        input.display(),
        doc.blocks.len(),
        doc.spans.len(),
        doc.text.len(),
        parse_ms,
        doc.warnings.len()
    );
    let base_dir = input.parent().map(|p| p.to_path_buf());
    if all {
        for (name, theme, scale) in [
            ("light@1.0", LIGHT, 1.0),
            ("light@1.5", LIGHT, 1.5),
            ("dark@1.0", DARK, 1.0),
            ("dark@1.5", DARK, 1.5),
        ] {
            run(
                &doc,
                width,
                scale,
                theme,
                None,
                Path::new("target/goldens").join(format!("{name}.png")),
                base_dir.clone(),
            );
        }
        return;
    }
    let theme = if theme == "dark" { DARK } else { LIGHT };
    let out = out.unwrap_or_else(|| PathBuf::from("target/goldens/showcase.png"));
    run(&doc, width, scale, theme, max_height, out, base_dir);
}

fn run(
    doc: &mdview_core::Document,
    width: f32,
    scale: f32,
    theme: Theme,
    max_height: Option<f32>,
    out: PathBuf,
    base_dir: Option<PathBuf>,
) {
    let start = Instant::now();
    let rendered = render(
        doc,
        &Options {
            width,
            scale,
            theme,
            max_height,
            base_dir,
        },
    );
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    rendered
        .bitmap
        .write_png(&out)
        .unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    println!(
        "{} @{scale} {}: {}x{} px, content {:.0} px, rendered in {:.1} ms, glyphs {} hit / {} miss ({:.0}%), peak {:.1} MiB",
        theme.name,
        out.display(),
        rendered.bitmap.width,
        rendered.bitmap.height,
        rendered.content_height,
        ms,
        rendered.stats.hits,
        rendered.stats.misses,
        rendered.stats.hit_rate() * 100.0,
        rendered.stats.peak_bytes as f64 / (1024.0 * 1024.0)
    );
}
