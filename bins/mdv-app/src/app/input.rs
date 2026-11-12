//! Pointer, wheel and keyboard input: turning winit events into hover, drags, clicks and keys.

use super::*;

impl App {
    // ---- pointer -------------------------------------------------------------------------------

    pub(super) fn pointer_moved(&mut self, x: f32, y: f32) {
        self.ui.mouse = (x, y);
        if let Some(drag) = self.ui.drag {
            self.drag_to(drag, x, y);
            self.redraw();
            return;
        }
        let edge = self.edge_at(x, y);
        let in_menu = self
            .ui
            .popup
            .as_ref()
            .is_some_and(|p| p.menu.contains(x, y));
        if let Some(p) = self.ui.popup.as_mut()
            && p.menu.pointer(x, y)
        {
            self.wants_frame = true;
        }
        let hit = if edge.is_some() || in_menu {
            None
        } else {
            self.hit_at(x, y)
        };
        if hit != self.ui.hover {
            self.ui.hover = hit;
            self.update_tip(hit);
        }
        if let Some(d) = self.ui.dialog.as_ref() {
            let row = overlays::dialog_row_at(d, x, y);
            if row != self.ui.dlg_hover_row {
                self.ui.dlg_hover_row = row;
            }
        }
        // Page hover: the link hint and the cursor.
        let mut link = None;
        let mut cursor = CursorIcon::Default;
        if let Some(dir) = edge {
            cursor = CursorIcon::from(dir);
        } else {
            match hit {
                Some(Hit::Page) => {
                    let (_, px, py) = self.page_point(x, y);
                    let hover = self.ws.hover(px, py);
                    cursor = if hover.pointing {
                        CursorIcon::Pointer
                    } else if hover.text {
                        CursorIcon::Text
                    } else {
                        CursorIcon::Default
                    };
                    link = hover.link;
                }
                Some(Hit::Find) => cursor = CursorIcon::Text,
                Some(Hit::Splitter(_)) => cursor = CursorIcon::ColResize,
                _ => {}
            }
        }
        if link != self.ui.link_hint {
            self.ui.link_hint = link;
        }
        self.set_cursor(cursor);
        self.redraw();
    }

    pub(super) fn update_tip(&mut self, hit: Option<Hit>) {
        if self.ui.tip_blocked.is_some() && self.ui.tip_blocked != hit {
            self.ui.tip_blocked = None;
        }
        let text = hit
            .filter(|h| Some(*h) != self.ui.tip_blocked)
            .and_then(|h| chrome::tooltip(h, &self.ws, self.maximized));
        self.ui.tip = match (hit, text) {
            (Some(h), Some(text)) => {
                // Moving between neighbouring buttons while a tip shows keeps tips immediate.
                let shown = self.ui.tip.as_ref().is_some_and(|t| t.shown);
                let anchor = self.hit_rect(h).unwrap_or_default();
                Some(Tip {
                    hit: h,
                    text,
                    anchor,
                    due: Instant::now() + if shown { Duration::ZERO } else { TIP_DELAY },
                    shown: false,
                })
            }
            _ => None,
        };
    }

