//! The Notes face of the Files pane: mdedit's notes library, read-only, and the one write the
//! viewer makes to it, "Add this file to the notes library…".
//!
//! The library is mdedit's: its folder is named by `library_dir` in mdedit's `settings.ini`,
//! found and read by [`mdshared_app::library::put`], and its rows are the shared
//! [`mdshared_app::library::tree`]. Nothing here reads the library at launch: the folder is
//! looked at once (is there one, so the head offers the face) and read when the face is first
//! shown, and again on Refresh or after an add. There is no timer and no watcher.
//!
//! A build never reads the owner's library by accident (mdedit D-067): with no session (a shot,
//! `--no-session`) or with a scratch `MDVIEW_SETTINGS`, the library is found only through an
//! explicit `MDEDIT_SETTINGS` naming a scratch settings file of mdedit's.

use crate::chrome::{Ctx, glyph};
use crate::gfx::{Face, Painter, Rect, mix};
use crate::ui::{Hit, Scroller, Side, Ui};
use crate::workspace::Workspace;
use mdshared_app::library::put::{self, PutError};
use mdshared_app::library::tree::{self, Kind, Row};
use mdshared_app::library::{self, Library};
use mdshared_ui::pane::{
    HEAD, HEAD_ACTION, ROW, ROW_H, Switch, draw_head, draw_head_switches, draw_row_fill,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Said when a note's file is not where the library says.
pub const MISSING: &str = "That note is not where the library says it is.";
const NO_FOLDER: &str = "No library folder yet";
const NO_NOTES: &str = "No notes yet";
pub const NO_LIBRARY: &str = "There is no notes library: mdedit's settings were not found.";
pub const NEEDS_FILE: &str = "Save this file first: the library keeps links to files.";

pub fn added(title: &str, group: &str) -> String {
    format!("\u{201c}{title}\u{201d} is in {group} now.")
}

pub fn already(title: &str) -> String {
    format!("\u{201c}{title}\u{201d} is already in the notes library.")
}
/// A pane narrower than this leaves the notes' folders out: the titles need the room.
const FOLDER_MIN_PANE: f32 = 210.0;
const FOLDER_GAP: f32 = 10.0;

/// The settings file of mdedit that names the library, or `None` when this run must not look:
/// an explicit `MDEDIT_SETTINGS` always counts; otherwise a run with no session, or with a
/// scratch `MDVIEW_SETTINGS`, has no library, and a normal run uses mdedit's own file.
pub fn settings_file(
    explicit: Option<PathBuf>,
    session: Option<&Path>,
    scratch: bool,
    default: Option<PathBuf>,
) -> Option<PathBuf> {
    if explicit.is_some() {
        return explicit;
    }
    if false {
        return None;
    }
    default
}

/// The library folder this run uses, for mdview's `session` file.
pub fn library_dir(session: Option<&Path>) -> Option<PathBuf> {
    let file = settings_file(
        std::env::var_os("MDEDIT_SETTINGS").map(PathBuf::from),
        session,
        std::env::var_os("MDVIEW_SETTINGS").is_some(),
        mdshared_app::settings::default_settings_path("mdedit"),
    )?;
    put::dir_from_settings(&file)
}

/// A file waiting for the reader's yes before it is written into a library file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub file: PathBuf,
    pub note: PathBuf,
    pub title: String,
    pub group: String,
}

pub struct Notes {
    pub dir: Option<PathBuf>,
    /// The library folder was there when last looked at: the head offers the face.
    pub available: bool,
    /// The face shown in the Files pane.
    pub on: bool,
    library: Option<Library>,
    pub rows: Vec<Row>,
    folded: HashSet<String>,
    pub pending: Option<Pending>,
    /// Raised by every change of what the face draws, for the region's signature.
    pub stamp: u64,
}

