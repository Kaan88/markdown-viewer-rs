//! Saved state: open tabs, recent files, folder, theme, zoom and opaque shell layout.

use mdshared_app::settings::{self, decode_path, encode_path, escape, unescape};
use std::io;
use std::path::{Path, PathBuf};

pub use mdshared_app::recents::{MAX_RECENT, RecentEntry, Recents};

/// The application name: the data folder, the settings section and the hand-over greeting.
const APP: &str = "mdview";

pub const ZOOM_MIN: u16 = 60;
pub const ZOOM_MAX: u16 = 250;
pub const ZOOM_STEP: u16 = 10;
pub const ZOOM_DEFAULT: u16 = 100;

pub fn clamp_zoom(z: u16) -> u16 {
    z.clamp(ZOOM_MIN, ZOOM_MAX)
}

pub fn zoom_in(z: u16) -> u16 {
    clamp_zoom(z.saturating_add(ZOOM_STEP))
}

pub fn zoom_out(z: u16) -> u16 {
    clamp_zoom(z.saturating_sub(ZOOM_STEP))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    fn to_setting(self) -> u8 {
        match self {
            Theme::System => 0,
            Theme::Light => 1,
            Theme::Dark => 2,
        }
    }
    fn from_setting(n: u8) -> Self {
        match n {
            1 => Theme::Light,
            2 => Theme::Dark,
            _ => Theme::System,
        }
    }
}

/// How wide the page's column of text may grow. The shell decides what each means in px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageWidth {
    /// A column sized for reading prose.
    #[default]
    Narrow,
    /// A wider column, for documents with large tables or long code lines.
    Wide,
    /// As wide as the window.
    Full,
}

impl PageWidth {
    pub const ALL: [PageWidth; 3] = [PageWidth::Narrow, PageWidth::Wide, PageWidth::Full];

    /// The word the session file and the command line use.
    pub fn name(self) -> &'static str {
        match self {
            PageWidth::Narrow => "narrow",
            PageWidth::Wide => "wide",
            PageWidth::Full => "full",
        }
    }

    pub fn from_name(name: &str) -> Option<PageWidth> {
        PageWidth::ALL
            .into_iter()
            .find(|w| w.name().eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub tabs: Vec<PathBuf>,
    pub active: usize,
    pub recent: Recents,
    pub folder: Option<PathBuf>,
    pub theme: Theme,
    pub zoom: u16,
    /// Shell-owned window state. Core stores it and never reads it.
    pub geometry: String,
    pub layout: String,
    /// Load remote (`http`/`https`) images in every document without asking. Off by default: a
    /// document then shows a bar offering to load its own.
    pub remote_images: bool,
    /// The reader's choice of page width, for every document.
    pub page_width: PageWidth,
    /// Tick task boxes in any file without asking first. Off by default: the first tick in a
    /// file then asks.
    pub edit_tasks: bool,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            tabs: Vec::new(),
            active: 0,
            recent: Recents::default(),
            folder: None,
            theme: Theme::System,
            zoom: ZOOM_DEFAULT,
            geometry: String::new(),
            layout: String::new(),
            remote_images: false,
            page_width: PageWidth::default(),
            edit_tasks: false,
        }
    }
}