    pub(super) fn drag_to(&mut self, drag: Drag, x: f32, y: f32) {
        match drag {
            Drag::Tab {
                index,
                x: start,
                moving,
            } => {
                let moving = moving || (x - start).abs() > 5.0;
                let mut index = index;
                if moving {
                    let target =
                        self.hits[Region::Title.index()]
                            .iter()
                            .find_map(|(h, r)| match h {
                                Hit::Tab(j) if x >= r.x && x < r.right() => Some(*j),
                                _ => None,
                            });
                    if let Some(t) = target.filter(|&t| t != index)
                        && self.ws.move_tab(index, t)
                    {
                        index = t;
                    }
                }
                self.ui.drag = Some(Drag::Tab {
                    index,
                    x: start,
                    moving,
                });
            }
            Drag::Splitter {
                side,
                x: start,
                width,
            } => {
                let (w, _) = self.logical();
                let other = match side {
                    Side::Files => {
                        if self.ui.panes.outline {
                            self.ui.panes.outline_w
                        } else {
                            0.0
                        }
                    }
                    Side::Outline => {
                        if self.ui.panes.files {
                            self.ui.panes.files_w
                        } else {
                            0.0
                        }
                    }
                };
                let max = PANE_MAX.min(w - other - PAGE_MIN).max(PANE_MIN);
                let next = match side {
                    Side::Files => width + (x - start),
                    Side::Outline => width - (x - start),
                }
                .clamp(PANE_MIN, max)
                .round();
                match side {
                    Side::Files => self.ui.panes.files_w = next,
                    Side::Outline => self.ui.panes.outline_w = next,
                }
            }
            Drag::Thumb { target, grab } => self.thumb_to(target, y - grab),
            Drag::Select => {
                let (vr, px, py) = self.page_point(x, y);
                let h = vr.h * self.scale;
                self.ws.pointer_drag(
                    px.clamp(0.0, vr.w * self.scale - 1.0),
                    py.clamp(0.0, (h - 1.0).max(0.0)),
                );
                let outside = y < vr.y || y >= vr.bottom();
                if outside && self.ui.autoscroll_due.is_none() {
                    self.ui.autoscroll_due = Some(Instant::now() + AUTOSCROLL_TICK);
                } else if !outside {
                    self.ui.autoscroll_due = None;
                }
            }
            Drag::FindSelect => {
                if let Some(frame) = self.hit_rect(Hit::Find) {
                    let (area, _) = chrome::find_parts(frame, !self.ui.find.is_empty());
                    let stops = self.stops();
                    self.ui.find.drag(x - area.x, &stops);
                    self.caret_reset();
                }
            }
        }
    }

    /// Scroll geometry for a scroller: (view area, content, view, offset) in matching units.
    pub(super) fn scroll_geom(&self, target: Scroller) -> Option<(Rect, f32, f32, f32)> {
        match target {
            Scroller::Page => {
                let vr = page::viewport(&self.ws, self.areas.page);
                Some((
                    vr,
                    self.ws.total_height(),
                    self.ws.viewport_height(),
                    self.ws.scroll(),
                ))
            }
            Scroller::Pane(Side::Files) => {
                let r = self.areas.files?;
                let list = Rect::new(
                    r.x,
                    r.bottom() - self.ui.panes.files_view,
                    r.w,
                    self.ui.panes.files_view,
                );
                Some((
                    list,
                    self.ui.panes.files_content,
                    self.ui.panes.files_view,
                    self.ui.panes.files_scroll,
                ))
            }
            Scroller::Pane(Side::Outline) => {
                let r = self.areas.outline?;
                let list = Rect::new(
                    r.x,
                    r.bottom() - self.ui.panes.outline_view,
                    r.w,
                    self.ui.panes.outline_view,
                );
                Some((
                    list,
                    self.ui.panes.outline_content,
                    self.ui.panes.outline_view,
                    self.ui.panes.outline_scroll,
                ))
            }
            Scroller::Dialog => {
                let d = self.ui.dialog.as_ref()?;
                Some((d.list, d.content, d.view, d.scroll))
            }
        }
    }

    pub(super) fn set_scroll(&mut self, target: Scroller, offset: f32) {
        match target {
            Scroller::Page => {
                self.ws.scroll_to(offset);
            }
            Scroller::Pane(side) => {
                let cur = *self.ui.panes.scroll_mut(side);
                self.ui.panes.scroll_by(side, offset - cur);
            }
            Scroller::Dialog => {
                if let Some(d) = self.ui.dialog.as_mut() {
                    d.scroll = offset.clamp(0.0, (d.content - d.view).max(0.0)).round();
                }
            }
        }
    }

