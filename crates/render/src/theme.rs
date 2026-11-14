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

impl Theme {
    /// The highlight behind `<mark>` text, GitHub's attention colour over the page.
    pub fn mark(&self) -> Color {
        if self.name == "dark" {
            [0x5c, 0x48, 0x14]
        } else {
            [0xff, 0xf8, 0xc5]
        }
    }

    /// An alert's title-and-bar colour and its background. A note keeps the theme's accent; the
    /// others take GitHub's colour for the kind, with a background that is the page tinted by it.
    pub fn alert(&self, kind: mdview_core::model::AlertKind) -> (Color, Color) {
        use mdview_core::model::AlertKind::*;
        let dark = self.name == "dark";
        let color = match (kind, dark) {
            (Note, _) => return (self.accent, self.tint),
            (Tip, false) => [0x1a, 0x7f, 0x37],
            (Tip, true) => [0x3f, 0xb9, 0x50],
            (Important, false) => [0x82, 0x50, 0xdf],
            (Important, true) => [0xab, 0x7d, 0xf8],
            (Warning, false) => [0x9a, 0x67, 0x00],
            (Warning, true) => [0xd2, 0x99, 0x22],
            (Caution, false) => [0xcf, 0x22, 0x2e],
            (Caution, true) => [0xf8, 0x51, 0x49],
        };
        let share = if dark { 28 } else { 18 };
        let mix =
            |c: u8, bg: u8| ((c as u32 * share + bg as u32 * (255 - share) + 127) / 255) as u8;
        let tint = [
            mix(color[0], self.bg[0]),
            mix(color[1], self.bg[1]),
            mix(color[2], self.bg[2]),
        ];
        (color, tint)
    }
}

/// The page's sans-serif and monospace family lists. Off Windows the colour emoji family comes
/// before the generic: an emoji cluster takes the first listed font that has the character, and
/// the generic can resolve to a font with a monochrome (or tall, boxed) glyph for it.
const SANS: &str = if cfg!(windows) {
    "Segoe UI, Noto Sans, Cantarell, DejaVu Sans, sans-serif"
} else {
    "Segoe UI, Noto Sans, Cantarell, DejaVu Sans, Noto Color Emoji, sans-serif"
};
const MONO: &str = if cfg!(windows) {
    "Consolas, Cascadia Mono, Noto Sans Mono, DejaVu Sans Mono, monospace"
} else {
    "Consolas, Cascadia Mono, Noto Sans Mono, DejaVu Sans Mono, Noto Color Emoji, monospace"
};

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
    sans: SANS,
    mono: MONO,
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
    sans: SANS,
    mono: MONO,
    body_size: 16.0,
    body_line_height: 1.6,
    margin: 40.0,
    reading_width: 700.0,
};

impl Theme {
    /// What makes one layout differ from another by look: the name and the two family lists. Two
    /// themes with the same key lay text out the same.
    pub fn key(&self) -> (&'static str, &'static str, &'static str) {
        (self.name, self.sans, self.mono)
    }

    /// This theme with the reader's families first: each is tried before the default list, which
    /// stays behind it, so an unknown family, or a character the chosen one lacks (emoji, symbols),
    /// falls through to what the page always used. An empty family leaves the default.
    pub fn with_fonts(mut self, page: &str, code: &str) -> Theme {
        self.sans = family_list(page, SANS);
        self.mono = family_list(code, MONO);
        self
    }

    pub fn is_dark(&self) -> bool {
        self.name == "dark"
    }
}

/// `chosen` in front of `default`, as one list that lives for the rest of the process. Lists are
/// kept by what they say, so setting the same font again, or typing and deleting, adds nothing.
fn family_list(chosen: &str, default: &'static str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    let chosen: String = chosen
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ',' | '"' | '\'' | ';' | '\\'))
        .take(64)
        .collect();
    let chosen = chosen.trim();
    if chosen.is_empty() {
        return default;
    }
    static LISTS: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut lists = LISTS.get_or_init(Default::default).lock().unwrap();
    let list = format!("{chosen}, {default}");
    lists
        .entry(list.clone())
        .or_insert_with(|| Box::leak(list.into_boxed_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_family_goes_first_and_the_default_stays_behind() {
        let t = LIGHT.with_fonts("Noto Serif", "Fira Code");
        assert!(t.sans.starts_with("Noto Serif, Segoe UI"));
        assert!(t.sans.ends_with("sans-serif"));
        assert!(t.mono.starts_with("Fira Code, Consolas"));
        if !cfg!(windows) {
            assert!(t.sans.contains("Noto Color Emoji"), "emoji still fall back");
        }
        assert_ne!(t.key(), LIGHT.key());
        assert_eq!(t.key(), LIGHT.with_fonts("Noto Serif", "Fira Code").key());
        assert!(std::ptr::eq(
            t.sans,
            LIGHT.with_fonts("Noto Serif", "").sans
        ));
    }

    #[test]
    fn no_family_is_the_default_list_itself_and_a_list_cannot_be_injected() {
        let t = DARK.with_fonts("", "  ");
        assert_eq!(t.key(), DARK.key());
        let t = LIGHT.with_fonts("Evil\", monospace; x", "");
        assert!(t.sans.starts_with("Evil monospace x, Segoe UI"));
    }
}
