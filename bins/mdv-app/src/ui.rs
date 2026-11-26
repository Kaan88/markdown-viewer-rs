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
use std::time::{Duration, Instant};

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

pub struct Popup {
    pub kind: PopupKind,
    pub menu: Menu<Act>,
    /// The submenu opened beside one of this menu's items; the menu stays visible under it.
    pub sub: Option<Menu<Act>>,
    /// The submenu item the pointer rests on and when its submenu opens by itself.
    pub dwell: Option<(usize, Instant)>,
    /// The shared `submenu::Cascade` this popup's menu is being moved onto (the main menu's
    /// Open recent beside it). While the old `sub`, `dwell` and `SUBMENU_DWELL` still drive the
    /// window, it is built alongside them.
    pub cascade: Option<Cascade<Act>>,
    /// The marked letters of the items are underlined: the menu was opened with Alt held, or Alt
    /// was pressed since (the Windows convention; a menu opened with the pointer shows none).
    pub access: bool,
    /// This opening of the menu, and of its submenu: what their fade-in is known by
    /// (`crate::motion`), so a menu opened again fades in again.
    pub serial: u64,
    pub sub_serial: u64,
}

/// A number no popup had before: each opening of a menu is a new one.
fn next_serial() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// How long the pointer rests on an item that leads to a submenu before it opens, as in Windows.
pub const SUBMENU_DWELL: Duration = Duration::from_millis(300);

impl Popup {
    pub fn new(kind: PopupKind, mut menu: Menu<Act>) -> Popup {
        menu.set_access_visible(false);
        Popup {
            kind,
            menu,
            sub: None,
            dwell: None,
            cascade: None,
            access: false,
            serial: next_serial(),
            sub_serial: 0,
        }
    }

    /// A popup whose menu is a `submenu::Cascade`, with the menu the cascade's root is made
    /// from until every call site reads the cascade.
    pub fn main(kind: PopupKind, mut menu: Menu<Act>, mut cascade: Cascade<Act>) -> Popup {
        menu.set_access_visible(false);
        cascade.set_access_visible(false);
        Popup {
            kind,
            menu,
            sub: None,
            dwell: None,
            cascade: Some(cascade),
            access: false,
            serial: next_serial(),
            sub_serial: 0,
        }
    }

    /// Underlines the access keys of the menu and its submenu (a menu that does not read access
    /// keys has none to show).
    pub fn show_access(&mut self) {
        self.access = true;
        self.menu.set_access_visible(true);
        if let Some(s) = self.sub.as_mut() {
            s.set_access_visible(true);
        }
        if let Some(c) = self.cascade.as_mut() {
            c.set_access_visible(true);
        }
    }

