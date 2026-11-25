//! Presentation mode in the window: starting and leaving it, its keys, pointer and wheel, its
//! frame, and the timer of the slide counter. The slides themselves are `crate::present`.

use super::*;
use crate::present::{Overview, Release, Show};
use mdview_core::commands::strings;
use winit::window::Fullscreen;

/// What a composed frame gives back: the damaged rectangles and what the frame cost.
type Frame = (Vec<(i32, i32, i32, i32)>, FrameTimes);

/// One arrow-key step of a tall slide, logical px (the page's own step).
const LINE: f32 = 48.0;

/// A pointer that moves less than this (logical px) does not wake the counter or the cursor.
const MOVE_SLOP: f32 = 3.0;

impl App {
    /// Whether a presentation is showing.
    pub(super) fn presenting(&self) -> bool {
        self.ui.present.is_some()
    }

    /// Present: starts the active document's slides, or ends them when they are showing.
    pub(super) fn present_toggle(&mut self) {
        if self.presenting() {
            self.present_end();
        } else {
            self.present_begin();
        }
    }

    pub(super) fn present_begin(&mut self) {
        let (tab, revision, source) = match self.ws.present_source() {
            Ok(x) => x,
            Err(e) => {
                self.ws.set_status(&e);
                return;
            }
        };
        let base_dir = self
            .ws
            .active_tab()
            .and_then(|t| t.path().parent().map(Path::to_path_buf));
        let Some(mut show) = Show::new(tab, revision, &source, base_dir) else {
            self.ws.present_end();
            self.ws.set_status(strings::NOTHING_TO_PRESENT);
            return;
        };
        show.set_subpixel(platform::subpixel_order());
        self.ui.popup = None;
        self.ui.dialog = None;
        self.ui.tips.hide();
        self.ui.find_focused = false;
        self.ui.drag = None;
        self.ui.link_hint = None;
        self.ui.present = Some(show);
        self.present_window();
        self.redraw();
    }

