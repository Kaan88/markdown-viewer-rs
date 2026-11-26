//! The window's own state, apart from the documents (which live in `Workspace`): what the pointer
//! is over, what is being dragged, which popup or dialog is open, the panes and the find box.
//! Drawing code reads it; the event loop in `app.rs` changes it.

use crate::gfx::{Painter, Rect};
use crate::keys::{KeyName, Mods};
use crate::menu::Menu;
use crate::textfield::TextField;
use crate::theme::Palette;
use crate::tree::{FileTree, new_tree};
use mdshared_ui::anim::Animator;
use mdshared_ui::menu::Entrance;
use mdshared_ui::submenu::{Cascade, Outcome};
use mdview_core::commands::CommandId;
use mdview_render::SourceKind;
use std::path::PathBuf;
use std::time::Instant;

pub use mdshared_ui::chrome::{BAR_H, TITLE_H};
pub const PANE_MIN: f32 = 160.0;
pub const PANE_MAX: f32 = 480.0;
/// Below this the page would be too narrow to read; panes give way first.
pub const PAGE_MIN: f32 = 360.0;
/// The missing-file banner over the page.
pub const BANNER_H: f32 = 34.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Side {
    Files,
    Outline,
}

/// What can scroll with an overlay scrollbar.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Scroller {
    Page,
    Pane(Side),
    Dialog,
}

/// The command bar's buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BarItem {
    Open,
    Reload,
    Recent,
    Files,
    Outline,
    ZoomOut,
    Zoom,
    ZoomIn,
    /// Steps the page width: narrow, wide, full.
    PageWidth,
    FindPrev,
    FindNext,
    Theme,
    More,
}

/// The screen areas that are redrawn independently.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Region {
    Title,
    Bar,
    Files,
    Outline,
    Page,
}

pub const REGIONS: [Region; 5] = [
    Region::Title,
    Region::Bar,
    Region::Files,
    Region::Outline,
    Region::Page,
];

impl Region {
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Something the pointer can be over.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Hit {
    Tab(usize),
    TabClose(usize),
    NewTab,
    TitleDrag,
    LeaveFocus,
    WinMin,
    WinMax,
    WinClose,
    Bar(BarItem),
    Find,
    FindClear,
    FilesRow(usize),
    FilesFolder,
    FilesOpenFolder,
    /// A switch in the Files pane's head: 0 the files, 1 the notes library.
    PaneFace(usize),
    /// A row of the Notes face.
    NotesRow(usize),
    /// The Notes face's button that reads the library again.
    NotesRefresh,
    OutlineRow(usize),
    /// An entry of the Backlinks list under the outline.
    BacklinkRow(usize),
    Splitter(Side),
    ScrollBar(Scroller),
    Page,
    /// The copy button of the code block under the pointer.
    CopyCode,
    EmptyOpen,
    EmptyRecent(usize),
    /// The bar over a page that holds remote images back.
    RemoteLoad,
    RemoteAlways,
    RemoteClose,
    /// The bar that offers a newer version.
    UpdateGo,
    /// The update bar's Restart button, once the new version is installed.
    UpdateRestart,
    UpdateClose,
    /// The bar that offers last time's crash report.
    CrashOpen,
    CrashCopy,
    CrashClose,
    /// The bar that offers to close the tabs not opened for a week of use.
    StaleClose,
    StaleKeep,
    StaleDismiss,
    DlgButton(usize),
    DlgOpen(usize),
    DlgRemove(usize),
    DlgCard,
    Backdrop,
    /// The close button of the diagram zoom view.
    ZoomClose,
}

impl Hit {
    /// The region whose look depends on this hit being hovered or pressed.
    pub fn region(self) -> Option<Region> {
        Some(match self {
            Hit::Tab(_) | Hit::TabClose(_) | Hit::NewTab | Hit::TitleDrag | Hit::LeaveFocus => {
                Region::Title
            }
            Hit::WinMin | Hit::WinMax | Hit::WinClose => Region::Title,
            Hit::Bar(_) | Hit::Find | Hit::FindClear => Region::Bar,
            Hit::FilesRow(_)
            | Hit::FilesFolder
            | Hit::FilesOpenFolder
            | Hit::PaneFace(_)
            | Hit::NotesRow(_)
            | Hit::NotesRefresh => Region::Files,
            Hit::Splitter(Side::Files) | Hit::ScrollBar(Scroller::Pane(Side::Files)) => {
                Region::Files
            }
            Hit::OutlineRow(_) | Hit::BacklinkRow(_) => Region::Outline,
            Hit::Splitter(Side::Outline) | Hit::ScrollBar(Scroller::Pane(Side::Outline)) => {
                Region::Outline
            }
            Hit::ScrollBar(Scroller::Page)
            | Hit::Page
            | Hit::CopyCode
            | Hit::EmptyOpen
            | Hit::EmptyRecent(_)
            | Hit::RemoteLoad
            | Hit::RemoteAlways
            | Hit::RemoteClose
            | Hit::UpdateGo
            | Hit::UpdateRestart
            | Hit::UpdateClose
            | Hit::CrashOpen
            | Hit::CrashCopy
            | Hit::CrashClose
            | Hit::StaleClose
            | Hit::StaleKeep
            | Hit::StaleDismiss => Region::Page,
            _ => return None,
        })
    }

