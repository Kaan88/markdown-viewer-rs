//! Lays the document model out in one top-down pass and paints it into a bitmap: no selection,
//! anchors or scroll cache yet. Images are drawn through parley inline boxes; math and diagrams
//! still show their source. Virtualized layout comes next.

use crate::bitmap::Bitmap;
use crate::image::{self, DecodedImage};
use crate::text::{Brush, TextEngine};
use crate::theme::{Color, DARK, LIGHT, Theme};
use mdview_core::model::{
    AlertKind, Align, BlockKind, Document, Footnote, ListMarker, NONE, SpanKind,
};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontStyle, FontWeight, InlineBox,
    InlineBoxKind, Layout, LayoutContext, LineHeight, PositionedLayoutItem, StyleProperty,
    VerticalAlign,
};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

pub struct Options {
    /// Logical page width, before the device scale.
    pub width: f32,
    pub scale: f32,
    pub theme: Theme,
    /// Stop laying out past this many logical px. The bitmap is clipped to it.
    pub max_height: Option<f32>,
    /// The directory the document came from, so relative image paths resolve. `None` leaves
    /// images as placeholders.
    pub base_dir: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            width: 780.0,
            scale: 1.0,
            theme: LIGHT,
            max_height: None,
            base_dir: None,
        }
    }
}

impl Options {
    pub fn light() -> Options {
        Options {
            theme: LIGHT,
            ..Options::default()
        }
    }

    pub fn dark() -> Options {
        Options {
            theme: DARK,
            ..Options::default()
        }
    }
}

/// One top-level block's vertical extent in device px, for scrolling and hit tests later.
#[derive(Debug, Clone, Copy)]
pub struct BlockBox {
    pub block: u32,
    pub y: f32,
    pub height: f32,
}

pub struct Rendered {
    pub bitmap: Bitmap,
    /// The height the document needed, in device px, before any `max_height` clamp.
    pub content_height: f32,
    pub blocks: Vec<BlockBox>,
    pub stats: crate::text::CacheStats,
}

/// A viewport-sized piece of a document. `top` is the absolute device-px row the bitmap starts at,
/// and `total_height` is the whole page, so the caller can size a scrollbar.
pub struct Slice {
    pub bitmap: Bitmap,
    pub top: f32,
    pub total_height: f32,
    pub stats: crate::text::CacheStats,
}

/// A viewport-sized bitmap kept between frames. Scrolling shifts the rows that stay on screen and
/// repaints only the band that came into view, so a frame costs a band rather than a viewport.
///
/// [`band`](Self::band) and [`shift`](Self::shift) describe the difference from the previous frame,
/// which is what lets a window send only that difference to the GPU: the rows outside the band moved
/// by `shift` rows, and the band rows are new. A caller that has lost its copy of the previous frame
/// (a resize, a new device) must draw the whole frame; `band` is `None` in that case, and also for
/// the first frame and for any jump of a viewport or more.
pub struct Frame {
    pub bitmap: Bitmap,
    /// Absolute device-px row the bitmap's first row shows.
    pub top: f32,
    pub stats: crate::text::CacheStats,
    /// Rows of `bitmap`, in bitmap rows, that were painted this frame. An empty range means the
    /// frame is unchanged. `None` means the whole frame is new and nothing can be reused.
    pub band: Option<(u32, u32)>,
    /// Rows the content moved: row `r` of this frame was row `r + shift` of the previous one.
    pub shift: i32,
}

/// Every top-level block's measured vertical extent, without any painting. Build it once per
/// width/scale/theme and reuse it for scroll frames; a 1 MB document costs one height pass.
pub struct PageLayout {
    spans: Vec<BlockBox>,
    footnotes_y: f32,
    footnotes_height: f32,
    total_height: f32,
    width: f32,
    scale: f32,
    theme: &'static str,
}

impl PageLayout {
    pub fn total_height(&self) -> f32 {
        self.total_height
    }

    pub fn matches(&self, options: &Options) -> bool {
        self.width == options.width
            && (self.scale - options.scale).abs() < 0.001
            && self.theme == options.theme.name
    }
}

/// Measures the whole document without painting it.
pub fn layout(doc: &Document, options: &Options) -> PageLayout {
    DocumentRenderer::new().layout(doc, options)
}

/// Renders only the part of the document that reaches `top..top + height`, given a layout made
/// earlier for the same width, scale and theme. Blocks above and below are not touched, so a scroll
/// frame costs one viewport of bitmap and one viewport of text layout.
pub fn render_view(
    doc: &Document,
    options: &Options,
    page: &PageLayout,
    top: f32,
    height: f32,
) -> Slice {
    DocumentRenderer::new().view(doc, options, page, top, height)
}

/// Holds the font and layout contexts and the glyph cache across frames, so scrolling a document
/// does not rebuild them (and keeps the glyph cache warm). One per window, or per benchmark.
pub struct DocumentRenderer {
    fonts: FontContext,
    layouts: LayoutContext<Brush>,
    engine: TextEngine,
    /// Laid-out paints per top-level block, relative to the block's top, for scroll frames.
    cache: HashMap<u32, Vec<Paint>>,
    cache_key: Option<(f32, f32, &'static str)>,
    /// Decoded images, shared by every frame so a cached paint can still find its pixels.
    images: HashMap<u32, Option<Rc<DecodedImage>>>,
    /// The frame [`view_frame`](Self::view_frame) keeps between calls.
    frame: Option<Frame>,
}

impl Default for DocumentRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentRenderer {
    pub fn new() -> Self {
        DocumentRenderer {
            fonts: FontContext::new(),
            layouts: LayoutContext::new(),
            engine: TextEngine::new(4 << 20),
            cache: HashMap::new(),
            cache_key: None,
            images: HashMap::new(),
            frame: None,
        }
    }

    /// Measures the whole document. Do this once per width, scale and theme.
    pub fn layout(&mut self, doc: &Document, options: &Options) -> PageLayout {
        self.cache.clear();
        self.images.clear();
        self.frame = None;
        self.cache_key = Some((options.width, options.scale, options.theme.name));
        Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            None,
            &mut self.images,
        )
        .measure_page()
    }

