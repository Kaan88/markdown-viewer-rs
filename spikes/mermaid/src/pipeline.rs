//! merman source -> SVG -> repair -> resvg PNG, with Segoe UI (or a stand-in) as the only faces.
use crate::metrics::{Face, Faces, FontMeasurer};
use crate::patch::{patch_svg_with, PatchOptions};
use merman::svg::{
    CssOverridePolicy, HostTheme, HostThemeAppearance, MeasurementProfileId, Presentation, SvgEnvironment,
    SvgOutputPolicy, SvgPipeline, SvgPipelinePreset, TextMeasurementPolicy, TextMeasurementProfile,
    TextMeasurementProfileIdentity, ThemeRole,
};
use merman::{Engine, OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use resvg::tiny_skia::{Color, Pixmap, Transform};
use resvg::usvg;
use merman::svg::TextStyle;
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

/// usvg's text layout is what blows up on a huge label (merman is done in milliseconds): 64 KiB
/// took 2.9 s and 595 MiB, 256 KiB did not finish in 20 s. Real labels are far under this.
pub const MAX_SOURCE_LINE_BYTES: usize = 4096;
/// 4000 x 4000, or a 64 MiB pixmap.
pub const MAX_RASTER_PIXELS: u64 = 16_000_000;
/// Below half size a 14 px label stops being readable, so show the source instead.
pub const MIN_RASTER_SCALE: f32 = 0.5;
const GANTT_SECTION_FONT_PX: f64 = 11.0;
const GANTT_TITLE_INSET_PX: f64 = 10.0;
/// A hostile title must not squeeze the chart away. The default canvas is 800 px.
const GANTT_MAX_GUTTER_PX: f64 = 300.0;

/// Regular, bold, italic, bold italic. The first directory that exists wins, so this runs
/// unchanged on Windows 11. Italic matters: class diagrams mark abstract members with it.
pub fn font_paths(name: &str) -> Result<[String; 4], String> {
    let (dirs, files): (&[&str], [&str; 4]) = match name {
        "segoe" => (
            &["C:/Windows/Fonts", "/usr/local/share/fonts/WindowsFonts"],
            ["segoeui.ttf", "segoeuib.ttf", "segoeuii.ttf", "segoeuiz.ttf"],
        ),
        "liberation" => (
            &["/usr/share/fonts/liberation"],
            ["LiberationSans-Regular.ttf", "LiberationSans-Bold.ttf", "LiberationSans-Italic.ttf", "LiberationSans-BoldItalic.ttf"],
        ),
        o => return Err(format!("unknown font set {o}")),
    };
    let dir = dirs
        .iter()
        .find(|d| std::path::Path::new(d).join(files[0]).exists())
        .ok_or(format!("no font directory for {name}"))?;
    Ok(files.map(|f| format!("{dir}/{f}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metrics {
    /// merman's default estimate.
    Deterministic,
    /// The painted font, shaped.
    Real,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn canvas(self) -> Color {
        match self {
            Theme::Light => Color::WHITE,
            Theme::Dark => Color::from_rgba8(0x20, 0x20, 0x20, 0xff),
        }
    }
}

/// Windows 11 dark surfaces. Transparent root: the viewer paints the document background.
fn dark_theme() -> HostTheme {
    HostTheme::new()
        .with_appearance(HostThemeAppearance::Dark)
        .try_with_role(ThemeRole::Canvas, "#202020").unwrap()
        .try_with_role(ThemeRole::Surface, "#2d2d2d").unwrap()
        .try_with_role(ThemeRole::SurfaceAlt, "#383838").unwrap()
        .try_with_role(ThemeRole::Text, "#f0f0f0").unwrap()
        .try_with_role(ThemeRole::Border, "#8a8a8a").unwrap()
        .try_with_role(ThemeRole::Line, "#c0c0c0").unwrap()
        .try_with_role(ThemeRole::NoteBackground, "#4a3f1d").unwrap()
        .try_with_role(ThemeRole::NoteBorder, "#d6b656").unwrap()
        .try_with_role(ThemeRole::NoteText, "#fff4cc").unwrap()
        .try_with_series_palette(["#60a5fa", "#34d399", "#f59e0b", "#f472b6", "#a78bfa", "#22d3ee", "#fb7185", "#a3e635"])
        .unwrap()
}

pub struct Route {
    measurer: Arc<FontMeasurer>,
    pub usvg_opt: usvg::Options<'static>,
    light: Renderer,
    dark: Renderer,
    dark_presentation: merman::svg::PresentationRenderPolicy,
}

impl Route {
    /// Extra fallback faces come from MERMAID_FALLBACK (`;`-separated paths), for the script probe.
    pub fn new(font_set: &str) -> Result<Self, String> {
        let paths = font_paths(font_set)?;
        let fallback: Vec<String> = std::env::var("MERMAID_FALLBACK")
            .map(|v| v.split(';').filter(|p| !p.is_empty()).map(str::to_owned).collect())
            .unwrap_or_default();
        Self::from_files(&paths, &fallback)
    }

    /// `fallback` faces are measured and painted for characters the primary faces lack.
    pub fn from_files(paths: &[String; 4], fallback: &[String]) -> Result<Self, String> {
        let read = |p: &str| std::fs::read(p).map(Arc::new).map_err(|e| format!("{p}: {e}"));
        let mut usvg_opt = usvg::Options::default();
        let db = usvg_opt.fontdb_mut();
        let mut load = |p: &str| -> Result<Face, String> {
            let d = read(p)?;
            db.load_font_source(fontdb_source(d.clone()));
            Face::from_data(d)
        };
        let faces = Faces {
            regular: load(&paths[0])?,
            bold: load(&paths[1])?,
            italic: load(&paths[2])?,
            bold_italic: load(&paths[3])?,
        };
        let mut fb = Vec::new();
        for p in fallback {
            fb.push(load(p)?);
        }
        let family = db.faces().next().and_then(|f| f.families.first()).map(|f| f.0.clone()).ok_or("no family")?;
        db.set_sans_serif_family(family.clone());
        usvg_opt.font_family = family;
        let measurer = Arc::new(FontMeasurer::new(faces, fb));
        let dark = Presentation::new().with_theme(dark_theme()).resolve();
        Ok(Self {
            measurer,
            usvg_opt,
            light: Renderer::new(),
            dark: Renderer::new().with_engine(dark.materialize_engine(Engine::new())),
            dark_presentation: dark.render_policy(),
        })
    }

    pub fn measurer(&self) -> &FontMeasurer {
        &self.measurer
    }

    fn environment(&self, m: Metrics) -> SvgEnvironment {
        let env = SvgEnvironment::deterministic();
        if m == Metrics::Deterministic {
            return env;
        }
        let id = MeasurementProfileId::new("mdview-font").expect("static id");
        let identity = TextMeasurementProfileIdentity::new(id, "1").expect("static id");
        let profile = TextMeasurementProfile::new(identity, self.measurer.clone());
        env.with_text_measurement_policy(TextMeasurementPolicy::uniform(profile))
    }

    /// Gantt section titles sit in a fixed 75 px left gutter and run under the bars when longer.
    /// merman reads `gantt.leftPadding` from the init directive, so size the gutter from the real
    /// width of the longest title. Other families pass through untouched.
    pub fn prepare_source<'a>(&self, src: &'a str) -> Cow<'a, str> {
        let is_gantt = src
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("%%"))
            .is_some_and(|l| l == "gantt" || l.starts_with("gantt "));
        if !is_gantt {
            return Cow::Borrowed(src);
        }
        let style = TextStyle { font_size: GANTT_SECTION_FONT_PX, ..Default::default() };
        let longest = src
            .lines()
            .filter_map(|l| l.trim_start().strip_prefix("section "))
            .map(|t| self.measurer.line_width(t.trim(), &style))
            .fold(0.0f64, f64::max);
        // Title starts at x=10; leave the same again before the first bar.
        let gutter = (longest + GANTT_TITLE_INSET_PX * 2.0).ceil().clamp(75.0, GANTT_MAX_GUTTER_PX);
        if gutter <= 75.0 {
            return Cow::Borrowed(src);
        }
        Cow::Owned(format!("%%{{init: {{\"gantt\": {{\"leftPadding\": {gutter}}}}}}}%%\n{src}"))
    }

    /// Source to SVG, before the repair pass.
    pub fn raw_svg(&self, src: &str, m: Metrics, theme: Theme, deadline: Option<Duration>) -> Result<String, String> {
        if let Some(n) = src.lines().map(str::len).find(|n| *n > MAX_SOURCE_LINE_BYTES) {
            return Err(format!("source line of {n} bytes is over {MAX_SOURCE_LINE_BYTES}"));
        }
        let src = self.prepare_source(src);
        let src = src.as_ref();
        let mut req = SvgRequest { environment: self.environment(m), ..Default::default() };
        let mut control = OperationControl::new();
        if let Some(d) = deadline {
            control = control.with_deadline(d);
        }
        let renderer = match theme {
            Theme::Light => {
                req.pipeline = Some(SvgPipeline::resvg_safe());
                &self.light
            }
            Theme::Dark => {
                req.pipeline = Some(
                    SvgOutputPolicy {
                        preset: SvgPipelinePreset::ResvgSafe,
                        css_override_policy: CssOverridePolicy::StripExistingImportant,
                        root_background_color: None,
                        ..SvgOutputPolicy::default()
                    }
                    .pipeline(),
                );
                req.presentation = self.dark_presentation.clone();
                &self.dark
            }
        };
        match renderer.render(RenderRequest::svg(src, control, req)) {
            Ok(RenderOutput::Svg(Some(s))) => Ok(s.svg().to_string()),
            Ok(_) => Err("no diagram detected".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// The route the engine would ship: real metrics plus the full repair.
    pub fn svg(&self, src: &str, theme: Theme, deadline: Option<Duration>) -> Result<String, String> {
        let raw = self.raw_svg(src, Metrics::Real, theme, deadline)?;
        Ok(patch_svg_with(&raw, PatchOptions::FULL).into_owned())
    }

    pub fn tree(&self, svg: &str) -> Result<usvg::Tree, String> {
        usvg::Tree::from_str(svg, &self.usvg_opt).map_err(|e| e.to_string())
    }

    /// Paints at `scale`, stepping down to fit MAX_RASTER_PIXELS, but never under MIN_RASTER_SCALE.
    pub fn raster(&self, svg: &str, scale: f32, theme: Theme) -> Result<Pixmap, String> {
        let tree = self.tree(svg)?;
        let sz = tree.size();
        let fit = (MAX_RASTER_PIXELS as f64 / (f64::from(sz.width()) * f64::from(sz.height()))).sqrt() as f32;
        let used = scale.min(fit);
        if used < MIN_RASTER_SCALE {
            return Err(format!("raster over pixel cap: {}x{} at {scale}x", sz.width(), sz.height()));
        }
        let (w, h) = ((sz.width() * used).ceil() as u32, (sz.height() * used).ceil() as u32);
        let mut pm = Pixmap::new(w, h).ok_or(format!("pixmap alloc failed: {w}x{h}"))?;
        pm.fill(theme.canvas());
        resvg::render(&tree, Transform::from_scale(used, used), &mut pm.as_mut());
        Ok(pm)
    }
}

fn fontdb_source(d: Arc<Vec<u8>>) -> usvg::fontdb::Source {
    usvg::fontdb::Source::Binary(d)
}

/// Every painted text node's box must lie inside the canvas: the check that catches labels cut
/// off at the image edge. Returns the offending text and overhang in px.
pub fn text_outside_canvas(tree: &usvg::Tree) -> Vec<(String, f32)> {
    fn walk(g: &usvg::Group, out: &mut Vec<usvg::Text>) {
        for n in g.children() {
            match n {
                usvg::Node::Group(g) => walk(g, out),
                usvg::Node::Text(t) => out.push((**t).clone()),
                _ => {}
            }
        }
    }
    let mut texts = Vec::new();
    walk(tree.root(), &mut texts);
    let (w, h) = (tree.size().width(), tree.size().height());
    let mut bad = Vec::new();
    for t in texts {
        let b = t.abs_stroke_bounding_box();
        let over = (-b.left()).max(-b.top()).max(b.right() - w).max(b.bottom() - h);
        if over > 1.0 {
            let s = t.chunks().iter().map(|c| c.text().to_owned()).collect::<Vec<_>>().join("");
            bad.push((s, over));
        }
    }
    bad
}