    /// Hits that act on release over the same hit, and show a pressed state meanwhile.
    pub fn is_button(self) -> bool {
        !matches!(
            self,
            Hit::TitleDrag
                | Hit::Page
                | Hit::Splitter(_)
                | Hit::ScrollBar(_)
                | Hit::Find
                | Hit::Tab(_)
                | Hit::DlgCard
        )
    }
}

/// What a popup menu item does.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Cmd(CommandId),
    /// A tab context menu command for this tab.
    TabCmd(CommandId, usize),
    Recent(PathBuf),
    /// Add the file waiting to be added to the notes library group `i` of its list.
    LibraryPut(usize),
    /// Open the Recent menu beside the item.
    RecentMenu,
    OpenLink(String),
    CopyLink(String),
    /// Copy the TeX or Mermaid text a formula or diagram was drawn from.
    CopySource(SourceKind, String),
    Copy,
    SelectAll,
    /// An editing command for the find box.
    Edit(EditAct),
}

impl Act {
    /// Whether the item leads to a submenu (the ones drawn with a "›").
    pub fn is_submenu(&self) -> bool {
        matches!(self, Act::RecentMenu)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditAct {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PopupKind {
    Main,
    Recent,
    Tab(usize),
    Page,
    /// The find box's editing menu.
    Find,
    /// The groups of the notes library, to choose where a file is added.
    Library,
}

/// What a popup's overlay signature holds of one open menu: its highlighted row, its scroll in
/// bits and how many rows it has.
type RowState = (Option<usize>, u32, usize);

/// Everything of a popup that changes what is drawn when it is the same kind: each open menu's
/// rows, and the path of open submenus.
pub type PopupState = (Vec<RowState>, Vec<usize>);

/// A popup's menu: one that opens nothing beside it, or the shared cascade that the main menu's
/// Open recent needs.
pub enum MenuBody {
    /// A menu alone: a context menu, the recent list, the find box's.
    Menu(Menu<Act>),
    /// A tree of menus: the main menu, and the submenu its Open recent row opens.
    Cascade(Cascade<Act>),
}

pub struct Popup {
    pub kind: PopupKind,
    pub body: MenuBody,
    /// The marked letters of the items are underlined: the menu was opened with Alt held, or Alt
    /// was pressed since (the Windows convention; a menu opened with the pointer shows none).
    pub access: bool,
    /// This opening of the menu: what its fade-in is known by (`crate::motion`), so a menu
    /// opened again fades in again.
    pub serial: u64,
}

/// A number no popup had before: each opening of a menu is a new one.
fn next_serial() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The room a cascade is placed in: the shared placement keeps 4 px from the edges of its area,
/// while a menu's own placement kept them from the edges of the window. The area is widened by
/// them at the top and the bottom, so the shared margin falls where the menu's edge was and a
/// menu taller than the window is placed as it always was.
fn cascade_room(area: Rect) -> Rect {
    Rect::new(area.x, area.y - 4.0, area.w, area.h + 4.0)
}

impl Popup {
    pub fn new(kind: PopupKind, mut menu: Menu<Act>) -> Popup {
        menu.set_access_visible(false);
        Popup {
            kind,
            body: MenuBody::Menu(menu),
            access: false,
            serial: next_serial(),
        }
    }

    /// A popup whose menu is a `submenu::Cascade`: the main menu, whose Open recent opens the
    /// recent list beside it.
    pub fn main(kind: PopupKind, mut cascade: Cascade<Act>) -> Popup {
        cascade.set_access_visible(false);
        Popup {
            kind,
            body: MenuBody::Cascade(cascade),
            access: false,
            serial: next_serial(),
        }
    }

    /// Underlines the access keys of every menu of the popup (a menu that does not read access
    /// keys has none to show).
    pub fn show_access(&mut self) {
        self.access = true;
        match &mut self.body {
            MenuBody::Menu(m) => m.set_access_visible(true),
            MenuBody::Cascade(c) => c.set_access_visible(true),
        }
    }

    /// The menu the popup draws first: its items and rows (`Menu::rect`, `Menu::items`). The
    /// window reads the menus through the methods here; the tests read these.
    #[cfg(test)]
    pub fn root(&self) -> &Menu<Act> {
        match &self.body {
            MenuBody::Menu(m) => m,
            MenuBody::Cascade(c) => c.menu(0).expect("a cascade has its root"),
        }
    }

    /// [`root`](Self::root), to change: its highlight, its scroll.
    pub fn root_mut(&mut self) -> &mut Menu<Act> {
        match &mut self.body {
            MenuBody::Menu(m) => m,
            MenuBody::Cascade(c) => c.menu_mut(0).expect("a cascade has its root"),
        }
    }

    /// The submenu open beside the menu, if one is: the cascade's second menu. The tests read it;
    /// the window asks the cascade.
    #[cfg(test)]
    pub fn submenu(&self) -> Option<&Menu<Act>> {
        match &self.body {
            MenuBody::Menu(_) => None,
            MenuBody::Cascade(c) => c.menu(1),
        }
    }

    /// Whether the point is on the menu or on its open submenu.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        match &self.body {
            MenuBody::Menu(m) => m.contains(x, y),
            MenuBody::Cascade(c) => c.contains(x, y),
        }
    }

    /// The pointer moved to `x`, `y` at `now`. True when what is drawn changed.
    pub fn pointer(&mut self, x: f32, y: f32, now: Instant) -> bool {
        match &mut self.body {
            MenuBody::Menu(m) => m.pointer(x, y),
            // The cascade never closes an open submenu on the move alone: on the way there the
            // pointer crosses padding and other rows, and only a rest there closes it.
            MenuBody::Cascade(c) => c.pointer(x, y, now),
        }
    }

    /// A click: the item under the point, when it is one. `Outcome::Outside` when the click was
    /// not on the menu, `Outcome::Changed` when it opened a submenu.
    pub fn click(&mut self, x: f32, y: f32) -> Outcome<Act> {
        match &mut self.body {
            MenuBody::Menu(m) => {
                if !m.contains(x, y) {
                    return Outcome::Outside;
                }
                m.chosen().map_or(Outcome::Nothing, Outcome::Chosen)
            }
            MenuBody::Cascade(c) => c.click(x, y),
        }
    }

    /// The wheel turned by `dy` over the menus: scrolls the menu under the point, if it is
    /// taller than its room. True when a menu scrolled.
    pub fn wheel(&mut self, x: f32, y: f32, dy: f32) -> bool {
        match &mut self.body {
            MenuBody::Menu(m) => {
                m.contains(x, y) && m.scrolls() && m.scroll_by(dy) && {
                    m.pointer(x, y);
                    true
                }
            }
            MenuBody::Cascade(c) => {
                let moved = c.wheel(x, y, dy);
                if moved {
                    // The rows moved under the pointer: its row follows them.
                    c.pointer(x, y, Instant::now());
                }
                moved
            }
        }
    }

    /// A key, for the submenu if one is open and the menu otherwise. `Chosen` closes the popup
    /// after acting; `Close` closes it; anything else changed only what is drawn.
    pub fn key(&mut self, name: &KeyName, mods: Mods) -> Keyed {
        match &mut self.body {
            MenuBody::Menu(m) => match name {
                KeyName::Up => {
                    m.step(false);
                    Keyed::Nothing
                }
                KeyName::Down | KeyName::Tab => {
                    m.step(true);
                    Keyed::Nothing
                }
                KeyName::Enter | KeyName::Space | KeyName::Right => {
                    let arrow = *name == KeyName::Right;
                    match m.chosen() {
                        Some(act) if !arrow || act.is_submenu() => Keyed::Chosen(act),
                        _ => Keyed::Nothing,
                    }
                }
                KeyName::Escape => Keyed::Close,
                // A letter picks the item marked with it, as Enter would (menus that read no
                // access keys answer None and the key does nothing, as before).
                KeyName::Char(ch) if !mods.ctrl && !mods.alt => match m.access_key(*ch) {
                    Some(act) => Keyed::Chosen(act),
                    None => Keyed::Nothing,
                },
                _ => Keyed::Nothing,
            },
            MenuBody::Cascade(c) => cascade_key(c, name, mods),
        }
    }

    /// When the pointer's rest is up: when a submenu opens, or the open one closes.
    pub fn deadline(&self) -> Option<Instant> {
        match &self.body {
            MenuBody::Menu(_) => None,
            MenuBody::Cascade(c) => c.deadline(),
        }
    }

    /// The pointer's rest is up at `now`: its ask is made. True when what is drawn changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        match &mut self.body {
            MenuBody::Menu(_) => false,
            MenuBody::Cascade(c) => c.tick(now),
        }
    }

    /// Opens the submenu of the enabled row that opens one (Open recent, in the main menu), with
    /// its first row highlighted for a keyboard opening. One asked by the pointer or by name
    /// lights none, as a submenu the pointer opens.
    pub fn open_submenu(&mut self, keyboard: bool) {
        let MenuBody::Cascade(c) = &mut self.body else {
            return;
        };
        let Some(i) = c.menu(0).and_then(|m| {
            m.items
                .iter()
                .position(|it| it.id.is_submenu() && it.enabled)
        }) else {
            return;
        };
        if let Some(root) = c.menu_mut(0) {
            root.hover = Some(i);
        }
        c.key(&KeyName::Right);
        // A keyboard opening lit its first row (`enter`); one asked by the pointer or by name
        // lights none, as a submenu the pointer opens.
        if !keyboard
            && c.depth() > 1
            && let Some(sub) = c.menu_mut(1)
        {
            sub.hover = None;
        }
    }

    /// Places the menu and its open submenu inside `area`.
    pub fn place_in(&mut self, p: &mut Painter, area: Rect) {
        match &mut self.body {
            MenuBody::Menu(m) => m.place_in(p, area),
            MenuBody::Cascade(c) => c.place_in(p, cascade_room(area)),
        }
    }

    /// Draws the menu and its open submenu at their presence `t.0` and `t.1`: each fades in as
    /// it opened, its row's hover fill fading with it (`draw_in_animated`).
    pub fn draw(
        &mut self,
        p: &mut Painter,
        pal: &Palette,
        area: Rect,
        t: (f32, f32),
        anim: &mut Animator,
    ) {
        let (tm, ts) = t;
        let serial = self.serial;
        match &mut self.body {
            MenuBody::Menu(m) => {
                m.draw_entering_with(p, area, tm, Entrance::Drop, |p, m| {
                    m.draw_in_animated(p, pal, area, anim, ("menu-hover", serial));
                });
            }
            MenuBody::Cascade(c) => {
                let room = cascade_room(area);
                c.place_in(p, room);
                let depth = c.depth();
                if let Some(root) = c.menu_mut(0) {
                    root.draw_entering_with(p, room, tm, Entrance::Drop, |p, m| {
                        m.draw_in_animated(p, pal, room, anim, ("menu-hover", serial));
                    });
                }
                for k in 1..depth {
                    let leftward = c.menu(k).is_some_and(|m| m.opened_left());
                    if let Some(sub) = c.menu_mut(k) {
                        sub.draw_entering_with(p, room, ts, Entrance::Side { leftward }, |p, m| {
                            m.draw_in_animated(p, pal, room, anim, ("submenu-hover", serial));
                        });
                    }
                }
            }
        }
    }

    /// The presence of the menu and of its open submenu, asked on every frame they are open
    /// (`crate::motion`): a menu opened again fades in again, and a submenu opened afresh (its
    /// path is new) fades in while one put back in its own place does not.
    pub fn presence(&self, anim: &mut Animator) -> (f32, f32) {
        let menu = crate::motion::menu(anim, self.serial);
        let sub = match &self.body {
            MenuBody::Cascade(c) if c.depth() > 1 => {
                crate::motion::submenu(anim, self.serial, c.path())
            }
            _ => 1.0,
        };
        (menu, sub)
    }

    /// Everything the popup draws at any presence, its shadow and its drop included.
    pub fn presence_bounds(&self) -> Rect {
        match &self.body {
            MenuBody::Menu(m) => m.presence_bounds(),
            MenuBody::Cascade(c) => c.presence_bounds(),
        }
    }

    /// The rows, scrolls and item counts of the menus, and the path of open submenus: what
    /// changes what is drawn for the overlay signature.
    pub fn state(&self) -> PopupState {
        match &self.body {
            MenuBody::Menu(m) => (
                vec![(m.hover, m.scroll().to_bits(), m.items.len())],
                Vec::new(),
            ),
            MenuBody::Cascade(c) => (
                (0..c.depth())
                    .filter_map(|k| c.menu(k))
                    .map(|m| (m.hover, m.scroll().to_bits(), m.items.len()))
                    .collect(),
                c.path().to_vec(),
            ),
        }
    }
}