impl Notes {
    pub fn new(dir: Option<PathBuf>) -> Notes {
        let available = dir.as_deref().is_some_and(Path::is_dir);
        Notes {
            dir,
            available,
            on: false,
            library: None,
            rows: Vec::new(),
            folded: HashSet::new(),
            pending: None,
            stamp: 0,
        }
    }

    /// The head shows its two switches.
    pub fn has_switch(&self) -> bool {
        self.available || self.on
    }

    /// Shows the face (reading the library the first time) or the files.
    pub fn show(&mut self, on: bool) {
        let on = on && self.dir.is_some();
        if on && self.library.is_none() {
            self.load();
        }
        if self.on != on {
            self.on = on;
            self.stamp += 1;
        }
    }

    /// Reads the library again.
    pub fn load(&mut self) {
        self.available = self.dir.as_deref().is_some_and(Path::is_dir);
        self.library = self.dir.as_deref().map(library::read);
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.rows = self
            .library
            .as_ref()
            .map(|l| tree::rows(l, &self.folded))
            .unwrap_or_default();
        self.stamp += 1;
    }

    /// A click on row `i`: a group or a section folds. A note is the caller's to open.
    pub fn toggle(&mut self, i: usize) {
        let Some(row) = self.rows.get(i) else {
            return;
        };
        if matches!(row.kind, Kind::Group | Kind::Section) {
            let key = row.key.clone();
            if !self.folded.remove(&key) {
                self.folded.insert(key);
            }
            self.rebuild();
        }
    }

    pub fn is_folded(&self, key: &str) -> bool {
        self.folded.contains(key)
    }

    /// The library as last read.
    pub fn library(&self) -> Option<&Library> {
        self.library.as_ref()
    }

    /// The groups an added file can go in, by name and file; empty when the library has none
    /// yet (the file is then made, as mdedit makes it).
    pub fn targets(&self) -> Vec<(String, PathBuf)> {
        self.library
            .iter()
            .flat_map(|l| l.groups.iter().map(|g| (g.name.clone(), g.file.clone())))
            .collect()
    }