    pub(super) fn thumb_to(&mut self, target: Scroller, thumb_y: f32) {
        let Some((area, content, view, offset)) = self.scroll_geom(target) else {
            return;
        };
        let Some((track, thumb)) = panes::scrollbar(area, content, view, offset) else {
            return;
        };
        let next = panes::offset_for_thumb(track, thumb.h, thumb_y, content, view);
        self.set_scroll(target, next);
    }

    pub(super) fn left_press(&mut self) {
        let (x, y) = self.ui.mouse;
        self.ui.tip = None;
        self.ui.tip_blocked = self.ui.hover;
        if let Some(dir) = self.edge_at(x, y) {
            if let Some(w) = &self.window {
                let _ = w.drag_resize_window(dir);
            }
            return;
        }
        if let Some(p) = &self.ui.popup {
            if p.menu.contains(x, y) {
                return;
            }
            self.ui.popup = None;
            self.redraw();
            return;
        }
        let Some(hit) = self.hit_at(x, y) else { return };
        // Clicking elsewhere takes focus from the find box, except for its own buttons.
        if !matches!(
            hit,
            Hit::Find | Hit::FindClear | Hit::Bar(BarItem::FindNext | BarItem::FindPrev)
        ) {
            self.ui.find_focused = false;
        }
        let now = Instant::now();
        let clicks = match self.ui.last_press {
            Some((t, (px, py), n))
                if now - t < platform::double_click_time()
                    && (px - x).abs() <= CLICK_SLOP
                    && (py - y).abs() <= CLICK_SLOP =>
            {
                n % 3 + 1
            }
            _ => 1,
        };
        self.ui.last_press = Some((now, (x, y), clicks));
        self.redraw();
        match hit {
            Hit::Backdrop => self.close_dialog(),
            Hit::TitleDrag => {
                if clicks == 2 {
                    self.toggle_maximized();
                } else if let Some(w) = &self.window {
                    let _ = w.drag_window();
                }
            }
            Hit::Tab(i) => {
                self.ws.activate(i);
                self.ui.drag = Some(Drag::Tab {
                    index: i,
                    x,
                    moving: false,
                });
            }
            Hit::Splitter(side) => {
                let width = match side {
                    Side::Files => self.areas.files.map_or(self.ui.panes.files_w, |r| r.w),
                    Side::Outline => self.areas.outline.map_or(self.ui.panes.outline_w, |r| r.w),
                };
                self.ui.drag = Some(Drag::Splitter { side, x, width });
            }
            Hit::ScrollBar(target) => {
                let Some((area, content, view, offset)) = self.scroll_geom(target) else {
                    return;
                };
                let Some((_, thumb)) = panes::scrollbar(area, content, view, offset) else {
                    return;
                };
                if y >= thumb.y && y < thumb.bottom() {
                    self.ui.drag = Some(Drag::Thumb {
                        target,
                        grab: y - thumb.y,
                    });
                } else {
                    let step = view * 0.9 * if y < thumb.y { -1.0 } else { 1.0 };
                    self.set_scroll(target, offset + step);
                }
            }
            Hit::Page => {
                let (_, px, py) = self.page_point(x, y);
                self.ws.pointer_press(px, py, clicks);
                self.ui.drag = Some(Drag::Select);
            }
            Hit::Find => {
                self.ui.find_focused = true;
                let Some(frame) = self.hit_rect(Hit::Find) else {
                    return;
                };
                let (area, _) = chrome::find_parts(frame, !self.ui.find.is_empty());
                let stops = self.stops();
                if clicks == 2 {
                    self.ui.find.select_word_at(x - area.x, &stops);
                } else if clicks == 3 {
                    self.ui.find.select_all();
                } else {
                    self.ui.find.press(x - area.x, &stops, self.mods.shift);
                    self.ui.drag = Some(Drag::FindSelect);
                }
                self.caret_reset();
            }
            Hit::DlgCard => {}
            h if h.is_button() => self.ui.pressed = Some(h),
            _ => {}
        }
    }

