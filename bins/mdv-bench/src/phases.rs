//! `--viewport`: what a window pays, phase by phase. The first layout (estimates only), the first
//! frame (which measures what it shows), refinement to the end in 4 ms slices, how far the
//! estimates were from the exact heights, scrolling, and then a resize, a zoom and a `<details>`
//! toggle, each to its next frame. Image decodes and engine jobs are counted per phase.

use mdview_core::model::BlockKind;
use mdview_render::{DocumentRenderer, ImageStats, Options, PageLayout};
use std::time::{Duration, Instant};

/// The slice a window gives refinement between events.
const SLICE: Duration = Duration::from_millis(4);

#[derive(Clone, Copy)]
pub struct Run {
    pub height: f32,
    pub scroll: f32,
    pub steps: usize,
    /// Logical width for the resize phase.
    pub resize: Option<f32>,
    /// Scale factor for the zoom phase.
    pub zoom: Option<f32>,
    pub details: bool,
    /// Compare the estimates with an eager layout (lays the whole document out once more).
    pub error: bool,
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn loads(before: ImageStats, after: ImageStats) -> String {
    format!(
        "{} decodes, {} jobs",
        after.decodes - before.decodes,
        after.engine_jobs - before.engine_jobs
    )
}

/// Refines to the end in window-sized slices; the time spent and the slice count.
fn refine_all(
    renderer: &mut DocumentRenderer,
    doc: &mdview_core::Document,
    options: &Options,
) -> (f64, usize) {
    let started = Instant::now();
    let mut slices = 0;
    while renderer.refine(doc, options, SLICE) {
        slices += 1;
    }
    (ms(started), slices + 1)
}

/// Lays out, draws the first frame, refines, and reports each phase. Returns the renderer for
/// the PNG of the last frame.
pub fn run(doc: &mdview_core::Document, options: &Options, run: &Run) -> DocumentRenderer {
    let mut renderer = DocumentRenderer::new();
    let stats = renderer.image_stats();
    let started = Instant::now();
    let estimated = renderer.layout(doc, options).clone();
    let layout_ms = ms(started);
    let frame = Instant::now();
    renderer.view_frame(doc, options, run.scroll, run.height);
    let frame_ms = ms(frame);
    println!(
        "first layout {layout_ms:.1} ms ({} of {} blocks estimated), first frame {frame_ms:.1} ms, \
         together {:.1} ms; {}",
        estimated.estimated_blocks(),
        estimated.blocks().len(),
        layout_ms + frame_ms,
        loads(stats, renderer.image_stats())
    );
    let stats = renderer.image_stats();
    let (refine_ms, slices) = refine_all(&mut renderer, doc, options);
    let page = renderer.page_layout().expect("laid out");
    println!(
        "refined in {refine_ms:.1} ms over {slices} slices of {} ms; page {:.0} px (estimated {:.0}); {}",
        SLICE.as_millis(),
        page.total_height(),
        estimated.total_height(),
        loads(stats, renderer.image_stats())
    );
    if run.error {
        let exact = DocumentRenderer::new().layout_exact(doc, options).clone();
        report_error(&estimated, &exact);
    }
    // Scrolling, from where the first frame was.
    let mut worst = 0.0f64;
    let mut sum = 0.0f64;
    let mut top = renderer.top().unwrap_or(run.scroll);
    for _ in 0..run.steps {
        top += 40.0;
        let started = Instant::now();
        top = renderer.view_frame(doc, options, top, run.height).top;
        let ms = ms(started);
        worst = worst.max(ms);
        sum += ms;
    }
    if run.steps > 0 {
        println!(
            "scroll frames: mean {:.2} ms, worst {worst:.2} ms over {} steps of 40 px",
            sum / run.steps as f64,
            run.steps
        );
    }
    if let Some(width) = run.resize {
        let options = Options {
            width,
            ..options.clone()
        };
        phase(&mut renderer, doc, &options, run, "resize");
        phase(&mut renderer, doc, &options, run, "resize back");
    }
    if let Some(zoom) = run.zoom {
        let options = Options {
            width: options.width / zoom,
            scale: options.scale * zoom,
            ..options.clone()
        };
        phase(&mut renderer, doc, &options, run, "zoom");
    }
    if run.details
        && let Some(block) = (0..doc.blocks.len() as u32)
            .find(|&b| matches!(doc.blocks[b as usize].kind, BlockKind::Details { .. }))
    {
        let open = !renderer.details_open(doc, block);
        let stats = renderer.image_stats();
        let top = renderer.top().unwrap_or(0.0);
        let started = Instant::now();
        renderer.set_details(doc, block, open);
        renderer.view_frame(doc, options, top, run.height);
        println!(
            "details toggle to next frame {:.1} ms; {}",
            ms(started),
            loads(stats, renderer.image_stats())
        );
    }
    renderer
}

/// A new layout for changed options, to its first frame, then refinement.
fn phase(
    renderer: &mut DocumentRenderer,
    doc: &mdview_core::Document,
    options: &Options,
    run: &Run,
    name: &str,
) {
    let stats = renderer.image_stats();
    let started = Instant::now();
    renderer.layout(doc, options);
    renderer.view_frame(doc, options, run.scroll, run.height);
    let first = ms(started);
    let to_frame = loads(stats, renderer.image_stats());
    let (refine_ms, _) = refine_all(renderer, doc, options);
    println!(
        "{name}: to first frame {first:.1} ms ({to_frame}), refined in {refine_ms:.1} ms; total {}",
        loads(stats, renderer.image_stats())
    );
}

/// How far the first layout's estimates were from the exact heights.
fn report_error(estimated: &PageLayout, exact: &PageLayout) {
    let mut sum = 0.0f64;
    let mut worst = 0.0f64;
    let mut counted = 0usize;
    for (e, x) in estimated.blocks().iter().zip(exact.blocks()) {
        if x.height <= 0.0 {
            continue;
        }
        let error = ((e.height - x.height).abs() / x.height) as f64;
        sum += error;
        worst = worst.max(error);
        counted += 1;
    }
    let mut off: Vec<(f32, u32, f32, f32)> = estimated
        .blocks()
        .iter()
        .zip(exact.blocks())
        .map(|(e, x)| ((e.height - x.height).abs(), e.block, e.height, x.height))
        .collect();
    off.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, block, e, x) in off.iter().take(5) {
        println!("  block {block}: estimated {e:.0} px, exact {x:.0} px");
    }
    let total = ((estimated.total_height() - exact.total_height()) / exact.total_height()) as f64;
    println!(
        "estimates: page {:.0} px vs exact {:.0} px ({:+.1}%); per block mean {:.1}%, worst {:.0}% over {counted} blocks",
        estimated.total_height(),
        exact.total_height(),
        total * 100.0,
        sum / counted.max(1) as f64 * 100.0,
        worst * 100.0
    );
}
