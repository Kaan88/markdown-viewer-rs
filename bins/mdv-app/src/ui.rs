//! The window's own state, apart from the documents (which live in `Workspace`): what the pointer
//! is over, what is being dragged, which popup or dialog is open, the panes and the find box.
//! Drawing code reads it; the event loop in `app.rs` changes it.

use crate::gfx::Rect;
use crate::menu::Menu;
use crate::textfield::TextField;
use crate::tree::{FileTree, new_tree};
use mdview_core::commands::CommandId;
use mdview_core::session::PageWidth;
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
    OutlineRow(usize),
    /// An entry of the Backlinks list under the outline.
    BacklinkRow(usize),
    Splitter(Side),
    ScrollBar(Scroller),
    Page,
    EmptyOpen,
    EmptyRecent(usize),
    /// The bar over a page that holds remote images back.
    RemoteLoad,
    RemoteAlways,
    RemoteClose,
    /// The bar that offers a newer version.
    UpdateGo,
    UpdateClose,
    DlgButton(usize),
    DlgOpen(usize),
    DlgRemove(usize),
    DlgCard,
    Backdrop,
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
            Hit::FilesRow(_) | Hit::FilesFolder | Hit::FilesOpenFolder => Region::Files,
            Hit::Splitter(Side::Files) | Hit::ScrollBar(Scroller::Pane(Side::Files)) => {
                Region::Files
            }
            Hit::OutlineRow(_) | Hit::BacklinkRow(_) => Region::Outline,
            Hit::Splitter(Side::Outline) | Hit::ScrollBar(Scroller::Pane(Side::Outline)) => {
                Region::Outline
            }
            Hit::ScrollBar(Scroller::Page)
            | Hit::Page
            | Hit::EmptyOpen
            | Hit::EmptyRecent(_)
            | Hit::RemoteLoad
            | Hit::RemoteAlways
            | Hit::RemoteClose
            | Hit::UpdateGo
            | Hit::UpdateClose => Region::Page,
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
    /// Replace the open menu by the Recent menu.
    RecentMenu,
    OpenLink(String),
    CopyLink(String),
    /// Copy the TeX or Mermaid text a formula or diagram was drawn from.
    CopySource(SourceKind, String),
    Copy,
    SelectAll,
    /// Toggle loading remote images in every document.
    RemoteAlways,
    /// Choose how wide the page's column of text may grow.
    PageWidth(PageWidth),
    /// An editing command for the find box.
    Edit(EditAct),
    /// Toggle ticking task boxes in every file without asking.
    EditAlways,
    /// Toggle playing animated images.
    Animations,
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
}

pub struct Popup {
    pub kind: PopupKind,
    pub menu: Menu<Act>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DialogKind {
    About,
    Shortcuts,
    Recent,
    /// Asks before the first task box is ticked in a file.
    AllowEdit,
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
}

/// A tooltip waiting to show, or showing.
#[derive(Clone, Debug, PartialEq)]
pub struct Tip {
    pub hit: Hit,
    pub text: String,
    pub anchor: Rect,
    pub due: Instant,
    pub shown: bool,
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
    pub panes: Panes,
    pub focus_mode: bool,
    pub tree: FileTree,
    pub find: TextField,
    pub find_focused: bool,
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
    pub tip: Option<Tip>,
    /// A tooltip was dismissed by a press; it stays hidden until the pointer leaves that hit.
    pub tip_blocked: Option<Hit>,
    /// The link under the pointer on the page.
    pub link_hint: Option<String>,
    /// Hover row in the Manage recent dialog.
    pub dlg_hover_row: Option<usize>,
    /// The last left press, for double and triple clicks: when, where, and the count.
    pub last_press: Option<(Instant, (f32, f32), u32)>,
    /// When the next auto-scroll step of a selection drag is due.
    pub autoscroll_due: Option<Instant>,
}

impl Ui {
    pub fn new() -> Ui {
        Ui {
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
            tree: new_tree(),
            find: TextField::new(),
            find_focused: false,
            caret_on: true,
            caret_due: None,
            mouse: (-1.0, -1.0),
            hover: None,
            pressed: None,
            drag: None,
            popup: None,
            dialog: None,
            tip: None,
            tip_blocked: None,
            link_hint: None,
            dlg_hover_row: None,
            last_press: None,
            autoscroll_due: None,
        }
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