/// A key for the cascade's menus. Home and End, and a letter with a modifier, were no menu key
/// before and are none now. The window's keys went to the open submenu whenever one was open,
/// however it was opened; the shared cascade keeps them in the menu the pointer was last in, so
/// a Right moves them in first, and the row it lights is put out again when no row of that
/// submenu was lit (the pointer opened it, and a pointer-opened submenu shows none).
fn cascade_key(c: &mut Cascade<Act>, name: &KeyName, mods: Mods) -> Keyed {
    if matches!(name, KeyName::Home | KeyName::End) {
        return Keyed::Nothing;
    }
    if matches!(name, KeyName::Char(_)) && (mods.ctrl || mods.alt) {
        return Keyed::Nothing;
    }
    // Escape and Left act on what is open now. A pointer change still waiting (the close of the
    // open submenu, because the pointer rested on another row a moment ago) is dropped instead
    // of made, and the row that opened the submenu is lit again — shared's `cancel_wait`, PR 40.
    // Left is chosen in `cascade_key` rather than by `Cascade::keys_cancel_wait` because the
    // `Right` below would make the waiting change first, and only these two keys drop it; the
    // rest still make it, as shared has them.
    if matches!(name, KeyName::Escape | KeyName::Left) {
        c.cancel_wait();
    }
    if c.depth() > 1 && c.focus() == 0 {
        let lit = c.menu(1).is_some_and(|s| s.hover.is_some());
        c.key(&KeyName::Right);
        if !lit && let Some(sub) = c.menu_mut(1) {
            sub.hover = None;
        }
    }
    // Tab has always stepped down the open menu.
    let key = if matches!(name, KeyName::Tab) {
        KeyName::Down
    } else {
        name.clone()
    };
    match c.key(&key) {
        Outcome::Chosen(act) => Keyed::Chosen(act),
        Outcome::Close => Keyed::Close,
        _ => Keyed::Nothing,
    }
}

