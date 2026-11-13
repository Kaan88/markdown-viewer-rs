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

use crate::overlays::command_icon;
use crate::platform;
use crate::workspace::Workspace;
use mdshared_ui::glyph;
use mdshared_ui::palette::{Entry, Palette};
use mdview_core::commands::{COMMANDS, CommandId as C, Menu};
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
/// Headings and the help rows have no header.
const G_PLAIN: u8 = 100;

/// What choosing an entry does.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Cmd(C),
    Tab(usize),
    Recent(PathBuf),
    File(PathBuf),
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

pub struct PaletteState {
    pub palette: Palette,
    targets: Vec<Target>,
    /// The source the entries came from; `None` before the first.
    mode: Option<Mode>,
    /// Whether the folder's files are among the entries (only once the query has text).
    with_files: bool,
}

impl PaletteState {
    /// A palette open on `query`, with its entries.
    pub fn open(ws: &Workspace, query: &str) -> PaletteState {
        let mut palette = Palette::new();
        palette.set_placeholder(PLACEHOLDER);
        palette.set_groups(vec![
            (G_TABS, "Open tabs".into()),
            (G_RECENT, "Recent".into()),
            (G_FILES, "Files".into()),
            (G_FILE, "File".into()),
            (G_EDIT, "Edit".into()),
            (G_VIEW, "View".into()),
            (G_HELP, "Help".into()),
            (G_TAB, "Tab".into()),
            (G_OTHER, "Other".into()),
        ]);
        palette.open(query);
        let mut state = PaletteState {
            palette,
            targets: Vec::new(),
            mode: None,
            with_files: false,
        };
        state.sync(ws);
        state
    }

    /// Swaps the entries when the query's prefix (or, for files, whether it has text) changed
    /// the source. Called after the query changed; typing within a mode only re-ranks, which the
    /// palette does by itself.
    pub fn sync(&mut self, ws: &Workspace) {
        let (mode, skip) = mode_of(self.palette.query());
        let with_files = mode == Mode::Files && !self.palette.needle().is_empty();
        if self.mode == Some(mode) && (mode != Mode::Files || with_files == self.with_files) {
            return;
        }
        self.mode = Some(mode);
        self.with_files = with_files;
        self.palette.set_ignore_prefix(skip);
        let (entries, targets) = match mode {
            Mode::Files => files(ws, with_files),
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

fn files(ws: &Workspace, with_files: bool) -> (Vec<Entry>, Vec<Target>) {
    let (mut entries, mut targets) = (Vec::new(), Vec::new());
    let mut listed: HashSet<PathBuf> = HashSet::new();
    for (i, tab) in ws.tabs().iter().enumerate() {
        let label = ws.labels().get(i).cloned().unwrap_or_default();
        let hint = folder_name(tab.path());
        let icon = if tab.is_missing() {
            glyph::WARN
        } else {
            glyph::DOC
        };
        listed.insert(tab.path().to_path_buf());
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
            push(
                &mut entries,
                &mut targets,
                Target::File(path.to_path_buf()),
                |id| Entry::new(id, rel).icon(glyph::DOC).group(G_FILES),
            );
        }
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
            _ => true,
        })
        .collect();
    // Grouped as the table groups them; the order within a group is the table's.
    table.sort_by_key(|c| group(c.menu));
    for c in table {
        let enabled = match c.id {
            C::Refresh | C::CloseTab | C::CloseAllTabs | C::CopyFilePath | C::ShowInFileManager => {
                has_tab
            }
            C::CloseOtherTabs | C::NextTab | C::PreviousTab => many,
            C::ClearRecent => any_recent,
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