    /// Renders one viewport slice, reusing the contexts and glyph cache.
    pub fn view(
        &mut self,
        doc: &Document,
        options: &Options,
        page: &PageLayout,
        top: f32,
        height: f32,
    ) -> Slice {
        debug_assert!(page.matches(options), "page layout for different options");
        let height = height.max(1.0);
        let top = top.clamp(0.0, (page.total_height - height).max(0.0));
        let key = (options.width, options.scale, options.theme.name);
        if self.cache_key != Some(key) {
            self.cache.clear();
            self.images.clear();
            self.cache_key = Some(key);
            self.frame = None;
        }
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            Some(&mut self.cache),
            &mut self.images,
        );
        pager.slice_page(page, top, height);
        pager.finish_slice(top, height, page.total_height, &mut self.engine)
    }

    /// Draws the viewport into a frame kept between calls. When the new view still overlaps the last
    /// one, those rows are shifted in place and only the band that came into view is painted.
    pub fn view_frame(
        &mut self,
        doc: &Document,
        options: &Options,
        page: &PageLayout,
        top: f32,
        height: f32,
    ) -> &Frame {
        debug_assert!(page.matches(options), "page layout for different options");
        let height = height.max(1.0);
        let top = top
            .clamp(0.0, (page.total_height - height).max(0.0))
            .round();
        let key = (options.width, options.scale, options.theme.name);
        if self.cache_key != Some(key) {
            self.cache.clear();
            self.images.clear();
            self.cache_key = Some(key);
            self.frame = None;
        }
        let background = options.theme.bg;
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            Some(&mut self.cache),
            &mut self.images,
        );
        let rows = height.ceil().max(1.0) as u32;
        let width = pager.width.round().max(1.0) as u32;
        if let Some(frame) = self.frame.as_ref()
            && frame.bitmap.width == width
            && frame.bitmap.height == rows
            && frame.top == top
        {
            // Nothing changed, so there is nothing for the caller to send again.
            let frame = self.frame.as_mut().expect("frame");
            frame.band = Some((0, 0));
            frame.shift = 0;
            return self.frame.as_ref().expect("frame");
        }
        let mut band = (top, rows as f32);
        let mut shift = 0i32;
        let mut whole = true;
        let same_size = self
            .frame
            .as_ref()
            .is_some_and(|frame| frame.bitmap.width == width && frame.bitmap.height == rows);
        if same_size {
            let frame = self.frame.as_mut().expect("frame");
            let dy = frame.top as i64 - top as i64;
            if dy.unsigned_abs() < rows as u64 {
                shift_rows(&mut frame.bitmap, dy as i32);
                // `dy` is old top minus new top: positive when the view scrolled up and the newly
                // exposed rows are at the top, negative when it scrolled down and they are at the
                // bottom.
                band = if dy > 0 {
                    (top, dy as f32)
                } else {
                    (top + rows as f32 + dy as f32, (-dy) as f32)
                };
                // Row `r` of the new frame was row `r - dy` of the old one.
                shift = -(dy as i32);
                whole = false;
            } else {
                frame.bitmap.clear(background);
            }
            frame.top = top;
        } else {
            let mut bitmap = Bitmap::new(width, rows);
            bitmap.clear(background);
            self.frame = Some(Frame {
                bitmap,
                top,
                stats: crate::text::CacheStats::default(),
                band: None,
                shift: 0,
            });
            band = (top, rows as f32);
        }
        let frame_top = self.frame.as_ref().expect("frame").top;
        // Blocks can reach past the band, so draw the band on its own bitmap and copy it in;
        // otherwise the overlap would be blended twice.
        let stats = if band.1 + 0.5 >= rows as f32 && (band.0 - frame_top).abs() < 0.5 {
            pager.slice_page(page, band.0, band.1);
            let frame = self.frame.as_mut().expect("frame");
            pager.draw(&mut frame.bitmap, frame_top, &mut self.engine)
        } else {
            let mut band_bitmap = Bitmap::new(width, band.1.ceil().max(1.0) as u32);
            band_bitmap.clear(background);
            pager.slice_page(page, band.0, band.1);
            let stats = pager.draw(&mut band_bitmap, band.0, &mut self.engine);
            let frame = self.frame.as_mut().expect("frame");
            copy_rows(
                &mut frame.bitmap,
                &band_bitmap,
                (band.0 - frame_top) as usize,
            );
            stats
        };
        let first = (band.0 - frame_top).round().max(0.0) as u32;
        let band_rows = (first, band.1.ceil().max(0.0) as u32);
        let frame = self.frame.as_mut().expect("frame");
        frame.stats = stats;
        frame.band = (!whole).then_some(band_rows);
        frame.shift = shift;
        self.frame.as_ref().expect("frame")
    }

    /// The frame [`view_frame`](Self::view_frame) drew last, if any.
    pub fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }

    /// Paints the whole page into one bitmap. For tests and small documents; scrolling uses
    /// [`layout`](Self::layout) and [`view`](Self::view).
    pub fn page(&mut self, doc: &Document, options: &Options) -> Rendered {
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            None,
            &mut self.images,
        );
        pager.page();
        pager.finish(&mut self.engine)
    }
}

/// One-shot form of [`layout`] plus [`render_view`].
pub fn render_slice(doc: &Document, options: &Options, top: f32, height: f32) -> Slice {
    let page = layout(doc, options);
    render_view(doc, options, &page, top, height)
}

pub fn render(doc: &Document, options: &Options) -> Rendered {
    DocumentRenderer::new().page(doc, options)
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    code: bool,
    kbd: bool,
    math: bool,
    link: bool,
    muted: bool,
    accent: bool,
    small: bool,
    sub: bool,
    sup: bool,
}

impl Style {
    fn size_factor(self) -> f32 {
        if self.code || self.kbd || self.math {
            0.92
        } else if self.small {
            0.78
        } else if self.sub || self.sup {
            0.8
        } else {
            1.0
        }
    }
}

#[derive(Clone, Default)]
struct Inline {
    text: String,
    runs: Vec<(Range<usize>, Style)>,
    /// A task-list checkbox at the start of the content, drawn before the text.
    task: Option<bool>,
    /// Images placed inline, at their byte offset in `text`.
    boxes: Vec<ImageBox>,
}

/// One inline image: its text offset, the image it draws and its natural device-px size.
#[derive(Clone, Copy)]
struct ImageBox {
    at: usize,
    index: u32,
    width: f32,
    height: f32,
}

impl Inline {
    fn push(&mut self, s: &str, style: Style) {
        if s.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(s);
        let end = self.text.len();
        if let Some((range, last)) = self.runs.last_mut()
            && *last == style
            && range.end == start
        {
            range.end = end;
            return;
        }
        self.runs.push((start..end, style));
    }

    fn style_at(&self, at: usize) -> Style {
        self.runs
            .iter()
            .find(|(range, _)| range.start <= at && at < range.end)
            .map_or(Style::default(), |(_, style)| *style)
    }
}