/// What a pressed key did to the open menu.
pub enum Keyed {
    /// Nothing that is drawn changed.
    Nothing,
    /// An item was chosen: act on it, and the popup closes.
    Chosen(Act),
    /// Escape with nothing open inside the menu: the popup closes.
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DialogKind {
    About,
    Shortcuts,
    Recent,
    /// Asks before the first task box is ticked in a file.
    AllowEdit,
    /// Asks before the first note is added to the notes library.
    AddToLibrary,
}

pub struct Dialog {
    pub kind: DialogKind,
    /// Scroll of a long list, logical px.
    pub scroll: f32,
    /// The scrollable list's height and its content's, set when drawn.
    pub view: f32,
    pub content: f32,
    pub list: Rect,
}

impl Dialog {
    pub fn new(kind: DialogKind) -> Dialog {
        Dialog {
            kind,
            scroll: 0.0,
            view: 0.0,
            content: 0.0,
            list: Rect::default(),
        }
    }
}

/// A press being held: what it started on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    /// A tab pressed; it becomes a reorder once the pointer moves a few px.
    Tab {
        index: usize,
        x: f32,
        moving: bool,
    },
    Splitter {
        side: Side,
        x: f32,
        width: f32,
    },
    /// A scrollbar thumb, grabbed `grab` logical px below its top.
    Thumb {
        target: Scroller,
        grab: f32,
    },
    /// Selecting text on the page.
    Select,
    /// Selecting in the find box.
    FindSelect,
    /// Panning the diagram zoom view; the pointer's last position, logical px.
    Zoom {
        last: (f32, f32),
    },
}

