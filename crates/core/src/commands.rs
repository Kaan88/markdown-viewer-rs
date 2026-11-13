//! The command table both shells build menus, toolbars and shortcuts from, and all status text.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    OpenFiles,
    OpenFolder,
    Refresh,
    OpenRecent,
    ManageRecent,
    ClearRecent,
    CloseTab,
    CloseAllTabs,
    Exit,
    Find,
    FindNext,
    FindPrevious,
    ClearSearch,
    Theme,
    ThemeSystem,
    ThemeLight,
    ThemeDark,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleFiles,
    ToggleOutline,
    FocusMode,
    ShowShortcuts,
    About,
    NextTab,
    PreviousTab,
    Escape,
    CloseOtherTabs,
    CopyFilePath,
    ShowInFileManager,
    /// Linux: make mdview the default app for Markdown files. In no menu bar menu; the shell's
    /// main menu offers it where it applies.
    MakeDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Edit,
    View,
    Help,
    /// The submenu `CommandId::Theme` opens in View.
    Theme,
    TabContext,
    /// Keyboard only.
    None,
}

impl Menu {
    /// The menu bar title for a top-level menu, with `&` marking its access key, as Qt takes it.
    pub const fn title(self) -> Option<&'static str> {
        match self {
            Menu::File => Some("&File"),
            Menu::Edit => Some("&Edit"),
            Menu::View => Some("&View"),
            Menu::Help => Some("&Help"),
            Menu::Theme | Menu::TabContext | Menu::None => None,
        }
    }
}