#[derive(Clone)]
enum Paint {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        color: Color,
    },
    Line {
        a: (f32, f32),
        b: (f32, f32),
        thickness: f32,
        color: Color,
    },
    Text {
        layout: Rc<Layout<Brush>>,
        x: f32,
        y: f32,
    },
    Image {
        index: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

impl Paint {
    fn shift(&mut self, dy: f32) {
        match self {
            Paint::Rect { y, .. } => *y += dy,
            Paint::Line { a, b, .. } => {
                a.1 += dy;
                b.1 += dy;
            }
            Paint::Text { y, .. } => *y += dy,
            Paint::Image { y, .. } => *y += dy,
        }
    }
}

struct Txt {
    size: f32,
    line_height: f32,
    weight: f32,
    mono: bool,
    color: Color,
    max: Option<f32>,
}

struct Pager<'a, 'b> {
    doc: &'a Document,
    theme: Theme,
    scale: f32,
    width: f32,
    left: f32,
    max_y: f32,
    y: f32,
    paints: Vec<Paint>,
    blocks: Vec<BlockBox>,
    fonts: &'b mut FontContext,
    layouts: &'b mut LayoutContext<Brush>,
    cache: Option<&'b mut HashMap<u32, Vec<Paint>>>,
    base_dir: Option<PathBuf>,
    /// Decoded images, owned by the `DocumentRenderer` so cached frames keep theirs. `None`
    /// remembers a file that will not load.
    images: &'b mut HashMap<u32, Option<Rc<DecodedImage>>>,
}

fn brush(c: Color) -> Brush {
    [c[0], c[1], c[2], 255]
}

impl<'a, 'b> Pager<'a, 'b> {
    fn new(
        doc: &'a Document,
        options: &Options,
        fonts: &'b mut FontContext,
        layouts: &'b mut LayoutContext<Brush>,
        cache: Option<&'b mut HashMap<u32, Vec<Paint>>>,
        images: &'b mut HashMap<u32, Option<Rc<DecodedImage>>>,
    ) -> Self {
        let scale = options.scale.max(0.1);
        let logical_width = options.width.max(120.0);
        let content = (logical_width - 2.0 * options.theme.margin)
            .max(80.0)
            .min(options.theme.reading_width);
        let left = (logical_width - content) / 2.0 * scale;
        let max_y = options
            .max_height
            .map_or(f32::MAX, |h| (h * scale).max(1.0));
        Pager {
            doc,
            theme: options.theme,
            scale,
            width: (logical_width * scale).round(),
            left: left.round(),
            max_y,
            y: 0.0,
            paints: Vec::new(),
            blocks: Vec::new(),
            fonts,
            layouts,
            cache,
            base_dir: options.base_dir.clone(),
            images,
        }
    }

    /// Loads an image once per render. `None` for an empty src, a missing file, a blocked format
    /// or anything over the plan's limits.
    fn image(&mut self, index: u32) -> Option<Rc<DecodedImage>> {
        if let Some(cached) = self.images.get(&index) {
            return cached.clone();
        }
        let src = self.doc.str(self.doc.images[index as usize].src);
        let decoded = if src.is_empty() {
            None
        } else {
            let path = PathBuf::from(src);
            let path = match &self.base_dir {
                Some(dir) if path.is_relative() => dir.join(path),
                _ => path,
            };
            let decoded = image::load(&path);
            decoded.map(Rc::new)
        };
        self.images.insert(index, decoded.clone());
        decoded
    }

    fn px(&self, logical: f32) -> f32 {
        (logical * self.scale).round()
    }

    fn hair(&self) -> f32 {
        self.scale.round().max(1.0)
    }

    fn content_width(&self) -> f32 {
        self.width - 2.0 * self.left
    }

    fn page(&mut self) {
        self.y = self.px(36.0);
        let content = self.content_width();
        self.blocks(NONE, self.left, content, self.theme.text, 0);
        self.footnotes();
    }

    fn blocks(&mut self, parent: u32, left: f32, width: f32, color: Color, depth: usize) {
        let children: Vec<u32> = self.doc.children(parent).collect();
        for b in children {
            if self.y > self.max_y {
                return;
            }
            let y0 = self.y;
            self.block(b, left, width, color, depth);
            if depth == 0 {
                self.blocks.push(BlockBox {
                    block: b,
                    y: y0,
                    height: self.y - y0,
                });
            }
        }
    }

    fn block(&mut self, b: u32, left: f32, width: f32, color: Color, depth: usize) {
        let kind = self.doc.blocks[b as usize].kind;
        match kind {
            BlockKind::Paragraph { align } => self.paragraph(b, left, width, color, align),
            BlockKind::Plain => self.plain(b, left, width, color),
            BlockKind::Heading { level, align, .. } => {
                self.heading(b, left, width, level, align);
            }
            BlockKind::BlockQuote => self.quote(b, left, width, None, color, depth),
            BlockKind::Alert(alert) => self.quote(b, left, width, Some(alert), color, depth),
            BlockKind::List { start } => self.list(b, left, width, color, depth, start),
            BlockKind::ListItem { .. } => self.blocks(b, left, width, color, depth + 1),
            BlockKind::CodeBlock { lang } => {
                let label = if self.doc.str(lang).is_empty() {
                    None
                } else {
                    Some(self.doc.str(lang).to_string())
                };
                self.code_block(b, left, width, label);
            }
            BlockKind::Diagram { lang, .. } => {
                let label = if self.doc.str(lang).is_empty() {
                    "diagram".to_string()
                } else {
                    self.doc.str(lang).to_string()
                };
                self.code_block(b, left, width, Some(label));
            }
            BlockKind::Math { .. } => self.code_block(b, left, width, Some("math".to_string())),
            BlockKind::ThematicBreak => {
                self.y += self.px(8.0);
                let h = self.hair();
                self.paints.push(Paint::Rect {
                    x: left,
                    y: self.y,
                    w: width,
                    h,
                    radius: 0.0,
                    color: self.theme.border,
                });
                self.y += h + self.px(16.0);
            }
            BlockKind::Table => self.table(b, left, width),
            BlockKind::TableRow { .. } | BlockKind::TableCell { .. } => {
                // Only reached outside a table, which the model's shape promise rules out.
                self.blocks(b, left, width, color, depth + 1);
            }
            BlockKind::Details { .. } => self.details(b, left, width, color, depth),
            BlockKind::Summary => {}
            BlockKind::FootnoteDefinition { .. } => {
                // Moved to the end list; never drawn in flow.
            }
            BlockKind::Div { .. } => self.blocks(b, left, width, color, depth + 1),
        }
    }

    fn text_layout(&mut self, inline: &Inline, txt: &Txt) -> Layout<Brush> {
        let mut builder =
            self.layouts
                .ranged_builder(&mut self.fonts, &inline.text, self.scale, true);
        let family = if txt.mono {
            self.theme.mono
        } else {
            self.theme.sans
        };
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(family)));
        builder.push_default(StyleProperty::FontSize(txt.size));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            txt.line_height,
        )));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(txt.weight)));
        builder.push_default(StyleProperty::Brush(brush(txt.color)));
        for (range, style) in &inline.runs {
            let range = range.clone();
            if style.bold {
                builder.push(StyleProperty::FontWeight(FontWeight::BOLD), range.clone());
            }
            if style.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), range.clone());
            }
            if style.strike {
                builder.push(StyleProperty::Strikethrough(true), range.clone());
            }
            if style.underline || style.link {
                builder.push(StyleProperty::Underline(true), range.clone());
            }
            if style.link || style.accent {
                builder.push(
                    StyleProperty::Brush(brush(self.theme.accent)),
                    range.clone(),
                );
            }
            if style.muted {
                builder.push(StyleProperty::Brush(brush(self.theme.muted)), range.clone());
            }
            if style.code || style.kbd || style.math {
                builder.push(
                    StyleProperty::FontFamily(FontFamily::from(self.theme.mono)),
                    range.clone(),
                );
                builder.push(
                    StyleProperty::FontSize(txt.size * style.size_factor()),
                    range.clone(),
                );
            } else if style.small || style.sub || style.sup {
                builder.push(
                    StyleProperty::FontSize(txt.size * style.size_factor()),
                    range.clone(),
                );
            }
        }
        for image in &inline.boxes {
            let (mut width, mut height) = (image.width, image.height);
            if let Some(max) = txt.max
                && width > max
            {
                height *= max / width;
                width = max;
            }
            builder.push_inline_box(InlineBox {
                id: image.index as u64,
                kind: InlineBoxKind::InFlow,
                index: image.at,
                width,
                height,
                baseline: None,
                vertical_align: VerticalAlign::BASELINE,
            });
        }
        let mut layout = builder.build(&inline.text);
        layout.break_all_lines(txt.max);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }

    fn aligned_x(&self, left: f32, width: f32, layout: &Layout<Brush>, align: Align) -> f32 {
        match align {
            Align::Center => left + ((width - layout.width()) / 2.0).round(),
            Align::Right => left + (width - layout.width()).round(),
            _ => left,
        }
    }

    /// Pushes the chips, the glyphs and the decorations for one laid-out inline block, in that order.
    fn emit(&mut self, inline: &Inline, layout: Layout<Brush>, x: f32, y: f32, base_size: f32) {
        let mut images = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::InlineBox(inline_box) = item {
                    images.push(Paint::Image {
                        index: inline_box.id as u32,
                        x: x + inline_box.x,
                        y: y + inline_box.y,
                        w: inline_box.width,
                        h: inline_box.height,
                    });
                }
            }
        }
        let mut chips = Vec::new();
        let mut decos = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                // A line-scoped run reports the whole line's range from 	ext_range, so the run's
                // own start is its first cluster's text range.
                let run_start = run.clusters().next().map_or(0, |c| c.text_range().start);
                let style = inline.style_at(run_start);
                let dev = base_size * style.size_factor() * self.scale;
                let (gx, baseline, advance) = (
                    x + glyph_run.offset(),
                    y + glyph_run.baseline(),
                    glyph_run.advance(),
                );
                if style.code || style.kbd || style.math {
                    let pad = self.px(3.0);
                    chips.push(Paint::Rect {
                        x: gx - pad,
                        y: (baseline - dev * 0.92).round(),
                        w: advance + 2.0 * pad,
                        h: (dev * 1.24).round().max(1.0),
                        radius: self.px(3.0),
                        color: self.theme.code_bg,
                    });
                }
                let color = glyph_run.style().brush;
                let color = [color[0], color[1], color[2]];
                let thickness = (dev * 0.065).round().max(1.0);
                if style.underline || style.link {
                    decos.push(Paint::Rect {
                        x: gx,
                        y: (baseline + dev * 0.13).round(),
                        w: advance,
                        h: thickness,
                        radius: 0.0,
                        color,
                    });
                }
                if style.strike {
                    decos.push(Paint::Rect {
                        x: gx,
                        y: (baseline - dev * 0.3).round(),
                        w: advance,
                        h: thickness,
                        radius: 0.0,
                        color,
                    });
                }
            }
        }
        self.paints.extend(images);
        self.paints.extend(chips);
        self.paints.push(Paint::Text {
            layout: Rc::new(layout),
            x,
            y,
        });
        self.paints.extend(decos);
    }

    /// A task-list box, drawn rather than typeset so it looks the same with any font stack.
    fn draw_checkbox(&mut self, x: f32, baseline: f32, checked: bool) {
        let side = self.px(15.0);
        let top = baseline - self.px(12.0);
        let radius = self.px(3.0);
        if checked {
            self.paints.push(Paint::Rect {
                x,
                y: top,
                w: side,
                h: side,
                radius,
                color: self.theme.accent,
            });
            let check = self.theme.bg;
            let thickness = self.px(1.7).max(1.0);
            self.paints.push(Paint::Line {
                a: (x + side * 0.24, top + side * 0.52),
                b: (x + side * 0.42, top + side * 0.70),
                thickness,
                color: check,
            });
            self.paints.push(Paint::Line {
                a: (x + side * 0.42, top + side * 0.70),
                b: (x + side * 0.76, top + side * 0.32),
                thickness,
                color: check,
            });
        } else {
            let border = self.theme.muted;
            let bg = self.theme.bg;
            let hair = self.hair();
            self.paints.push(Paint::Rect {
                x,
                y: top,
                w: side,
                h: side,
                radius,
                color: border,
            });
            self.paints.push(Paint::Rect {
                x: x + hair,
                y: top + hair,
                w: side - 2.0 * hair,
                h: side - 2.0 * hair,
                radius: (radius - hair).max(0.0),
                color: bg,
            });
        }
    }

    fn task_indent(&self) -> f32 {
        self.px(22.0)
    }

    fn paragraph(&mut self, b: u32, left: f32, width: f32, color: Color, align: Align) {
        let inline = self.inline_of(b);
        let task = inline.task;
        if inline.text.trim().is_empty() && inline.boxes.is_empty() && task.is_none() {
            return;
        }
        let indent = if task.is_some() {
            self.task_indent()
        } else {
            0.0
        };
        let txt = Txt {
            size: self.theme.body_size,
            line_height: self.theme.body_line_height,
            weight: 400.0,
            mono: false,
            color,
            max: Some(width - indent),
        };
        let layout = self.text_layout(&inline, &txt);
        let height = layout.height().round().max(if task.is_some() {
            self.theme.body_size * self.theme.body_line_height * self.scale
        } else {
            0.0
        });
        let x = self.aligned_x(left + indent, width - indent, &layout, align);
        let baseline = layout
            .lines()
            .next()
            .map_or(height * 0.78, |line| line.metrics().baseline);
        if let Some(checked) = task {
            self.draw_checkbox(left, self.y + baseline, checked);
        }
        let size = txt.size;
        if !inline.text.trim().is_empty() || !inline.boxes.is_empty() {
            self.emit(&inline, layout, x, self.y, size);
        }
        self.y += height + self.px(16.0);
    }

    fn plain(&mut self, b: u32, left: f32, width: f32, color: Color) {
        let inline = self.inline_of(b);
        let task = inline.task;
        if inline.text.trim().is_empty() && inline.boxes.is_empty() && task.is_none() {
            return;
        }
        let indent = if task.is_some() {
            self.task_indent()
        } else {
            0.0
        };
        let txt = Txt {
            size: self.theme.body_size,
            line_height: self.theme.body_line_height,
            weight: 400.0,
            mono: false,
            color,
            max: Some(width - indent),
        };
        let layout = self.text_layout(&inline, &txt);
        let height = layout.height().round().max(if task.is_some() {
            self.theme.body_size * self.theme.body_line_height * self.scale
        } else {
            0.0
        });
        let baseline = layout
            .lines()
            .next()
            .map_or(height * 0.78, |line| line.metrics().baseline);
        if let Some(checked) = task {
            self.draw_checkbox(left, self.y + baseline, checked);
        }
        let size = txt.size;
        if !inline.text.trim().is_empty() || !inline.boxes.is_empty() {
            self.emit(&inline, layout, left + indent, self.y, size);
        }
        self.y += height + self.px(4.0);
    }

    fn heading(&mut self, b: u32, left: f32, width: f32, level: u8, align: Align) {
        let (size, top, bottom) = match level {
            1 => (34.0, 0.0, 14.0),
            2 => (24.0, 22.0, 12.0),
            3 => (18.0, 12.0, 8.0),
            4 => (17.0, 10.0, 8.0),
            5 => (16.0, 8.0, 6.0),
            _ => (15.0, 8.0, 6.0),
        };
        let inline = self.inline_of(b);
        if inline.text.trim().is_empty() && inline.boxes.is_empty() {
            return;
        }
        self.y += self.px(top);
        let txt = Txt {
            size,
            line_height: 1.25,
            weight: 600.0,
            mono: false,
            color: self.theme.heading,
            max: Some(width),
        };
        let layout = self.text_layout(&inline, &txt);
        let x = self.aligned_x(left, width, &layout, align);
        let height = layout.height().round();
        self.emit(&inline, layout, x, self.y, size);
        self.y += height + self.px(bottom);
    }

    fn quote(
        &mut self,
        b: u32,
        left: f32,
        width: f32,
        alert: Option<AlertKind>,
        color: Color,
        depth: usize,
    ) {
        let _ = color;
        let pad = self.px(if alert.is_some() { 12.0 } else { 2.0 });
        let inset = self.px(18.0);
        let bar = self.px(3.0);
        let y0 = self.y;
        let start = self.paints.len();
        let inner_left = left + inset;
        let inner_width = width - inset - self.px(14.0);
        self.y = y0 + pad;
        if let Some(alert) = alert {
            let mut title = Inline::default();
            title.push(
                alert.label(),
                Style {
                    bold: true,
                    ..Style::default()
                },
            );
            let txt = Txt {
                size: self.theme.body_size,
                line_height: 1.3,
                weight: 600.0,
                mono: false,
                color: self.theme.accent,
                max: Some(inner_width),
            };
            let layout = self.text_layout(&title, &txt);
            let height = layout.height().round();
            let size = txt.size;
            self.emit(&title, layout, inner_left, self.y, size);
            self.y += height + self.px(4.0);
        }
        let body_color = if alert.is_some() {
            self.theme.text
        } else {
            self.theme.muted
        };
        self.blocks(b, inner_left, inner_width, body_color, depth + 1);
        let height = (self.y - y0).max(self.px(8.0));
        let bottom = self.px(if alert.is_some() { 12.0 } else { 2.0 });
        let full_height = height + bottom;
        if alert.is_some() {
            let tint = self.theme.tint;
            self.paints.insert(
                start,
                Paint::Rect {
                    x: left,
                    y: y0,
                    w: width,
                    h: full_height,
                    radius: self.px(6.0),
                    color: tint,
                },
            );
        }
        let bar_color = if alert.is_some() {
            self.theme.accent
        } else {
            self.theme.border
        };
        // The bar goes over the alert's tint, not under it; both sit behind the text.
        let bar_at = if alert.is_some() { start + 1 } else { start };
        self.paints.insert(
            bar_at,
            Paint::Rect {
                x: left,
                y: y0,
                w: bar,
                h: full_height,
                radius: bar / 2.0,
                color: bar_color,
            },
        );
        self.y = y0 + full_height + self.px(16.0);
    }

    fn list(
        &mut self,
        b: u32,
        left: f32,
        width: f32,
        color: Color,
        depth: usize,
        start: Option<u32>,
    ) {
        let items: Vec<u32> = self.doc.children(b).collect();
        let indent = self.px(26.0);
        let count = items.len();
        for (index, item) in items.into_iter().enumerate() {
            if self.y > self.max_y {
                return;
            }
            let task = matches!(
                self.doc.blocks[item as usize].kind,
                BlockKind::ListItem { task: true }
            );
            if !task {
                let marker =
                    self.doc
                        .attrs_of(item)
                        .and_then(|a| a.marker)
                        .unwrap_or(if start.is_some() {
                            ListMarker::Decimal
                        } else {
                            ListMarker::Disc
                        });
                if marker != ListMarker::None {
                    let number = start.unwrap_or(1).saturating_add(index as u32).max(1);
                    self.marker(marker, number, left, self.y);
                }
            }
            self.blocks(item, left + indent, width - indent, color, depth + 1);
            self.y += self.px(if index + 1 < count { 4.0 } else { 16.0 });
        }
    }

    fn marker(&mut self, marker: ListMarker, number: u32, left: f32, y: f32) {
        match marker {
            ListMarker::Disc | ListMarker::Circle | ListMarker::Square => {
                let d = self.px(5.0);
                let radius = if marker == ListMarker::Square {
                    0.0
                } else {
                    d / 2.0
                };
                self.paints.push(Paint::Rect {
                    x: left + self.px(8.0),
                    y: y + self.px(10.0),
                    w: d,
                    h: d,
                    radius,
                    color: self.theme.muted,
                });
            }
            other => {
                let label = match other {
                    ListMarker::LowerAlpha => alpha(number, true),
                    ListMarker::UpperAlpha => alpha(number, false),
                    ListMarker::LowerRoman => roman(number).to_lowercase(),
                    ListMarker::UpperRoman => roman(number),
                    _ => number.to_string(),
                };
                let mut inline = Inline::default();
                inline.push(
                    &format!("{label}."),
                    Style {
                        muted: true,
                        ..Style::default()
                    },
                );
                let txt = Txt {
                    size: self.theme.body_size,
                    line_height: self.theme.body_line_height,
                    weight: 400.0,
                    mono: false,
                    color: self.theme.muted,
                    max: None,
                };
                let layout = self.text_layout(&inline, &txt);
                let size = txt.size;
                self.emit(&inline, layout, left, y + self.px(1.0), size);
            }
        }
    }

    fn code_block(&mut self, b: u32, left: f32, width: f32, label: Option<String>) {
        let source = self.doc.block_text(b).trim_end_matches('\n').to_string();
        let mut inline = Inline::default();
        inline.push(
            &source,
            Style {
                code: true,
                ..Style::default()
            },
        );
        let pad_x = self.px(16.0);
        let pad_y = self.px(13.0);
        let txt = Txt {
            size: 14.0,
            line_height: 1.5,
            weight: 400.0,
            mono: true,
            color: self.theme.text,
            max: Some((width - 2.0 * pad_x).max(1.0)),
        };
        let layout = self.text_layout(&inline, &txt);
        let size = txt.size;
        let text_height = layout.height().round().max(txt.size * self.scale);
        let height = text_height + 2.0 * pad_y;
        let radius = self.px(6.0);
        let hr = self.hair();
        let y0 = self.y;
        self.paints.push(Paint::Rect {
            x: left,
            y: y0,
            w: width,
            h: height,
            radius,
            color: self.theme.border,
        });
        self.paints.push(Paint::Rect {
            x: left + hr,
            y: y0 + hr,
            w: width - 2.0 * hr,
            h: height - 2.0 * hr,
            radius: (radius - hr).max(0.0),
            color: self.theme.code_bg,
        });
        if !source.is_empty() {
            self.emit(&inline, layout, left + pad_x, y0 + pad_y, size);
        }
        if let Some(label) = label.filter(|l| !l.is_empty()) {
            let mut tag = Inline::default();
            tag.push(
                &label,
                Style {
                    muted: true,
                    ..Style::default()
                },
            );
            let txt = Txt {
                size: 12.0,
                line_height: 1.3,
                weight: 400.0,
                mono: false,
                color: self.theme.muted,
                max: None,
            };
            let layout = self.text_layout(&tag, &txt);
            let x = left + width - self.px(12.0) - layout.width();
            let size = txt.size;
            self.emit(&tag, layout, x, y0 + self.px(8.0), size);
        }
        self.y = y0 + height + self.px(16.0);
    }

    fn details(&mut self, b: u32, left: f32, width: f32, color: Color, depth: usize) {
        let children: Vec<u32> = self.doc.children(b).collect();
        let summary = children
            .iter()
            .copied()
            .find(|c| matches!(self.doc.blocks[*c as usize].kind, BlockKind::Summary));
        let open = self.doc.details_open_as_written(b);
        if let Some(summary) = summary {
            let inline = self.inline_of(summary);
            let txt = Txt {
                size: self.theme.body_size,
                line_height: self.theme.body_line_height,
                weight: 600.0,
                mono: false,
                color: self.theme.heading,
                max: Some(width - self.px(18.0)),
            };
            let layout = self.text_layout(&inline, &txt);
            let height = layout.height().round();
            let baseline = layout
                .lines()
                .next()
                .map_or(height * 0.8, |line| line.metrics().baseline);
            // A drawn chevron, because a symbol glyph is not in every font stack.
            let cx = left + self.px(5.0);
            let cy = self.y + baseline - self.px(5.0);
            let w = self.px(7.0);
            let h = self.px(4.5);
            let stroke = self.px(1.6).max(1.0);
            let color = self.theme.muted;
            if open {
                self.paints.push(Paint::Line {
                    a: (cx, cy),
                    b: (cx + w / 2.0, cy + h),
                    thickness: stroke,
                    color,
                });
                self.paints.push(Paint::Line {
                    a: (cx + w / 2.0, cy + h),
                    b: (cx + w, cy),
                    thickness: stroke,
                    color,
                });
            } else {
                self.paints.push(Paint::Line {
                    a: (cx, cy - h / 2.0),
                    b: (cx + h / 2.0, cy),
                    thickness: stroke,
                    color,
                });
                self.paints.push(Paint::Line {
                    a: (cx + h / 2.0, cy),
                    b: (cx, cy + h / 2.0),
                    thickness: stroke,
                    color,
                });
            }
            let size = txt.size;
            self.emit(&inline, layout, left + self.px(18.0), self.y, size);
            self.y += height + self.px(4.0);
        }
        if open {
            for child in children {
                if Some(child) == summary {
                    continue;
                }
                if self.y > self.max_y {
                    return;
                }
                self.block(child, left, width, color, depth + 1);
            }
            self.y += self.px(12.0);
        }
    }

    fn table(&mut self, b: u32, left: f32, width: f32) {
        struct Cell {
            inline: Inline,
            align: Align,
            header: bool,
        }
        struct Row {
            header: bool,
            cells: Vec<Cell>,
        }
        struct Laid {
            inline: Inline,
            align: Align,
            layout: Layout<Brush>,
        }
        let rows: Vec<u32> = self.doc.children(b).collect();
        let mut table: Vec<Row> = Vec::new();
        for row in rows {
            let header = matches!(
                self.doc.blocks[row as usize].kind,
                BlockKind::TableRow { header: true }
            );
            let mut cells = Vec::new();
            for cell in self.doc.children(row) {
                let (align, header_cell) = match self.doc.blocks[cell as usize].kind {
                    BlockKind::TableCell { align, header, .. } => (align, header),
                    _ => (Align::None, header),
                };
                cells.push(Cell {
                    inline: self.cell_inline(cell),
                    align,
                    header: header_cell,
                });
            }
            table.push(Row { header, cells });
        }
        let columns = table.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        if columns == 0 || table.is_empty() {
            return;
        }
        let pad_x = self.px(13.0);
        let pad_y = self.px(8.0);
        let hr = self.hair();
        let text_size = 15.0;
        let mut natural = vec![0f32; columns];
        for row in &table {
            for (ci, cell) in row.cells.iter().enumerate() {
                if cell.inline.text.trim().is_empty() && cell.inline.boxes.is_empty() {
                    continue;
                }
                let txt = Txt {
                    size: text_size,
                    line_height: 1.45,
                    weight: if cell.header { 600.0 } else { 400.0 },
                    mono: false,
                    color: self.theme.text,
                    max: None,
                };
                let layout = self.text_layout(&cell.inline, &txt);
                natural[ci] = natural[ci].max(layout.width().ceil());
            }
        }
        let available = width - 2.0 * hr;
        let total: f32 = natural.iter().sum::<f32>() + 2.0 * pad_x * columns as f32;
        let mut columns_w = vec![0f32; columns];
        if total <= available {
            let extra = ((available - total) / columns as f32).floor();
            for (ci, w) in natural.iter().enumerate() {
                columns_w[ci] = w + 2.0 * pad_x + extra;
            }
        } else {
            let scale = available / total;
            for (ci, w) in natural.iter().enumerate() {
                columns_w[ci] = ((w + 2.0 * pad_x) * scale).max(2.0 * pad_x + self.px(8.0));
            }
        }
        let mut laid: Vec<Vec<Laid>> = Vec::new();
        for row in &table {
            let mut row_layouts = Vec::new();
            for (ci, cell) in row.cells.iter().enumerate() {
                let txt = Txt {
                    size: text_size,
                    line_height: 1.45,
                    weight: if cell.header { 600.0 } else { 400.0 },
                    mono: false,
                    color: self.theme.text,
                    max: Some((columns_w[ci] - 2.0 * pad_x).max(1.0)),
                };
                row_layouts.push(Laid {
                    inline: cell.inline.clone(),
                    align: cell.align,
                    layout: self.text_layout(&cell.inline, &txt),
                });
            }
            laid.push(row_layouts);
        }
        let heights: Vec<f32> = laid
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.layout.height().round())
                    .fold(0.0, f32::max)
                    + 2.0 * pad_y
            })
            .collect();
        let total_h: f32 = heights.iter().sum::<f32>() + heights.len() as f32 * hr + hr;
        let radius = self.px(6.0);
        let y0 = self.y;
        let inner_radius = (radius - hr).max(0.0);
        self.paints.push(Paint::Rect {
            x: left,
            y: y0,
            w: width,
            h: total_h,
            radius,
            color: self.theme.border,
        });
        self.paints.push(Paint::Rect {
            x: left + hr,
            y: y0 + hr,
            w: width - 2.0 * hr,
            h: total_h - 2.0 * hr,
            radius: inner_radius,
            color: self.theme.bg,
        });
        if table[0].header {
            self.paints.push(Paint::Rect {
                x: left + hr,
                y: y0 + hr,
                w: width - 2.0 * hr,
                h: heights[0],
                radius: inner_radius,
                color: self.theme.head_bg,
            });
            if heights[0] > inner_radius * 2.0 {
                self.paints.push(Paint::Rect {
                    x: left + hr,
                    y: y0 + hr + inner_radius,
                    w: width - 2.0 * hr,
                    h: heights[0] - 2.0 * inner_radius,
                    radius: 0.0,
                    color: self.theme.head_bg,
                });
            }
        }
        let mut y = y0 + hr;
        for (ri, row) in laid.into_iter().enumerate() {
            let mut x = left + hr;
            for (ci, cell) in row.into_iter().enumerate() {
                let dx = match cell.align {
                    Align::Center => ((columns_w[ci] - cell.layout.width()) / 2.0).round(),
                    Align::Right => (columns_w[ci] - pad_x - cell.layout.width()).round(),
                    _ => pad_x,
                };
                self.emit(&cell.inline, cell.layout, x + dx, y + pad_y, text_size);
                x += columns_w[ci];
            }
            y += heights[ri];
            if ri + 1 < heights.len() {
                self.paints.push(Paint::Rect {
                    x: left + hr,
                    y,
                    w: width - 2.0 * hr,
                    h: hr,
                    radius: 0.0,
                    color: self.theme.border,
                });
            }
            y += hr;
        }
        self.y = y0 + total_h + self.px(16.0);
    }

    fn cell_inline(&mut self, cell: u32) -> Inline {
        let mut leaves = Vec::new();
        self.collect_leaves(cell, &mut leaves);
        let mut merged = Inline::default();
        let mut first = true;
        for leaf in leaves {
            let inline = self.inline_of(leaf);
            if inline.text.is_empty() {
                continue;
            }
            if !first {
                merged.push(" ", Style::default());
            }
            for (range, style) in &inline.runs {
                merged.push(&inline.text[range.clone()], *style);
            }
            first = false;
        }
        merged
    }

    fn collect_leaves(&self, b: u32, out: &mut Vec<u32>) {
        let block = self.doc.blocks[b as usize];
        let kind = block.kind;
        // A Markdown table cell holds its inline content directly, so it is a leaf when it has no
        // child blocks; cells with block content recurse.
        let leaf = matches!(
            kind,
            BlockKind::Paragraph { .. }
                | BlockKind::Plain
                | BlockKind::Heading { .. }
                | BlockKind::CodeBlock { .. }
                | BlockKind::Math { .. }
                | BlockKind::Diagram { .. }
        ) || (matches!(kind, BlockKind::TableCell { .. }) && block.end == b + 1);
        if leaf {
            out.push(b);
            return;
        }
        for child in self.doc.children(b) {
            self.collect_leaves(child, out);
        }
    }

    fn footnotes(&mut self) {
        let mut notes: Vec<&Footnote> = self
            .doc
            .footnotes
            .iter()
            .filter(|f| f.number > 0 && f.defined)
            .collect();
        if notes.is_empty() {
            return;
        }
        notes.sort_by_key(|f| f.number);
        self.y += self.px(20.0);
        let left = self.left;
        let width = self.content_width();
        let hr = self.hair();
        self.paints.push(Paint::Rect {
            x: left,
            y: self.y,
            w: width,
            h: hr,
            radius: 0.0,
            color: self.theme.border,
        });
        self.y += hr + self.px(10.0);
        // The links borrow `self.doc`, so collect the text first.
        let entries: Vec<(u32, Inline)> = notes
            .iter()
            .map(|note| {
                let mut inline = Inline::default();
                inline.push(
                    &format!("{}. ", note.number),
                    Style {
                        muted: true,
                        ..Style::default()
                    },
                );
                if note.block != NONE {
                    let mut leaves = Vec::new();
                    self.collect_leaves(note.block, &mut leaves);
                    let mut first = true;
                    for leaf in leaves {
                        let body = self.inline_of(leaf);
                        if body.text.is_empty() {
                            continue;
                        }
                        if !first {
                            inline.push(" ", Style::default());
                        }
                        for (range, style) in &body.runs {
                            inline.push(&body.text[range.clone()], *style);
                        }
                        first = false;
                    }
                } else {
                    inline.push(
                        "definition was too deep to keep",
                        Style {
                            muted: true,
                            italic: true,
                            ..Style::default()
                        },
                    );
                }
                (note.number, inline)
            })
            .collect();
        for (_, inline) in entries {
            let txt = Txt {
                size: 14.0,
                line_height: 1.6,
                weight: 400.0,
                mono: false,
                color: self.theme.muted,
                max: Some(width),
            };
            let layout = self.text_layout(&inline, &txt);
            let height = layout.height().round();
            let size = txt.size;
            self.emit(&inline, layout, left, self.y, size);
            self.y += height + self.px(6.0);
        }
    }

    /// The model's inline content of a leaf block as styled runs, with atoms replaced by the text
    /// the page shows for them.
    fn inline_of(&mut self, block: u32) -> Inline {
        let block = self.doc.blocks[block as usize];
        let base = block.text.start as usize;
        let end = block.text.end as usize;
        let src = &self.doc.text[base..end];
        let mut cuts = vec![base, end];
        let mut spans: Vec<(usize, usize, SpanKind)> = Vec::new();
        let mut atoms: Vec<(usize, SpanKind)> = Vec::new();
        for span in &self.doc.spans[block.spans.usize()] {
            let (start, stop) = (span.start as usize, span.end as usize);
            if start == stop {
                if start >= base && start <= end {
                    atoms.push((start, span.kind));
                    cuts.push(start);
                }
                continue;
            }
            let (start, stop) = (start.max(base), stop.min(end));
            if start < stop {
                cuts.push(start);
                cuts.push(stop);
                spans.push((start, stop, span.kind));
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Inline::default();
        for i in 0..cuts.len() {
            let at = cuts[i];
            for (position, kind) in &atoms {
                if *position == at {
                    self.push_atom(&mut out, *kind);
                }
            }
            if i + 1 == cuts.len() {
                continue;
            }
            let to = cuts[i + 1];
            if to <= at {
                continue;
            }
            let style = self.style_at(&spans, at);
            out.push(&src[at - base..to - base], style);
        }
        out
    }

    fn style_at(&self, spans: &[(usize, usize, SpanKind)], at: usize) -> Style {
        let mut style = Style::default();
        for (start, stop, kind) in spans {
            if *start > at || at >= *stop {
                continue;
            }
            match kind {
                SpanKind::Emphasis => style.italic = true,
                SpanKind::Strong => style.bold = true,
                SpanKind::Strike => style.strike = true,
                SpanKind::Underline => style.underline = true,
                SpanKind::Code => style.code = true,
                SpanKind::Kbd => style.kbd = true,
                SpanKind::Sub => style.sub = true,
                SpanKind::Sup => style.sup = true,
                SpanKind::Mark => {}
                SpanKind::Link(_) => style.link = true,
                SpanKind::Math { display: false, .. } => style.math = true,
                _ => {}
            }
        }
        style
    }

    fn push_atom(&mut self, out: &mut Inline, kind: SpanKind) {
        match kind {
            SpanKind::Image(index) => {
                if let Some(image) = self.image(index) {
                    let width = image.width as f32 * self.scale;
                    let height = image.height as f32 * self.scale;
                    out.boxes.push(ImageBox {
                        at: out.text.len(),
                        index,
                        width,
                        height,
                    });
                    return;
                }
                let alt = self.doc.str(self.doc.images[index as usize].alt);
                if alt.is_empty() {
                    out.push(
                        "[image]",
                        Style {
                            muted: true,
                            ..Style::default()
                        },
                    );
                } else {
                    out.push(
                        &format!("[image: {alt}]"),
                        Style {
                            muted: true,
                            italic: true,
                            ..Style::default()
                        },
                    );
                }
            }
            SpanKind::FootnoteRef(index) => {
                let number = self.doc.footnotes[index as usize].number;
                out.push(
                    &format!("[{number}]"),
                    Style {
                        link: true,
                        small: true,
                        ..Style::default()
                    },
                );
            }
            SpanKind::Checkbox { checked } => {
                if out.text.is_empty() && out.task.is_none() {
                    out.task = Some(checked);
                } else {
                    // Only a task item starts with its box; anything else keeps a readable mark.
                    out.push(
                        if checked { "[x] " } else { "[ ] " },
                        Style {
                            muted: true,
                            ..Style::default()
                        },
                    );
                }
            }
            _ => {}
        }
    }

    /// Measures every top-level block without keeping its paints, so a viewport can be placed and
    /// the total page height known without allocating a page-sized bitmap.
    fn measure_page(mut self) -> PageLayout {
        self.y = self.px(36.0);
        let content = self.content_width();
        let children: Vec<u32> = self.doc.children(NONE).collect();
        let mut spans = Vec::with_capacity(children.len());
        for block in children {
            let y = self.y;
            let mark = self.paints.len();
            self.block(block, self.left, content, self.theme.text, 0);
            spans.push(BlockBox {
                block,
                y,
                height: self.y - y,
            });
            self.paints.truncate(mark);
        }
        let footnotes_y = self.y;
        self.footnotes();
        let footnotes_height = self.y - footnotes_y;
        self.paints.clear();
        let total_height = self.y + self.px(24.0);
        PageLayout {
            spans,
            footnotes_y,
            footnotes_height,
            total_height,
            width: self.width / self.scale,
            scale: self.scale,
            theme: self.theme.name,
        }
    }

    /// Lays out and paints only the blocks that reach the viewport.
    fn slice_page(&mut self, page: &PageLayout, top: f32, height: f32) {
        self.y = self.px(36.0);
        let content = self.content_width();
        let bottom = top + height;
        for span in &page.spans {
            if span.y + span.height <= top || span.y >= bottom {
                continue;
            }
            if let Some(cache) = self.cache.as_deref()
                && let Some(cached) = cache.get(&span.block)
            {
                let mut paints = cached.clone();
                for paint in &mut paints {
                    paint.shift(span.y);
                }
                self.paints.extend(paints);
                continue;
            }
            let mark = self.paints.len();
            self.y = span.y;
            self.block(span.block, self.left, content, self.theme.text, 0);
            if let Some(cache) = self.cache.as_deref_mut() {
                let mut made = self.paints.split_off(mark);
                for paint in &mut made {
                    paint.shift(-span.y);
                }
                cache.insert(span.block, made.clone());
                for paint in &mut made {
                    paint.shift(span.y);
                }
                self.paints.extend(made);
            }
        }
        if page.footnotes_height > 0.0
            && page.footnotes_y + page.footnotes_height > top
            && page.footnotes_y < bottom
        {
            self.y = page.footnotes_y;
            self.footnotes();
        }
    }

    fn finish(mut self, engine: &mut TextEngine) -> Rendered {
        let content_height = self.y + self.px(24.0);
        let height = content_height.ceil().max(1.0) as u32;
        let width = self.width.round().max(1.0) as u32;
        let mut frame = Bitmap::new(width, height);
        frame.clear(self.theme.bg);
        let stats = self.draw(&mut frame, 0.0, engine);
        Rendered {
            bitmap: frame,
            content_height,
            blocks: self.blocks,
            stats,
        }
    }

    fn finish_slice(
        mut self,
        top: f32,
        height: f32,
        total_height: f32,
        engine: &mut TextEngine,
    ) -> Slice {
        let rows = height.ceil().max(1.0) as u32;
        let width = self.width.round().max(1.0) as u32;
        let mut frame = Bitmap::new(width, rows);
        frame.clear(self.theme.bg);
        let stats = self.draw(&mut frame, top, engine);
        Slice {
            bitmap: frame,
            top,
            total_height,
            stats,
        }
    }

    fn draw(
        &mut self,
        frame: &mut Bitmap,
        shift: f32,
        engine: &mut TextEngine,
    ) -> crate::text::CacheStats {
        for paint in self.paints.drain(..) {
            match paint {
                Paint::Rect {
                    x,
                    y,
                    w,
                    h,
                    radius,
                    color,
                } => frame.rrect(x, y - shift, w, h, radius, color),
                Paint::Line {
                    a,
                    b,
                    thickness,
                    color,
                } => frame.line((a.0, a.1 - shift), (b.0, b.1 - shift), thickness, color),
                Paint::Text { layout, x, y } => engine.draw_layout(frame, &layout, x, y - shift),
                Paint::Image { index, x, y, w, h } => {
                    if let Some(Some(image)) = self.images.get(&index) {
                        frame.image(&image.rgba, image.width, image.height, x, y - shift, w, h);
                    }
                }
            }
        }
        engine.stats
    }
}

/// Copies `source` into `target` starting at row `row`.
fn copy_rows(target: &mut Bitmap, source: &Bitmap, row: usize) {
    let stride = target.width as usize * 4;
    let rows = source.height as usize;
    let start = row * stride;
    target.pixels[start..start + rows * stride].copy_from_slice(&source.pixels[..rows * stride]);
}

/// Moves the rows of `bitmap` down by `dy` rows when `dy` is positive, up when negative.
fn shift_rows(bitmap: &mut Bitmap, dy: i32) {
    let stride = bitmap.width as usize * 4;
    let rows = bitmap.height as usize;
    let shift = dy.unsigned_abs() as usize;
    if shift == 0 || shift >= rows {
        return;
    }
    if dy > 0 {
        // The view scrolled up: the content moves down.
        bitmap
            .pixels
            .copy_within(0..(rows - shift) * stride, shift * stride);
    } else {
        // The view scrolled down: the content moves up.
        bitmap.pixels.copy_within(shift * stride..rows * stride, 0);
    }
}

fn alpha(mut n: u32, lower: bool) -> String {
    let mut out = String::new();
    while n > 0 {
        n -= 1;
        out.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    if out.is_empty() {
        out.push('a');
    }
    if !lower {
        out.make_ascii_uppercase();
    }
    out
}

fn roman(mut n: u32) -> String {
    const TABLE: &[(u32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for &(value, glyph) in TABLE {
        while n >= value {
            out.push_str(glyph);
            n -= value;
        }
    }
    out
}
