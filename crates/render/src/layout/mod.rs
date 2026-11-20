//! Lays the document model out block by block and paints it into a bitmap: blocks are
//! virtualized, scroll frames reuse the previous bitmap's rows, and every laid-out text run keeps
//! its caret stops so a click can be turned back into a document offset, a link or a selection.
//! Images are drawn through parley inline boxes; math and diagrams are engine-rendered images.
//!
//! The pieces: [`page`] holds [`PageLayout`] (per-block heights, derived positions),
//! [`relayout`] measures blocks and corrects the layout in place around the viewport's anchor,
//! [`estimate`] guesses heights without shaping, [`refine`] measures the rest in idle slices,
//! [`frame`] draws the scroll frame, [`hit`] and [`selection`] the caret geometry, [`store`] keeps
//! every raster across layouts, [`animate`] moves animated images in the kept frame, [`pager`] the layout walk and text emission, [`inline`] the
//! model-to-runs conversion, [`paint`] the paint list and raster, and [`prose`], [`containers`],
//! [`table`] and [`media`] the block kinds, and [`floats`] the floated images that reach across
//! blocks and the state that lets a block beside one be laid out on its own.

mod animate;
mod containers;
mod estimate;
mod floats;
mod frame;
mod hit;
mod inline;
mod media;
mod page;
mod pager;
mod paint;
mod prose;
mod refine;
mod relayout;
mod selection;
mod store;
mod table;

pub use animate::AnimationTick;
pub use page::PageLayout;
pub use relayout::{Jump, RemoteState};
pub use store::ImageStats;

use crate::bitmap::Bitmap;
use crate::text::{Brush, TextEngine};
use crate::theme::{DARK, LIGHT, Theme};
use mdshared_text::shared_fonts;
use mdview_core::model::Document;
use pager::Pager;
use paint::CachedBlock;
use parley::{FontContext, LayoutContext};
use std::collections::HashMap;
use std::path::PathBuf;
use store::ImageStore;

#[derive(Clone)]
pub struct Options {
    /// Logical page width, before the device scale.
    pub width: f32,
    pub scale: f32,
    pub theme: Theme,
    /// Stop laying out past this many logical px. The bitmap is clipped to it. Only
    /// [`DocumentRenderer::page`] honours it; the scrolling path lays out what is shown.
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

    /// Whether a layout made for `self` serves `other`: same width, scale, theme and image
    /// directory.
    fn same_layout(&self, other: &Options) -> bool {
        self.width == other.width
            && (self.scale - other.scale).abs() < 0.001
            && self.theme.key() == other.theme.key()
            && self.base_dir == other.base_dir
    }
}

/// One top-level block's vertical extent in device px, for scrolling and hit tests later.
#[derive(Debug, Clone, Copy)]
pub struct BlockBox {
    pub block: u32,
    pub y: f32,
    pub height: f32,
}

/// Top-level blocks an editor shows as their source, in place: those that start on a source line
/// from `first_line` to `last_line` (from 1, both included). The first of them is laid out as
/// empty room `height` device px tall and the others take no room; none of them is drawn or hit.
/// Set with [`DocumentRenderer::set_gap`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gap {
    pub first_line: u32,
    pub last_line: u32,
    pub height: f32,
}

impl Gap {
    pub(super) fn covers(&self, doc: &Document, block: u32) -> bool {
        let line = doc.blocks.get(block as usize).map_or(0, |b| b.line);
        (self.first_line..=self.last_line).contains(&line)
    }

    /// The first top-level block it covers. Blocks are in source order, so a binary search finds
    /// the first block on or after `first_line`; when that one is nested, the top-level block
    /// after its parent is the candidate.
    pub(super) fn lead(&self, doc: &Document) -> Option<u32> {
        let at = doc.blocks.partition_point(|b| b.line < self.first_line) as u32;
        let mut top = page::top_level(doc, at)?;
        if top < at {
            top = doc.blocks[top as usize].end;
        }
        (top < doc.blocks.len() as u32 && self.covers(doc, top)).then_some(top)
    }
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub block: u32,
    pub offset: u32,
    pub link: Option<u32>,
    pub footnote: Option<u32>,
    /// The `<details>` block whose summary is under the point; clicking toggles it.
    pub details: Option<u32>,
    /// The block whose task-list box is under the point.
    pub task: Option<u32>,
    /// The formula or diagram drawn at the point, when it is inside the picture itself.
    pub source: Option<Source>,
    /// The code box the point is in, whether on its text or beside it.
    pub code: Option<Code>,
    /// Whether the point is on a line of text, rather than beside, above or below the nearest
    /// one. `offset` is the nearest position either way, which is what a drag wants; a link or
    /// footnote is reported only when the point is on it.
    pub on_text: bool,
}

