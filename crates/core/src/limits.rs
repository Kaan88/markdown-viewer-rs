//! Document and image limits carried over from the old app.

use std::fmt;

pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DIAGRAM_BYTES: usize = 64 * 1024;
pub const MAX_EQUATION_BYTES: usize = 4 * 1024;

pub const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 24_000_000;
pub const MAX_SVG_WIDTH: u32 = 8000;
pub const MAX_SVG_HEIGHT: u32 = 12000;
pub const IMAGE_CACHE_BYTES: u64 = 64 * 1024 * 1024;
pub const IMAGE_TIMEOUT_MS: u32 = 10_000;

pub const FIND_CAP: usize = 10_000;

/// Open HTML elements beyond which further ones are ignored, the depth Chromium also stops at. It
/// keeps every walk of the open-element stack short whatever the input.
pub const MAX_HTML_DEPTH: usize = 512;

/// Why a block keeps showing its source instead of being rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Oversize {
    Diagram,
    Equation,
}

impl fmt::Display for Oversize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Oversize::Diagram => "Diagram exceeds the 64 KiB limit",
            Oversize::Equation => "Equation exceeds the 4 KiB limit",
        })
    }
}