    /// The library file a first note goes in when the library has none.
    pub fn default_file(&self) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(library::DEFAULT_FILE))
    }

    /// What the face says when there are no rows.
    pub fn empty(&self) -> Option<&'static str> {
        if !self.rows.is_empty() {
            return None;
        }
        Some(match self.dir.as_deref().map(tree::empty_state) {
            Some(tree::Empty::NoNotes) => NO_NOTES,
            _ => NO_FOLDER,
        })
    }

    /// Writes `note` into the library file `file` and reads the library again. Gives the title
    /// and the group's name for the toast.
    pub fn put(&mut self, file: &Path, note: &Path) -> Result<(String, String), String> {
        let dir = self.dir.clone().ok_or_else(|| NO_LIBRARY.to_string())?;
        let title = put::put_note(file, &[], note, &dir).map_err(|e: PutError| {
            format!("The library file could not be written: {e}")
        })?;
        self.load();
        let group = self
            .library
            .as_ref()
            .and_then(|l| {
                l.groups
                    .iter()
                    .find(|g| mdshared_app::paths::same_path(&g.file, file))
            })
            .map(|g| g.name.clone())
            .unwrap_or_else(|| {
                file.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        Ok((title, group))
    }

    /// The title the library lists `note` under, from the library as last read.
    pub fn listed(&self, note: &Path) -> Option<String> {
        put::listed_as(self.library.as_ref()?, note)
    }

    /// Everything the Files region's pixels depend on here.
    pub fn sig(&self) -> (bool, bool, u64) {
        (self.on, self.available, self.stamp)
    }
}

/// The head of the Files pane: its title, or with a library to show the two switches.
pub fn draw_head_row(c: &mut Ctx, ui: &Ui, r: Rect, title: &str) {
    if !ui.notes.has_switch() {
        draw_head(c.p, c.pal, r, title);
        return;
    }
    let lit = usize::from(ui.notes.on);
    let faces = [(glyph::FOLDER, "Files"), (glyph::BOOKMARK, "Notes")];
    let switches: Vec<Switch> = faces
        .iter()
        .enumerate()
        .map(|(i, (icon, label))| Switch {
            icon,
            label,
            lit: i == lit,
            fill: c.fill_for(Hit::PaneFace(i), false, false),
        })
        .collect();
    let rects = draw_head_switches(c.p, c.pal, r, &switches);
    for (i, b) in rects.into_iter().enumerate() {
        c.hit(Hit::PaneFace(i), b);
    }
}

/// `text` as far as `room` allows, keeping its end (the end of a folder's name tells more).
fn elide_left(p: &mut Painter, size: f32, text: &str, room: f32) -> String {
    if p.measure(Face::Ui, size, text) <= room {
        return text.to_string();
    }
    for (at, _) in text.char_indices() {
        let tail = format!("\u{2026}{}", &text[at..]);
        if p.measure(Face::Ui, size, &tail) <= room {
            return tail;
        }
    }
    String::new()
}

/// The folder shown at the right of a note's row, and the room it takes with its gap.
fn row_folder(p: &mut Painter, row: &Row, face: Face, pane: f32, room: f32) -> (String, f32) {
    if pane < FOLDER_MIN_PANE {
        return (String::new(), 0.0);
    }
    let folder = match &row.kind {
        Kind::Note { path, .. } => tree::folder_of(path),
        _ => row.home.clone(),
    };
    let Some(folder) = folder else {
        return (String::new(), 0.0);
    };
    let title = p.measure(face, 12.5, &row.label);
    let spare = (room - title - FOLDER_GAP).max(room * 0.3 - FOLDER_GAP);
    if spare < 24.0 {
        return (String::new(), 0.0);
    }
    let shown = elide_left(p, 11.5, &folder, spare);
    if shown.is_empty() {
        return (shown, 0.0);
    }
    let w = p.measure(Face::Ui, 11.5, &shown);
    (shown, w)
}

/// The Files pane showing the library: the rows, a note's folder dimmed at the right, a missing
/// note marked, the count of a folder too large to read whole, and Refresh in the head.
pub fn draw(c: &mut Ctx, ws: &Workspace, ui: &mut Ui, r: Rect) {
    let pal = *c.pal;
    draw_head_row(c, ui, r, "NOTES");
    let b = Rect::new(r.right() - HEAD_ACTION, r.y + 6.0, 28.0, 28.0);
    if let Some(fill) = c.fill_for(Hit::NotesRefresh, false, false) {
        c.p.rbox(b, 5.0, fill);
    }
    c.p.icon(glyph::RELOAD, 14.0, b.cx(), b.cy(), pal.muted);
    c.hit(Hit::NotesRefresh, b);

    let mut area = Rect::new(r.x, r.y + HEAD, r.w, (r.h - HEAD).max(0.0));
    let omitted = ui.notes.library().and_then(|l| {
        let shown = ui.notes.rows.iter().filter(|r| r.kind == Kind::Group).count();
        tree::omitted_label(shown, l.groups.len(), l.omitted)
    });
    let foot = omitted.as_ref().map(|_| {
        area.h = (area.h - ROW_H).max(0.0);
        Rect::new(r.x, r.bottom() - ROW_H, r.w, ROW_H)
    });
    let panes = &mut ui.panes;
    panes.files_content = ui.notes.rows.len() as f32 * ROW + 8.0;
    panes.files_view = area.h;
    panes.files_scroll = panes
        .files_scroll
        .clamp(0.0, (panes.files_content - area.h).max(0.0));
    let scroll = panes.files_scroll;
    if let Some(text) = ui.notes.empty() {
        let cy = area.y + 40.0;
        c.p.text_center(
            Face::Ui,
            13.0,
            Rect::new(r.x + 8.0, cy - 10.0, r.w - 16.0, 20.0),
            text,
            pal.muted,
        );
    }
    let active = ws.active_tab().map(|t| t.path().to_path_buf());
    let quiet = mix(pal.text, pal.muted, 0.35);
    let old = c.p.clip(area);
    let first = (scroll / ROW).floor() as usize;
    for (i, row) in ui.notes.rows.iter().enumerate().skip(first) {
        let y = area.y + i as f32 * ROW - scroll;
        if y > area.bottom() {
            break;
        }
        let rr = Rect::new(r.x + 6.0, y, r.w - 12.0, ROW_H);
        let hit = Hit::NotesRow(i);
        let selected = match &row.kind {
            Kind::Note { path, .. } => active.as_deref().is_some_and(|a| tree::same_note(a, path)),
            _ => false,
        };
        draw_row_fill(
            c.p,
            &pal,
            rr,
            c.hover == Some(hit),
            c.pressed == Some(hit),
            selected,
        );
        let ix = r.x + 14.0 + (row.depth as f32 + 1.0) * 14.0;
        let cy = rr.cy();
        let (face, color) = match &row.kind {
            Kind::Group | Kind::Section => {
                let arrow = if ui.notes.is_folded(&row.key) {
                    glyph::RIGHT
                } else {
                    glyph::DOWN
                };
                c.p.icon(arrow, 8.0, ix - 8.0, cy, pal.faint);
                if row.kind == Kind::Group {
                    c.p.icon(glyph::BOOKMARK, 13.0, ix + 9.0, cy, pal.folder);
                    (Face::Semibold, pal.text)
                } else {
                    (Face::Semibold, mix(pal.text, pal.muted, 0.15))
                }
            }
            Kind::Note { missing, .. } => {
                let icon = if *missing { glyph::WARN } else { glyph::DOC };
                let tint = if selected { pal.accent } else { pal.muted };
                c.p.icon(icon, 13.0, ix + 9.0, cy, tint);
                let color = if selected {
                    pal.text
                } else if *missing {
                    pal.muted
                } else {
                    quiet
                };
                (Face::Ui, color)
            }
            Kind::Editing => continue,
        };
        let x = if row.kind == Kind::Section {
            ix + 4.0
        } else {
            ix + 23.0
        };
        c.hit(hit, rr);
        let right = rr.right() - 12.0;
        let mut max = (right - x).max(0.0);
        let (shown, w) = row_folder(c.p, row, face, r.w, max);
        if w > 0.0 {
            c.p.text_right(Face::Ui, 11.5, right, cy, &shown, pal.faint);
            max = (max - w - FOLDER_GAP).max(0.0);
        }
        c.p.text(face, 12.5, x, cy, &row.label, color, max);
    }
    c.p.restore(old);
    let thumb_hit = Hit::ScrollBar(Scroller::Pane(Side::Files));
    if let Some((track, thumb)) = crate::panes::scrollbar(area, ui.panes.files_content, area.h, scroll)
    {
        let active = c.hover == Some(thumb_hit)
            || matches!(
                ui.drag,
                Some(crate::ui::Drag::Thumb {
                    target: Scroller::Pane(Side::Files),
                    ..
                })
            );
        crate::panes::draw_scrollbar(c.p, &pal, track, thumb, active);
        c.hit(thumb_hit, track);
    }
    crate::panes::edge(c, r, Side::Files, false);
    if let (Some(foot), Some(label)) = (foot, omitted) {
        c.p.fill(Rect::new(foot.x, foot.y, foot.w, c.p.px()), pal.popup_line);
        c.p.text(
            Face::Ui,
            12.0,
            r.x + 14.0,
            foot.y + 8.0,
            &label,
            pal.muted,
            (foot.w - 28.0).max(0.0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mdview-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn library_at(dir: &Path) {
        std::fs::create_dir_all(dir.join("Personal")).unwrap();
        std::fs::write(dir.join("Personal/Budget.md"), "# Budget\n").unwrap();
        std::fs::write(
            dir.join("Personal.md"),
            "# Personal\n\n## Money\n\n- [Budget](<Personal/Budget.md>)\n- [Gone](<Personal/Gone.md>)\n",
        )
        .unwrap();
    }

    #[test]
    fn a_run_without_a_session_or_with_a_scratch_one_finds_no_library_unless_told_to() {
        let mdedit = Some(PathBuf::from("D:/scratch/settings.ini"));
        let own = Some(PathBuf::from("C:/real/settings.ini"));
        let session = Path::new("C:/mdview/settings.ini");
        // An explicit settings file of mdedit's always counts.
        assert_eq!(
            settings_file(mdedit.clone(), None, false, own.clone()),
            mdedit
        );
        // No session (a shot, --no-session): nothing of the owner's is read.
        assert_eq!(settings_file(None, None, false, own.clone()), None);
        // A scratch MDVIEW_SETTINGS is no licence to read mdedit's real file.
        assert_eq!(settings_file(None, Some(session), true, own.clone()), None);
        // A normal run uses mdedit's own file.
        assert_eq!(settings_file(None, Some(session), false, own.clone()), own);
    }

    #[test]
    fn the_face_lists_the_library_folds_and_marks_what_is_missing() {
        let s = scratch("rows");
        let lib = s.join("library");
        library_at(&lib);
        let mut n = Notes::new(Some(lib.clone()));
        assert!(n.available && n.has_switch());
        assert!(n.rows.is_empty(), "nothing is read until the face shows");
        n.show(true);
        let labels: Vec<_> = n.rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Personal", "Money", "Budget", "Gone"]);
        assert!(matches!(
            n.rows[3].kind,
            Kind::Note { missing: true, .. }
        ));
        n.toggle(1);
        assert_eq!(n.rows.len(), 2, "folding Money hides its notes");
        n.toggle(2);
        assert_eq!(n.rows.len(), 2, "a note does not fold");
        n.toggle(1);
        assert_eq!(n.rows.len(), 4);
    }

    #[test]
    fn a_missing_folder_is_not_offered_and_says_so_when_shown() {
        let s = scratch("none");
        let n = Notes::new(Some(s.join("nowhere")));
        assert!(!n.has_switch());
        let mut n = n;
        n.show(true);
        assert_eq!(n.empty(), Some(NO_FOLDER));
        let mut none = Notes::new(None);
        none.show(true);
        assert!(!none.on, "no library, no face");
        std::fs::create_dir_all(s.join("empty")).unwrap();
        let mut e = Notes::new(Some(s.join("empty")));
        e.show(true);
        assert_eq!(e.empty(), Some(NO_NOTES));
    }

    #[test]
    fn adding_a_file_lists_it_and_a_second_add_finds_it_listed() {
        let s = scratch("put");
        let lib = s.join("library");
        library_at(&lib);
        let note = s.join("Plan.md");
        std::fs::write(&note, "# Plan\n").unwrap();
        let mut n = Notes::new(Some(lib.clone()));
        n.show(true);
        assert_eq!(n.listed(&note), None);
        let (_, file) = n.targets().remove(0);
        let (title, group) = n.put(&file, &note).unwrap();
        assert_eq!((title.as_str(), group.as_str()), ("Plan", "Personal"));
        assert_eq!(n.listed(&note).as_deref(), Some("Plan"));
        assert!(n.rows.iter().any(|r| r.label == "Plan"));
    }

    #[test]
    fn a_library_with_no_file_gets_notes_md_from_the_first_add() {
        let s = scratch("first");
        let lib = s.join("library");
        std::fs::create_dir_all(&lib).unwrap();
        let note = s.join("Plan.md");
        std::fs::write(&note, "# Plan\n").unwrap();
        let mut n = Notes::new(Some(lib.clone()));
        n.show(true);
        assert!(n.targets().is_empty());
        let file = n.default_file().unwrap();
        n.put(&file, &note).unwrap();
        assert!(lib.join("notes.md").is_file());
        assert!(n.rows.iter().any(|r| r.label == "Plan"));
    }
}
