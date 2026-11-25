//! What clicks, menu items and shortcuts do: activation, the command table's commands, popups and
//! dialogs, plus wakes from other threads and the loop's timers.

use super::*;
use crate::ui::EditAct;
use crate::workspace::{SourcePlace, resolve};

use mdview_core::session::PageWidth;
use mdview_render::SourceKind;

impl App {
    // ---- activation and commands ---------------------------------------------------------------

    pub(super) fn activate(&mut self, hit: Hit) {
        match hit {
            Hit::WinMin => {
                if let Some(w) = &self.window {
                    w.set_minimized(true);
                }
            }
            Hit::WinMax => self.toggle_maximized(),
            Hit::WinClose => self.exit = true,
            Hit::NewTab => self.command(C::OpenFiles),
            Hit::CopyCode => self.copy_code_under_pointer(),
            Hit::LeaveFocus => self.command(C::FocusMode),
            // A pinned tab shows a pin there: it unpins.
            Hit::TabClose(i) if self.ws.is_pinned(i) => {
                self.ws.set_pinned(i, false);
            }
            Hit::TabClose(i) => {
                self.ws.close(i);
            }
            Hit::Bar(item) => match item {
                BarItem::Open => self.command(C::OpenFiles),
                BarItem::Reload => self.command(C::Refresh),
                BarItem::Recent => self.command(C::OpenRecent),
                BarItem::Files => self.command(C::ToggleFiles),
                BarItem::Outline => self.command(C::ToggleOutline),
                BarItem::ZoomOut => self.command(C::ZoomOut),
                BarItem::Zoom => self.command(C::ZoomReset),
                BarItem::ZoomIn => self.command(C::ZoomIn),
                BarItem::PageWidth => {
                    let next = match self.ws.page_width() {
                        PageWidth::Narrow => PageWidth::Wide,
                        PageWidth::Wide => PageWidth::Full,
                        PageWidth::Full => PageWidth::Narrow,
                    };
                    self.ws.set_page_width(next);
                }
                BarItem::FindPrev => self.command(C::FindPrevious),
                BarItem::FindNext => self.command(C::FindNext),
                BarItem::Theme => self.command(C::Theme),
                BarItem::More => self.open_popup(PopupKind::Main),
            },
            Hit::FindClear => {
                self.ui.find.clear();
                self.ws.find_clear();
                self.ui.find_focused = true;
                self.caret_reset();
            }
            Hit::FilesRow(i) => {
                let Some(row) = self.ui.tree.rows().get(i).cloned() else {
                    return;
                };
                if row.is_dir {
                    self.ui.tree.toggle(i);
                } else {
                    let _ = self.ws.open(&row.path);
                }
            }
            Hit::FilesFolder | Hit::FilesOpenFolder => self.command(C::OpenFolder),
            Hit::PaneFace(face) => self.pane_face(face),
            Hit::NotesRow(i) => self.notes_row(i),
            Hit::NotesRefresh => self.notes_refresh(),
            Hit::OutlineRow(i) => {
                self.ui.outline_focus = Some(i);
                self.outline_enter(i);
            }
            Hit::BacklinkRow(i) => {
                let at = self.ws.outline().len() + i;
                self.ui.outline_focus = Some(at);
                self.outline_enter(at);
            }
            Hit::EmptyOpen => self.command(C::OpenFiles),
            Hit::RemoteLoad => self.ws.allow_remote(false),
            Hit::RemoteAlways => self.ws.allow_remote(true),
            Hit::RemoteClose => self.ws.dismiss_remote(),
            Hit::UpdateGo => self.install_update(),
            Hit::UpdateRestart => self.restart_into_update(),
            Hit::UpdateClose => self.ws.dismiss_update(),
            Hit::CrashOpen | Hit::CrashCopy | Hit::CrashClose => self.crash_bar(hit),
            Hit::StaleClose => self.ws.close_stale(),
            Hit::StaleKeep => self.ws.keep_stale(),
            Hit::ZoomClose => self.close_zoom(),
            Hit::StaleDismiss => self.ws.dismiss_stale(),
            Hit::EmptyRecent(i) => {
                if let Some(p) = self.ws.recents().get(i).cloned() {
                    let _ = self.ws.open_recent(&p);
                }
            }
            Hit::DlgButton(n) => match self.ui.dialog.as_ref().map(|d| d.kind) {
                Some(DialogKind::AllowEdit) => {
                    self.close_dialog();
                    self.ws.answer_edit(match n {
                        1 => Some(false),
                        2 => Some(true),
                        _ => None,
                    });
                }
                Some(DialogKind::AddToLibrary) => {
                    self.close_dialog();
                    if n == 1 {
                        self.library_write();
                    } else {
                        self.ui.notes.pending = None;
                    }
                }
                Some(DialogKind::About) if n == 1 => {
                    // The answer shows on the page (the bar or a toast), so the dialog goes.
                    self.close_dialog();
                    if matches!(self.ws.update_stage(), updater::Stage::Available(_)) {
                        self.install_update();
                    } else if self.ws.update_relaunch_target().is_some() {
                        self.restart_into_update();
                    } else {
                        self.check_for_updates(true);
                    }
                }
                Some(DialogKind::About) if n == 2 => {
                    let on = !self.ws.checks_updates();
                    self.ws.set_checks_updates(on);
                }
                _ if n == 0 => self.close_dialog(),
                _ => self.ws.clear_recents(),
            },
            Hit::DlgOpen(i) => {
                if let Some(p) = self.ws.recents().get(i).cloned()
                    && self.ws.open_recent(&p).is_ok()
                {
                    self.close_dialog();
                }
            }
            Hit::DlgRemove(i) => {
                if let Some(p) = self.ws.recents().get(i).cloned() {
                    self.ws.remove_recent(&p);
                }
            }
            _ => {}
        }
        self.redraw();
    }

    pub(super) fn act(&mut self, act: Act) {
        match act {
            Act::Cmd(id) => self.command(id),
            Act::TabCmd(id, i) => self.tab_command(id, i),
            Act::LibraryPut(i) => self.library_put_in(i),
            Act::Recent(path) => {
                let _ = self.ws.open_recent(&path);
            }
            Act::RecentMenu => self.open_popup(PopupKind::Recent),
            Act::OpenLink(dest) => {
                let outcome = self.ws.open_link(&dest);
                self.link_outcome(outcome);
            }
            Act::CopyLink(dest) => {
                // A wiki-link's address is its target as written.
                let dest = dest
                    .strip_prefix(crate::workspace::WIKI_SCHEME)
                    .unwrap_or(&dest);
                if platform::set_clipboard_text(dest) {
                    self.ws.confirm("Link address copied");
                }
            }
            Act::CopySource(kind, text) => {
                if platform::set_clipboard_text(&text) {
                    self.ws.confirm(match kind {
                        SourceKind::Formula => "Formula source copied",
                        SourceKind::Diagram => "Diagram source copied",
                    });
                }
            }
            Act::Copy => self.copy(),
            Act::SelectAll => {
                self.ws.select_all();
            }
            Act::Edit(edit) => {
                let f = &mut self.ui.find;
                let changed = match edit {
                    EditAct::Cut => {
                        f.has_selection()
                            && platform::set_clipboard_text(f.selected_text())
                            && f.delete_selection()
                    }
                    EditAct::Copy => {
                        if f.has_selection() {
                            platform::set_clipboard_text(f.selected_text());
                        }
                        false
                    }
                    EditAct::Paste => match platform::clipboard_text() {
                        // A search is one line: a pasted line break would never match.
                        Some(text) => f.insert(text.lines().next().unwrap_or("")),
                        None => false,
                    },
                    EditAct::SelectAll => {
                        f.select_all();
                        false
                    }
                };
                self.ui.find_focused = true;
                if changed {
                    self.find_changed();
                }
                self.caret_reset();
            }
        }
        self.redraw();
    }

