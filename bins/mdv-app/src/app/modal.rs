//! The command palette and the settings dialog as the window hosts them: both are drawn last,
//! and while one is open every key, pointer and wheel event goes to it and not to the page.
//! Every outcome but "nothing" redraws.
//!
//! **Keys the palette or the form does not use** (`Unused`) are dropped, except the shortcut
//! that opened it, which closes it again (Ctrl+P, Ctrl+, ). A file shortcut such as Ctrl+O or
//! Ctrl+W acting on the page behind an open list would be a surprise, and the list is one
//! Escape away.

use super::*;
use crate::folder_search::Waker;
use crate::palette::{PaletteState, Target};
use crate::settings as setting;
use mdshared_ui::corners::Corners;
use mdshared_ui::palette::Outcome as Pick;
use mdshared_ui::settings::{Outcome as Form, Value};
use mdview_core::session::{PageWidth, clamp_zoom};
use std::sync::Arc;

impl App {
    // ---- opening and closing -------------------------------------------------------------------

    /// Opens the command palette with `query` in its box (the shots pass one).
    pub(super) fn open_palette(&mut self, query: &str) {
        self.ui.popup = None;
        self.ui.tip = None;
        self.ui.hover = None;
        self.ui.find_focused = false;
        self.ui.dialog = None;
        self.ui.settings = None;
        self.ui.palette = Some(PaletteState::open(&self.ws, query));
        self.caret_reset();
        self.modal_changed();
    }

    /// Find in folder over the open folder, with the find box's text in its box. Without a folder
    /// it says so.
    pub(super) fn open_folder_search(&mut self) {
        let Some(root) = self.ws.folder().map(Path::to_path_buf) else {
            self.ws.set_status("Open a folder to search it");
            return;
        };
        let wake: Option<Waker> = self.proxy.clone().map(|proxy| {
            Arc::new(move || {
                let _ = proxy.send_event(Wake::FolderSearch);
            }) as Waker
        });
        let query = self.ui.find.text().to_string();
        self.ui.popup = None;
        self.ui.tip = None;
        self.ui.hover = None;
        self.ui.find_focused = false;
        self.ui.dialog = None;
        self.ui.settings = None;
        self.ui.palette = Some(PaletteState::open_folder(root, &query, wake));
        self.caret_reset();
        self.modal_changed();
    }

    /// Opens a result of Find in folder: the file, with the query found in the page as the find
    /// box would, at the match nearest the line.
    fn open_search_hit(&mut self, path: &Path, line: u32, query: &str) {
        if !self.ws.open_search_result(path, query, line) {
            return;
        }
        self.ui.find.set_text(query);
        self.redraw();
    }

    pub(super) fn close_palette(&mut self) {
        self.ui.palette = None;
        self.modal_closed();
    }

    pub(super) fn open_settings(&mut self) {
        self.ui.popup = None;
        self.ui.tip = None;
        self.ui.hover = None;
        self.ui.find_focused = false;
        self.ui.dialog = None;
        self.ui.palette = None;
        self.ui.settings = Some(setting::form(&self.ws, self.corners_offered()));
        self.modal_changed();
    }

    pub(super) fn close_settings(&mut self) {
        self.ui.settings = None;
        self.modal_closed();
    }

    pub(super) fn modal_changed(&mut self) {
        self.ui.overlay_tick += 1;
        self.redraw();
    }

    fn modal_closed(&mut self) {
        self.ui.pressed = None;
        self.ui.hover = None;
        self.cursor_default();
        self.modal_changed();
    }

    fn cursor_default(&mut self) {
        self.set_cursor(CursorIcon::Default);
    }

    // ---- events --------------------------------------------------------------------------------