pub struct Panes {
    pub files: bool,
    pub outline: bool,
    pub files_w: f32,
    pub outline_w: f32,
    pub files_scroll: f32,
    pub outline_scroll: f32,
    /// Content heights, set when drawn, for wheel clamping.
    pub files_content: f32,
    pub outline_content: f32,
    /// Row pitch view heights, set when drawn.
    pub files_view: f32,
    pub outline_view: f32,
}

impl Panes {
    pub fn scroll_mut(&mut self, side: Side) -> &mut f32 {
        match side {
            Side::Files => &mut self.files_scroll,
            Side::Outline => &mut self.outline_scroll,
        }
    }

    /// Scrolls a pane by `dy`, clamped. True if it moved.
    pub fn scroll_by(&mut self, side: Side, dy: f32) -> bool {
        let (content, view) = match side {
            Side::Files => (self.files_content, self.files_view),
            Side::Outline => (self.outline_content, self.outline_view),
        };
        let s = self.scroll_mut(side);
        let next = (*s + dy).clamp(0.0, (content - view).max(0.0)).round();
        let moved = next != *s;
        *s = next;
        moved
    }
}

pub struct Ui {
    /// The command palette, while open. Built when it opens, dropped when it closes.
    pub palette: Option<crate::palette::PaletteState>,
    /// The settings dialog, while open.
    pub settings: Option<mdshared_ui::settings::pages::Form>,
    /// The page the settings dialog last showed this run, where it opens again.
    pub settings_page: usize,
    /// Raised by every event the palette or the settings dialog handles, so the overlay signature
    /// changes and the frame is drawn again (neither says what changed inside).
    pub overlay_tick: u64,
    pub panes: Panes,
    pub focus_mode: bool,
    /// Presentation mode, while it runs. See `present`.
    pub present: Option<crate::present::Show>,
    /// A large diagram opened at a reading size, while it shows. See `zoom`.
    pub zoom: Option<crate::zoom::ZoomView>,
    pub tree: FileTree,
    /// The Notes face of the Files pane and the notes library behind it.
    pub notes: crate::notes::Notes,
    pub find: TextField,
    pub find_focused: bool,
    /// The Outline pane's focused row while the keyboard is in that pane: the headings first,
    /// then the backlinks, as one list.
    pub outline_focus: Option<usize>,
    pub caret_on: bool,
    /// When the caret next blinks.
    pub caret_due: Option<Instant>,
    /// Logical pointer position.
    pub mouse: (f32, f32),
    pub hover: Option<Hit>,
    pub pressed: Option<Hit>,
    pub drag: Option<Drag>,
    pub popup: Option<Popup>,
    pub dialog: Option<Dialog>,
    /// The tooltip waiting to show or showing, by the shared rules: the delay counted from the
    /// pointer coming to rest (`hover_at`), a pressed control showing none until left.
    pub tips: mdshared_ui::tooltip::Tooltip<Hit>,
    /// The link under the pointer on the page.
    pub link_hint: Option<String>,
    /// The card for a footnote or link the pointer rests on, and what files showed before.
    pub card: Option<crate::preview::Card>,
    pub previews: crate::preview::Cache,
    /// Hover row in the Manage recent dialog.
    pub dlg_hover_row: Option<usize>,
    /// The last left press, for double and triple clicks: when, where, and the count.
    pub last_press: Option<(Instant, (f32, f32), u32)>,
    /// When the next auto-scroll step of a selection drag is due.
    pub autoscroll_due: Option<Instant>,
    /// The code block whose copy button shows a check mark (by the start of its code), and until
    /// when.
    pub copied: Option<(u32, Instant)>,
}