    /// The copy button was clicked: the code of the block under the pointer goes to the clipboard
    /// and the button shows a check mark for a moment.
    pub(super) fn copy_code_under_pointer(&mut self) {
        let vr = page::viewport(&self.ws, self.areas.page);
        let Some(spot) = crate::codebtn::spot(&mut self.ws, &self.ui, vr, self.scale) else {
            return;
        };
        if let Some(text) = self.ws.code_text(spot.start)
            && crate::codebtn::to_clipboard(&text)
        {
            self.ui.copied = Some((spot.start, Instant::now() + crate::codebtn::COPIED_FOR));
        }
    }

    /// "Copy code block": the code at the selection, else of the first code block in view.
    pub(super) fn copy_code_block(&mut self) {
        match self.ws.code_in_reach() {
            Some(text) => {
                if crate::codebtn::to_clipboard(&text) {
                    self.ws.confirm("Code copied");
                }
            }
            None => self.ws.set_status("No code block in view"),
        }
    }

    pub(super) fn copy(&mut self) {
        if let Some(text) = self.ws.selected_text()
            && (self
                .ws
                .selected_html()
                .is_some_and(|html| platform::set_clipboard_html(&html, &text))
                || platform::set_clipboard_text(&text))
        {
            self.ws.confirm("Copied");
        }
    }

    /// "Copy as Markdown": the source of the blocks the selection touches, as plain text.
    pub(super) fn copy_markdown(&mut self) {
        match self.ws.selected_markdown() {
            Ok(text) => {
                if crate::codebtn::to_clipboard(&text) {
                    self.ws.confirm("Copied");
                }
            }
            Err(message) => self.ws.set_status(message),
        }
    }

    pub(super) fn tab_command(&mut self, id: C, index: usize) {
        let Some(path) = self.ws.tab(index).map(|t| t.path().to_path_buf()) else {
            return;
        };
        // A document mdedit has not saved yet: there is no file to name or show, and the
        // editor is asked for it by the name it gave.
        let untitled = (self.ws.tab(index)).and_then(|t| {
            Some((
                t.untitled_doc()?.to_string(),
                self.ws.labels().get(index)?.clone(),
            ))
        });
        match id {
            C::CopyFilePath | C::ShowInFileManager if untitled.is_some() => {
                self.ws
                    .set_status(mdview_core::commands::strings::UNTITLED_NO_FILE);
            }
            C::EditInMdedit if untitled.is_some() => {
                let (doc, title) = untitled.unwrap_or_default();
                let at = self.ws.source_place(index);
                let request = Request {
                    line: at.map(|at| at.line),
                    column: at.and_then(|at| at.column),
                    from: Some("mdview".into()),
                    ..Request::document(doc, title)
                };
                if !mdshared_app::sister::send("mdedit", &request) {
                    self.ws
                        .set_status(mdview_core::commands::strings::EDITOR_NOT_RUNNING);
                }
            }
            C::LibraryAdd => self.library_add(index),
            C::CloseTab => {
                self.ws.close(index);
            }
            C::PinTab => {
                let on = !self.ws.is_pinned(index);
                self.ws.set_pinned(index, on);
            }
            C::CloseOtherTabs => {
                self.ws.close_others(index);
            }
            C::CloseAllTabs => {
                self.ws.close_all();
            }
            C::CopyFilePath => {
                if platform::set_clipboard_text(&path.display().to_string()) {
                    self.ws.confirm("File path copied");
                }
            }
            C::ShowInFileManager => {
                platform::reveal_in_file_manager(&path);
            }
            C::EditInMdedit => {
                let at = self.ws.source_place(index);
                let request = Request {
                    line: at.map(|at| at.line),
                    column: at.and_then(|at| at.column),
                    from: Some("mdview".into()),
                    ..Request::new(path)
                };
                if let Err(message) = mdshared_app::sister::open("mdedit", &request) {
                    self.ws.set_status(&message);
                }
            }
            // The page of that tab: it becomes the active one, as a click on it would make it.
            C::ExportHtml | C::Print => {
                self.ws.activate(index);
                if self.ws.active() == Some(index) {
                    self.command(id);
                }
            }
            other => self.command(other),
        }
    }

    /// Where a popup hangs: below its bar button, or below "⋯" when the button is hidden.
    pub(super) fn popup_anchor(&self, kind: PopupKind) -> (f32, f32, bool) {
        let below =
            |r: Rect, right: bool| (if right { r.right() } else { r.x }, r.bottom() + 4.0, right);
        match kind {
            PopupKind::Recent => {
                if let Some(r) = self.hit_rect(Hit::Bar(BarItem::Recent)) {
                    return below(r, false);
                }
                if let Some(r) = self.hit_rect(Hit::Bar(BarItem::More)) {
                    return below(r, true);
                }
            }
            PopupKind::Main => {
                if let Some(r) = self.hit_rect(Hit::Bar(BarItem::More)) {
                    return below(r, true);
                }
            }
            _ => {}
        }
        let (w, _) = self.logical();
        (w - 8.0, self.areas.page.y + 4.0, true)
    }

    pub(super) fn open_popup(&mut self, kind: PopupKind) {
        if self.ui.popup_kind() == Some(kind) {
            self.ui.popup = None;
            return;
        }
        // From the main menu, Recent opens beside it and the main menu stays.
        if kind == PopupKind::Recent
            && let Some(p) = self.ui.popup.as_mut()
            && p.kind == PopupKind::Main
        {
            // Asked by name, not by the pointer: highlight the item it belongs to.
            if let Some(i) = p
                .menu
                .items
                .iter()
                .position(|it| it.id.is_submenu() && it.enabled)
            {
                p.menu.hover = Some(i);
            }
            self.open_submenu(false);
            return;
        }
        let (x, y, right) = self.popup_anchor(kind);
        let menu = match kind {
            PopupKind::Main => overlays::main_menu(&self.ws, &self.ui, x, y),
            PopupKind::Recent => overlays::recent_menu(&self.ws, x, y, right),
            _ => return,
        };
        let mut popup = Popup::new(kind, menu);
        if self.mods.alt {
            popup.show_access();
        }
        self.ui.popup = Some(popup);
        self.ui.tips.hide();
    }

