//! Renders a Markdown file to a PNG with the headless renderer. Phase 2's measurement and golden
//! harness starts here; `--all` writes the four comparison images, and `--viewport` renders only a
//! scrolled slice for the 1 MB budget.

use mdview_render::{DARK, DocumentRenderer, LIGHT, Options, Theme, render};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let mut input = PathBuf::from("fixtures/showcase.md");
    let mut out: Option<PathBuf> = None;
    let mut width = 780.0f32;
    let mut scale = 1.0f32;
    let mut theme = "light".to_string();
    let mut max_height: Option<f32> = None;
    let mut scroll: Option<f32> = None;
    let mut viewport: Option<f32> = None;
    let mut repeat: usize = 1;
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
            "--scroll" => {
                scroll = Some(
                    args.next()
                        .expect("--scroll needs a number")
                        .parse()
                        .unwrap(),
                )
            }
            "--viewport" => {
                viewport = Some(
                    args.next()
                        .expect("--viewport needs a number")
                        .parse()
                        .unwrap(),
                )
            }
            "--repeat" => {
                repeat = args
                    .next()
                    .expect("--repeat needs a count")
                    .parse()
                    .unwrap()
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
                None,
                None,
                1,
            );
        }
        return;
    }
    let theme = if theme == "dark" { DARK } else { LIGHT };
    let out = out.unwrap_or_else(|| PathBuf::from("target/goldens/showcase.png"));
    run(
        &doc, width, scale, theme, max_height, out, base_dir, scroll, viewport, repeat,
    );
}

#[allow(clippy::too_many_arguments)]
fn run(
    doc: &mdview_core::Document,
    width: f32,
    scale: f32,
    theme: Theme,
    max_height: Option<f32>,
    out: PathBuf,
    base_dir: Option<PathBuf>,
    scroll: Option<f32>,
    viewport: Option<f32>,
    repeat: usize,
) {
    let options = Options {
        width,
        scale,
        theme,
        max_height,
        base_dir,
    };
    let start = Instant::now();
    let (bitmap, total_height, stats, mode) = match viewport {
        Some(height) => {
            let mut renderer = DocumentRenderer::new();
            renderer.layout(doc, &options);
            let base = scroll.unwrap_or(0.0);
            let first = Instant::now();
            renderer.view_frame(doc, &options, base, height);
            let first_ms = first.elapsed().as_secs_f64() * 1000.0;
            let mut worst = 0.0f64;
            let mut sum = 0.0f64;
            let mut frames = 0usize;
            for frame in 1..repeat.max(1) {
                let top = base + frame as f32 * 40.0;
                let started = Instant::now();
                renderer.view_frame(doc, &options, top, height);
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                worst = worst.max(ms);
                sum += ms;
                frames += 1;
            }
            println!(
                "height pass {first_ms:.1} ms; scroll frames worst {worst:.1} ms, mean {:.1} ms over {frames}",
                sum / frames.max(1) as f64
            );
            let images = renderer.image_stats();
            println!(
                "images: {} decodes, {} engine jobs, {:.1} MiB held",
                images.decodes,
                images.engine_jobs,
                images.bytes as f64 / (1024.0 * 1024.0)
            );
            let frame = renderer.frame().expect("frame");
            (
                frame.bitmap.clone(),
                renderer.page_layout().expect("laid out").total_height(),
                frame.stats,
                format!(" scroll {:.0}..{:.0}", frame.top, frame.top + height),
            )
        }
        None => {
            let rendered = render(doc, &options);
            (
                rendered.bitmap,
                rendered.content_height,
                rendered.stats,
                String::new(),
            )
        }
    };
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    bitmap
        .write_png(&out)
        .unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    println!(
        "{} @{scale}{mode} {}: {}x{} px, page {:.0} px, rendered in {:.1} ms, glyphs {} hit / {} miss ({:.0}%), peak {:.1} MiB",
        theme.name,
        out.display(),
        bitmap.width,
        bitmap.height,
        total_height,
        ms,
        stats.hits,
        stats.misses,
        stats.hit_rate() * 100.0,
        stats.peak_bytes as f64 / (1024.0 * 1024.0)
    );
}
