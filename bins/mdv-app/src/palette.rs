//! The command palette's sources. The card, the query box, the ranking and the list are
//! `mdshared_ui::palette`; this module decides what the entries are, by the prefix of the query:
//!
//! - no prefix: the open tabs, the recent files, and (once something is typed) the Markdown files
//!   of the open folder, as the note index lists them. The index is built on its own thread when
//!   the folder is set; nothing here walks a disk, so opening the palette never waits for one.
//! - `>`: the commands that can run, from core's command table, with their shortcuts.
//! - `#`: the headings of the current document, the Outline's data.
//! - `?`: the prefixes themselves.
//!
//! Nothing is built until the palette opens; an entry's id is its index in `targets`, which says
//! what choosing it does.

use crate::folder_search::{self, FolderSearch, Waker};
use crate::overlays::command_icon;
use crate::platform;
use crate::workspace::Workspace;
use mdshared_ui::glyph;
use mdshared_ui::palette::{Entry, Palette};
use mdview::commands::{COMMANDS, CommandId as C, Menu};
use std::collections::HashSet;
use std::path::PathBuf;

/// What the empty box says.
pub const PLACEHOLDER: &str = "Type a file name, > for commands, # for headings";
/// The most folder files listed, as the Files pane's watcher also stops following notes there.
pub const MAX_FILES: usize = 5_000;

const G_TABS: u8 = 0;
const G_RECENT: u8 = 1;
const G_FILES: u8 = 2;
const G_FILE: u8 = 10;
const G_EDIT: u8 = 11;
const G_VIEW: u8 = 12;
const G_HELP: u8 = 13;
const G_TAB: u8 = 14;
const G_OTHER: u8 = 15;
/// The notes library: one group per library file, from this number to [`G_LIBRARY_LAST`]. A
/// group is a byte, so a library of more files lists the rest under one header,
/// [`G_LIBRARY_MORE`], each note's hint naming its file (audit 4, E4: the ids ran into
/// [`G_PLAIN`] from the 85th file and wrapped or repeated past the 240th, listing notes under
/// another file's header).
const G_LIBRARY: u8 = 16;
const G_LIBRARY_LAST: u8 = 253;
const G_LIBRARY_MORE: u8 = 254;
/// The header over the library files past [`G_LIBRARY_LAST`].
const MORE_LIBRARY: &str = "More library files";
/// Headings and the help rows have no header: no group has this number.
const G_PLAIN: u8 = 255;

/// What choosing an entry does.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Cmd(C),
    Tab(usize),
    Recent(PathBuf),
    File(PathBuf),
    /// A note of the notes library: opened as any file is.
    Note(PathBuf),
    /// Scrolls to this block of the current document.
    Heading(u32),
    /// A row of the `?` list: puts this prefix in the box.
    Prefix(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Files,
    Commands,
    Headings,
    Help,
}

/// The mode a query is in and how many chars of it are the prefix.
fn mode_of(query: &str) -> (Mode, usize) {
    match query.chars().next() {
        Some('>') => (Mode::Commands, 1),
        Some('#') => (Mode::Headings, 1),
        Some('?') => (Mode::Help, 1),
        _ => (Mode::Files, 0),
    }
}

/// A note of the library as the card lists it: the title is the row, its section (and that it is
/// gone) the hint.
struct Note {
    title: String,
    hint: String,
    path: PathBuf,
    /// `path` as a tab would hold it, resolved once per read of the library
    /// ([`crate::notes::Notes::resolve_notes`]), to leave out a note already listed as a tab or
    /// a recent file without asking the disk while the reader types (audit 4, E5).
    key: PathBuf,
    missing: bool,
    /// The group (the library file) it is under, as a header.
    group: u8,
}

impl Note {
    /// The note `e`, under the header `group`; `file` names its library file in the hint when
    /// the header is not that file's own.
    fn of(e: &mdshared_app::library::Entry, group: u8, file: Option<&str>, key: PathBuf) -> Note {
        let section = (file.into_iter())
            .chain(e.sections.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" \u{203a} ");
        let hint = match (section.as_str(), e.missing) {
            ("", false) => String::new(),
            ("", true) => MISSING.to_string(),
            (section, false) => section.to_string(),
            (section, true) => format!("{section} \u{2014} {MISSING}"),
        };
        Note {
            title: e.title.clone(),
            hint,
            path: e.path.clone(),
            key,
            missing: e.missing,
            group,
        }
    }
}