    /// Opens the submenu of the highlighted item of the open menu beside that item. `keyboard`
    /// also highlights its first row, so the arrow keys carry on inside it.
    pub(super) fn open_submenu(&mut self, keyboard: bool) {
        let (w, _) = self.logical();
        let Some(p) = self.ui.popup.as_mut() else {
            return;
        };
        p.dwell = None;
        let Some(i) = p.menu.hover.filter(|&i| p.menu.items[i].id.is_submenu()) else {
            return;
        };
        let parent = p.menu.rect();
        let row_y = parent.y + overlays::menu_row_offset(&p.menu.items, i) - p.menu.scroll();
        // Beside the item, on the right unless the window has no room there.
        let room = w - parent.right();
        let left = room < 420.0 && parent.x > room;
        let (x, right) = if left {
            (parent.x + 4.0, true)
        } else {
            (parent.right() - 4.0, false)
        };
        let mut menu = overlays::recent_menu(&self.ws, x, row_y - 5.0, right);
        if keyboard {
            menu.step(true);
        }
        // A submenu that opens fades in; one put in the place of the one showing does not.
        if p.sub.is_none() {
            p.sub_serial += 1;
        }
        p.sub = Some(menu);
        self.ui.tips.hide();
    }

    /// What choosing a menu item does: a submenu item opens its submenu and keeps the menus,
    /// anything else closes them and acts.
    pub(super) fn choose(&mut self, act: Act, keyboard: bool) {
        if act.is_submenu() {
            self.open_submenu(keyboard);
        } else {
            self.ui.popup = None;
            self.act(act);
        }
    }

    pub(super) fn open_dialog(&mut self, kind: DialogKind) {
        self.ui.popup = None;
        self.ui.tips.hide();
        self.ui.find_focused = false;
        self.ui.dialog = Some(Dialog::new(kind));
        self.ui.dlg_hover_row = None;
    }

    pub(super) fn close_dialog(&mut self) {
        self.ui.dialog = None;
        self.ui.pressed = None;
        self.ui.dlg_hover_row = None;
        let (x, y) = self.ui.mouse;
        self.ui.hover = None;
        self.wants_frame = true;
        // The regions under the dialog are redrawn next frame; their hits come back then.
        let _ = (x, y);
    }

    /// Opens a rendered diagram at a reading size, over the whole window.
    pub(super) fn open_zoom(&mut self, source: String) {
        let Some(image) = self.ws.diagram_raster(&source) else {
            self.ws.set_status("The diagram could not be drawn again");
            self.redraw();
            return;
        };
        self.show_zoom(image);
    }

    /// The zoom view with a raster already in hand.
    pub(super) fn show_zoom(&mut self, image: Rc<mdview_render::DecodedImage>) {
        let area = self.zoom_area();
        let mut view = crate::zoom::ZoomView::open(image, area, self.scale);
        view.origin = self.ws.active_tab().map(|t| crate::zoom::Origin {
            tab: t.path().to_path_buf(),
            revision: t.revision(),
            dark: self.pal.dark,
        });
        self.ui.zoom = Some(view);
        self.ui.popup = None;
        self.ui.tips.hide();
        self.ui.card = None;
        self.ui.link_hint = None;
        self.ui.hover = None;
        self.ui.pressed = None;
        self.ui.drag = None;
        self.set_cursor(CursorIcon::Grab);
        self.redraw();
    }

    /// Opens the `n`-th (from 1) drawable Mermaid diagram of the showing document: what
    /// `--zoom-diagram` does, for shots and checks. False when there is no such diagram.
    pub(super) fn open_zoom_nth(&mut self, n: usize) -> bool {
        let Some(source) = self.ws.diagram_source(n.saturating_sub(1)) else {
            return false;
        };
        self.open_zoom(source);
        self.ui.zoom.is_some()
    }

    pub(super) fn close_zoom(&mut self) {
        if self.ui.zoom.take().is_none() {
            return;
        }
        self.ui.pressed = None;
        self.ui.drag = None;
        self.ui.hover = None;
        self.overlay_hits.clear();
        self.set_cursor(CursorIcon::Default);
        self.wants_frame = true;
        // The regions under the view are drawn again next frame.
        self.full = true;
        self.redraw();
    }

    pub(super) fn start_dir(&self) -> Option<PathBuf> {
        self.ws
            .active_tab()
            .filter(|t| !t.is_untitled())
            .and_then(|t| t.path().parent().map(Path::to_path_buf))
            .or_else(|| self.ws.folder().map(Path::to_path_buf))
    }

    /// Starts a file dialog on its helper thread through `start`, which is given a proxy for the
    /// answer and says whether the dialog opened. The window keeps drawing meanwhile; a request
    /// made while one is open is ignored. The answer comes back as `Wake::Dialog`.
    #[cfg(not(windows))]
    fn open_dialog_async(&mut self, start: impl FnOnce(LoopProxy) -> bool) {
        if self.dialog_open {
            return;
        }
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        self.dialog_open = start(proxy);
    }

    pub(super) fn set_folder(&mut self, dir: PathBuf) {
        let dir = mdview_core::files::canonical_or_absolute(&dir);
        self.ws.set_folder(Some(dir.clone()));
        self.ui.tree.set_root(Some(dir));
        self.ui.panes.files_scroll = 0.0;
        self.last_active = None;
        if !self.ui.panes.files || self.ui.focus_mode {
            self.ui.focus_mode = false;
            self.ui.panes.files = true;
            self.ws
                .set_panes(self.ui.panes.files, self.ui.panes.outline);
        }
    }

