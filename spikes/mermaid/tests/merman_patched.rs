//! End-to-end checks on the shipped route: real metrics, repair pass, resvg. Needs the fonts
//! named in pipeline::font_paths. Run with MERMAID_FONT=liberation to prove font independence.
use mermaid_spike::corpus::{all, expected_of, showcase, CORE, CORE_EXPECT, FAMILIES, HARD, PROBES, SHOWCASE_EXPECT};
use mermaid_spike::patch::{patch_svg_with, PatchOptions};
use mermaid_spike::pipeline::{text_outside_canvas, Metrics, Route, Theme};
use std::sync::OnceLock;

fn route() -> &'static Route {
    static R: OnceLock<Route> = OnceLock::new();
    R.get_or_init(|| Route::new(&std::env::var("MERMAID_FONT").unwrap_or("segoe".into())).unwrap())
}

/// Words a line-wrapped label may split across tspans, so match word by word in painted text.
fn painted_text(svg: &str) -> String {
    fn walk(g: &resvg::usvg::Group, out: &mut String) {
        for n in g.children() {
            match n {
                resvg::usvg::Node::Group(g) => walk(g, out),
                resvg::usvg::Node::Text(t) => {
                    for c in t.chunks() {
                        out.push_str(c.text());
                        out.push(' ');
                    }
                }
                _ => {}
            }
        }
    }
    let tree = route().tree(svg).unwrap();
    let mut s = String::new();
    walk(tree.root(), &mut s);
    s
}

fn assert_words(name: &str, svg: &str, expected: &[&str]) {
    let painted = painted_text(svg);
    for phrase in expected {
        for word in phrase.split_whitespace() {
            assert!(painted.contains(word), "{name}: `{word}` of `{phrase}` is not painted\n{painted}");
        }
    }
}

#[test]
fn showcase_paints_every_label() {
    for (i, src) in showcase().iter().enumerate() {
        let svg = route().svg(src, Theme::Light, None).unwrap();
        assert_words(FAMILIES[i], &svg, SHOWCASE_EXPECT[i]);
    }
}

#[test]
fn core_tests_paint_every_label() {
    for (i, src) in CORE.iter().enumerate() {
        let svg = route().svg(src, Theme::Light, None).unwrap();
        assert_words(FAMILIES[i], &svg, CORE_EXPECT[i]);
    }
}