    pub(super) fn modal_key(&mut self, name: KeyName, typed: Option<&str>) {
        let mods = self.mods;
        let opener = keys::command_for(&name, mods);
        if let Some(p) = self.ui.palette.as_mut() {
            match p.palette.key(&name, mods, typed) {
                Pick::Unused => {
                    if opener == Some(C::CommandPalette) {
                        self.close_palette();
                    }
                }
                out => self.palette_outcome(out),
            }
            self.caret_reset();
        } else if let Some(f) = self.ui.settings.as_mut() {
            match f.key(&name, mods, typed) {
                Form::Unused => {
                    if opener == Some(C::Settings) {
                        self.close_settings();
                    }
                }
                out => self.settings_outcome(out),
            }
        }
    }

    pub(super) fn modal_pointer(&mut self, x: f32, y: f32) {
        let moved = match (self.ui.palette.as_mut(), self.ui.settings.as_mut()) {
            (Some(p), _) => p.palette.pointer_move(x, y),
            (_, Some(f)) => f.pointer_move(x, y),
            _ => false,
        };
        let edge = self.edge_at(x, y).map(CursorIcon::from);
        self.set_cursor(edge.unwrap_or(CursorIcon::Default));
        if moved {
            self.modal_changed();
        }
    }

    pub(super) fn modal_press(&mut self, clicks: u32) {
        let (x, y) = self.ui.mouse;
        let (mods, clicks) = (self.mods, clicks.min(2) as u8);
        if let Some(p) = self.ui.palette.as_mut() {
            let out = p.palette.press(x, y, mods, clicks);
            self.palette_outcome(out);
        } else if let Some(f) = self.ui.settings.as_mut() {
            let out = f.press(x, y, mods, clicks);
            self.settings_outcome(out);
        }
        self.caret_reset();
    }

    pub(super) fn modal_release(&mut self) {
        let (x, y) = self.ui.mouse;
        if let Some(p) = self.ui.palette.as_mut() {
            let out = p.palette.release(x, y);
            self.palette_outcome(out);
        } else if let Some(f) = self.ui.settings.as_mut() {
            let out = f.release(x, y);
            self.settings_outcome(out);
        }
    }

    /// `dy` in logical px, positive towards the end.
    pub(super) fn modal_wheel(&mut self, dy: f32) {
        let (x, y) = self.ui.mouse;
        let moved = match (self.ui.palette.as_mut(), self.ui.settings.as_mut()) {
            // Over the palette it scrolls the list; beside it, nothing.
            (Some(p), _) => p.palette.contains(x, y) && p.palette.wheel(dy),
            (_, Some(f)) => f.wheel(x, y, dy),
            _ => false,
        };
        if moved {
            self.modal_changed();
        }
    }

    // ---- outcomes ------------------------------------------------------------------------------

    fn palette_outcome(&mut self, out: Pick) {
        match out {
            Pick::None | Pick::Unused | Pick::Submitted => {}
            Pick::Redraw => self.modal_changed(),
            Pick::QueryChanged => {
                if let Some(p) = self.ui.palette.as_mut() {
                    p.sync(&self.ws);
                }
                self.modal_changed();
            }
            Pick::Closed => self.close_palette(),
            Pick::Chosen(id) => {
                // A result of Find in folder: the file, at the line.
                if let Some((path, line, query)) =
                    self.ui.palette.as_ref().and_then(|p| p.folder_hit(id))
                {
                    self.close_palette();
                    self.open_search_hit(&path, line, &query);
                    return;
                }
                let target = self.ui.palette.as_ref().and_then(|p| p.target(id)).cloned();
                match target {
                    Some(Target::Prefix(prefix)) => {
                        if let Some(p) = self.ui.palette.as_mut() {
                            p.set_query(&self.ws, prefix);
                        }
                        self.modal_changed();
                    }
                    Some(target) => {
                        // Closed first, so what it runs (a dialog, the find box) starts clean.
                        self.close_palette();
                        self.run(target);
                    }
                    None => {}
                }
            }
        }
    }

    fn run(&mut self, target: Target) {
        match target {
            Target::Cmd(id) => self.command(id),
            Target::Tab(i) => {
                self.ws.activate(i);
            }
            Target::Recent(path) => {
                let _ = self.ws.open_recent(&path);
            }
            Target::File(path) => {
                let _ = self.ws.open(&path);
            }
            Target::Heading(block) => {
                self.ws.outline_jump(block);
            }
            Target::Prefix(_) => {}
        }
        self.redraw();
    }

