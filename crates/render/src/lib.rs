//! Headless document renderer: layout and CPU raster for both shells. The painter is independent of
//! any window system, so it runs in tests and in `mdv-bench` without a display.

mod engine;
mod image;
mod layout;
mod theme;

use mdshared_highlight as highlight;
use mdshared_text::{bitmap, text};

pub use bitmap::Bitmap;
pub use image::{DecodedImage, decode_image};
pub use layout::{
    BlockBox, DocumentRenderer, Frame, Hit, ImageStats, Jump, Options, PageLayout, Rect,
    RemoteState, Rendered, Slice, Source, SourceKind, layout, render, render_slice,
};
pub use mdshared_text::shared_fonts;
pub use text::{CacheStats, Subpixel, TextEngine};
pub use theme::{Color, DARK, LIGHT, Theme};