    /// The window goes fullscreen for the presentation; what it was is remembered.
    pub(super) fn present_window(&mut self) {
        let (Some(w), Some(show)) = (self.window.as_ref(), self.ui.present.as_mut()) else {
            return;
        };
        show.was_fullscreen = w.fullscreen().is_some();
        if !show.was_fullscreen {
            w.set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        w.set_cursor_visible(true);
    }

    /// Leaves the presentation: the window is as it was, and the page where the reader left it.
    pub(super) fn present_end(&mut self) {
        let Some(show) = self.ui.present.take() else {
            return;
        };
        self.ws.present_end();
        if let Some(w) = &self.window {
            w.set_cursor_visible(true);
            if !show.was_fullscreen {
                w.set_fullscreen(None);
            }
        }
        // Everything under the slide is drawn again.
        self.full = true;
        self.sigs = [0; 5];
        self.overlay_sig = 0;
        self.ws.mark_dirty();
        self.redraw();
    }

    // ---- input -----------------------------------------------------------------------------------

    /// A key while presenting. Escape leaves; the slide keys move; Present and Exit still work as
    /// everywhere; nothing else does.
    pub(super) fn present_key(&mut self, name: KeyName, mods: Mods, typed: Option<&str>) {
        let now = Instant::now();
        let Some(show) = self.ui.present.as_mut() else {
            return;
        };
        // The list of slides takes every key (Present and Exit still work).
        if show.overview_open() {
            if let Some(id @ (C::Present | C::Exit)) = keys::command_for(&name, mods) {
                self.command(id);
                return;
            }
            let out = show.overview_key(&name, mods, typed);
            self.overview_done(out, now);
            return;
        }
        let _ = match name {
            KeyName::Char('g') if !mods.ctrl && !mods.alt => {
                show.open_overview();
                self.redraw();
                return;
            }
            KeyName::Char('a') if mods.ctrl && !mods.alt => {
                show.select_all();
                self.redraw();
                return;
            }
            KeyName::Char('c') if mods.ctrl => {
                self.present_copy();
                return;
            }
            KeyName::Char('\u{3}') => {
                self.present_copy();
                return;
            }
            KeyName::Escape => {
                self.present_end();
                return;
            }
            KeyName::Right | KeyName::Enter => show.next(),
            KeyName::Left | KeyName::Backspace => show.prev(),
            KeyName::PageDown => show.page(true),
            KeyName::PageUp => show.page(false),
            KeyName::Space => show.page(!mods.shift),
            KeyName::Home => show.first(),
            KeyName::End => show.last(),
            KeyName::Down => show.scroll_by(self.scale * LINE),
            KeyName::Up => show.scroll_by(-self.scale * LINE),
            _ => {
                if let Some(id @ (C::Present | C::Exit)) = keys::command_for(&name, mods) {
                    self.command(id);
                }
                return;
            }
        };
        // The counter shows for any slide key, even at an end where nothing moved.
        if !matches!(name, KeyName::Up | KeyName::Down) {
            show.touch(now);
        }
        self.redraw();
    }

    /// What the list of slides did: a chosen slide shows the counter, a closed list is gone.
    fn overview_done(&mut self, out: Overview, now: Instant) {
        if out == Overview::Went
            && let Some(show) = self.ui.present.as_mut()
        {
            show.touch(now);
        }
        self.redraw();
    }

    /// The left button went down: in the list of slides it acts there; on the slide it starts a
    /// selection, a click on a link, or a click that goes on (decided when the button comes up).
    pub(super) fn present_press(&mut self) {
        let ((x, y), mods, scale) = (self.ui.mouse, self.mods, self.scale);
        let Some(show) = self.ui.present.as_mut() else {
            return;
        };
        if show.overview_open() {
            let out = show.overview_press(x, y, mods);
            self.overview_done(out, Instant::now());
        } else {
            show.press_at(x * scale, y * scale);
        }
    }

    /// The left button came up: a plain click goes forward as Space does, a click on a link
    /// follows it, after a drag the selected text stays.
    pub(super) fn present_release(&mut self) {
        let ((x, y), now) = (self.ui.mouse, Instant::now());
        let Some(show) = self.ui.present.as_mut() else {
            return;
        };
        if show.overview_open() {
            let out = show.overview_release(x, y);
            self.overview_done(out, now);
            return;
        }
        match show.release() {
            Release::Advance => {
                show.page(true);
                show.touch(now);
            }
            Release::Link(destination) => self.present_link(&destination),
            Release::Selected | Release::Nothing => {}
        }
        self.redraw();
    }

    /// A link on a slide, followed as on the page. A link to a place in this document goes to
    /// the slide that holds it (the page behind is not moved); any other goes through the
    /// workspace as on the page: a web address opens in the browser, a Markdown file in a tab
    /// (which ends the presentation, its tab no longer being the one shown), and what the page
    /// refuses is refused.
    fn present_link(&mut self, destination: &str) {
        let doc_dir = self
            .ws
            .active_tab()
            .and_then(|t| t.path().parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let is_wiki = destination.starts_with(crate::workspace::WIKI_SCHEME);
        if !is_wiki
            && let mdview_core::links::LinkAction::Fragment(fragment) =
                mdview_core::links::classify_link(destination, &doc_dir)
        {
            let found = self
                .ui
                .present
                .as_mut()
                .is_some_and(|show| show.go_to_fragment(&fragment));
            if found {
                if let Some(show) = self.ui.present.as_mut() {
                    show.touch(Instant::now());
                }
            } else {
                self.ws.set_status(&format!("no anchor named {fragment}"));
            }
            return;
        }
        let outcome = self.ws.open_link(destination);
        self.link_outcome(outcome);
    }

    /// Ctrl+C: the selected text of the slide goes on the clipboard, as the page's does.
    fn present_copy(&mut self) {
        let Some(show) = self.ui.present.as_ref() else {
            return;
        };
        if let Some(text) = show.selected_text()
            && (if cfg!(test) {
                // A test run must not replace what the person at this machine has copied.
                crate::codebtn::to_clipboard(&text)
            } else {
                show.selected_html()
                    .is_some_and(|html| platform::set_clipboard_html(&html, &text))
                    || platform::set_clipboard_text(&text)
            })
        {
            self.ws.confirm("Copied");
        }
    }

    /// The wheel scrolls a slide taller than the window, or the list of slides.
    pub(super) fn present_wheel(&mut self, notches: f32, pixels: Option<f32>) {
        let ((x, y), scale) = (self.ui.mouse, self.scale);
        if let Some(show) = self.ui.present.as_mut() {
            if show.overview_open() {
                let dy = match pixels {
                    Some(py) => -py / scale,
                    None => -notches * PANE_WHEEL,
                };
                show.overview_wheel(x, y, dy);
            } else {
                let dy = match pixels {
                    Some(py) => -py,
                    None => -notches * WHEEL_LINES * LINE * scale,
                };
                show.scroll_by(dy);
            }
        }
        self.redraw();
    }

    /// The pointer moved: it shows again, and the counter with it. With the button down on the
    /// slide it selects; over a link it is a hand.
    pub(super) fn present_pointer(&mut self, x: f32, y: f32) {
        let (px, py) = self.ui.mouse;
        self.ui.mouse = (x, y);
        let scale = self.scale;
        let Some(show) = self.ui.present.as_mut() else {
            return;
        };
        if show.overview_open() {
            show.overview_move(x, y);
            self.set_cursor(CursorIcon::Default);
            self.redraw();
            return;
        }
        if show.pressing() {
            if show.drag_to(x * scale, y * scale) {
                self.redraw();
            }
            return;
        }
        if (px - x).abs() < MOVE_SLOP && (py - y).abs() < MOVE_SLOP {
            return;
        }
        if show.cursor_hidden
            && let Some(w) = &self.window
        {
            w.set_cursor_visible(true);
        }
        show.touch(Instant::now());
        let hand = show.link_at(x * scale, y * scale);
        self.set_cursor(if hand {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        });
        self.redraw();
    }

    // ---- frame and timer -------------------------------------------------------------------------

    /// The frame of a presentation: the slide alone, drawn whole when something about it changed.
    /// `None` when the presentation ended on the way (its tab is gone, or its text has no slides
    /// left), and the frame is the window's.
    pub(super) fn compose_present(&mut self) -> Option<Frame> {
        let started = Instant::now();
        let show = self.ui.present.as_ref()?;
        if self.ws.active() != Some(show.tab) {
            self.present_end();
            return None;
        }
        let (tab, known) = (show.tab, show.revision());
        let changed = self.ws.present_update(tab, known);
        let theme = self.ws.theme();
        let animate = self.ws.animations();
        let (w, h) = (self.screen.width, self.screen.height);
        let now = Instant::now();
        let redraw = std::mem::take(&mut self.full);
        let show = self.ui.present.as_mut()?;
        if let Some((revision, source)) = changed
            && !show.set_source(revision, &source)
        {
            self.present_end();
            self.ws.set_status(strings::NOTHING_TO_PRESENT);
            return None;
        }
        let show = self.ui.present.as_mut()?;
        let mut times = FrameTimes::default();
        if !show.prepare(w, h, self.scale, &theme) {
            return Some((Vec::new(), times));
        }
        times.layout = started.elapsed();
        let signature = show.signature(now, animate, &self.pal);
        if redraw {
            show.forget();
        }
        if show.is_drawn(signature) {
            return Some((Vec::new(), times));
        }
        let t = Instant::now();
        let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
        show.draw(&mut p, &self.pal, now, animate);
        times.paint = t.elapsed();
        // The slide's own scroll may have been corrected by the draw: the next frame agrees.
        let signature = show.signature(now, animate, &self.pal);
        show.is_drawn(signature);
        times.rows = h;
        Some((vec![(0, 0, w as i32, h as i32)], times))
    }

    /// The counter's timer: a frame when it changes, and the pointer goes once it is gone.
    /// Returns when to come back.
    pub(super) fn present_tick(&mut self, now: Instant) -> Option<Instant> {
        let animate = self.ws.animations();
        let show = self.ui.present.as_mut()?;
        let next = show.next_tick(now, animate);
        if next.is_some() || show.counter_alpha(now, animate) > 0.0 {
            self.wants_frame = true;
        }
        if next.is_none() && !show.cursor_hidden {
            show.cursor_hidden = true;
            self.wants_frame = true;
            if let Some(w) = &self.window {
                w.set_cursor_visible(false);
            }
        }
        next
    }
}