    pub(super) fn command(&mut self, id: C) {
        match id {
            #[cfg(windows)]
            C::OpenFiles => {
                for path in platform::pick_files(self.start_dir().as_deref()) {
                    let _ = self.ws.open(&path);
                }
            }
            #[cfg(not(windows))]
            C::OpenFiles => {
                let start = self.start_dir();
                self.open_dialog_async(|proxy| {
                    platform::pick_files_async(start.as_deref(), move |files| {
                        let _ = proxy.send_event(Wake::Dialog(DialogAnswer::Files(files)));
                    })
                });
            }
            #[cfg(windows)]
            C::OpenFolder => {
                let start = self
                    .ws
                    .folder()
                    .map(Path::to_path_buf)
                    .or_else(|| self.start_dir());
                if let Some(dir) = platform::pick_folder(start.as_deref()) {
                    self.set_folder(dir);
                }
            }
            #[cfg(not(windows))]
            C::OpenFolder => {
                let start = self
                    .ws
                    .folder()
                    .map(Path::to_path_buf)
                    .or_else(|| self.start_dir());
                self.open_dialog_async(|proxy| {
                    platform::pick_folder_async(start.as_deref(), move |dir| {
                        let _ = proxy.send_event(Wake::Dialog(DialogAnswer::Folder(dir)));
                    })
                });
            }
            C::ExportHtml => self.export_html(),
            C::Print => self.print_page(),
            C::MakeDefault => {
                let result = platform::make_mdview_default();
                self.ws.set_status(match result {
                    Ok(()) => mdview_core::commands::strings::DEFAULT_APP_DONE,
                    Err(_) => mdview_core::commands::strings::DEFAULT_APP_FAILED,
                });
            }
            C::Refresh => {
                self.ws.reload();
                self.ws.notes_changed();
                self.ui.tree.refresh();
            }
            C::OpenRecent => self.open_popup(PopupKind::Recent),
            C::ManageRecent => self.open_dialog(DialogKind::Recent),
            C::ClearRecent => self.ws.clear_recents(),
            C::CloseTab => {
                self.ws.close_active();
            }
            C::CloseAllTabs => {
                self.ws.close_all();
            }
            C::Exit => self.exit = true,
            C::Find => {
                self.ui.find_focused = true;
                self.ui.find.select_all();
                self.caret_reset();
            }
            C::FindNext | C::FindPrevious => {
                if self.ws.find_query().is_empty() {
                    self.command(C::Find);
                } else if id == C::FindNext {
                    self.ws.find_next();
                } else {
                    self.ws.find_previous();
                }
            }
            C::ClearSearch => {
                self.ui.find.clear();
                self.ws.find_clear();
            }
            C::Theme => {
                let next = match self.ws.theme_choice() {
                    ThemeChoice::System => ThemeChoice::Light,
                    ThemeChoice::Light => ThemeChoice::Dark,
                    ThemeChoice::Dark => ThemeChoice::System,
                };
                self.ws.set_theme_choice(next);
            }
            C::ThemeSystem => {
                self.ws.set_theme_choice(ThemeChoice::System);
            }
            C::ThemeLight => {
                self.ws.set_theme_choice(ThemeChoice::Light);
            }
            C::ThemeDark => {
                self.ws.set_theme_choice(ThemeChoice::Dark);
            }
            C::ZoomIn => {
                self.ws.zoom_in();
            }
            C::ZoomOut => {
                self.ws.zoom_out();
            }
            C::ZoomReset => {
                self.ws.zoom_reset();
            }
            C::ToggleFiles | C::ToggleOutline => {
                if id == C::ToggleOutline {
                    self.ui.outline_focus = None;
                }
                let p = &mut self.ui.panes;
                if self.ui.focus_mode {
                    self.ui.focus_mode = false;
                    p.files = id == C::ToggleFiles || p.files;
                    p.outline = id == C::ToggleOutline || p.outline;
                } else if id == C::ToggleFiles {
                    p.files = !p.files;
                } else {
                    p.outline = !p.outline;
                }
                let (f, o) = (p.files, p.outline);
                self.ws.set_panes(f, o);
                if f {
                    // A closed pane is not watched, so its listing may be old.
                    self.ui.tree.refresh();
                }
            }
            C::FocusOutline => self.focus_outline(),
            C::FocusMode => {
                self.ui.focus_mode = !self.ui.focus_mode;
                self.ui.popup = None;
                if self.ui.focus_mode {
                    self.ui.find_focused = false;
                    self.ws.set_status("Focus mode: press Escape to leave");
                }
            }
            C::FindInFolder => self.open_folder_search(),
            C::Present => self.present_toggle(),
            C::CommandPalette => {
                if self.ui.palette.is_some() {
                    self.close_palette();
                } else {
                    self.open_palette("");
                }
            }
            C::Settings => self.open_settings(),
            C::ShowShortcuts => self.open_dialog(DialogKind::Shortcuts),
            C::About => self.open_dialog(DialogKind::About),
            C::NextTab => {
                self.ws.next_tab();
            }
            C::PreviousTab => {
                self.ws.previous_tab();
            }
            C::Back => {
                self.ws.go_back();
            }
            C::Forward => {
                self.ws.go_forward();
            }
            C::Escape => self.escape(),
            C::CopyCodeBlock => self.copy_code_block(),
            C::CopyMarkdown => self.copy_markdown(),
            C::CloseOtherTabs
            | C::CopyFilePath
            | C::ShowInFileManager
            | C::EditInMdedit
            | C::LibraryAdd
            | C::PinTab => {
                if let Some(i) = self.ws.active() {
                    self.tab_command(id, i);
                }
            }
        }
        self.redraw();
    }

    /// Puts the keyboard in the Outline pane: opens the pane if it is closed and focuses the
    /// current heading (the first row when there is none).
    pub(super) fn focus_outline(&mut self) {
        if self.ui.focus_mode || !self.ui.panes.outline {
            self.command(C::ToggleOutline);
        }
        self.ui.find_focused = false;
        let total = self.outline_rows();
        if total == 0 {
            self.ui.outline_focus = None;
            self.ws.set_status("The outline is empty");
            return;
        }
        let at = self.ws.current_heading().unwrap_or(0).min(total - 1);
        self.ui.outline_focus = Some(at);
        self.reveal_focused_outline();
    }

    /// How many rows the outline pane has: the headings, then the backlinks.
    pub(super) fn outline_rows(&self) -> usize {
        self.ws.outline().len() + self.ws.backlinks().len()
    }

    pub(super) fn reveal_focused_outline(&mut self) {
        if let Some(i) = self.ui.outline_focus {
            let headings = self.ws.outline().len();
            panes::reveal_outline_item(&mut self.ui, i, headings);
        }
    }

    /// What Enter (or a click) on outline row `i` does: a heading is gone to, a backlink opened.
    /// Opening a backlink changes the document, so the keyboard goes back to the page.
    pub(super) fn outline_enter(&mut self, i: usize) {
        let headings = self.ws.outline().len();
        if let Some(block) = self.ws.outline().get(i).map(|e| e.block) {
            self.ws.outline_jump(block);
        } else {
            self.ws.open_backlink(i - headings);
            self.ui.outline_focus = None;
        }
    }

    /// Keys for the focused Outline pane. True when the key was used.
    pub(super) fn outline_key(&mut self, name: &KeyName, mods: Mods) -> bool {
        let Some(at) = self.ui.outline_focus else {
            return false;
        };
        let total = self.outline_rows();
        if !self.ui.panes.outline || total == 0 {
            self.ui.outline_focus = None;
            return false;
        }
        if mods.ctrl || mods.alt {
            return false;
        }
        let at = at.min(total - 1);
        let page = ((self.ui.panes.outline_view / panes::ROW) as usize)
            .saturating_sub(1)
            .max(1);
        let next = match name {
            KeyName::Up => at.saturating_sub(1),
            KeyName::Down => (at + 1).min(total - 1),
            KeyName::Home => 0,
            KeyName::End => total - 1,
            KeyName::PageUp => at.saturating_sub(page),
            KeyName::PageDown => (at + page).min(total - 1),
            KeyName::Enter => {
                self.ui.outline_focus = Some(at);
                self.outline_enter(at);
                return true;
            }
            KeyName::Escape => {
                self.ui.outline_focus = None;
                return true;
            }
            _ => return false,
        };
        self.ui.outline_focus = Some(next);
        self.reveal_focused_outline();
        true
    }

