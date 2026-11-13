//! What clicks, menu items and shortcuts do: activation, the command table's commands, popups and
//! dialogs, plus wakes from other threads and the loop's timers.

use super::*;
use crate::ui::EditAct;

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
            Hit::LeaveFocus => self.command(C::FocusMode),
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
            Hit::OutlineRow(i) => {
                if let Some(block) = self.ws.outline().get(i).map(|e| e.block) {
                    self.ws.outline_jump(block);
                }
            }
            Hit::BacklinkRow(i) => {
                self.ws.open_backlink(i);
            }
            Hit::EmptyOpen => self.command(C::OpenFiles),
            Hit::RemoteLoad => self.ws.allow_remote(false),
            Hit::RemoteAlways => self.ws.allow_remote(true),
            Hit::RemoteClose => self.ws.dismiss_remote(),
            Hit::UpdateGo => self.install_update(),
            Hit::UpdateClose => self.ws.dismiss_update(),
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
                Some(DialogKind::About) if n == 1 => {
                    // The answer shows on the page (the bar or a toast), so the dialog goes.
                    self.close_dialog();
                    if matches!(self.ws.update_stage(), updater::Stage::Available(_)) {
                        self.install_update();
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
                    self.ws.set_status("Link address copied");
                }
            }
            Act::CopySource(kind, text) => {
                if platform::set_clipboard_text(&text) {
                    self.ws.set_status(match kind {
                        SourceKind::Formula => "Formula source copied",
                        SourceKind::Diagram => "Diagram source copied",
                    });
                }
            }
            Act::Copy => self.copy(),
            Act::SelectAll => {
                self.ws.select_all();
            }
            Act::RemoteAlways => {
                let on = !self.ws.remote_always();
                self.ws.set_remote_always(on);
            }
            Act::PageWidth(width) => {
                self.ws.set_page_width(width);
            }
            Act::EditAlways => {
                let on = !self.ws.edit_always();
                self.ws.set_edit_always(on);
            }
            Act::Animations => {
                let on = !self.ws.animations();
                self.ws.set_animations(on);
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

    pub(super) fn copy(&mut self) {
        if let Some(text) = self.ws.selected_text()
            && platform::set_clipboard_text(&text)
        {
            self.ws.set_status("Copied");
        }
    }

    pub(super) fn tab_command(&mut self, id: C, index: usize) {
        let Some(path) = self.ws.tab(index).map(|t| t.path().to_path_buf()) else {
            return;
        };
        match id {
            C::CloseTab => {
                self.ws.close(index);
            }
            C::CloseOtherTabs => {
                self.ws.close_others(index);
            }
            C::CloseAllTabs => {
                self.ws.close_all();
            }
            C::CopyFilePath => {
                if platform::set_clipboard_text(&path.display().to_string()) {
                    self.ws.set_status("File path copied");
                }
            }
            C::ShowInFileManager => {
                platform::reveal_in_file_manager(&path);
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
        self.ui.popup = Some(Popup::new(kind, menu));
        self.ui.tip = None;
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
        p.sub = Some(menu);
        self.ui.tip = None;
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
        self.ui.tip = None;
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

    pub(super) fn start_dir(&self) -> Option<PathBuf> {
        self.ws
            .active_tab()
            .and_then(|t| t.path().parent().map(Path::to_path_buf))
            .or_else(|| self.ws.folder().map(Path::to_path_buf))
    }

    /// Starts a file dialog on its helper thread through `start`, which is given a proxy for the
    /// answer and says whether the dialog opened. The window keeps drawing meanwhile; a request
    /// made while one is open is ignored. The answer comes back as `Wake::Dialog`.
    #[cfg(not(windows))]
    fn open_dialog_async(&mut self, start: impl FnOnce(EventLoopProxy<Wake>) -> bool) {
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
            C::FocusMode => {
                self.ui.focus_mode = !self.ui.focus_mode;
                self.ui.popup = None;
                if self.ui.focus_mode {
                    self.ui.find_focused = false;
                    self.ws.set_status("Focus mode: press Escape to leave");
                }
            }
            C::ShowShortcuts => self.open_dialog(DialogKind::Shortcuts),
            C::About => self.open_dialog(DialogKind::About),
            C::NextTab => {
                self.ws.next_tab();
            }
            C::PreviousTab => {
                self.ws.previous_tab();
            }
            C::Escape => self.escape(),
            C::CloseOtherTabs | C::CopyFilePath | C::ShowInFileManager => {
                if let Some(i) = self.ws.active() {
                    self.tab_command(id, i);
                }
            }
        }
        self.redraw();
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

    // ---- wakes and timers ----------------------------------------------------------------------

    pub(super) fn woken(&mut self, wake: Wake) {
        match wake {
            Wake::Instance => {
                let paths: Vec<PathBuf> = self
                    .incoming
                    .as_ref()
                    .map(|r| r.try_iter().collect())
                    .unwrap_or_default();
                for p in paths {
                    let _ = self.ws.open(&p);
                }
                if let Some(w) = &self.window {
                    w.set_minimized(false);
                    w.focus_window();
                }
            }
            Wake::Files => {
                if let Some(w) = &self.watcher {
                    w.woken();
                }
                self.ws.check_changed();
                self.ws.notes_changed();
                let changed = self.ui.tree.refresh();
                if self.trace {
                    let rows = self.ui.tree.rows().len();
                    eprintln!(
                        "mdview: file watcher woke; files tree changed: {changed}, {rows} rows"
                    );
                }
            }
            Wake::Images => {
                self.ws.remote_completed();
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
            Wake::Update(updater::Event::Checked { manual, result }) => {
                self.ws.update_checked(unix_now(), manual, result);
            }
            Wake::Update(updater::Event::Installed(result)) => {
                self.ws.update_finished(result);
            }
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

    /// Downloads and swaps in the version that was offered, on its own thread.
    pub(super) fn install_update(&mut self) {
        let (Some(files), Some(proxy)) = (updater::installed_files(), self.proxy.clone()) else {
            self.ws.set_status(updater::NOT_INSTALLED);
            return;
        };
        let Some(offer) = self.ws.update_begin() else {
            return;
        };
        spawn_updater(move || {
            let result = updater::install(&offer, &files, &updater::fetch);
            if result.is_ok() {
                updater::record_version(&offer.version);
            }
            let _ = proxy.send_event(Wake::Update(updater::Event::Installed(result)));
        });
    }

    pub(super) fn timers(&mut self, now: Instant) -> Option<Instant> {
        let mut next: Option<Instant> = None;
        let mut at = |t: Instant| next = Some(next.map_or(t, |n| n.min(t)));
        // Caret blink, only while the find box has focus.
        if self.ui.find_focused && self.window_focused() {
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
        // The pointer resting on a submenu item: open it when the dwell is up. Nothing is
        // scheduled when no menu is open, because the dwell lives in the popup.
        let dwell = self.ui.popup.as_ref().and_then(|p| p.dwell);
        if let Some((_, due)) = dwell {
            if now >= due {
                self.open_submenu(false);
                self.wants_frame = true;
            } else {
                at(due);
            }
        }
        if let Some(tip) = self.ui.tip.as_mut().filter(|t| !t.shown) {
            if now >= tip.due {
                tip.shown = true;
                self.wants_frame = true;
            } else {
                at(tip.due);
            }
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
        // Animated images, only while the window can be seen. A window that is minimised or
        // covered asks for nothing, so its animations stand still and cost nothing.
        if self.shows_page() {
            let tick = self.ws.animate(now);
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

    /// Whether the window is on screen: created, shown, not minimised and not fully covered.
    pub(super) fn shows_page(&self) -> bool {
        !self.first
            && !self.occluded
            && self
                .window
                .as_ref()
                .is_some_and(|w| w.is_minimized() != Some(true) && w.is_visible() != Some(false))
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
        if w.is_minimized() == Some(true) {
            return;
        }
        let size = w.inner_size().to_logical::<f64>(w.scale_factor());
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