    fn settings_outcome(&mut self, out: Form) {
        match out {
            Form::None | Form::Unused => {}
            Form::Redraw => self.modal_changed(),
            Form::Closed => self.close_settings(),
            Form::Changed { id, value } => {
                self.apply_setting(id, value);
                self.modal_changed();
            }
            Form::Action(id) => {
                // The answer shows on the page (a bar or a toast), so the dialog goes, as About's
                // buttons do.
                self.close_settings();
                match id {
                    setting::CHECK_NOW => {
                        if matches!(self.ws.update_stage(), updater::Stage::Available(_)) {
                            self.install_update();
                        } else {
                            self.check_for_updates(true);
                        }
                    }
                    setting::DEFAULT_APP => self.command(C::MakeDefault),
                    _ => {}
                }
            }
        }
    }

    /// Whether the window can be rounded here, so the setting is offered: Windows (11 rounds it,
    /// 10 ignores the call), or a Linux window with a shadow route (Wayland; X11 has none unless
    /// forced and its window is opaque). A shot has no window and shows the row.
    pub(super) fn corners_offered(&self) -> bool {
        cfg!(windows) || self.shadow.is_some() || self.args.shot.is_some()
    }

    /// Takes a new corner setting and keeps it: Windows is told its preference; with the
    /// client-drawn margin the whole window is drawn again (KDE chooses its route from the setting
    /// at the next start).
    pub(super) fn set_corners(&mut self, c: Corners) {
        if Corners::from_setting(self.ws.corners()) == c {
            return;
        }
        self.ws.set_corners(c.to_setting());
        #[cfg(windows)]
        if let Some(w) = &self.window {
            style(w, self.pal.dark, c);
        }
        if self.shadow.as_mut().is_some_and(|s| s.set_corners(c)) {
            self.full = true;
            self.expose = true;
            self.redraw();
        }
    }

    /// Applies one change with the setter the menus used, which also saves it.
    pub(super) fn apply_setting(&mut self, id: u32, value: Value) {
        match (id, value) {
            (setting::PAGE_WIDTH, Value::Choice(i)) => {
                let width = PageWidth::ALL.get(i).copied().unwrap_or_default();
                self.ws.set_page_width(width);
            }
            (setting::TEXT_SIZE, Value::Number(n)) => {
                self.ws
                    .set_zoom(clamp_zoom(n.clamp(0, u16::MAX as i32) as u16));
            }
            (setting::THEME, Value::Choice(i)) => {
                self.ws.set_theme_choice(setting::theme_of(i));
            }
            (setting::WINDOW_CORNERS, Value::Choice(i)) => {
                self.set_corners(Corners::from_index(i));
            }
            (setting::PAGE_FONT, Value::Text(family)) => _ = self.ws.set_font(false, &family),
            (setting::CODE_FONT, Value::Text(family)) => _ = self.ws.set_font(true, &family),
            (setting::REMOTE_IMAGES, Value::Bool(on)) => self.ws.set_remote_always(on),
            (setting::TICK_BOXES, Value::Bool(on)) => self.ws.set_edit_always(on),
            (setting::ANIMATIONS, Value::Bool(on)) => self.ws.set_animations(on),
            (setting::START_ON_PINNED, Value::Bool(on)) => self.ws.set_start_on_pinned(on),
            (setting::OFFER_STALE, Value::Bool(on)) => self.ws.set_offer_stale(on),
            (setting::PREVIEWS, Value::Bool(on)) => {
                self.ws.set_previews(on);
                self.ui.card = None;
            }
            (setting::BACKDROP, Value::Choice(i)) => {
                (self.ws).set_backdrop(mdshared_app::backdrop::Backdrop::from_index(i))
            }
            (setting::CHECK_AT_LAUNCH, Value::Bool(on)) => self.ws.set_checks_updates(on),
            _ => {}
        }
    }
}
