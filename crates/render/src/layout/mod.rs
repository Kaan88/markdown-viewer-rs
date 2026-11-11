//! Lays the document model out in one top-down pass and paints it into a bitmap: blocks are
//! virtualized, scroll frames reuse the previous bitmap's rows, and every laid-out text run keeps
//! its caret stops so a click can be turned back into a document offset, a link or a selection.
//! Images are drawn through parley inline boxes; math and diagrams are engine-rendered images.
//!
//! The pieces: [`page`] holds [`PageLayout`] and the height pass, [`frame`] the scroll frame,
//! [`hit`] and [`selection`] the caret geometry, [`pager`] the layout walk and text emission,
//! [`inline`] the model-to-runs conversion, [`paint`] the paint list and raster, and [`prose`],
//! [`containers`], [`table`] and [`media`] the block kinds.

mod containers;
mod frame;
mod hit;
mod inline;
mod media;
mod page;
mod pager;
mod paint;
mod prose;
mod selection;
mod store;
mod table;

pub use page::PageLayout;
pub use store::ImageStats;

use crate::bitmap::Bitmap;
use crate::text::{Brush, TextEngine};
use crate::theme::{DARK, LIGHT, Theme};
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
    /// Every raster drawn, kept across layouts, widths, zooms and documents.
    store: ImageStore,
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

thread_local! {
    /// System fonts are enumerated once per thread; every renderer starts from a clone of this. A
    /// window makes several renderers (one per tab and a spare for `mem::take`), and enumerating
    /// fonts for each cost about 35 ms apiece at launch.
    static FONTS: std::cell::RefCell<Option<FontContext>> = const { std::cell::RefCell::new(None) };
}

/// The per-thread font context every renderer clones. A shell that draws its own text takes a
/// clone too, so system fonts are still enumerated once.
pub fn shared_fonts() -> FontContext {
    FONTS.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(FontContext::new)
            .clone()
    })
}

impl DocumentRenderer {
    pub fn new() -> Self {
        DocumentRenderer {
            fonts: shared_fonts(),
            layouts: LayoutContext::new(),
            engine: TextEngine::new(4 << 20),
            cache: HashMap::new(),
            cache_key: None,
            store: ImageStore::new(mdview_core::limits::IMAGE_CACHE_BYTES),
            details: HashMap::new(),
            frame: None,
        }
    }

    /// Measures the whole document. Do this once per width, scale and theme.
    pub fn layout(&mut self, doc: &Document, options: &Options) -> PageLayout {
        self.cache.clear();
        self.frame = None;
        self.cache_key = Some((options.width, options.scale, options.theme.name));
        Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            None,
            &mut self.store,
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
            self.cache_key = Some(key);
            self.frame = None;
        }
        self.store.begin_frame();
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            Some(&mut self.cache),
            &mut self.store,
            &self.details,
        );
        pager.slice_page(page, top, height);
        pager.finish_slice(top, height, page.total_height, &mut self.engine)
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

    /// Paints the whole page into one bitmap. For tests and small documents; scrolling uses
    /// [`layout`](Self::layout) and [`view`](Self::view).
    pub fn page(&mut self, doc: &Document, options: &Options) -> Rendered {
        self.store.begin_frame();
        let mut pager = Pager::new(
            doc,
            options,
            &mut self.fonts,
            &mut self.layouts,
            None,
            &mut self.store,
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