/// What a rendered picture was made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// Inline or display math; the text is TeX.
    Formula,
    /// A Mermaid block; the text is its source.
    Diagram,
}

/// The source of a formula or diagram: the range of `Document::text` it was drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    pub kind: SourceKind,
    pub start: u32,
    pub end: u32,
}

/// A code box: where it is drawn and the range of `Document::text` its code was taken from (the
/// block's text, which ends in the fence's newline). Kept in the cache relative to its block like
/// the lines; in a [`Hit`] it is in page coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Code {
    pub rect: Rect,
    pub start: u32,
    pub end: u32,
    /// How far in from the box's right edge the language label reaches, 0 without one.
    pub tag: f32,
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
/// the first frame, for any jump of a viewport or more, and whenever a correction changed what the
/// kept rows show.
pub struct Frame {
    pub bitmap: Bitmap,
    /// Absolute device-px row the bitmap's first row shows. A correction above the viewport moves
    /// it (the rows still show the same content); the shell takes it as its scroll position.
    pub top: f32,
    pub stats: crate::text::CacheStats,
    /// Rows of `bitmap`, in bitmap rows, that were painted this frame. An empty range means the
    /// frame is unchanged. `None` means the whole frame is new and nothing can be reused.
    pub band: Option<(u32, u32)>,
    /// Rows the content moved: row `r` of this frame was row `r + shift` of the previous one.
    pub shift: i32,
}

/// Measures the whole document exactly, without painting it, and hands the layout over.
pub fn layout(doc: &Document, options: &Options) -> PageLayout {
    let mut renderer = DocumentRenderer::new();
    renderer.layout_exact(doc, options);
    renderer.page.take().expect("laid out")
}

/// Holds the font and layout contexts and the glyph cache across frames, so scrolling a document
/// does not rebuild them (and keeps the glyph cache warm). One per window, or per benchmark.
///
/// It also owns the current document's [`PageLayout`]: every call that lays a block out (a
/// frame, a hit test, a selection, a jump, a `<details>` toggle, an image arriving, a refinement
/// slice) may correct that block's height in place. Corrections keep the viewport's anchor (a
/// block and an offset from its top) fixed, so content on screen does not jump; [`top`](Self::top)
/// is the viewport top in the corrected coordinates, which a shell adopts as its scroll position.
pub struct DocumentRenderer {
    fonts: FontContext,
    layouts: LayoutContext<Brush>,
    engine: TextEngine,
    /// Blocks shown by an editor as source; see [`Gap`].
    gap: Option<Gap>,
    /// Laid-out paints and caret stops per top-level block, relative to the block's top, for scroll
    /// frames and hit tests.
    cache: HashMap<u32, CachedBlock>,
    /// Every raster drawn, kept across layouts, widths, zooms and documents.
    store: ImageStore,
    /// `<details>` open overrides, by block, so a toggle is view state the document does not carry.
    details: HashMap<u32, bool>,
    /// The frame [`view_frame`](Self::view_frame) keeps between calls.
    frame: Option<Frame>,
    /// The current layout and the options it was made for.
    page: Option<PageLayout>,
    options: Option<Options>,
    view: relayout::ViewState,
    /// Which top-level blocks (by index) used each store id when last laid out, so an arriving
    /// image re-measures only those.
    uses: HashMap<u32, Vec<u32>>,
    /// The average glyph advances estimates use, for the scale and theme they were taken at.
    metrics: Option<(
        u32,
        (&'static str, &'static str, &'static str),
        estimate::Metrics,
    )>,
    /// Sorted document ranges of formulas and diagrams the engine failed to draw in the blocks
    /// laid out so far, and how many times that list has changed.
    undrawn: Vec<(u32, u32)>,
    undrawn_changes: u64,
    /// Which blocks can start a float and what each measured block leaves open for the next.
    floats: floats::FloatBook,
}

impl Default for DocumentRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentRenderer {
    pub fn new() -> Self {
        DocumentRenderer {
            fonts: shared_fonts(),
            layouts: LayoutContext::new(),
            engine: TextEngine::new(4 << 20),
            gap: None,
            cache: HashMap::new(),
            store: ImageStore::new(mdview_core::limits::IMAGE_CACHE_BYTES),
            details: HashMap::new(),
            frame: None,
            page: None,
            options: None,
            view: relayout::ViewState::default(),
            uses: HashMap::new(),
            metrics: None,
            undrawn: Vec::new(),
            undrawn_changes: 0,
            floats: floats::FloatBook::default(),
        }
    }