    pub(super) fn left_release(&mut self) {
        let (x, y) = self.ui.mouse;
        self.ui.autoscroll_due = None;
        if let Some(drag) = self.ui.drag.take() {
            if matches!(drag, Drag::Splitter { .. }) {
                let panes = &self.ui.panes;
                self.ws
                    .set_pane_widths(panes.files_w as u32, panes.outline_w as u32);
            }
            if drag == Drag::Select {
                let click = self.ws.pointer_release();
                self.handle_click(click);
            }
            self.redraw();
            return;
        }
        if let Some(p) = &self.ui.popup
            && p.menu.contains(x, y)
        {
            let act = p.menu.chosen();
            if let Some(act) = act {
                self.ui.popup = None;
                self.act(act);
            }
            self.redraw();
            return;
        }
        if let Some(pressed) = self.ui.pressed.take() {
            if self.hit_at(x, y) == Some(pressed) {
                self.activate(pressed);
            }
            self.redraw();
        }
    }

    pub(super) fn right_press(&mut self) {
        let (x, y) = self.ui.mouse;
        self.ui.tip = None;
        if self.ui.dialog.is_some() {
            return;
        }
        self.ui.popup = None;
        match self.hit_at(x, y) {
            Some(Hit::Tab(i) | Hit::TabClose(i)) => {
                self.ui.popup = Some(Popup {
                    kind: PopupKind::Tab(i),
                    menu: overlays::tab_menu(&self.ws, i, x, y + 2.0),
                });
            }
            Some(Hit::Page) => {
                let (_, px, py) = self.page_point(x, y);
                let ctx = self.ws.context_at(px, py);
                self.ui.popup = Some(Popup {
                    kind: PopupKind::Page,
                    menu: overlays::page_menu(&ctx, x + 2.0, y + 2.0),
                });
            }
            Some(Hit::Find | Hit::FindClear) => {
                // The box takes the focus and keeps its selection, so Cut and Copy act on it.
                self.ui.find_focused = true;
                self.caret_reset();
                let find = &self.ui.find;
                self.ui.popup = Some(Popup {
                    kind: PopupKind::Find,
                    menu: overlays::find_menu(
                        !find.is_empty(),
                        find.has_selection(),
                        x + 2.0,
                        y + 2.0,
                    ),
                });
            }
            _ => {}
        }
        self.redraw();
    }

    /// The mouse's Back and Forward buttons, and Alt+Left and Alt+Right.
    pub(super) fn travel(&mut self, back: bool) {
        if self.ui.dialog.is_some() {
            return;
        }
        self.ui.popup = None;
        self.ui.tip = None;
        if back {
            self.ws.go_back();
        } else {
            self.ws.go_forward();
        }
        self.redraw();
    }

    pub(super) fn middle_press(&mut self) {
        let (x, y) = self.ui.mouse;
        if self.ui.dialog.is_some() || self.ui.popup.is_some() {
            return;
        }
        if let Some(Hit::Tab(i) | Hit::TabClose(i)) = self.hit_at(x, y) {
            self.ws.close(i);
            self.redraw();
        }
    }