/// The headers of the library's files and its notes, in its order: each file its own group up to
/// [`G_LIBRARY_LAST`], the rest under [`MORE_LIBRARY`] with their file's name in the hint.
/// `key` gives each note's resolved path.
fn library_notes(
    library: &mdshared_app::library::Library,
    key: impl Fn(&std::path::Path) -> PathBuf,
) -> (Vec<(u8, String)>, Vec<Note>) {
    let own = usize::from(G_LIBRARY_LAST - G_LIBRARY) + 1;
    let mut groups = Vec::new();
    let mut notes = Vec::new();
    for (g, group) in library.groups.iter().enumerate() {
        let (id, file) = match u8::try_from(g).ok().filter(|_| g < own) {
            Some(g) => (G_LIBRARY + g, None),
            None => (G_LIBRARY_MORE, Some(group.name.as_str())),
        };
        if file.is_none() {
            groups.push((id, group.name.clone()));
        } else if g == own {
            groups.push((id, MORE_LIBRARY.to_string()));
        }
        notes.extend((group.entries.iter()).map(|e| Note::of(e, id, file, key(&e.path))));
    }
    (groups, notes)
}

/// Said in a note's hint when its file is not where the library says.
const MISSING: &str = "missing";

pub struct PaletteState {
    pub palette: Palette,
    targets: Vec<Target>,
    /// The notes library as it was read when the card opened: the titles, each with its section
    /// and whether it is still where the library says.
    notes: Vec<Note>,
    /// The source the entries came from; `None` before the first.
    mode: Option<Mode>,
    /// Whether the folder's files are among the entries (only once the query has text).
    with_files: bool,
    /// Find in folder: the card lists the lines of the folder's files that hold the query, not
    /// the entries above.
    folder: Option<FolderSearch>,
}

impl PaletteState {
    /// A palette open on `query`, with its entries. `notes` is the notes library as the window
    /// has it read: its notes are listed under a header per library file, after the files.
    pub fn open(ws: &Workspace, notes: Option<&crate::notes::Notes>, query: &str) -> PaletteState {
        let mut palette = Palette::new();
        palette.set_placeholder(PLACEHOLDER);
        let mut groups = vec![
            (G_TABS, "Open tabs".into()),
            (G_RECENT, "Recent".into()),
            (G_FILES, "Files".into()),
            (G_FILE, "File".into()),
            (G_EDIT, "Edit".into()),
            (G_VIEW, "View".into()),
            (G_HELP, "Help".into()),
            (G_TAB, "Tab".into()),
            (G_OTHER, "Other".into()),
        ];
        let mut listed = Vec::new();
        if let Some(n) = notes
            && let Some(library) = n.library()
        {
            let (more, these) = library_notes(library, |p| n.resolved(p));
            groups.extend(more);
            listed = these;
        }
        palette.set_groups(groups);
        palette.open(query);
        let mut state = PaletteState {
            palette,
            targets: Vec::new(),
            notes: listed,
            mode: None,
            with_files: false,
            folder: None,
        };
        state.sync(ws);
        state
    }

    /// The card as Find in folder, searching `root` for `query` at once. `wake` is called from the
    /// search thread when results are waiting ([`poll_folder`](Self::poll_folder) takes them).
    pub fn open_folder(root: PathBuf, query: &str, wake: Option<Waker>) -> PaletteState {
        let mut palette = Palette::new();
        palette.set_literal(true);
        palette.set_placeholder(folder_search::PLACEHOLDER);
        palette.open(query);
        let mut folder = FolderSearch::new(root, wake);
        let query = palette.query().to_string();
        folder.start(&query, &mut palette);
        PaletteState {
            palette,
            targets: Vec::new(),
            notes: Vec::new(),
            mode: None,
            with_files: false,
            folder: Some(folder),
        }
    }