impl Session {
    /// Never fails: a missing or damaged file gives defaults, and unknown lines are skipped.
    pub fn load(path: &Path) -> Session {
        std::fs::read(path)
            .map(|b| Session::parse(&String::from_utf8_lossy(&b)))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Session {
        let mut s = Session::default();
        let mut recent = Vec::new();
        for (key, value) in settings::pairs(text) {
            match key {
                "tab" => s.tabs.push(decode_path(value)),
                "recent" => recent.push(decode_path(value)),
                "active" => s.active = value.trim().parse().unwrap_or(0),
                "folder" if !value.is_empty() => s.folder = Some(decode_path(value)),
                "theme" => s.theme = Theme::from_setting(value.trim().parse().unwrap_or(0)),
                "zoom" => s.zoom = clamp_zoom(value.trim().parse().unwrap_or(ZOOM_DEFAULT)),
                "geometry" => s.geometry = unescape(value),
                "layout" => s.layout = unescape(value),
                "remote_images" => s.remote_images = value.trim() == "1",
                "page_width" => s.page_width = PageWidth::from_name(value).unwrap_or_default(),
                "edit_tasks" => s.edit_tasks = value.trim() == "1",
                _ => {}
            }
        }
        // Added in reverse so the file's order, most recent first, is kept through `add`.
        for p in recent.into_iter().rev() {
            s.recent.add(p);
        }
        s.active = s.active.min(s.tabs.len().saturating_sub(1));
        s
    }

    pub fn serialize(&self) -> String {
        let mut out = settings::Writer::new(APP);
        let mut line = |k: &str, v: &str| out.line(k, v);
        line("theme", &self.theme.to_setting().to_string());
        line("zoom", &clamp_zoom(self.zoom).to_string());
        line("active", &self.active.to_string());
        if let Some(f) = &self.folder {
            line("folder", &encode_path(f));
        }
        for t in &self.tabs {
            line("tab", &encode_path(t));
        }
        for r in self.recent.paths() {
            line("recent", &encode_path(r));
        }
        line("geometry", &escape(&self.geometry));
        line("layout", &escape(&self.layout));
        line("remote_images", if self.remote_images { "1" } else { "0" });
        line("page_width", self.page_width.name());
        line("edit_tasks", if self.edit_tasks { "1" } else { "0" });
        out.finish()
    }

    /// Writes through a temporary file and rename, so a crash cannot leave half a settings file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        settings::write_atomic(path, &self.serialize())
    }

    /// Which side panes were showing: `(files, outline)`. Both shells write the `layout` line as
    /// `files=1;outline=0`; an empty or foreign value gives both hidden.
    pub fn panes(&self) -> (bool, bool) {
        let mut files = false;
        let mut outline = false;
        for part in self.layout.split(';') {
            match part.trim() {
                "files=1" => files = true,
                "outline=1" => outline = true,
                _ => {}
            }
        }
        (files, outline)
    }

    pub fn set_panes(&mut self, files: bool, outline: bool) {
        let widths = self.pane_widths();
        self.write_layout(files, outline, widths);
    }

    /// The saved widths of the Files and Outline panes in logical px, each `None` until the
    /// reader has dragged that pane's edge. Written as `files_w=250;outline_w=230` after the
    /// pane flags.
    pub fn pane_widths(&self) -> (Option<u32>, Option<u32>) {
        let mut widths = (None, None);
        for part in self.layout.split(';') {
            let Some((key, value)) = part.trim().split_once('=') else {
                continue;
            };
            let value = value
                .parse::<u32>()
                .ok()
                .filter(|v| (40..=4000).contains(v));
            match key {
                "files_w" => widths.0 = value,
                "outline_w" => widths.1 = value,
                _ => {}
            }
        }
        widths
    }

    pub fn set_pane_widths(&mut self, files: Option<u32>, outline: Option<u32>) {
        let (files_on, outline_on) = self.panes();
        self.write_layout(files_on, outline_on, (files, outline));
    }

    fn write_layout(&mut self, files: bool, outline: bool, widths: (Option<u32>, Option<u32>)) {
        self.layout = format!("files={};outline={}", u8::from(files), u8::from(outline));
        if let Some(w) = widths.0 {
            self.layout.push_str(&format!(";files_w={w}"));
        }
        if let Some(w) = widths.1 {
            self.layout.push_str(&format!(";outline_w={w}"));
        }
    }

    /// The saved window size in device-independent pixels and whether it was maximized, written
    /// as `1100x900` or `1100x900 max`. Sizes outside 320..=16384 are ignored.
    pub fn window_size(&self) -> Option<(u32, u32, bool)> {
        let mut words = self.geometry.split_whitespace();
        let (w, h) = words.next()?.split_once('x')?;
        let (w, h): (u32, u32) = (w.parse().ok()?, h.parse().ok()?);
        let ok = |v: u32| (320..=16384).contains(&v);
        if !ok(w) || !ok(h) {
            return None;
        }
        Some((w, h, words.any(|word| word == "max")))
    }

    pub fn set_window_size(&mut self, width: u32, height: u32, maximized: bool) {
        let position = self.window_position();
        self.geometry = format!("{width}x{height}{}", if maximized { " max" } else { "" });
        if let Some((x, y)) = position {
            self.geometry.push_str(&format!(" @{x},{y}"));
        }
    }

