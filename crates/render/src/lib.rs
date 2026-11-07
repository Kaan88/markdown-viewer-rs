//! Headless document renderer: layout and CPU raster for both shells. The painter is independent of
//! any window system, so it runs in tests and in `mdv-bench` without a display.

mod bitmap;
mod image;
mod layout;
mod text;
mod theme;

pub use bitmap::Bitmap;
pub use layout::{
    BlockBox, DocumentRenderer, Frame, Options, PageLayout, Rendered, Slice, layout, render,
    render_slice, render_view,
};
pub use text::{CacheStats, TextEngine};
pub use theme::{Color, DARK, LIGHT, Theme};
