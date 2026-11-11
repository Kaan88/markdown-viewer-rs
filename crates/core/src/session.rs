//! Saved state: open tabs, recent files, folder, theme, zoom and opaque shell layout.

use std::io;
use std::path::{Path, PathBuf};

pub const MAX_RECENT: usize = 15;
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

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Most recent first, without duplicates, at most `MAX_RECENT`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recents {
    paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEntry {
    pub path: PathBuf,
    /// Menu text: the filename, marked when the file is gone.
    pub label: String,
    pub missing: bool,
}

impl Recents {
    pub fn add(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        self.paths.retain(|p| !same_path(p, &path));
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT);
    }

    pub fn remove(&mut self, path: &Path) {
        self.paths.retain(|p| !same_path(p, path));
    }

    pub fn clear(&mut self) {
        self.paths.clear();
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Checks the disk for each entry, so call it when a menu or dialog opens, not on startup.
    pub fn entries(&self) -> Vec<RecentEntry> {
        self.paths
            .iter()
            .map(|p| {
                let missing = !p.exists();
                let name = p.file_name().map_or_else(
                    || p.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                let label = if missing {
                    format!("{name} (missing)")
                } else {
                    name
                };
                RecentEntry {
                    path: p.clone(),
                    label,
                    missing,
                }
            })
            .collect()
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
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "tab" => s.tabs.push(decode_path(value)),
                "recent" => recent.push(decode_path(value)),
                "active" => s.active = value.trim().parse().unwrap_or(0),
                "folder" if !value.is_empty() => s.folder = Some(decode_path(value)),
                "theme" => s.theme = Theme::from_setting(value.trim().parse().unwrap_or(0)),
                "zoom" => s.zoom = clamp_zoom(value.trim().parse().unwrap_or(ZOOM_DEFAULT)),
                "geometry" => s.geometry = unescape(value),
                "layout" => s.layout = unescape(value),
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
        let mut out = String::from("[mdview]\n");
        let mut line = |k: &str, v: &str| {
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        };
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
        out
    }

    /// Writes through a temporary file and rename, so a crash cannot leave half a settings file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("ini.tmp");
        std::fs::write(&tmp, self.serialize())?;
        std::fs::rename(&tmp, path)
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
        self.layout = format!("files={};outline={}", u8::from(files), u8::from(outline));
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
        Some((w, h, words.next() == Some("max")))
    }

    pub fn set_window_size(&mut self, width: u32, height: u32, maximized: bool) {
        self.geometry = format!("{width}x{height}{}", if maximized { " max" } else { "" });
    }
}

/// `settings.ini` in the per-user application data directory.
pub fn default_settings_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| b.join("mdview").join("settings.ini"))
}

fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        let (valid, invalid) = match std::str::from_utf8(rest) {
            Ok(v) => (v, &[][..]),
            Err(e) => {
                let (a, b) = rest.split_at(e.valid_up_to());
                (std::str::from_utf8(a).unwrap_or_default(), b)
            }
        };
        for c in valid.chars() {
            match c {
                '%' | '\n' | '\r' => out.push_str(&format!("%{:02X}", c as u32)),
                c => out.push(c),
            }
        }
        match invalid.first() {
            Some(b) => {
                out.push_str(&format!("%{b:02X}"));
                rest = &invalid[1..];
            }
            None => rest = &[],
        }
    }
    out
}

fn unescape_bytes(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%')
            .then(|| b.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| {
                Some(char::from(h[0]).to_digit(16)? * 16 + char::from(h[1]).to_digit(16)?)
            });
        match hex {
            Some(v) => {
                out.push(v as u8);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    out
}

fn escape(s: &str) -> String {
    escape_bytes(s.as_bytes())
}

fn unescape(s: &str) -> String {
    String::from_utf8_lossy(&unescape_bytes(s)).into_owned()
}

#[cfg(unix)]
fn encode_path(p: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    escape_bytes(p.as_os_str().as_bytes())
}

#[cfg(unix)]
fn decode_path(s: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(std::ffi::OsString::from_vec(unescape_bytes(s)))
}

#[cfg(not(unix))]
fn encode_path(p: &Path) -> String {
    escape(&p.to_string_lossy())
}

#[cfg(not(unix))]
fn decode_path(s: &str) -> PathBuf {
    PathBuf::from(unescape(s))
}

#[cfg(test)]
mod window_state_tests {
    use super::Session;

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