/// A key the platform defines, so each shell maps it to its native sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardKey {
    Quit,
    ZoomIn,
    ZoomOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Keys(&'static str),
    Standard(StandardKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStyle {
    Action,
    Checkable,
    /// One of an exclusive radio group.
    Radio,
    Submenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub id: CommandId,
    pub menu: Menu,
    pub label: &'static str,
    pub shortcuts: &'static [Shortcut],
    pub style: CommandStyle,
    /// Draw a separator before this item.
    pub separator_before: bool,
}

const fn cmd(
    id: CommandId,
    menu: Menu,
    label: &'static str,
    shortcuts: &'static [Shortcut],
) -> Command {
    Command {
        id,
        menu,
        label,
        shortcuts,
        style: CommandStyle::Action,
        separator_before: false,
    }
}

const fn styled(mut c: Command, style: CommandStyle) -> Command {
    c.style = style;
    c
}

const fn sep(mut c: Command) -> Command {
    c.separator_before = true;
    c
}

use CommandId as C;
use Shortcut::{Keys as K, Standard as S};

const SHOW_IN_FILE_MANAGER: &str = if cfg!(windows) {
    "Show in File Explorer"
} else {
    "Show in file manager"
};

/// In menu order. Items of one menu appear in the order shown to the user.
pub const COMMANDS: &[Command] = &[
    cmd(C::OpenFiles, Menu::File, "Open files…", &[K("Ctrl+O")]),
    cmd(
        C::OpenFolder,
        Menu::File,
        "Open folder…",
        &[K("Ctrl+Shift+O")],
    ),
    cmd(C::Refresh, Menu::File, "Refresh", &[K("F5")]),
    sep(styled(
        cmd(C::OpenRecent, Menu::File, "Open recent", &[]),
        CommandStyle::Submenu,
    )),
    cmd(C::ManageRecent, Menu::File, "Manage recent files…", &[]),
    cmd(C::ClearRecent, Menu::File, "Clear recent files", &[]),
    sep(cmd(
        C::CloseTab,
        Menu::File,
        "Close tab",
        &[K("Ctrl+W"), K("Ctrl+F4")],
    )),
    cmd(
        C::CloseAllTabs,
        Menu::File,
        "Close all tabs",
        &[K("Ctrl+Shift+W")],
    ),
    sep(cmd(C::Exit, Menu::File, "Exit", &[S(StandardKey::Quit)])),
    cmd(C::Find, Menu::Edit, "Find", &[K("Ctrl+F")]),
    cmd(C::FindNext, Menu::Edit, "Find next", &[K("F3")]),
    cmd(
        C::FindPrevious,
        Menu::Edit,
        "Find previous",
        &[K("Shift+F3")],
    ),
    cmd(C::ClearSearch, Menu::Edit, "Clear search", &[]),
    styled(
        cmd(C::Theme, Menu::View, "Theme", &[]),
        CommandStyle::Submenu,
    ),
    styled(
        cmd(C::ThemeSystem, Menu::Theme, "System", &[]),
        CommandStyle::Radio,
    ),
    styled(
        cmd(C::ThemeLight, Menu::Theme, "Light", &[]),
        CommandStyle::Radio,
    ),
    styled(
        cmd(C::ThemeDark, Menu::Theme, "Dark", &[]),
        CommandStyle::Radio,
    ),
    sep(cmd(
        C::ZoomIn,
        Menu::View,
        "Zoom in",
        &[S(StandardKey::ZoomIn)],
    )),
    cmd(
        C::ZoomOut,
        Menu::View,
        "Zoom out",
        &[S(StandardKey::ZoomOut)],
    ),
    cmd(
        C::ZoomReset,
        Menu::View,
        "Reset zoom to 100%",
        &[K("Ctrl+0")],
    ),
    styled(
        cmd(C::ToggleFiles, Menu::View, "FILES", &[K("Ctrl+Shift+1")]),
        CommandStyle::Checkable,
    ),
    styled(
        cmd(
            C::ToggleOutline,
            Menu::View,
            "ON THIS PAGE",
            &[K("Ctrl+Shift+2")],
        ),
        CommandStyle::Checkable,
    ),
    sep(styled(
        cmd(
            C::FocusMode,
            Menu::View,
            "Focus reading mode",
            &[K("Ctrl+Shift+F")],
        ),
        CommandStyle::Checkable,
    )),
    cmd(C::ShowShortcuts, Menu::Help, "Keyboard shortcuts", &[]),
    cmd(C::About, Menu::Help, "About Markdown Viewer", &[]),
    cmd(C::CloseTab, Menu::TabContext, "Close tab", &[]),
    cmd(C::CloseOtherTabs, Menu::TabContext, "Close other tabs", &[]),
    cmd(C::CopyFilePath, Menu::TabContext, "Copy file path", &[]),
    cmd(
        C::ShowInFileManager,
        Menu::TabContext,
        SHOW_IN_FILE_MANAGER,
        &[],
    ),
    cmd(
        C::MakeDefault,
        Menu::None,
        "Make mdview the default for Markdown files",
        &[],
    ),
    cmd(C::NextTab, Menu::None, "Next tab", &[K("Ctrl+Tab")]),
    cmd(
        C::PreviousTab,
        Menu::None,
        "Previous tab",
        &[K("Ctrl+Shift+Tab")],
    ),
    cmd(
        C::Escape,
        Menu::None,
        "Leave focus mode or search",
        &[K("Escape")],
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarItem {
    /// Runs the command. The label and tooltip may differ from its menu item's.
    Button {
        id: CommandId,
        label: &'static str,
        tooltip: Option<&'static str>,
    },
    Separator,
    /// The find box, `strings::FIND_PLACEHOLDER`.
    FindBox,
    /// The match counter beside it, `strings::find_label`.
    MatchCount,
}

const fn button(id: CommandId, label: &'static str, tooltip: Option<&'static str>) -> ToolbarItem {
    ToolbarItem::Button { id, label, tooltip }
}

/// In the order shown, titled `strings::TOOLBAR_TITLE`.
pub const TOOLBAR: &[ToolbarItem] = &[
    button(C::OpenFiles, "Open files…", None),
    button(C::OpenFolder, "Open folder…", None),
    button(C::Refresh, "Refresh", None),
    ToolbarItem::Separator,
    ToolbarItem::FindBox,
    ToolbarItem::MatchCount,
    button(
        C::FindPrevious,
        "Previous",
        Some("Previous match (Shift+F3)"),
    ),
    button(C::FindNext, "Next", Some("Next match (F3)")),
];

pub fn command(id: CommandId) -> &'static Command {
    COMMANDS
        .iter()
        .find(|c| c.id == id)
        .expect("every CommandId has a table entry")
}

pub fn menu_items(menu: Menu) -> impl Iterator<Item = &'static Command> {
    COMMANDS.iter().filter(move |c| c.menu == menu)
}

/// Lines for the Keyboard shortcuts dialog, generated so it cannot drift from the real bindings.
pub fn shortcut_help() -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for c in COMMANDS {
        if c.shortcuts.is_empty() || c.menu == Menu::TabContext {
            continue;
        }
        let keys: Vec<&str> = c
            .shortcuts
            .iter()
            .map(|s| match s {
                Shortcut::Keys(k) => *k,
                Shortcut::Standard(StandardKey::Quit) => "Ctrl+Q",
                Shortcut::Standard(StandardKey::ZoomIn) => "Ctrl++",
                Shortcut::Standard(StandardKey::ZoomOut) => "Ctrl+-",
            })
            .collect();
        out.push((keys.join(", "), c.label));
    }
    out
}

pub mod strings {
    pub const APP_NAME: &str = "Markdown Viewer";
    pub const ABOUT: &str = "A native, read-only Markdown reader.";
    pub const TRANSIENT_MS: u32 = 2500;

    pub const TOOLBAR_TITLE: &str = "Reader";
    pub const MANAGE_RECENT_TITLE: &str = "Manage recent files";
    pub const SHORTCUTS_TITLE: &str = "Keyboard shortcuts";
    pub const ABOUT_TITLE: &str = "About Markdown Viewer";
    pub const MATCH_COUNT_NAME: &str = "Search result count";

    pub const EMPTY_HINT: &str = "Open a Markdown file or drag one into this window.";
    pub const EMPTY_BUTTON: &str = "Open Markdown file…";
    pub const STATUS_STARTUP: &str = "Open a README or drop a Markdown file here.";
    pub const STATUS_ALL_CLOSED: &str = "Open a Markdown file or drag one into this window.";

    pub const REFRESHING: &str = "Refreshing document…";
    pub const REFRESHED: &str = "Document refreshed from disk";
    pub const RENDER_TIMEOUT: &str = "Rendering exceeded 15 seconds. Try a smaller document.";
    pub const REMOTE_ONE: &str = "This document has 1 remote image that was not loaded.";
    pub const REMOTE_MANY: &str = "This document has {} remote images that were not loaded.";
    pub const REMOTE_LOAD: &str = "Load images";
    pub const REMOTE_ALWAYS: &str = "Always load";
    pub const FILE_DELETED: &str =
        "File was deleted or is unavailable. Showing the last loaded content.";
    pub const FILE_NOT_FOUND: &str =
        "File not found. Choose another file or remove this entry from Recent.";
    pub const DEFAULT_APP_DONE: &str = "mdview is now the default for Markdown files";
    pub const DEFAULT_APP_FAILED: &str = "Could not make mdview the default";
    pub const ONLY_MARKDOWN_LINKS: &str = "Only Markdown file links open inside the viewer.";

    pub const RECENT_EMPTY: &str = "No recent files";
    pub const RECENT_CLEARED: &str = "Recent files cleared";
    pub const RECENT_REMOVED_MISSING: &str = "Removed a missing file from Recent";
    pub const RECENT_REMOVE_SELECTED: &str = "Remove selected";
    pub const RECENT_CLEAR_LIST: &str = "Clear list";
    pub const RECENT_DONE: &str = "Done";

    pub const FIND_PLACEHOLDER: &str = "Find in document";
    pub const FIND_NO_MATCHES: &str = "No matches";
    pub const DETAILS_DEFAULT_SUMMARY: &str = "Details";
    pub const IMAGE_PLACEHOLDER: &str = "Image loading / unavailable";

    pub const FILES_DOCK: &str = "FILES";
    pub const OUTLINE_DOCK: &str = "ON THIS PAGE";
    pub const MARKDOWN_FILTER: &str = "Markdown (*.md *.markdown *.mdown *.mkd);;All files (*)";

    pub fn window_title(file: Option<&str>) -> String {
        match file {
            Some(f) => format!("{f} — {APP_NAME}"),
            None => APP_NAME.to_string(),
        }
    }

    /// The match counter beside the find box. `current` is 1-based; 0 means no match is active.
    pub fn find_label(query_empty: bool, current: usize, total: usize, capped: bool) -> String {
        let plus = if capped { "+" } else { "" };
        match (query_empty, total, current) {
            (true, _, _) => String::new(),
            (false, 0, _) => FIND_NO_MATCHES.to_string(),
            (false, _, 0) => format!("{total}{plus} matches"),
            _ => format!("{current} of {total}{plus}"),
        }
    }

    pub fn find_tooltip(total: usize, capped: bool) -> String {
        match (total, capped) {
            (0, _) => "No matches in this document".to_string(),
            (_, true) => format!("{total}+ matches in this document"),
            _ => format!("{total} matches in this document"),
        }
    }
}