    /// Takes the results the search thread has found. True when the card changed.
    pub fn poll_folder(&mut self) -> bool {
        match self.folder.as_mut() {
            Some(f) => f.poll(&mut self.palette),
            None => false,
        }
    }

    /// Files changed: Find in folder searches again for its query, and its list is replaced
    /// when that is done ([`poll_folder`](Self::poll_folder)). False when this is no such card or
    /// its box is empty.
    pub fn refresh_folder(&mut self) -> bool {
        self.folder.as_mut().is_some_and(FolderSearch::refresh)
    }

    /// Whether a search started by a change of files is still reading.
    #[cfg(test)]
    pub fn folder_refreshing(&self) -> bool {
        self.folder.as_ref().is_some_and(FolderSearch::refreshing)
    }

    /// Whether this card is Find in folder, which wants to hear about files changing.
    pub fn is_folder_search(&self) -> bool {
        self.folder.is_some()
    }

    /// Whether Find in folder is still reading files.
    pub fn folder_running(&self) -> bool {
        self.folder.as_ref().is_some_and(FolderSearch::running)
    }

    /// What choosing a result row of Find in folder opens: the file, the line and the query.
    pub fn folder_hit(&self, id: u32) -> Option<(PathBuf, u32, String)> {
        let f = self.folder.as_ref()?;
        let (path, line) = f.hit(id)?;
        Some((path.to_path_buf(), line, f.query().to_string()))
    }

    /// Swaps the entries when the query's prefix (or, for files, whether it has text) changed
    /// the source. Called after the query changed; typing within a mode only re-ranks, which the
    /// palette does by itself.
    pub fn sync(&mut self, ws: &Workspace) {
        // Find in folder has one source: a new query is a new search.
        if let Some(f) = self.folder.as_mut() {
            if f.query() != self.palette.query() {
                let query = self.palette.query().to_string();
                f.start(&query, &mut self.palette);
            }
            return;
        }
        let (mode, skip) = mode_of(self.palette.query());
        let with_files = mode == Mode::Files && !self.palette.needle().is_empty();
        if self.mode == Some(mode) && (mode != Mode::Files || with_files == self.with_files) {
            return;
        }
        self.mode = Some(mode);
        self.with_files = with_files;
        self.palette.set_ignore_prefix(skip);
        let (entries, targets) = match mode {
            Mode::Files => files(ws, with_files, &self.notes),
            Mode::Commands => commands(ws),
            Mode::Headings => headings(ws),
            Mode::Help => help(),
        };
        self.palette.set_message(
            (mode == Mode::Headings && entries.is_empty())
                .then(|| "This document has no headings".to_string()),
        );
        self.targets = targets;
        self.palette.set_entries(entries);
    }

    /// What the entry with this id does.
    pub fn target(&self, id: u32) -> Option<&Target> {
        self.targets.get(id as usize)
    }

    /// Puts `query` in the box (a row of the `?` list) and lists what it asks for.
    pub fn set_query(&mut self, ws: &Workspace, query: &str) {
        self.palette.set_query(query);
        self.sync(ws);
    }
}

/// Pushes an entry and its target; the entry's id is the target's index.
fn push(
    entries: &mut Vec<Entry>,
    targets: &mut Vec<Target>,
    target: Target,
    e: impl FnOnce(u32) -> Entry,
) {
    entries.push(e(targets.len() as u32));
    targets.push(target);
}