    /// Escape peels one layer: a menu, a dialog, the find box, focus mode, the find, the selection.
    pub(super) fn escape(&mut self) {
        if self.ui.popup.take().is_some() {
            return;
        }
        if self.ui.dialog.is_some() {
            self.close_dialog();
            return;
        }
        if self.ui.find_focused {
            self.ui.find_focused = false;
            self.ui.find.clear();
            self.ws.find_clear();
            return;
        }
        if self.ui.focus_mode {
            self.ui.focus_mode = false;
            return;
        }
        if !self.ws.find_query().is_empty() {
            self.ui.find.clear();
            self.ws.find_clear();
            return;
        }
        self.ws.clear_selection();
    }

    // ---- hand-off from the editor and later launches ------------------------------------------

    /// What other launches and the editor asked for since the last frame. A file is opened and
    /// shown at its line. A live update (`follow`) only scrolls the file that is showing, and the
    /// window is neither raised nor focused for it; of several waiting, the last counts, so one
    /// frame moves the page once. Anything else raises the window, as a second launch always did.
    pub(super) fn handed_over(&mut self, requests: Vec<Request>) {
        let mut follow: Option<&Request> = None;
        for request in &requests {
            if request.unplace {
                // The editor gave up the side-by-side arrangement, or its preview ended: the
                // window goes back to where it was. The editor is not told; it knows.
                self.end_side_by_side(false);
                continue;
            }
            if let Some(doc) = request.doc.as_deref() {
                // A document mdedit has not saved: by its name, not by a path. Saved, it
                // comes with its file, and is then followed like any file.
                if request.path.as_os_str().is_empty() {
                    self.handed_document(doc, request);
                    continue;
                }
                self.ws.document_saved(doc, &request.path);
                // A document the editor asked for is its file's tab now. The file is resolved
                // here, once, for the checks each turn of the loop makes.
                let file = resolve(&request.path);
                for t in &mut self.editor_shown {
                    if t.doc.as_deref() == Some(doc) {
                        (t.path, t.doc, t.file) = (request.path.clone(), None, file.clone());
                    }
                }
                // The window placed for the document stays placed for its file.
                if let Some((path, named)) = &mut self.placed_for
                    && named.as_deref() == Some(doc)
                {
                    (*path, *named) = (file, None);
                }
            }
            if request.live_off {
                self.ws.live_off(&request.path);
            } else if let Some(text) = &request.text {
                // Of several texts that arrive together the last is the one shown.
                (self.ws).live_text(&request.path, text.clone(), SourcePlace::of(request));
            } else if request.follow {
                follow = Some(request);
            } else {
                let opened = self.ws.open(&request.path).is_ok();
                // Remembered even when it could not be opened: no tab shows it, so the editor is
                // told at once that its preview has none (audit 4, E11).
                if request.from.as_deref() == Some(EDITOR) {
                    self.editor_asked(request.path.clone(), None);
                }
                if opened && let Some(at) = SourcePlace::of(request) {
                    self.ws.scroll_to_line(at);
                }
            }
        }
        if let Some(request) = follow
            && let Some(at) = SourcePlace::of(request)
        {
            self.ws.follow(&request.path, at);
        }
        if let Some(request) = requests.iter().rev().find(|r| r.place.is_some())
            && let Some(rect) = request.place
        {
            self.place(rect, (request.path.clone(), request.doc.clone()));
        }
        let only_following = !requests.is_empty() && requests.iter().all(|r| r.follow);
        // A `--behind` window is never brought forward by a hand-over: no focus, no unminimize.
        if !only_following && self.args.presence().focus {
            #[cfg(test)]
            {
                self.hand_overs_focused += 1;
            }
            if let Some(w) = &self.window {
                w.set_minimized(false);
                w.focus_window();
            }
        }
    }

    /// What the editor asked for the document `doc`, which has no file: to show it (`open`),
    /// its text as it is now, the caret's line, or that its text is over.
    fn handed_document(&mut self, doc: &str, request: &Request) {
        if request.live_off {
            self.ws.document_off(doc);
            return;
        }
        let title = request.title.as_deref();
        let text = request.text.clone();
        (self.ws).document_text(doc, title, text, SourcePlace::of(request), request.open);
        if request.open && self.ws.shows(Path::new(""), Some(doc)) {
            self.editor_asked(PathBuf::new(), Some(doc.to_string()));
        }
    }

    /// A launch of the editor's "Preview in mdview" (`--from mdedit`) started this window: its
    /// file is one the editor asked for.
    pub(super) fn started_by_editor(&mut self) {
        if self.args.from.as_deref() == Some(EDITOR)
            && let Some(file) = self.args.files.first()
        {
            self.editor_asked(file.clone(), None);
        }
    }

    /// The editor asked this window to show the file `path`, or its document without a file
    /// `doc`: when no tab shows it any more, the editor is told ([`editor_closed`]).
    ///
    /// [`editor_closed`]: Self::editor_closed
    pub(super) fn editor_asked(&mut self, path: PathBuf, doc: Option<String>) {
        if (self.editor_shown.iter()).any(|t| t.path == path && t.doc == doc) {
            return;
        }
        // Resolved once, here, not on each turn of the loop that looks for its tab.
        let file = if path.as_os_str().is_empty() {
            PathBuf::new()
        } else {
            resolve(&path)
        };
        self.editor_shown.push(EditorTab { path, doc, file });
    }

    /// Tells the editor of the tabs it asked for that are closed, off this thread (a send waits
    /// for the answer). Nothing runs while none is closed.
    pub(super) fn tell_editor_closed(&mut self) {
        let closed = self.editor_closed();
        if !closed.is_empty() {
            std::thread::spawn(move || {
                for request in &closed {
                    mdshared_app::sister::send(EDITOR, request);
                }
            });
        }
    }

    /// What to tell the editor about the tabs it asked for that no tab shows any more (closed,
    /// in whatever way): one `closed` each. They are forgotten here; nothing is asked while
    /// the editor has asked for none. Called on every turn of the loop, so it compares the paths
    /// resolved when the editor asked and asks the disk nothing (audit 4, E1).
    pub(super) fn editor_closed(&mut self) -> Vec<Request> {
        if self.editor_shown.is_empty() {
            return Vec::new();
        }
        let ws = &self.ws;
        let mut closed = Vec::new();
        self.editor_shown.retain(|t| {
            let shown = ws.shows_resolved(&t.file, t.doc.as_deref());
            if !shown {
                closed.push(closed_request(&t.path, t.doc.clone()));
            }
            shown
        });
        closed
    }

