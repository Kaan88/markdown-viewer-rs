//! Document and image limits carried over from the old app.

use std::fmt;

/// Checked against the bytes on disk, which is the limit a user can see.
pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
/// The most text a file within `MAX_FILE_BYTES` can decode to: a replaced invalid byte and a UTF-16
/// CJK unit both grow to three bytes per byte at worst. Every model offset still fits `u32`.
pub const MAX_DECODED_BYTES: usize = 3 * MAX_FILE_BYTES;
pub const MAX_DIAGRAM_BYTES: usize = 64 * 1024;
pub const MAX_EQUATION_BYTES: usize = 4 * 1024;

pub const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 24_000_000;
pub const MAX_SVG_WIDTH: u32 = 8000;
pub const MAX_SVG_HEIGHT: u32 = 12000;
pub const IMAGE_CACHE_BYTES: u64 = 64 * 1024 * 1024;
pub const IMAGE_TIMEOUT_MS: u32 = 10_000;

pub const FIND_CAP: usize = 10_000;
/// Character comparisons find may spend per byte of text before it stops and reports itself capped.
/// A query this long or shorter can never reach that, so its results are always exact. Only a longer
/// query on text that nearly matches it everywhere can.
pub const FIND_WORK_PER_BYTE: usize = 64;

/// Open HTML elements beyond which further ones are ignored, the depth Chromium also stops at. It
/// keeps every walk of the open-element stack short whatever the input.
pub const MAX_HTML_DEPTH: usize = 512;

/// Largest pixel width or height taken from an HTML attribute, the same as the SVG height limit.
pub const MAX_HTML_LENGTH_PX: u32 = MAX_SVG_HEIGHT;
/// GitHub clamps `<img>` width and height to this, keeping their ratio. Table attributes are not
/// clamped this way.
pub const MAX_IMAGE_DIMENSION_PX: u32 = 2000;
pub const MAX_COLSPAN: u16 = 1000;
pub const MAX_ROWSPAN: u16 = 65534;

/// Containers nested deeper than this add no block: what they hold joins the deepest one, text kept.
/// Rows, cells, items and leaf blocks of a container already open may go up to five levels past it
/// (an implied table row, its cell, and a leaf inside), which is the bound the fuzz test asserts.
/// Layout inherits this bound, and real documents stay far under it.
pub const MAX_BLOCK_DEPTH: usize = 256;

/// Why a block keeps showing its source instead of being rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Oversize {
    Diagram,
    Equation,
    /// A Mermaid family the engine is built without. Not a size, but the same outcome: the source
    /// stays on the page, so find sees it and no engine job is made for it.
    UnsupportedDiagram,
}

/// Mermaid families the engine does not draw (`crates/engine/Cargo.toml` leaves them out).
const UNSUPPORTED_DIAGRAMS: [&str; 3] = ["architecture", "wardley", "info"];

/// Whether a Mermaid source opens with a family the engine does not draw. The family is the first
/// word after any front matter, `%%` comment or directive lines and blank lines; a `-beta` suffix
/// names the same family.
pub fn unsupported_diagram(source: &str) -> bool {
    let mut lines = source.lines().map(str::trim);
    let mut in_front_matter = false;
    let mut first = true;
    let word = loop {
        let Some(line) = lines.next() else {
            return false;
        };
        if line == "---" && (first || in_front_matter) {
            in_front_matter = !in_front_matter;
            first = false;
            continue;
        }
        first = false;
        if in_front_matter || line.is_empty() || line.starts_with("%%") {
            continue;
        }
        break line
            .split(|c: char| c.is_whitespace() || c == ':' || c == ';')
            .next()
            .unwrap_or("");
    };
    let family = word.strip_suffix("-beta").unwrap_or(word);
    UNSUPPORTED_DIAGRAMS.contains(&family)
}

impl fmt::Display for Oversize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Oversize::Diagram => "Diagram exceeds the 64 KiB limit",
            Oversize::Equation => "Equation exceeds the 4 KiB limit",
            Oversize::UnsupportedDiagram => "This kind of diagram is not supported",
        })
    }
}