    /// Whether the point is on the menu or on its open submenu.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.menu.contains(x, y) || self.sub.as_ref().is_some_and(|s| s.contains(x, y))
    }

    /// The pointer moved to `x`, `y` at `now`. True when what is drawn changed.
    pub fn pointer(&mut self, x: f32, y: f32, now: Instant) -> bool {
        // An open submenu is never closed by the move itself, or the pointer could not reach
        // it: on its way there it crosses the menu's padding, a separator, or the corner of
        // another row. Off the rows it stays; on another row it stays until the pointer has
        // rested there for the dwell.
        let opener = self
            .sub
            .as_ref()
            .and_then(|_| self.menu.items.iter().position(|it| it.id.is_submenu()));
        let before = self.menu.hover;
        let on_sub = self.sub.as_ref().is_some_and(|s| s.contains(x, y));
        let mut changed = false;
        if let Some(s) = self.sub.as_mut()
            && s.pointer(x, y)
        {
            changed = true;
        }
        if on_sub || (self.sub.is_some() && !self.menu.contains(x, y)) {
            // On the submenu, or off both menus: the parent keeps its highlight on the item
            // the submenu came from, and nothing is pending.
            self.dwell = None;
            self.menu.hover = opener;
        } else {
            self.menu.pointer(x, y);
            match self.menu.hover {
                None => {
                    self.dwell = None;
                    if opener.is_some() {
                        self.menu.hover = opener;
                    }
                }
                Some(i) => {
                    // Something happens after a rest here only if it would change what is
                    // open: a submenu item opens its submenu, any other item closes it.
                    if self.menu.items[i].id.is_submenu() == self.sub.is_some() {
                        self.dwell = None;
                    } else if self.dwell.map(|d| d.0) != Some(i) {
                        self.dwell = Some((i, now + SUBMENU_DWELL));
                    }
                }
            }
        }
        changed || self.menu.hover != before
    }

    /// A click: the item under the point, when it is one. `Outcome::Outside` when the click
    /// was not on the menu.
    pub fn click(&mut self, x: f32, y: f32) -> Outcome<Act> {
        if !self.contains(x, y) {
            return Outcome::Outside;
        }
        let on_sub = self.sub.as_ref().is_some_and(|s| s.contains(x, y));
        let act = if on_sub {
            self.sub.as_ref().and_then(|s| s.chosen())
        } else {
            self.menu.chosen()
        };
        act.map_or(Outcome::Nothing, Outcome::Chosen)
    }

    /// The wheel turned by `dy` over the menus: scrolls the menu under the point, if it is
    /// taller than its room. True when a menu scrolled.
    pub fn wheel(&mut self, x: f32, y: f32, dy: f32) -> bool {
        let over_sub = self.sub.as_ref().is_some_and(|s| s.contains(x, y));
        let menu = match self.sub.as_mut() {
            Some(s) if over_sub => Some(s),
            _ => Some(&mut self.menu).filter(|m| m.contains(x, y)),
        };
        if let Some(m) = menu.filter(|m| m.scrolls())
            && m.scroll_by(dy)
        {
            m.pointer(x, y);
            return true;
        }
        false
    }

    /// A key, for the submenu if one is open and the menu otherwise. `Chosen` closes the popup
    /// after acting; `Close` closes it; anything else changed only what is drawn.
    pub fn key(&mut self, name: &KeyName, mods: Mods) -> Keyed {
        self.dwell = None;
        // With a submenu open the keys go to it; Escape and Left close only the submenu.
        if let Some(s) = self.sub.as_mut() {
            match name {
                KeyName::Up => s.step(false),
                KeyName::Down | KeyName::Tab => s.step(true),
                KeyName::Enter | KeyName::Space => {
                    if let Some(act) = s.chosen() {
                        return Keyed::Chosen(act);
                    }
                }
                KeyName::Escape | KeyName::Left => self.sub = None,
                _ => {}
            }
            return Keyed::Nothing;
        }
        match name {
            KeyName::Up => self.menu.step(false),
            KeyName::Down | KeyName::Tab => self.menu.step(true),
            KeyName::Enter | KeyName::Space | KeyName::Right => {
                let arrow = *name == KeyName::Right;
                if let Some(act) = self.menu.chosen()
                    && (!arrow || act.is_submenu())
                {
                    return Keyed::Chosen(act);
                }
            }
            KeyName::Escape => return Keyed::Close,
            // A letter picks the item marked with it, as Enter would (menus that read no
            // access keys answer None and the key does nothing, as before).
            KeyName::Char(ch) if !mods.ctrl && !mods.alt => {
                if let Some(act) = self.menu.access_key(*ch) {
                    return Keyed::Chosen(act);
                }
            }
            _ => {}
        }
        Keyed::Nothing
    }

    /// When the pointer's rest on a row is up: when its submenu opens, or the open one closes.
    pub fn deadline(&self) -> Option<Instant> {
        self.dwell.map(|d| d.1)
    }

    /// The pointer's rest is up at `now`: the highlighted row's submenu is to be opened (true;
    /// the caller builds it from the recent files) or the open one closes (false).
    pub fn tick(&mut self, now: Instant) -> bool {
        let Some((_, due)) = self.dwell else {
            return false;
        };
        if now < due {
            return false;
        }
        let i = self.dwell.take().map(|d| d.0);
        let opens = i.is_some_and(|i| self.menu.items.get(i).is_some_and(|it| it.id.is_submenu()));
        if opens {
            self.menu.hover = i;
        } else {
            self.sub = None;
        }
        opens
    }

    /// Highlights the enabled row that opens a submenu, if the menu has one: the Recent button
    /// asked by name while the main menu is open.
    pub fn hover_submenu_row(&mut self) {
        if let Some(i) = self
            .menu
            .items
            .iter()
            .position(|it| it.id.is_submenu() && it.enabled)
        {
            self.menu.hover = Some(i);
        }
    }

    /// Puts `menu` beside the highlighted row. A keyboard opening highlights its first row. A
    /// submenu that opens fades in; one put in the place of the one showing does not.
    pub fn set_submenu(&mut self, mut menu: Menu<Act>, keyboard: bool) {
        if keyboard {
            menu.step(true);
        }
        if self.sub.is_none() {
            self.sub_serial += 1;
        }
        self.sub = Some(menu);
    }

    /// Places the menu and its open submenu inside `area`.
    pub fn place_in(&mut self, p: &mut Painter, area: Rect) {
        self.menu.place_in(p, area);
        if let Some(s) = self.sub.as_mut() {
            s.place_in(p, area);
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
        let serial = self.serial;
        let sub_serial = self.sub_serial;
        let (tm, ts) = t;
        self.menu
            .draw_entering_with(p, area, tm, Entrance::Drop, |p, m| {
                m.draw_in_animated(p, pal, area, anim, ("menu-hover", serial));
            });
        if let Some(s) = self.sub.as_mut() {
            let leftward = s.rect().x < self.menu.rect().x;
            s.draw_entering_with(p, area, ts, Entrance::Side { leftward }, |p, m| {
                m.draw_in_animated(p, pal, area, anim, ("submenu-hover", sub_serial));
            });
        }
    }

    /// The presence of the menu and of its open submenu, asked on every frame they are open
    /// (`crate::motion`): a menu opened again fades in again.
    pub fn presence(&self, anim: &mut Animator) -> (f32, f32) {
        (
            crate::motion::menu(anim, self.serial),
            match self.sub.is_some() {
                true => crate::motion::submenu(anim, self.serial, self.sub_serial),
                false => 1.0,
            },
        )
    }

    /// Everything the popup draws at any presence, its shadow and its drop included.
    pub fn presence_bounds(&self) -> Rect {
        let mut r = self.menu.presence_bounds();
        if let Some(s) = &self.sub {
            let b = s.presence_bounds();
            let (x, y) = (r.x.min(b.x), r.y.min(b.y));
            r = Rect::new(
                x,
                y,
                r.right().max(b.right()) - x,
                r.bottom().max(b.bottom()) - y,
            );
        }
        r
    }

    /// The rows, scrolls and item counts of the menus, and the path of open submenus: what
    /// changes what is drawn for the overlay signature.
    pub fn state(&self) -> PopupState {
        let mut rows = vec![(
            self.menu.hover,
            self.menu.scroll().to_bits(),
            self.menu.items.len(),
        )];
        if let Some(s) = &self.sub {
            rows.push((s.hover, s.scroll().to_bits(), s.items.len()));
        }
        (rows, Vec::new())
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