    /// The window closes: the editor is told that every tab it asked for is closed, and this
    /// waits for that at most `wait`, as the process ends after it. Nothing is sent, and nothing
    /// waited for, while the editor asked for none. Whether everything was told in time.
    pub(super) fn tell_editor_all_closed(&mut self, wait: std::time::Duration) -> bool {
        let closed: Vec<Request> = (self.editor_shown.drain(..))
            .map(|t| closed_request(&t.path, t.doc))
            .collect();
        if closed.is_empty() {
            return true;
        }
        let (done, told) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for request in &closed {
                mdshared_app::sister::send(EDITOR, request);
            }
            _ = done.send(());
        });
        told.recv_timeout(wait).is_ok()
    }

    /// Whether no tab shows what the window was placed for any more. Asked on every turn of the
    /// loop while the window is placed, so it compares the path resolved when it was placed and
    /// asks the disk nothing (audit 4, E1).
    pub(super) fn placed_tab_gone(&self) -> bool {
        self.placed_for.as_ref().is_some_and(|(path, doc)| {
            !(path.as_os_str().is_empty() && doc.is_none())
                && !self.ws.shows_resolved(path, doc.as_deref())
        })
    }

    /// Moves and resizes the window to `rect` (physical screen px), or says it cannot be done.
    /// Where it was is remembered, to go back to when the tab it was placed `for_tab` closes or
    /// the editor says the arrangement is over.
    pub(super) fn place(
        &mut self,
        rect: mdshared_app::sister::Rect,
        for_tab: (std::path::PathBuf, Option<String>),
    ) {
        let Some(w) = self.window.clone() else {
            return;
        };
        if self.tiling.set(&w, rect) {
            // Resolved once, here: the loop looks for its tab on every turn.
            let (path, doc) = for_tab;
            let path = if path.as_os_str().is_empty() {
                path
            } else {
                resolve(&path)
            };
            self.placed_for = Some((path, doc));
        } else {
            self.ws
                .set_status("This desktop does not let a window be placed");
        }
    }

    /// The side-by-side arrangement is over: the window goes back to where it was before,
    /// unless the user has moved it since. `tell` the editor to do the same with its own when
    /// the ending is this window's doing; that goes off this thread, as a send waits for the
    /// answer.
    pub(super) fn end_side_by_side(&mut self, tell: bool) {
        self.placed_for = None;
        if !self.tiling.is_some() {
            return;
        }
        if let Some(w) = self.window.clone() {
            self.tiling.restore(&w);
        } else {
            self.tiling.forget();
        }
        if tell {
            std::thread::spawn(|| {
                mdshared_app::sister::send("mdedit", &unplace_request());
            });
        }
    }

    /// The exit while the windows are side by side: the place the window had before is the one
    /// the next start gets, and the editor is told to go back to its own. True when that was
    /// so: the size is saved then, and [`save_size`](Self::save_size) would save the half.
    pub(super) fn leave_side_by_side(&mut self) -> bool {
        let Some(w) = self.window.clone() else {
            return false;
        };
        let Some(before) = self.tiling.before(&w) else {
            return false;
        };
        self.tiling.forget();
        let (cw, ch) = self.content_size(before.inner.0, before.inner.1);
        let size = winit::dpi::PhysicalSize::new(cw, ch).to_logical::<f64>(w.scale_factor());
        if !before.maximized {
            (self.ws).set_window_position(before.outer.x, before.outer.y);
        }
        self.ws.set_window_size(
            size.width.round() as u32,
            size.height.round() as u32,
            before.maximized,
        );
        mdshared_app::sister::send("mdedit", &unplace_request());
        true
    }

    // ---- wakes and timers ----------------------------------------------------------------------

    pub(super) fn woken(&mut self, wake: Wake) {
        match wake {
            Wake::Open(paths) => {
                for path in paths {
                    self.dropped(path);
                }
            }
            Wake::Menu(item) => {
                use mdshared_ui::menu_bar::AppCommand;
                self.command(match item {
                    AppCommand::About => C::About,
                    AppCommand::Settings => C::Settings,
                    AppCommand::Quit => C::Exit,
                });
            }
            Wake::Instance => {
                let requests: Vec<Request> = self
                    .incoming
                    .as_ref()
                    .map(|r| r.try_iter().collect())
                    .unwrap_or_default();
                self.handed_over(requests);
            }
            Wake::Files => {
                // A burst (a git checkout) is looked at once: the watcher stays "pending" until
                // the check runs, so the events of the next moments do not wake again.
                if self.watch_due.is_none() {
                    self.watch_due = Some(Instant::now() + WATCH_DEBOUNCE);
                }
            }
            Wake::Sister(alive) => {
                self.sister_asking = false;
                if !alive {
                    self.ws.live_end_all();
                    // Nobody to tell: the editor is gone.
                    self.end_side_by_side(false);
                }
            }
            Wake::Preview => {
                if self.ui.previews.collect() {
                    self.resolve_card(Instant::now());
                }
            }
            Wake::Images => {
                self.ws.remote_completed();
            }
            Wake::Library(found) => self.library_found(found),
            Wake::FolderSearch => {
                if self.ui.palette.as_mut().is_some_and(|p| p.poll_folder()) {
                    self.modal_changed();
                }
            }
            Wake::Notes => {
                let taken = self.ws.notes_ready();
                if self.trace
                    && taken
                    && let Some(stats) = self.ws.notes_stats()
                {
                    eprintln!(
                        "mdview: note index ready: {} files, {} links, {} read, {} kept, cut short: {}",
                        stats.files,
                        stats.links,
                        stats.read_files,
                        stats.reused_files,
                        stats.truncated
                    );
                }
            }
            // A decoded frame is waiting: `timers` puts it up when it is due, and asks for a
            // frame only if something on screen changed.
            Wake::Animation => return,
            Wake::Motion => {
                self.read_motion();
                self.wants_frame = true;
                return;
            }
            Wake::Update(updater::Event::Checked { manual, result }) => {
                self.ws.update_checked(unix_now(), manual, result);
            }
            Wake::Update(updater::Event::Installed(result)) => {
                self.ws.update_finished(result);
            }
            Wake::Export(progress) => self.export_progress(progress),
            #[cfg(not(windows))]
            Wake::Dialog(answer) => {
                self.dialog_open = false;
                match answer {
                    DialogAnswer::Files(files) => {
                        for path in files {
                            let _ = self.ws.open(&path);
                        }
                    }
                    DialogAnswer::Folder(Some(dir)) => self.set_folder(dir),
                    DialogAnswer::Folder(None) => {}
                }
            }
        }
        self.redraw();
    }

    pub(super) fn dropped(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.set_folder(path);
        } else if mdview_core::links::is_markdown_path(&path) {
            let _ = self.ws.open(&path);
        } else {
            self.ws
                .set_status(mdview_core::commands::strings::ONLY_MARKDOWN_LINKS);
        }
        self.redraw();
    }

    /// Runs whatever timers are due and returns the next deadline.
    /// At launch: clears what an earlier update left behind and, when a look for a newer version
    /// is due, takes it. Nothing here runs for a session-less launch or a copy that cannot update
    /// itself, and all of it is on its own thread.
    pub(super) fn updates_at_launch(&mut self) {
        if self.session.is_none() {
            return;
        }
        let Some(files) = updater::installed_files() else {
            return;
        };
        let due = self.ws.update_check_due(unix_now());
        let proxy = self.proxy.clone();
        spawn_updater(move || {
            updater::clean_up(&files);
            if due && let Some(proxy) = proxy {
                let result = updater::check(&updater::fetch);
                let _ = proxy.send_event(Wake::Update(updater::Event::Checked {
                    manual: false,
                    result,
                }));
            }
        });
    }

    /// Looks for a newer version now, on its own thread.
    pub(super) fn check_for_updates(&mut self, manual: bool) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        spawn_updater(move || {
            let result = updater::check(&updater::fetch);
            let _ = proxy.send_event(Wake::Update(updater::Event::Checked { manual, result }));
        });
    }

    /// Closes the window as Exit does (the session is saved on the way out), then starts the
    /// version the update installed: the file is taken now and started by `run()` after the
    /// window and the engine are gone. The viewer has no unsaved text to ask about.
    pub(super) fn restart_into_update(&mut self) {
        if let Some(target) = self.ws.update_relaunch_target() {
            self.restart = Some(target);
            self.exit = true;
        }
    }

    /// Downloads and swaps in the version that was offered, on its own thread.
    pub(super) fn install_update(&mut self) {
        let (Some(files), Some(proxy)) = (updater::installed_files(), self.proxy.clone()) else {
            self.ws.set_status(updater::NOT_INSTALLED);
            return;
        };
        let Some(offer) = self.ws.update_begin() else {
            return;
        };
        self.ws
            .update_set_relaunch(mdshared_app::relaunch::target(Some(&files)));
        spawn_updater(move || {
            let result = updater::install(&offer, &files, &updater::fetch);
            if result.is_ok() {
                updater::record_version(&offer.version);
            }
            let _ = proxy.send_event(Wake::Update(updater::Event::Installed(result)));
        });
    }

    /// Looks at the open files and the Files pane's folders once the burst of changes is over.
    pub(super) fn watch_check(&mut self) {
        if let Some(w) = &self.watcher {
            w.woken();
        }
        self.ws.check_changed();
        self.ws.notes_changed();
        let changed = self.ui.tree.refresh();
        if changed {
            self.wants_frame = true;
        }
        // Find in folder, open: the results are looked for again (off this thread, as the
        // search always is); the list on screen is replaced when that is done.
        if self.ui.palette.as_mut().is_some_and(|p| p.refresh_folder()) {
            self.modal_changed();
        }
        if self.trace {
            let rows = self.ui.tree.rows().len();
            eprintln!("mdview: file watcher woke; files tree changed: {changed}, {rows} rows");
        }
    }

    pub(super) fn timers(&mut self, now: Instant) -> Option<Instant> {
        let mut next: Option<Instant> = None;
        let mut at = |t: Instant| next = Some(next.map_or(t, |n| n.min(t)));
        // Caret blink, only while the find box or the palette has focus.
        if (self.ui.find_focused || self.ui.palette.is_some()) && self.window_focused() {
            if let Some(period) = platform::caret_blink_time() {
                let due = *self.ui.caret_due.get_or_insert(now + period);
                if now >= due {
                    self.ui.caret_on = !self.ui.caret_on;
                    self.ui.caret_due = Some(now + period);
                    self.wants_frame = true;
                }
                at(self.ui.caret_due.unwrap_or(now + period));
            }
        } else {
            self.ui.caret_due = None;
        }
        if let Some(t) = self.present_tick(now) {
            at(t);
        }
        // The next frame of what is moving: asked for once, then the frame's `end_frame` sets
        // it again or leaves none. A covered window draws no frame, so it sets none either.
        if let Some(due) = self.anim_due {
            if now >= due {
                self.anim_due = None;
                self.wants_frame = true;
            } else {
                at(due);
            }
        }
        // The pointer resting on a submenu item: open it when the dwell is up. Nothing is
        // scheduled when no menu is open, because the dwell lives in the popup.
        let dwell = self.ui.popup.as_ref().and_then(|p| p.dwell);
        if let Some((_, due)) = dwell {
            if now >= due {
                // The pointer rested: on a submenu item its submenu opens, on any other item
                // the open submenu closes.
                let opens = self.ui.popup.as_mut().is_some_and(|p| {
                    let i = p.dwell.take().map(|d| d.0);
                    let opens =
                        i.is_some_and(|i| p.menu.items.get(i).is_some_and(|it| it.id.is_submenu()));
                    if opens {
                        p.menu.hover = i;
                    } else {
                        p.sub = None;
                    }
                    opens
                });
                if opens {
                    self.open_submenu(false);
                }
                self.wants_frame = true;
            } else {
                at(due);
            }
        }
        if self.ui.tips.tick(now) {
            self.wants_frame = true;
        }
        if let Some(due) = self.ui.tips.deadline() {
            at(due);
        }
        if let Some((_, until)) = self.ui.copied {
            if now >= until {
                self.ui.copied = None;
                self.wants_frame = true;
            } else {
                at(until);
            }
        }
        if let Some(due) = self.card_timer(now) {
            at(due);
        }
        if let Some(until) = self.toast_until {
            if now >= until {
                self.toast_until = None;
                self.wants_frame = true;
            } else {
                at(until);
            }
        } else if let Some(until) = self.ws.status_expires().filter(|&u| u > now) {
            // A message set since the last frame.
            at(until);
        }
        if let Some(due) = self.ui.autoscroll_due {
            if now >= due {
                self.autoscroll();
                self.ui.autoscroll_due =
                    (self.ui.drag == Some(Drag::Select)).then(|| now + AUTOSCROLL_TICK);
            }
            if let Some(d) = self.ui.autoscroll_due {
                at(d);
            }
        }
        if let Some(due) = self.size_save_due {
            if now >= due {
                self.size_save_due = None;
                self.save_size();
            } else {
                at(due);
            }
        }
        if let Some(due) = self.watch_due {
            if now >= due {
                self.watch_due = None;
                self.watch_check();
            } else {
                at(due);
            }
        }
        // Animated images, only while the window can be seen. A window that is minimised or
        // covered asks for nothing, so its animations stand still and cost nothing.
        if self.shows_page() && self.anim_seen && self.covered_now(now) {
            // Windows never says a window is covered, so it is asked: no frame is drawn while it
            // is, and it is asked again in a second (only while an animated image is on the page).
            at(now + COVER_CHECK);
        } else if self.shows_page() {
            let tick = self.ws.animate(now);
            self.anim_seen = tick.next.is_some() || tick.rows.is_some();
            if let Some((a, b)) = tick.rows {
                self.anim_rows = Some(match self.anim_rows {
                    Some((c, d)) => (a.min(c), b.max(d)),
                    None => (a, b),
                });
                self.wants_frame = true;
            }
            if let Some(due) = tick.next {
                at(due.max(now));
            }
        }
        // The editor may die without a word: while a tab shows its text, or the window is where
        // its preview put it, ask now and then.
        if self.ws.any_live() || self.tiling.is_some() {
            let due = *self.sister_due.get_or_insert(now + SISTER_CHECK);
            if now >= due {
                self.ask_sister();
                self.sister_due = Some(now + SISTER_CHECK);
            }
            at(self.sister_due.unwrap_or(now + SISTER_CHECK));
        } else {
            self.sister_due = None;
        }
        let active = self.ws.active();
        let unloadable = self
            .ws
            .tabs()
            .iter()
            .enumerate()
            .any(|(i, t)| Some(i) != active && t.is_loaded() && !t.is_missing());
        if unloadable {
            let due = *self.idle_due.get_or_insert(now + IDLE_CHECK);
            if now >= due {
                self.ws.drop_idle(now, IDLE_UNLOAD);
                self.idle_due = Some(now + IDLE_CHECK);
            }
            at(self.idle_due.unwrap_or(now + IDLE_CHECK));
        } else {
            self.idle_due = None;
        }
        next
    }

    /// Asks, on a helper thread, whether a window of mdedit answers, and posts the answer. Sends
    /// nothing to it. One question at a time: a slow answer is waited for, not asked again.
    fn ask_sister(&mut self) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        if self.sister_asking {
            return;
        }
        self.sister_asking = true;
        std::thread::spawn(move || {
            let _ = proxy.send_event(Wake::Sister(mdshared_app::sister::alive("mdedit")));
        });
    }

    /// Whether the window is on screen: created, shown, not minimised and not fully covered.
    /// Covered is what winit reports with `WindowEvent::Occluded` (X11 and macOS; Windows and
    /// Wayland never send it, so a window covered there is not known to be). Without a window (the
    /// tests') there is nothing to ask, and nothing hides it.
    pub(super) fn shows_page(&self) -> bool {
        !self.first
            && !self.occluded
            && self
                .window
                .as_ref()
                .is_none_or(|w| w.is_minimized() != Some(true) && w.is_visible() != Some(false))
    }

    /// Whether another window hides all of this one, as far as the system tells (Windows: the
    /// visible region is empty). The answer is kept for [`COVER_CHECK`], so asking costs one
    /// small system call a second while animated images play, and nothing otherwise. False
    /// without a window and on systems that report covering themselves (`occluded`).
    pub(super) fn covered_now(&mut self, now: Instant) -> bool {
        if let Some((at, covered)) = self.covered
            && now.saturating_duration_since(at) < COVER_CHECK
        {
            return covered;
        }
        let covered = self
            .window
            .as_ref()
            .is_some_and(|w| platform::is_covered(w));
        self.covered = Some((now, covered));
        covered
    }

    pub(super) fn window_focused(&self) -> bool {
        self.window.as_ref().is_none_or(|w| w.has_focus())
    }

    pub(super) fn autoscroll(&mut self) {
        let (x, y) = self.ui.mouse;
        let vr = page::viewport(&self.ws, self.areas.page);
        let beyond = if y < vr.y {
            y - vr.y
        } else if y >= vr.bottom() {
            y - vr.bottom() + 1.0
        } else {
            0.0
        };
        if beyond == 0.0 {
            return;
        }
        let step = (beyond.abs() * 0.6).clamp(4.0, 60.0) * beyond.signum() * self.scale;
        self.ws.scroll_by(step);
        self.drag_to(Drag::Select, x, y);
        self.wants_frame = true;
    }

    pub(super) fn save_size(&mut self) {
        let Some(w) = &self.window else { return };
        if w.is_minimized() == Some(true) || w.fullscreen().is_some() {
            return;
        }
        // The CONTENT size (the window reports the surface, margin included, with the
        // client-drawn shadow), so the saved size does not grow by the margin on every start.
        let surface = w.inner_size();
        let (cw, ch) = self.content_size(surface.width, surface.height);
        let size = winit::dpi::PhysicalSize::new(cw, ch).to_logical::<f64>(w.scale_factor());
        self.ws.set_window_size(
            size.width.round() as u32,
            size.height.round() as u32,
            w.is_maximized(),
        );
    }

    /// `--scroll-steps`: one wheel step per frame, then a summary and exit.
    pub(super) fn step(&mut self) {
        let Some(n) = self.steps else { return };
        if n == 0 {
            // "Ours" is everything but the OS blit inside softbuffer's present.
            let ours =
                |t: &FrameTimes| (t.layout + t.paint + t.compose + t.copy).as_secs_f64() * 1000.0;
            let mut totals: Vec<f64> = self.step_times.iter().map(ours).collect();
            totals.sort_by(f64::total_cmp);
            let pick = |q: f64| {
                totals
                    .get(((totals.len() as f64 - 1.0) * q).round() as usize)
                    .copied()
                    .unwrap_or(0.0)
            };
            let avg = |f: fn(&FrameTimes) -> Duration| {
                self.step_times
                    .iter()
                    .map(|t| f(t).as_secs_f64() * 1000.0)
                    .sum::<f64>()
                    / self.step_times.len().max(1) as f64
            };
            eprintln!(
                "mdview: {} scroll frames at {}x{} px, own work: median {:.2} ms, p90 {:.2}, max {:.2}; mean layout {:.2} paint {:.2} compose {:.2} copy {:.2}, OS present {:.2}",
                totals.len(),
                self.size.0,
                self.size.1,
                pick(0.5),
                pick(0.9),
                pick(1.0),
                avg(|t| t.layout),
                avg(|t| t.paint),
                avg(|t| t.compose),
                avg(|t| t.copy),
                avg(|t| t.present),
            );
            self.exit = true;
            return;
        }
        self.steps = Some(n - 1);
        if self.ws.scroll() >= self.ws.max_scroll() {
            self.ws.scroll_home();
        } else {
            self.ws.scroll_lines(WHEEL_LINES);
        }
        self.draw();
    }
}

/// Seconds since the Unix epoch.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Runs `work` on a thread named for the updater; if the thread cannot start, nothing happens.
fn spawn_updater(work: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("mdview-update".into())
        .spawn(work);
}

/// The request that asks the editor to put its window back where it was.
pub(super) fn unplace_request() -> Request {
    Request {
        unplace: true,
        from: Some("mdview".into()),
        ..Request::default()
    }
}

/// `closed`, from this window to the editor, for the file `path` or its document `doc`.
fn closed_request(path: &Path, doc: Option<String>) -> Request {
    Request {
        closed: true,
        doc,
        from: Some("mdview".into()),
        ..Request::new(path)
    }
}