impl Ui {
    pub fn new() -> Ui {
        Ui {
            palette: None,
            settings: None,
            settings_page: 0,
            overlay_tick: 0,
            panes: Panes {
                files: true,
                outline: true,
                files_w: 250.0,
                outline_w: 230.0,
                files_scroll: 0.0,
                outline_scroll: 0.0,
                files_content: 0.0,
                outline_content: 0.0,
                files_view: 0.0,
                outline_view: 0.0,
            },
            focus_mode: false,
            present: None,
            zoom: None,
            tree: new_tree(),
            notes: crate::notes::Notes::new(None),
            find: TextField::new(),
            find_focused: false,
            outline_focus: None,
            caret_on: true,
            caret_due: None,
            mouse: (-1.0, -1.0),
            hover: None,
            pressed: None,
            drag: None,
            popup: None,
            dialog: None,
            tips: mdshared_ui::tooltip::Tooltip::new().with_delay(crate::motion::TIP_DELAY),
            link_hint: None,
            card: None,
            previews: Default::default(),
            dlg_hover_row: None,
            last_press: None,
            autoscroll_due: None,
            copied: None,
        }
    }

    /// The palette or the settings dialog is open: it takes every key, pointer and wheel event.
    pub fn modal(&self) -> bool {
        self.palette.is_some() || self.settings.is_some()
    }

