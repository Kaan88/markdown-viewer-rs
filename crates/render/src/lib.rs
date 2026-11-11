//! Headless document renderer: layout and CPU raster for both shells. The painter is independent of
//! any window system, so it runs in tests and in `mdv-bench` without a display.

mod bitmap;
mod engine;
mod highlight;
mod image;
mod layout;
mod text;
mod theme;

pub use bitmap::Bitmap;
pub use layout::{
    BlockBox, DocumentRenderer, Frame, Hit, Options, PageLayout, Rect, Rendered, Slice, layout,
    render, render_slice, render_view, shared_fonts,
};
pub use text::{CacheStats, TextEngine};
pub use theme::{Color, DARK, LIGHT, Theme};
