//! The page's colours and metrics, taken from the Phase 0 look spike (`spikes/look-shared`), where
//! the user approved the look. Values are logical px; the renderer multiplies them by the device
//! scale. Font families are comma lists so each platform's font stack can differ.

pub type Color = [u8; 3];

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Color,
    pub text: Color,
    pub heading: Color,
    pub muted: Color,
    pub accent: Color,
    pub border: Color,
    pub code_bg: Color,
    pub head_bg: Color,
    pub tint: Color,
    pub kw: Color,
    pub string: Color,

    pub sans: &'static str,
    pub mono: &'static str,

    pub body_size: f32,
    pub body_line_height: f32,
    /// Side margin and the widest line the reader is meant to scan.
    pub margin: f32,
    pub reading_width: f32,
}

pub const LIGHT: Theme = Theme {
    name: "light",
    bg: [0xfb, 0xfb, 0xfb],
    text: [0x24, 0x24, 0x24],
    heading: [0x1a, 0x1a, 0x1a],
    muted: [0x5c, 0x5c, 0x5c],
    accent: [0x00, 0x5f, 0xb8],
    border: [0xe0, 0xe0, 0xe0],
    code_bg: [0xf3, 0xf3, 0xf3],
    head_bg: [0xf3, 0xf3, 0xf3],
    tint: [0xee, 0xf4, 0xfb],
    kw: [0x8a, 0x2b, 0xb0],
    string: [0x0f, 0x6e, 0x3c],
    sans: "Segoe UI, Noto Sans, Cantarell, DejaVu Sans, sans-serif",
    mono: "Consolas, Cascadia Mono, Noto Sans Mono, DejaVu Sans Mono, monospace",
    body_size: 16.0,
    body_line_height: 1.6,
    margin: 40.0,
    reading_width: 700.0,
};

pub const DARK: Theme = Theme {
    name: "dark",
    bg: [0x20, 0x20, 0x20],
    text: [0xe4, 0xe4, 0xe4],
    heading: [0xf5, 0xf5, 0xf5],
    muted: [0xa6, 0xa6, 0xa6],
    accent: [0x60, 0xcd, 0xff],
    border: [0x3d, 0x3d, 0x3d],
    code_bg: [0x2b, 0x2b, 0x2b],
    head_bg: [0x2b, 0x2b, 0x2b],
    tint: [0x24, 0x30, 0x3a],
    kw: [0xd2, 0xa8, 0xff],
    string: [0x8b, 0xd6, 0x9c],
    sans: "Segoe UI, Noto Sans, Cantarell, DejaVu Sans, sans-serif",
    mono: "Consolas, Cascadia Mono, Noto Sans Mono, DejaVu Sans Mono, monospace",
    body_size: 16.0,
    body_line_height: 1.6,
    margin: 40.0,
    reading_width: 700.0,
};

impl Theme {
    pub fn is_dark(&self) -> bool {
        self.name == "dark"
    }
}