#[test]
fn hard_corpus_paints_every_label() {
    let mut failed = vec![];
    for (name, src) in HARD.iter().chain(PROBES.iter()).copied() {
        match route().svg(src, Theme::Light, None) {
            Ok(svg) => assert_words(name, &svg, &expected_of(src)),
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

#[test]
fn output_has_no_clipping_style_and_no_negative_stroke() {
    for (name, src) in all() {
        let svg = route().svg(&src, Theme::Light, None).unwrap();
        assert!(!svg.contains("overflow:hidden"), "{name}");
        assert!(!svg.contains("stroke-width:-"), "{name}");
    }
}

#[test]
fn no_text_is_cut_off_at_the_canvas_edge() {
    let mut bad = vec![];
    for (name, src) in all() {
        let svg = route().svg(&src, Theme::Light, None).unwrap();
        let out = text_outside_canvas(&route().tree(&svg).unwrap());
        if !out.is_empty() {
            bad.push((name, out));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// With real metrics the label boxes already fit, so on the small sets removing the clip changes
/// nothing a person could see (blending rounding only). The hard set keeps a little ink past the
/// box edge, which is why the repair stays; see RESULT.md.
#[test]
fn real_metrics_alone_leave_nothing_to_unclip() {
    let mut bad = vec![];
    for (name, src) in all().into_iter().filter(|(n, _)| n.starts_with("showcase-") || n.starts_with("core-")) {
        let raw = route().raw_svg(&src, Metrics::Real, Theme::Light, None).unwrap();
        let clipped = patch_svg_with(&raw, PatchOptions::STROKE_ONLY);
        let open = patch_svg_with(&raw, PatchOptions::FULL);
        let a = route().raster(&clipped, 2.0, Theme::Light).unwrap();
        let b = route().raster(&open, 2.0, Theme::Light).unwrap();
        let worst = a.data().iter().zip(b.data()).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        // Rounding is up to 2 on Segoe UI; Liberation reaches 14 on one glyph edge pixel.
        if worst > 16 {
            bad.push((name, worst));
        }
    }
    assert!(bad.is_empty(), "labels still rely on overflow:visible: {bad:?}");
}

#[test]
fn deterministic_metrics_do_clip_so_the_check_above_means_something() {
    let src = &showcase()[2];
    let raw = route().raw_svg(src, Metrics::Deterministic, Theme::Light, None).unwrap();
    let a = route().raster(&patch_svg_with(&raw, PatchOptions::STROKE_ONLY), 2.0, Theme::Light).unwrap();
    let b = route().raster(&patch_svg_with(&raw, PatchOptions::FULL), 2.0, Theme::Light).unwrap();
    assert_ne!(a.data(), b.data());
}

#[test]
fn gantt_gutter_grows_only_for_long_section_titles() {
    assert!(!route().prepare_source("gantt\n section Work\n T :a, 2026-01-01, 1d").contains("leftPadding"));
    assert!(route().prepare_source("gantt\n section A long section title for a gantt chart").contains("leftPadding"));
    assert!(!route().prepare_source("flowchart LR\n section A long section title for a gantt chart").contains("leftPadding"));
}

#[test]
fn huge_label_is_refused_before_it_reaches_resvg() {
    let src = format!("flowchart LR\n A[{}] --> B", "x".repeat(1024 * 1024));
    let t = std::time::Instant::now();
    let e = route().svg(&src, Theme::Light, None).unwrap_err();
    assert!(e.contains("source line"), "{e}");
    assert!(t.elapsed().as_millis() < 50);
}

#[test]
fn tall_diagram_steps_down_to_the_pixel_cap() {
    let src = all().into_iter().find(|(n, _)| n == "probe-flow-150").unwrap().1;
    let svg = route().svg(&src, Theme::Light, None).unwrap();
    let pm = route().raster(&svg, 2.0, Theme::Light).unwrap();
    assert!(u64::from(pm.width()) * u64::from(pm.height()) <= mermaid_spike::pipeline::MAX_RASTER_PIXELS + 20_000);
}

#[test]
fn mindmap_edge_past_depth_five_is_stroked() {
    let raw = route().raw_svg(&showcase()[7], Metrics::Real, Theme::Light, None).unwrap();
    assert!(raw.contains("stroke-width:-1"), "merman fixed this upstream; the repair can go");
    let patched = patch_svg_with(&raw, PatchOptions::FULL);
    assert!(!patched.contains("stroke-width:-"));
}

#[test]
fn measurement_matches_the_painted_width() {
    use merman::svg::TextStyle;
    let style = TextStyle { font_size: 16.0, ..Default::default() };
    for label in ["Parse Markdown", "Görüntüleyici", "WAVE AVATAR", "iiiiiiii", "MMMMMMMM"] {
        let measured = route().measurer().line_width(label, &style);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="900" height="60"><text x="10" y="30" font-size="16" font-family="sans-serif">{label}</text></svg>"#
        );
        let tree = route().tree(&svg).unwrap();
        let usvg_width = first_text_advance(tree.root()).unwrap();
        // The ink box differs from the advance only by the two outer side bearings.
        assert!((measured as f32 - usvg_width).abs() < 3.0, "{label}: measured {measured}, painted {usvg_width}");
    }
}

fn first_text_advance(g: &resvg::usvg::Group) -> Option<f32> {
    for n in g.children() {
        match n {
            resvg::usvg::Node::Group(g) => {
                if let Some(w) = first_text_advance(g) {
                    return Some(w);
                }
            }
            resvg::usvg::Node::Text(t) => return Some(t.bounding_box().width()),
            _ => {}
        }
    }
    None
}