    /// The formulas and diagrams whose source the page shows because the engine could not draw
    /// them, as sorted ranges of `Document::text`, for
    /// [`find_shown`](mdview_core::find::find_shown). Only blocks laid out so far are known, so
    /// the list grows as the page is refined; the count says when to search again.
    pub fn undrawn(&self) -> (&[(u32, u32)], u64) {
        (&self.undrawn, self.undrawn_changes)
    }

    /// Shows the blocks `gap` covers as empty room, or none with `None`. The blocks it covered
    /// or covers now are measured again by the next frame; as with any correction, what is on
    /// screen above them stays where it is.
    pub fn set_gap(&mut self, doc: &Document, gap: Option<Gap>) {
        if gap == self.gap {
            return;
        }
        let old = std::mem::replace(&mut self.gap, gap);
        let Some(page) = self.page.as_ref() else {
            return;
        };
        let touched: Vec<usize> = page
            .spans
            .iter()
            .enumerate()
            .filter(|(_, s)| [old, gap].iter().flatten().any(|g| g.covers(doc, s.block)))
            .map(|(i, _)| i)
            .collect();
        for index in touched {
            self.invalidate(index);
        }
    }

    /// The gap's top on the page (device px), once its first block is laid out.
    pub fn gap_y(&self, doc: &Document) -> Option<f32> {
        let lead = self.gap?.lead(doc)?;
        let page = self.page.as_ref()?;
        page.index_of(lead).map(|i| page.spans[i].y)
    }

    /// The source lines of top-level block `block`: its first line, and the line the next
    /// top-level block starts on (`None` for the last), from 1. Blank lines before the next
    /// block are inside the range; an editor trims them.
    pub fn block_lines(&self, doc: &Document, block: u32) -> Option<(u32, Option<u32>)> {
        let page = self.page.as_ref()?;
        let index = page.index_of(block)?;
        let first = doc.blocks.get(block as usize)?.line;
        let next = page
            .spans
            .get(index + 1)
            .and_then(|s| doc.blocks.get(s.block as usize))
            .map(|b| b.line);
        Some((first, next))
    }

    /// The current layout, once [`layout`](Self::layout) has made one.
    pub fn page_layout(&self) -> Option<&PageLayout> {
        self.page.as_ref()
    }

    /// Decodes and engine jobs so far, and what the image store holds.
    pub fn image_stats(&self) -> ImageStats {
        self.store.stats()
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

    /// Draws text for this subpixel stripe order from now on (RGB until told otherwise). A change
    /// forgets what was drawn, since every cached row holds glyphs antialiased the old way.
    pub fn set_subpixel(&mut self, subpixel: crate::Subpixel) {
        if self.engine.subpixel() != subpixel {
            self.engine.set_subpixel(subpixel);
            self.drop_layout();
        }
    }

    /// Forgets every toggle, for a new document whose block numbers no longer mean the same. The
    /// layout goes with them; the next [`layout`](Self::layout) starts clean.
    pub fn clear_details(&mut self) {
        if !self.details.is_empty() {
            self.details.clear();
            self.drop_layout();
        }
    }

    /// Paints the whole page into one bitmap in one top-down pass. For tests, goldens and small
    /// documents; scrolling uses [`layout`](Self::layout) and [`view_frame`](Self::view_frame).
    pub fn page(&mut self, doc: &Document, options: &Options) -> Rendered {
        self.store.begin_frame();
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            false,
            &mut self.store,
            &self.details,
            self.gap,
        );
        pager.page();
        pager.finish(&mut self.engine)
    }
}

/// One-shot viewport render: an exact layout, then the rows `top..top + height`.
pub fn render_slice(doc: &Document, options: &Options, top: f32, height: f32) -> Slice {
    let mut renderer = DocumentRenderer::new();
    renderer.layout_exact(doc, options);
    renderer.view(doc, options, top, height)
}

pub fn render(doc: &Document, options: &Options) -> Rendered {
    DocumentRenderer::new().page(doc, options)
}