    pub(super) fn wheel(&mut self, delta: MouseScrollDelta) {
        let (x, y) = self.ui.mouse;
        // Notches (positive = towards the user = scroll up) and device px.
        let (notches, pixels) = match delta {
            MouseScrollDelta::LineDelta(_, dy) => (dy, None),
            MouseScrollDelta::PixelDelta(p) => (p.y as f32 / (48.0 * self.scale), Some(p.y as f32)),
        };
        if self.ui.popup.is_some() {
            self.ui.popup = None;
            self.redraw();
            return;
        }
        if self.ui.dialog.is_some() {
            if let Some(d) = self.ui.dialog.as_ref() {
                let next = d.scroll - notches * PANE_WHEEL;
                self.set_scroll(Scroller::Dialog, next);
            }
            self.redraw();
            return;
        }
        if self.mods.ctrl {
            self.wheel_rest += notches;
            while self.wheel_rest >= 1.0 {
                self.wheel_rest -= 1.0;
                self.ws.zoom_in();
            }
            while self.wheel_rest <= -1.0 {
                self.wheel_rest += 1.0;
                self.ws.zoom_out();
            }
            self.redraw();
            return;
        }
        let over = |r: Option<Rect>| r.is_some_and(|r| r.contains(x, y));
        if over(self.areas.files) {
            self.ui.panes.scroll_by(Side::Files, -notches * PANE_WHEEL);
        } else if over(self.areas.outline) {
            self.ui
                .panes
                .scroll_by(Side::Outline, -notches * PANE_WHEEL);
        } else {
            match pixels {
                Some(py) => self.ws.scroll_by(-py),
                None => self.ws.scroll_lines(-notches * WHEEL_LINES),
            };
            if self.ws.is_pressing() {
                // Keep extending a selection while the wheel moves the page under the pointer.
                self.drag_to(Drag::Select, x, y);
            }
            // The page moved under the pointer: refresh the link hint and cursor.
            self.pointer_moved(x, y);
        }
        self.redraw();
    }

    pub(super) fn handle_click(&mut self, click: Click) {
        if let Click::Link(outcome) = click {
            self.link_outcome(outcome);
        }
    }

    pub(super) fn link_outcome(&mut self, outcome: LinkOutcome) {
        if let LinkOutcome::OpenUrl(url) = outcome
            && !platform::open_url(&url)
        {
            self.ws.set_status(&format!("Could not open {url}"));
        }
    }

    pub(super) fn toggle_maximized(&mut self) {
        if let Some(w) = &self.window {
            w.set_maximized(!w.is_maximized());
        }
    }

    // ---- keyboard ------------------------------------------------------------------------------

    pub(super) fn stops(&mut self) -> Vec<f32> {
        let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
        p.caret_stops(Face::Ui, 13.0, self.ui.find.text())
    }

    pub(super) fn caret_reset(&mut self) {
        self.ui.caret_on = true;
        self.ui.caret_due = platform::caret_blink_time().map(|d| Instant::now() + d);
    }

    pub(super) fn find_changed(&mut self) {
        let q = self.ui.find.text().to_string();
        self.ws.find_set(&q);
    }

    pub(super) fn key(&mut self, event: &KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        #[cfg(any(windows, target_os = "linux"))]
        let bare = {
            use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
            event.key_without_modifiers()
        };
        #[cfg(not(any(windows, target_os = "linux")))]
        let bare = event.logical_key.clone();
        let Some(name) = keys::key_name(&bare).or_else(|| keys::key_name(&event.logical_key))
        else {
            return;
        };
        self.key_named(name, event.text.as_deref());
    }