    pub fn popup_kind(&self) -> Option<PopupKind> {
        self.popup.as_ref().map(|p| p.kind)
    }
}

/// Where everything goes this frame, in logical px.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Areas {
    pub w: f32,
    pub h: f32,
    pub title: Rect,
    pub bar: Option<Rect>,
    pub files: Option<Rect>,
    pub outline: Option<Rect>,
    /// The page region, banner included.
    pub page: Rect,
}

impl Areas {
    pub fn region(&self, r: Region) -> Option<Rect> {
        match r {
            Region::Title => Some(self.title),
            Region::Bar => self.bar,
            Region::Files => self.files,
            Region::Outline => self.outline,
            Region::Page => Some(self.page),
        }
    }

    /// Splits a `w` x `h` window. Panes shrink (then hide) so the page keeps `PAGE_MIN`.
    pub fn compute(w: f32, h: f32, ui: &Ui) -> Areas {
        let title = Rect::new(0.0, 0.0, w, TITLE_H);
        let mut top = TITLE_H;
        let mut bar = None;
        let (mut files_on, mut outline_on) = (ui.panes.files, ui.panes.outline);
        if !ui.focus_mode {
            bar = Some(Rect::new(0.0, top, w, BAR_H));
            top += BAR_H;
        } else {
            files_on = false;
            outline_on = false;
        }
        let body_h = (h - top).max(0.0);
        let mut fw = if files_on { ui.panes.files_w } else { 0.0 };
        let mut ow = if outline_on { ui.panes.outline_w } else { 0.0 };
        // Give way: outline first, then files, down to their minimum, then hide them.
        let over = |fw: f32, ow: f32| fw + ow + PAGE_MIN - w;
        if over(fw, ow) > 0.0 && ow > 0.0 {
            ow = (ow - over(fw, ow)).max(PANE_MIN);
        }
        if over(fw, ow) > 0.0 && fw > 0.0 {
            fw = (fw - over(fw, ow)).max(PANE_MIN);
        }
        if over(fw, ow) > 0.0 && ow > 0.0 {
            ow = 0.0;
        }
        if over(fw, ow) > 0.0 && fw > 0.0 {
            fw = 0.0;
        }
        let files = (fw > 0.0).then(|| Rect::new(0.0, top, fw, body_h));
        let outline = (ow > 0.0).then(|| Rect::new(w - ow, top, ow, body_h));
        let page = Rect::new(fw, top, (w - fw - ow).max(0.0), body_h);
        Areas {
            w,
            h,
            title,
            bar,
            files,
            outline,
            page,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_give_way_to_the_page() {
        let ui = Ui::new();
        let f = Areas::compute(1280.0, 800.0, &ui);
        assert_eq!(f.files.unwrap().w, 250.0);
        assert_eq!(f.outline.unwrap().w, 230.0);
        assert_eq!(f.page.x, 250.0);
        assert_eq!(f.page.w, 800.0);
        assert_eq!(f.page.y, TITLE_H + BAR_H);
        let f = Areas::compute(700.0, 600.0, &ui);
        assert_eq!(f.outline.unwrap().w, PANE_MIN);
        assert_eq!(f.files.unwrap().w, 180.0);
        assert!(f.page.w >= PAGE_MIN);
        let f = Areas::compute(560.0, 600.0, &ui);
        assert!(f.outline.is_none());
        assert!(f.page.w >= PAGE_MIN);
        let mut ui = Ui::new();
        ui.focus_mode = true;
        let f = Areas::compute(1280.0, 800.0, &ui);
        assert!(f.bar.is_none() && f.files.is_none() && f.outline.is_none());
        assert_eq!(f.page, Rect::new(0.0, TITLE_H, 1280.0, 760.0));
    }
}