fn folder_name(path: &std::path::Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn files(ws: &Workspace, with_files: bool, notes: &[Note]) -> (Vec<Entry>, Vec<Target>) {
    let (mut entries, mut targets) = (Vec::new(), Vec::new());
    // A tab's path and a recent file's are the resolved ones the workspace keeps; a note's link
    // may spell its file another way, and is compared by the key resolved for it when the
    // library was read. Nothing here asks the disk: this runs as the reader types (audit 4, E5).
    let mut listed: HashSet<PathBuf> = HashSet::new();
    // The tabs and the recent files, for the notes below.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for (i, tab) in ws.tabs().iter().enumerate() {
        let label = ws.labels().get(i).cloned().unwrap_or_default();
        let hint = match tab.is_untitled() {
            true => "mdedit".to_string(),
            false => folder_name(tab.path()),
        };
        let icon = if tab.is_missing() {
            glyph::WARN
        } else {
            glyph::DOC
        };
        listed.insert(tab.path().to_path_buf());
        seen.insert(tab.path().to_path_buf());
        push(&mut entries, &mut targets, Target::Tab(i), |id| {
            Entry::new(id, label).hint(hint).icon(icon).group(G_TABS)
        });
    }
    for e in ws.recent_entries() {
        if listed.contains(&e.path) {
            continue;
        }
        let hint = folder_name(&e.path);
        let icon = if e.missing { glyph::WARN } else { glyph::DOC };
        listed.insert(e.path.clone());
        seen.insert(e.path.clone());
        let label = e.label;
        push(&mut entries, &mut targets, Target::Recent(e.path), |id| {
            Entry::new(id, label).hint(hint).icon(icon).group(G_RECENT)
        });
    }
    if with_files {
        for (path, rel) in ws.folder_files(MAX_FILES) {
            if listed.contains(path) {
                continue;
            }
            listed.insert(path.to_path_buf());
            push(
                &mut entries,
                &mut targets,
                Target::File(path.to_path_buf()),
                |id| Entry::new(id, rel).icon(glyph::DOC).group(G_FILES),
            );
        }
    }
    for n in notes {
        if listed.contains(&n.path) || seen.contains(&n.key) {
            continue;
        }
        push(
            &mut entries,
            &mut targets,
            Target::Note(n.path.clone()),
            |id| {
                Entry::new(id, n.title.as_str())
                    .hint(n.hint.as_str())
                    .icon(if n.missing { glyph::WARN } else { glyph::DOC })
                    .group(n.group)
                    .enabled(!n.missing)
            },
        );
    }
    (entries, targets)
}

/// The words a command goes by in the palette: those of the main menu where it differs from the
/// table's.
fn label(id: C, table: &str) -> String {
    match id {
        C::Refresh => "Reload".into(),
        C::About => "About mdview".into(),
        C::ToggleFiles => "Files pane".into(),
        C::ToggleOutline => "Outline pane".into(),
        C::ThemeSystem | C::ThemeLight | C::ThemeDark => format!("Theme: {table}"),
        _ => table.to_string(),
    }
}

fn group(menu: Menu) -> u8 {
    match menu {
        Menu::File => G_FILE,
        Menu::Edit => G_EDIT,
        Menu::View | Menu::Theme => G_VIEW,
        Menu::Help => G_HELP,
        Menu::TabContext => G_TAB,
        Menu::None => G_OTHER,
    }
}

/// Every command that can run, as the menus offer them: one that cannot run now is listed
/// greyed, as the main menu does.
fn commands(ws: &Workspace) -> (Vec<Entry>, Vec<Target>) {
    let (mut entries, mut targets) = (Vec::new(), Vec::new());
    let has_tab = ws.active().is_some();
    let many = ws.tab_count() > 1;
    let has_selection = ws.has_selection();
    let any_recent = !ws.recents().is_empty();
    let mut table: Vec<_> = COMMANDS
        .iter()
        .filter(|c| match c.id {
            // The theme has its three choices; recent files are the other source; Escape is no
            // action; the palette is open already.
            C::Theme | C::OpenRecent | C::Escape | C::CommandPalette => false,
            C::MakeDefault => platform::can_offer_default_app(),
            // The tab menu's Close tab is the File menu's.
            C::CloseTab => c.menu != Menu::TabContext,
            C::LibraryAdd => ws.library_offered(),
            _ => true,
        })
        .collect();
    // Grouped as the table groups them; the order within a group is the table's.
    table.sort_by_key(|c| group(c.menu));
    for c in table {
        let enabled = match c.id {
            C::Refresh
            | C::CloseTab
            | C::CloseAllTabs
            | C::CopyFilePath
            | C::CopyCodeBlock
            | C::ShowInFileManager
            | C::EditInMdedit
            | C::ExportHtml
            | C::Print
            | C::LibraryAdd
            | C::PinTab
            | C::Present => has_tab,
            C::CloseOtherTabs | C::NextTab | C::PreviousTab => many,
            C::Back => ws.can_go_back(),
            C::Forward => ws.can_go_forward(),
            C::ClearRecent => any_recent,
            C::CopyMarkdown => has_selection,
            _ => true,
        };
        let name = label(c.id, c.label);
        let hint = crate::keys::hint(c.id);
        push(&mut entries, &mut targets, Target::Cmd(c.id), |id| {
            Entry::new(id, name)
                .hint(hint)
                .icon(command_icon(c.id))
                .group(group(c.menu))
                .enabled(enabled)
        });
    }
    (entries, targets)
}