    /// Where the window's top-left corner was, in the desktop's device px, written after the
    /// size as `@120,80`. It is the place of the restored window: a maximized or minimized one
    /// does not overwrite it. Whether that place is still on a screen is for the shell to check.
    pub fn window_position(&self) -> Option<(i32, i32)> {
        let word = self
            .geometry
            .split_whitespace()
            .find_map(|w| w.strip_prefix('@'))?;
        let (x, y) = word.split_once(',')?;
        let (x, y): (i32, i32) = (x.parse().ok()?, y.parse().ok()?);
        let ok = |v: i32| (-100_000..=100_000).contains(&v);
        (ok(x) && ok(y)).then_some((x, y))
    }

    pub fn set_window_position(&mut self, x: i32, y: i32) {
        let size: String = self
            .geometry
            .split_whitespace()
            .filter(|word| !word.starts_with('@'))
            .collect::<Vec<_>>()
            .join(" ");
        self.geometry = format!("{size} @{x},{y}").trim_start().to_string();
    }
}

/// `settings.ini` in the per-user application data directory.
pub fn default_settings_path() -> Option<PathBuf> {
    settings::default_settings_path(APP)
}

#[cfg(test)]
mod window_state_tests {
    use super::{PageWidth, Session};

    #[test]
    fn page_width_round_trips_and_defaults_to_narrow() {
        assert_eq!(Session::parse("").page_width, PageWidth::Narrow);
        assert_eq!(
            Session::parse("page_width=sideways").page_width,
            PageWidth::Narrow
        );
        for width in PageWidth::ALL {
            let s = Session {
                page_width: width,
                ..Session::default()
            };
            assert_eq!(Session::parse(&s.serialize()).page_width, width);
        }
    }

    #[test]
    fn panes_round_trip() {
        let mut s = Session::default();
        assert_eq!(s.panes(), (false, false));
        s.set_panes(true, false);
        assert_eq!(s.panes(), (true, false));
        s.set_panes(false, true);
        assert_eq!(s.panes(), (false, true));
        s.layout = "AAAAqt-state".into();
        assert_eq!(s.panes(), (false, false));
    }

    #[test]
    fn position_and_pane_widths_round_trip_beside_the_older_keys() {
        let mut s = Session::default();
        assert_eq!(s.window_position(), None);
        assert_eq!(s.pane_widths(), (None, None));
        s.set_window_size(1280, 800, true);
        s.set_window_position(-1900, 40);
        assert_eq!(s.window_size(), Some((1280, 800, true)));
        assert_eq!(s.window_position(), Some((-1900, 40)));
        // A new size keeps the place, and a new place keeps the size.
        s.set_window_size(1000, 700, false);
        assert_eq!(s.window_position(), Some((-1900, 40)));
        s.set_window_position(10, 20);
        assert_eq!(s.window_size(), Some((1000, 700, false)));
        assert_eq!(s.window_position(), Some((10, 20)));

        s.set_panes(true, false);
        s.set_pane_widths(Some(300), None);
        assert_eq!(s.panes(), (true, false));
        assert_eq!(s.pane_widths(), (Some(300), None));
        s.set_panes(true, true);
        assert_eq!(s.pane_widths(), (Some(300), None));
        s.set_pane_widths(Some(300), Some(260));
        assert_eq!(s.panes(), (true, true));
        // Both survive the file.
        let again = Session::parse(&s.serialize());
        assert_eq!(again.window_position(), Some((10, 20)));
        assert_eq!(again.pane_widths(), (Some(300), Some(260)));
        s.layout = "files=1;files_w=9999999".into();
        assert_eq!(s.pane_widths(), (None, None));
    }

    #[test]
    fn window_size_round_trip() {
        let mut s = Session::default();
        assert_eq!(s.window_size(), None);
        s.set_window_size(1280, 800, false);
        assert_eq!(s.window_size(), Some((1280, 800, false)));
        s.set_window_size(1280, 800, true);
        assert_eq!(s.window_size(), Some((1280, 800, true)));
        s.geometry = "10x10".into();
        assert_eq!(s.window_size(), None);
        s.geometry = "junk".into();
        assert_eq!(s.window_size(), None);
    }
}
