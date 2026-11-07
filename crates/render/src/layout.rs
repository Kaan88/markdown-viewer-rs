//! Lays the document model out in one top-down pass and paints it into a bitmap: blocks are
//! virtualized, scroll frames reuse the previous bitmap's rows, and every laid-out text run keeps
//! its caret stops so a click can be turned back into a document offset, a link or a selection.
//! Images are drawn through parley inline boxes; math and diagrams still show their source.

use crate::bitmap::Bitmap;
use crate::engine;
use crate::image::{self, DecodedImage};
use crate::text::{Brush, TextEngine};
use crate::theme::{Color, DARK, LIGHT, Theme};
use mdview_core::model::{
    AlertKind, Align, BlockKind, Document, Footnote, ListMarker, NONE, SpanKind,
};
use parley::{
    Alignment, AlignmentOptions, Cluster, ClusterSide, FontContext, FontFamily, FontStyle,
    FontWeight, InlineBox, InlineBoxKind, Layout, LayoutContext, LineHeight, PositionedLayoutItem,
    StyleProperty, VerticalAlign,
};
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone)]
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

/// A rectangle in page device px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Where a click landed: the top-level block, the offset into `Document::text`, and anything
/// actionable under the point. `link` indexes `Document::links`, `footnote` indexes
/// `Document::footnotes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub block: u32,
    pub offset: u32,
    pub link: Option<u32>,
    pub footnote: Option<u32>,
    /// The `<details>` block whose summary is under the point; clicking toggles it.
    pub details: Option<u32>,
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
    /// The measured y of each footnote entry, by footnote number, in reading order.
    notes: Vec<(u32, f32)>,
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

    /// Every top-level block's measured extent, in page order.
    pub fn blocks(&self) -> &[BlockBox] {
        &self.spans
    }

    /// The top-level block at page row `y`. Clamps into the first or last block outside the
    /// content, so a click in the page margins still lands on text.
    pub fn block_at(&self, y: f32) -> Option<BlockBox> {
        self.spans.get(self.index_at(y)?).copied()
    }

    /// The index of the block at page row `y`, for callers that keep parallel data.
    pub fn index_at(&self, y: f32) -> Option<usize> {
        if self.spans.is_empty() {
            return None;
        }
        if y < self.spans[0].y {
            return Some(0);
        }
        let index = self
            .spans
            .partition_point(|span| span.y + span.height <= y)
            .min(self.spans.len() - 1);
        Some(index)
    }

    /// The page row of a block, including one nested in a list, quote, table or details, so an
    /// anchor can be scrolled to. Footnote entries are not drawn where their definitions are; use
    /// [`footnote_y`](Self::footnote_y) for those.
    pub fn y_of(&self, doc: &Document, block: u32) -> Option<f32> {
        let mut block = block;
        loop {
            let parent = doc.blocks.get(block as usize)?.parent;
            if parent == NONE {
                break;
            }
            block = parent;
        }
        self.spans
            .iter()
            .find(|span| span.block == block)
            .map(|span| span.y)
    }

    /// The page row of a footnote entry, by the number shown in the text.
    pub fn footnote_y(&self, number: u32) -> Option<f32> {
        self.notes
            .iter()
            .find(|(n, _)| *n == number)
            .map(|(_, y)| *y)
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
    /// Laid-out paints and caret stops per top-level block, relative to the block's top, for scroll
    /// frames and hit tests.
    cache: HashMap<u32, CachedBlock>,
    cache_key: Option<(f32, f32, &'static str)>,
    /// Decoded images, shared by every frame so a cached paint can still find its pixels.
    images: HashMap<u32, Option<Rc<DecodedImage>>>,
    /// `<details>` open overrides, by block, so a toggle is view state the document does not carry.
    details: HashMap<u32, bool>,
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
            details: HashMap::new(),
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
            &self.details,
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
            &self.details,
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
            &self.details,
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

    /// Whether a `<details>` is open, honouring a shell override and falling back to how the
    /// document was written.
    pub fn details_open(&self, doc: &Document, block: u32) -> bool {
        self.details
            .get(&block)
            .copied()
            .unwrap_or_else(|| doc.details_open_as_written(block))
    }

    /// Opens or closes a `<details>`. The block's height changes, so the cached frame and the paint
    /// cache go; the caller has to measure the page again.
    pub fn set_details(&mut self, block: u32, open: bool) {
        self.details.insert(block, open);
        self.cache.clear();
        self.frame = None;
    }

    /// Forgets every toggle, for a new document whose block numbers no longer mean the same.
    pub fn clear_details(&mut self) {
        if !self.details.is_empty() {
            self.details.clear();
            self.cache.clear();
            self.frame = None;
        }
    }

    /// What is at page position `x, y`: the document offset, plus a link or footnote reference when
    /// the point is over one. Lays out the one block that was hit if the frame has not painted it
    /// yet, so this works before or after [`view_frame`](Self::view_frame) for that row.
    pub fn hit_test(
        &mut self,
        doc: &Document,
        options: &Options,
        page: &PageLayout,
        x: f32,
        y: f32,
    ) -> Option<Hit> {
        let index = page.index_at(y)?;
        let span = page.spans[index];
        self.ensure_block(doc, options, page, index);
        let cached = self.cache.get(&span.block)?;
        let local = y - span.y;
        let line = cached.lines.iter().min_by(|a, b| {
            distance(a, local)
                .total_cmp(&distance(b, local))
                .then_with(|| a.top.total_cmp(&b.top))
        })?;
        let offset = offset_at(line, x, local);
        let mut hit = Hit {
            block: span.block,
            offset,
            link: None,
            footnote: None,
            details: None,
        };
        if let Some(inner) = doc.block_at(offset) {
            let mut block = inner;
            loop {
                let current = &doc.blocks[block as usize];
                if matches!(current.kind, BlockKind::Summary)
                    && current.parent != NONE
                    && matches!(
                        doc.blocks[current.parent as usize].kind,
                        BlockKind::Details { .. }
                    )
                {
                    hit.details = Some(current.parent);
                    break;
                }
                if current.parent == NONE || matches!(current.kind, BlockKind::Details { .. }) {
                    break;
                }
                block = current.parent;
            }
            for i in doc.blocks[inner as usize].spans.usize() {
                let span = &doc.spans[i];
                match span.kind {
                    SpanKind::Link(link) if span.start <= offset && offset < span.end => {
                        hit.link = Some(link);
                    }
                    SpanKind::FootnoteRef(note) if span.start == offset => {
                        hit.footnote = Some(note);
                    }
                    _ => {}
                }
            }
        }
        Some(hit)
    }

    /// The page rectangles a selection between two document offsets paints. Multi-line ranges give
    /// one rect per line, and blocks that are not currently painted are laid out for the test. The
    /// selected pixels are the caller's to draw, so the frame delta stays about the document only.
    pub fn selection_rects(
        &mut self,
        doc: &Document,
        options: &Options,
        page: &PageLayout,
        from: u32,
        to: u32,
    ) -> Vec<Rect> {
        let (start, end) = if from <= to { (from, to) } else { (to, from) };
        if start == end {
            return Vec::new();
        }
        let mut rects = Vec::new();
        for (index, span) in page.spans.iter().enumerate() {
            let text = doc.blocks[span.block as usize].text;
            if text.end <= start || text.start >= end {
                continue;
            }
            self.ensure_block(doc, options, page, index);
            let Some(cached) = self.cache.get(&span.block) else {
                continue;
            };
            for line in &cached.lines {
                let first = start.max(line.doc_start);
                let last = end.min(line.doc_end);
                if first >= last {
                    continue;
                }
                let x0 = x_at(line, first);
                let x1 = x_at(line, last);
                if x1 - x0 > 0.25 {
                    rects.push(Rect {
                        x: x0,
                        y: span.y + line.top,
                        w: x1 - x0,
                        h: line.bottom - line.top,
                    });
                }
            }
        }
        rects
    }

    /// Lays out one block so its paint cache and caret stops exist. Blocks painted by a frame are
    /// already cached; this covers a click before the block has come on screen.
    fn ensure_block(&mut self, doc: &Document, options: &Options, page: &PageLayout, index: usize) {
        if self.cache.contains_key(&page.spans[index].block) {
            return;
        }
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            Some(&mut self.cache),
            &mut self.images,
            &self.details,
        );
        pager.block_paints(page, index);
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
            &self.details,
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
    /// Inline byte range -> offset in `Document::text`, so a caret position can be turned back into
    /// a document position. `fixed` ranges stand for an atom (an image placeholder, a footnote
    /// mark): every offset in them maps to the atom's one position.
    origins: Vec<(Range<usize>, u32, bool)>,
}

/// One inline image: its text offset, the image it draws, its size in device px and, for math, the
/// distance from its top to the baseline.
#[derive(Clone, Copy)]
struct ImageBox {
    at: usize,
    index: u32,
    width: f32,
    height: f32,
    baseline: Option<f32>,
    /// End of the TeX source in the document for a math box; zero for a file image.
    math_end: u32,
}

/// Math images live above the file-image indices in the shared cache, keyed by the source's
/// document offset (there is at most one math span per offset).
const MATH_INDEX: u32 = 0x8000_0000;

fn math_index(offset: u32) -> u32 {
    MATH_INDEX | (offset & 0x7fff_ffff)
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

    /// Pushes a slice copied from the document, remembering the document offset it starts at.
    fn push_doc(&mut self, s: &str, style: Style, at: u32) {
        if s.is_empty() {
            return;
        }
        let start = self.text.len();
        self.push(s, style);
        let end = self.text.len();
        self.origins.push((start..end, at, false));
    }

    /// Pushes text that stands for the atom at `at`, which has no text of its own in the document.
    fn push_atom(&mut self, s: &str, style: Style, at: u32) {
        if s.is_empty() {
            return;
        }
        let start = self.text.len();
        self.push(s, style);
        let end = self.text.len();
        self.origins.push((start..end, at, true));
    }

    /// The document offset for an inline byte offset, or `None` for text the renderer invented
    /// (a list marker, a language label, an alert title) that is not part of the document.
    fn doc_at(&self, at: usize) -> Option<u32> {
        let mut last = None;
        for (range, origin, fixed) in &self.origins {
            if at < range.start {
                continue;
            }
            if at < range.end {
                return Some(if *fixed {
                    *origin
                } else {
                    origin + (at - range.start) as u32
                });
            }
            last = Some(if *fixed {
                *origin
            } else {
                origin + range.len() as u32
            });
        }
        last
    }

    /// The inline byte offset for a document offset, the inverse of [`doc_at`](Self::doc_at). An
    /// atom maps to the start of the text that stands for it.
    fn inline_at(&self, doc: u32) -> Option<usize> {
        for (range, origin, fixed) in &self.origins {
            if *fixed {
                if *origin == doc {
                    return Some(range.start);
                }
            } else if doc >= *origin && doc <= *origin + range.len() as u32 {
                return Some(range.start + (doc - *origin) as usize);
            }
        }
        None
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

/// One laid-out line: where it sits, what text it covers and the layout that owns it, so a click
/// can be resolved back to a document offset without keeping a caret table per glyph.
#[derive(Clone)]
struct LineGeom {
    top: f32,
    bottom: f32,
    /// The layout this line belongs to, and where its origin sits in page coordinates.
    layout: Rc<Layout<Brush>>,
    source: Rc<Inline>,
    x: f32,
    y: f32,
    /// The line's inline byte range.
    end: usize,
    /// The line's x range: start and end of the text, in page coordinates.
    x0: f32,
    x1: f32,
    /// The document offsets the line covers.
    doc_start: u32,
    doc_end: u32,
}

impl LineGeom {
    fn shift(&mut self, dy: f32) {
        self.top += dy;
        self.bottom += dy;
        self.y += dy;
    }
}

/// Everything a top-level block keeps for later frames: what to paint and where its text is.
#[derive(Clone, Default)]
struct CachedBlock {
    paints: Vec<Paint>,
    lines: Vec<LineGeom>,
}

/// Vertical distance from `y` to a line, zero inside it.
fn distance(line: &LineGeom, y: f32) -> f32 {
    if y < line.top {
        line.top - y
    } else if y > line.bottom {
        y - line.bottom
    } else {
        0.0
    }
}

/// The page x of a document offset on a line, from the line's own layout.
fn x_at(line: &LineGeom, offset: u32) -> f32 {
    if offset <= line.doc_start {
        return line.x0;
    }
    if offset >= line.doc_end {
        return line.x1;
    }
    let Some(at) = line.source.inline_at(offset) else {
        return line.x0;
    };
    if at >= line.end {
        return line.x1;
    }
    let Some(cluster) = Cluster::from_byte_index(&line.layout, at) else {
        return line.x0;
    };
    line.x + cluster.visual_offset().unwrap_or(0.0)
}

/// The document offset under a point on a line, using the line's own layout for the search.
fn offset_at(line: &LineGeom, x: f32, y: f32) -> u32 {
    let px = (x - line.x).max(0.0);
    let py = y - line.y;
    let Some((cluster, side)) = Cluster::from_point(&line.layout, px, py) else {
        return line.doc_start;
    };
    let range = cluster.text_range();
    let at = if side == ClusterSide::Right {
        range.end
    } else {
        range.start
    };
    line.source.doc_at(at).unwrap_or(line.doc_start)
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
    /// Caret geometry for the block being laid out, in page coordinates until it moves into the
    /// cache, where it is stored relative to that block's top like the paints.
    hits: Vec<LineGeom>,
    /// Whether to keep hit geometry; the height pass does not need it.
    record: bool,
    /// The measured y of each footnote entry, built by the height pass.
    notes: Vec<(u32, f32)>,
    fonts: &'b mut FontContext,
    layouts: &'b mut LayoutContext<Brush>,
    cache: Option<&'b mut HashMap<u32, CachedBlock>>,
    base_dir: Option<PathBuf>,
    /// Decoded images, owned by the `DocumentRenderer` so cached frames keep theirs. `None`
    /// remembers a file that will not load.
    images: &'b mut HashMap<u32, Option<Rc<DecodedImage>>>,
    /// `<details>` open overrides from the shell; empty means use the document.
    details: &'b HashMap<u32, bool>,
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
        cache: Option<&'b mut HashMap<u32, CachedBlock>>,
        images: &'b mut HashMap<u32, Option<Rc<DecodedImage>>>,
        details: &'b HashMap<u32, bool>,
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
        let record = cache.is_some();
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
            hits: Vec::new(),
            record,
            notes: Vec::new(),
            fonts,
            layouts,
            cache,
            base_dir: options.base_dir.clone(),
            images,
            details,
        }
    }

    /// Whether a `<details>` shows its body: the shell's toggle if there is one, else the source.
    fn details_open(&self, block: u32) -> bool {
        self.details
            .get(&block)
            .copied()
            .unwrap_or_else(|| self.doc.details_open_as_written(block))
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
            BlockKind::Math { .. } => {
                if !self.math_block(b, left, width) {
                    self.code_block(b, left, width, Some("math".to_string()));
                }
            }
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
            let (mut width, mut height, mut baseline) = (image.width, image.height, image.baseline);
            if image.math_end != 0 {
                // The probe in `push_atom` used the body size. A heading, table cell or footnote
                // asks for another size; render at that one so it stays sharp.
                let target = (txt.size * 2.0 * self.scale).clamp(8.0, 512.0);
                let probe = self.theme.body_size * 2.0 * self.scale;
                let start = (image.index & 0x7fff_ffff) as usize;
                if (target - probe).abs() > 0.01 {
                    let source = self
                        .doc
                        .text
                        .get(start..image.math_end as usize)
                        .unwrap_or("")
                        .to_string();
                    let decoded =
                        engine::render_math(&source, false, target, txt.color).map(Rc::new);
                    self.images.insert(image.index, decoded);
                }
                if let Some(Some(math)) = self.images.get(&image.index) {
                    width = math.width as f32 / 2.0;
                    height = math.height as f32 / 2.0;
                    baseline = Some(math.baseline as f32 / 2.0);
                } else {
                    width = 0.0;
                    height = 0.0;
                    baseline = None;
                }
            }
            if let Some(max) = txt.max
                && width > max
            {
                let shrink = max / width;
                height *= shrink;
                baseline = baseline.map(|b| b * shrink);
                width = max;
            }
            builder.push_inline_box(InlineBox {
                id: image.index as u64,
                kind: InlineBoxKind::InFlow,
                index: image.at,
                width,
                height,
                baseline,
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
        let layout = Rc::new(layout);
        // Only document text is hittable; a list marker or a language label is drawn but has no
        // document offsets to select.
        let track = self.record && !inline.origins.is_empty();
        let source = Rc::new(inline.clone());
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
        let mut lines = Vec::new();
        for line in layout.lines() {
            let metrics = line.metrics();
            let line_height = metrics.line_height.max(1.0);
            let baseline = y + metrics.baseline;
            let top = baseline - line_height * 0.82;
            let bottom = baseline + line_height * 0.18;
            if track && let Some(doc_start) = inline.doc_at(line.text_range().start) {
                lines.push(LineGeom {
                    top,
                    bottom,
                    layout: layout.clone(),
                    source: source.clone(),
                    x,
                    y,
                    end: line.text_range().end,
                    x0: x + metrics.offset,
                    x1: x + metrics.offset + metrics.advance,
                    doc_start,
                    doc_end: inline.doc_at(line.text_range().end).unwrap_or(doc_start),
                });
            }
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
        if track {
            self.hits.extend(lines);
        }
        self.paints.extend(images);
        self.paints.extend(chips);
        self.paints.push(Paint::Text { layout, x, y });
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

    /// Draws a `$$...$$` block as a centred formula. `false` hands it back to the source view when
    /// there is no engine or TeX will not parse.
    fn math_block(&mut self, b: u32, left: f32, width: f32) -> bool {
        let block = self.doc.blocks[b as usize];
        if matches!(block.kind, BlockKind::Math { oversize: Some(_) }) {
            return false;
        }
        let (start, end) = (block.text.start, block.text.end);
        let source = self.doc.text[start as usize..end as usize].trim();
        if source.is_empty() {
            return false;
        }
        let key = math_index(start);
        if !self.images.contains_key(&key) {
            // Display style runs a little larger than body text, and twice that keeps it crisp.
            let size = self.theme.body_size * 1.15 * 2.0 * self.scale;
            let decoded = engine::render_math(source, true, size, self.theme.text).map(Rc::new);
            self.images.insert(key, decoded);
        }
        let Some((image_w, image_h)) = self
            .images
            .get(&key)
            .and_then(|slot| slot.as_ref())
            .map(|image| (image.width as f32 / 2.0, image.height as f32 / 2.0))
        else {
            return false;
        };
        let avail = (width - 2.0 * self.px(16.0)).max(1.0);
        let shrink = (avail / image_w).min(1.0);
        let (w, h) = (image_w * shrink, image_h * shrink);
        let x = left + ((width - w) / 2.0).round();
        let y = self.y + self.px(10.0);
        self.paints.push(Paint::Image {
            index: key,
            x,
            y,
            w,
            h,
        });
        self.y = y + h + self.px(18.0);
        true
    }

    fn code_block(&mut self, b: u32, left: f32, width: f32, label: Option<String>) {
        let source = self.doc.block_text(b).trim_end_matches('\n').to_string();
        let text_start = self.doc.blocks[b as usize].text.start;
        let mut inline = Inline::default();
        inline.push_doc(
            &source,
            Style {
                code: true,
                ..Style::default()
            },
            text_start,
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
        let open = self.details_open(b);
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
            if inline.text.is_empty() && inline.boxes.is_empty() {
                continue;
            }
            if !first {
                merged.push(" ", Style::default());
            }
            let start = merged.text.len();
            for (range, style) in &inline.runs {
                let at = inline.doc_at(range.start).unwrap_or(0);
                merged.push_doc(&inline.text[range.clone()], *style, at);
            }
            // `inline_of` measured boxes in this leaf's own text; shift them into the merged text.
            for mut image in inline.boxes {
                image.at += start;
                merged.boxes.push(image);
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
                            let at = body.doc_at(range.start).unwrap_or(0);
                            inline.push_doc(&body.text[range.clone()], *style, at);
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
        for (number, inline) in entries {
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
            self.notes.push((number, self.y));
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
        let mut atoms: Vec<(usize, usize, SpanKind)> = Vec::new();
        let mut math_ranges: Vec<(usize, usize)> = Vec::new();
        for span in &self.doc.spans[block.spans.usize()] {
            let (start, stop) = (span.start as usize, span.end as usize);
            if start == stop {
                if start >= base && start <= end {
                    atoms.push((start, start, span.kind));
                    cuts.push(start);
                }
                continue;
            }
            let (start, stop) = (start.max(base), stop.min(end));
            if start < stop {
                cuts.push(start);
                cuts.push(stop);
                if matches!(span.kind, SpanKind::Math { display: false, .. }) {
                    // Inline math becomes one image atom: no source text reaches the line.
                    atoms.push((start, stop, span.kind));
                    math_ranges.push((start, stop));
                } else {
                    spans.push((start, stop, span.kind));
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Inline::default();
        for i in 0..cuts.len() {
            let at = cuts[i];
            for (position, end, kind) in &atoms {
                if *position == at {
                    self.push_atom(&mut out, *kind, *position as u32, *end as u32);
                }
            }
            if i + 1 == cuts.len() {
                continue;
            }
            let to = cuts[i + 1];
            if to <= at {
                continue;
            }
            if math_ranges
                .iter()
                .any(|(start, end)| at >= *start && at < *end)
            {
                continue;
            }
            let style = self.style_at(&spans, at);
            out.push_doc(&src[at - base..to - base], style, at as u32);
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

    fn push_atom(&mut self, out: &mut Inline, kind: SpanKind, at: u32, end: u32) {
        match kind {
            SpanKind::Math {
                display: false,
                oversize,
            } => {
                let key = math_index(at);
                if oversize.is_none() && !self.images.contains_key(&key) {
                    let source = self.doc.text.get(at as usize..end as usize).unwrap_or("");
                    // Twice the body size keeps the raster crisp when a heading or a zoom
                    // enlarges it; the box and baseline are halved back in `text_layout`.
                    let size = self.theme.body_size * 2.0 * self.scale;
                    let decoded =
                        engine::render_math(source, false, size, self.theme.text).map(Rc::new);
                    self.images.insert(key, decoded);
                }
                if oversize.is_none()
                    && let Some(Some(image)) = self.images.get(&key)
                {
                    let width = image.width as f32 / 2.0;
                    let height = image.height as f32 / 2.0;
                    let baseline = image.baseline as f32 / 2.0;
                    out.boxes.push(ImageBox {
                        at: out.text.len(),
                        index: key,
                        width,
                        height,
                        baseline: Some(baseline),
                        math_end: end,
                    });
                    return;
                }
                // No engine, oversize source, or TeX it could not parse: keep it readable.
                let source = self.doc.text.get(at as usize..end as usize).unwrap_or("");
                out.push_doc(
                    source,
                    Style {
                        math: true,
                        ..Style::default()
                    },
                    at,
                );
            }
            SpanKind::Image(index) => {
                if let Some(image) = self.image(index) {
                    let width = image.width as f32 * self.scale;
                    let height = image.height as f32 * self.scale;
                    out.boxes.push(ImageBox {
                        at: out.text.len(),
                        index,
                        width,
                        height,
                        baseline: None,
                        math_end: 0,
                    });
                    return;
                }
                let alt = self.doc.str(self.doc.images[index as usize].alt);
                if alt.is_empty() {
                    out.push_atom(
                        "[image]",
                        Style {
                            muted: true,
                            ..Style::default()
                        },
                        at,
                    );
                } else {
                    out.push_atom(
                        &format!("[image: {alt}]"),
                        Style {
                            muted: true,
                            italic: true,
                            ..Style::default()
                        },
                        at,
                    );
                }
            }
            SpanKind::FootnoteRef(index) => {
                let number = self.doc.footnotes[index as usize].number;
                out.push_atom(
                    &format!("[{number}]"),
                    Style {
                        link: true,
                        small: true,
                        ..Style::default()
                    },
                    at,
                );
            }
            SpanKind::Checkbox { checked } => {
                if out.text.is_empty() && out.task.is_none() {
                    out.task = Some(checked);
                } else {
                    // Only a task item starts with its box; anything else keeps a readable mark.
                    out.push_atom(
                        if checked { "[x] " } else { "[ ] " },
                        Style {
                            muted: true,
                            ..Style::default()
                        },
                        at,
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
        self.hits.clear();
        let total_height = self.y + self.px(24.0);
        PageLayout {
            spans,
            notes: self.notes,
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
        let bottom = top + height;
        for index in 0..page.spans.len() {
            let span = page.spans[index];
            if span.y + span.height <= top || span.y >= bottom {
                continue;
            }
            self.block_paints(page, index);
        }
        if page.footnotes_height > 0.0
            && page.footnotes_y + page.footnotes_height > top
            && page.footnotes_y < bottom
        {
            self.y = page.footnotes_y;
            self.notes.clear();
            self.footnotes();
        }
    }

    /// Paints one top-level block, its paints and caret stops cached relative to its top so scroll
    /// frames only move them. The cache is the renderer's, so hit tests can read it later.
    fn block_paints(&mut self, page: &PageLayout, index: usize) {
        let span = page.spans[index];
        let content = self.content_width();
        if let Some(cache) = self.cache.as_deref()
            && let Some(cached) = cache.get(&span.block)
        {
            let mut paints = cached.paints.clone();
            for paint in &mut paints {
                paint.shift(span.y);
            }
            self.paints.extend(paints);
            return;
        }
        let paints_mark = self.paints.len();
        let hits_mark = self.hits.len();
        self.y = span.y;
        self.block(span.block, self.left, content, self.theme.text, 0);
        if let Some(cache) = self.cache.as_deref_mut() {
            let mut made = self.paints.split_off(paints_mark);
            let mut lines = self.hits.split_off(hits_mark);
            for paint in &mut made {
                paint.shift(-span.y);
            }
            for line in &mut lines {
                line.shift(-span.y);
            }
            cache.insert(
                span.block,
                CachedBlock {
                    paints: made.clone(),
                    lines,
                },
            );
            for paint in &mut made {
                paint.shift(span.y);
            }
            self.paints.extend(made);
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