fn headings(ws: &Workspace) -> (Vec<Entry>, Vec<Target>) {
    let (mut entries, mut targets) = (Vec::new(), Vec::new());
    for h in ws.outline() {
        if h.title.trim().is_empty() {
            continue;
        }
        let level = format!("H{}", h.level);
        push(&mut entries, &mut targets, Target::Heading(h.block), |id| {
            Entry::new(id, h.title).hint(level).group(G_PLAIN)
        });
    }
    (entries, targets)
}

fn help() -> (Vec<Entry>, Vec<Target>) {
    let (mut entries, mut targets) = (Vec::new(), Vec::new());
    for (prefix, text, hint) in [
        (
            "",
            "Open tabs, recent files and files of the folder",
            "file name",
        ),
        (">", "Commands", ">"),
        ("#", "Headings of this document", "#"),
    ] {
        push(&mut entries, &mut targets, Target::Prefix(prefix), |id| {
            Entry::new(id, text).hint(hint).group(G_PLAIN)
        });
    }
    (entries, targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdshared_app::library::{Entry as LibEntry, Group, Library};

    fn library(files: usize) -> Library {
        let groups = (0..files)
            .map(|g| Group {
                name: format!("File {g}"),
                home: None,
                file: PathBuf::from(format!("/lib/f{g}.md")),
                stamp: mdshared_app::files::FileStamp::Missing,
                sections: Vec::new(),
                entries: vec![LibEntry {
                    title: format!("Note {g}"),
                    path: PathBuf::from(format!("/lib/n{g}.md")),
                    sections: vec!["Money".into()],
                    line: 3,
                    missing: false,
                }],
            })
            .collect();
        Library {
            dir: PathBuf::from("/lib"),
            groups,
            omitted: 0,
        }
    }

    /// Audit 4, E4: a group is a byte. Every library file up to the last id has a header of its
    /// own; past that the notes go under one "More library files" header that says which file
    /// each is in. No note is ever under another file's name, and no header takes the number of
    /// another source's group (the headings' list has none).
    #[test]
    fn every_note_is_under_its_own_files_header_or_one_that_names_its_file() {
        // 512 is the most library files the shared reader keeps.
        let (groups, notes) = library_notes(&library(512), |p| p.to_path_buf());
        let name = |id: u8| {
            groups
                .iter()
                .find(|(g, _)| *g == id)
                .map(|(_, n)| n.as_str())
        };
        let mut ids: Vec<u8> = groups.iter().map(|(g, _)| *g).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            groups.len(),
            "every header has a number of its own"
        );
        for (g, n) in notes.iter().enumerate() {
            assert!(
                (G_LIBRARY..=G_LIBRARY_MORE).contains(&n.group),
                "note {g} is in the library's range"
            );
            match name(n.group) {
                Some(MORE_LIBRARY) => {
                    assert_eq!(n.hint, format!("File {g} \u{203a} Money"), "note {g}")
                }
                header => {
                    assert_eq!(header, Some(format!("File {g}").as_str()), "note {g}");
                    assert_eq!(n.hint, "Money");
                }
            }
        }
        let own = usize::from(G_LIBRARY_LAST - G_LIBRARY) + 1;
        assert_eq!(notes[own - 1].group, G_LIBRARY_LAST);
        assert_eq!(notes[own].group, G_LIBRARY_MORE);
        assert_eq!(name(G_PLAIN), None, "the headings keep no header");
        for g in [
            G_TABS, G_RECENT, G_FILES, G_FILE, G_EDIT, G_VIEW, G_HELP, G_TAB, G_OTHER,
        ] {
            assert!(g < G_LIBRARY && name(g).is_none());
        }
    }
}