    /// A pressed key by name, with the text it typed; split from [`key`](Self::key) so tests can
    /// drive it without a winit event.
    pub(super) fn key_named(&mut self, name: KeyName, typed: Option<&str>) {
        let mods = self.mods;
        self.ui.tip = None;
        self.redraw();

        // A menu takes the keyboard.
        if let Some(p) = self.ui.popup.as_mut() {
            match name {
                KeyName::Up => p.menu.step(false),
                KeyName::Down | KeyName::Tab => p.menu.step(true),
                KeyName::Enter | KeyName::Space => {
                    if let Some(act) = p.menu.chosen() {
                        self.ui.popup = None;
                        self.act(act);
                    }
                }
                KeyName::Escape => self.ui.popup = None,
                _ => {}
            }
            return;
        }
        // A dialog too.
        if let Some(d) = self.ui.dialog.as_ref() {
            let scroll = d.scroll;
            match name {
                KeyName::Escape | KeyName::Enter => self.close_dialog(),
                KeyName::Up => self.set_scroll(Scroller::Dialog, scroll - 40.0),
                KeyName::Down => self.set_scroll(Scroller::Dialog, scroll + 40.0),
                KeyName::PageUp => self.set_scroll(Scroller::Dialog, scroll - 300.0),
                KeyName::PageDown => self.set_scroll(Scroller::Dialog, scroll + 300.0),
                _ => {}
            }
            return;
        }
        if mods.alt && !mods.ctrl && matches!(name, KeyName::Left | KeyName::Right) {
            self.travel(name == KeyName::Left);
            return;
        }
        if self.ui.find_focused && self.find_key(&name, mods, typed) {
            return;
        }
        if let Some(id) = keys::command_for(&name, mods) {
            self.command(id);
            return;
        }
        // Page keys.
        match (&name, mods.ctrl, mods.shift) {
            (KeyName::Home, ..) => self.ws.scroll_home(),
            (KeyName::End, ..) => self.ws.scroll_end(),
            (KeyName::Up, false, _) => self.ws.scroll_lines(-1.0),
            (KeyName::Down, false, _) => self.ws.scroll_lines(1.0),
            (KeyName::PageUp, ..) | (KeyName::Space, false, true) => self.ws.page_up(),
            (KeyName::PageDown, ..) | (KeyName::Space, false, false) => self.ws.page_down(),
            (KeyName::Char('a'), true, false) => self.ws.select_all(),
            (KeyName::Char('c'), true, false) | (KeyName::Char('\u{3}'), ..) => {
                self.copy();
                true
            }
            _ => false,
        };
    }

    /// Keys for the focused find box. True when the key was used.
    pub(super) fn find_key(&mut self, name: &KeyName, mods: Mods, typed: Option<&str>) -> bool {
        let f = &mut self.ui.find;
        let (word, extend) = (mods.ctrl, mods.shift);
        let changed = match (name, mods.ctrl) {
            (KeyName::Enter, _) => {
                if mods.shift {
                    self.ws.find_previous()
                } else {
                    self.ws.find_next()
                };
                return true;
            }
            (KeyName::Escape, _) => {
                self.escape();
                return true;
            }
            (KeyName::Left, _) => f.key(EditKey::Left, word, extend),
            (KeyName::Right, _) => f.key(EditKey::Right, word, extend),
            (KeyName::Home, _) => f.key(EditKey::Home, false, extend),
            (KeyName::End, _) => f.key(EditKey::End, false, extend),
            (KeyName::Backspace, _) => f.key(EditKey::Backspace, word, false),
            (KeyName::Delete, _) => f.key(EditKey::Delete, word, false),
            (KeyName::Char('a'), true) if !mods.alt => {
                f.select_all();
                false
            }
            (KeyName::Char('c'), true) if !mods.alt => {
                if f.has_selection() {
                    platform::set_clipboard_text(f.selected_text());
                }
                false
            }
            (KeyName::Char('x'), true) if !mods.alt => {
                if f.has_selection() {
                    platform::set_clipboard_text(f.selected_text());
                }
                f.delete_selection()
            }
            (KeyName::Char('v'), true) if !mods.alt => match platform::clipboard_text() {
                Some(t) => f.insert(&t),
                None => false,
            },
            (KeyName::Up | KeyName::Down | KeyName::PageUp | KeyName::PageDown, false) => {
                return false;
            }
            _ => {
                // Typed text; Ctrl alone is a shortcut, Ctrl+Alt is AltGr on Windows.
                let typed = typed.filter(|t| !t.chars().any(char::is_control));
                match typed {
                    Some(t) if !(mods.ctrl && !mods.alt) => f.insert(t),
                    _ => return false,
                }
            }
        };
        if changed {
            self.find_changed();
        }
        self.caret_reset();
        true
    }
}
