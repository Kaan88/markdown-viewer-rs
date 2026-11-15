//! Presentation mode in the window: starting and leaving it, its keys, pointer and wheel, its
//! frame, and the timer of the slide counter. The slides themselves are `crate::present`.

use super::*;
use crate::present::Show;
use mdview_core::commands::strings;
use winit::window::Fullscreen;

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
        self.ui.tip = None;
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
    pub(super) fn present_key(&mut self, name: KeyName, mods: Mods) {
        let now = Instant::now();
        let Some(show) = self.ui.present.as_mut() else {
            return;
        };
        let _ = match name {
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

    /// A left click goes forward as Space does.
    pub(super) fn present_press(&mut self) {
        if let Some(show) = self.ui.present.as_mut() {
            show.page(true);
            show.touch(Instant::now());
        }
        self.redraw();
    }

    /// The wheel scrolls a slide taller than the window.
    pub(super) fn present_wheel(&mut self, notches: f32, pixels: Option<f32>) {
        if let Some(show) = self.ui.present.as_mut() {
            let dy = match pixels {
                Some(py) => -py,
                None => -notches * WHEEL_LINES * LINE * self.scale,
            };
            show.scroll_by(dy);
        }
        self.redraw();
    }

    /// The pointer moved: it shows again, and the counter with it.
    pub(super) fn present_pointer(&mut self, x: f32, y: f32) {
        let (px, py) = self.ui.mouse;
        self.ui.mouse = (x, y);
        if (px - x).abs() < MOVE_SLOP && (py - y).abs() < MOVE_SLOP {
            return;
        }
        if let Some(show) = self.ui.present.as_mut() {
            if show.cursor_hidden
                && let Some(w) = &self.window
            {
                w.set_cursor_visible(true);
            }
            show.touch(Instant::now());
        }
        self.redraw();
    }

    // ---- frame and timer -------------------------------------------------------------------------

    /// The frame of a presentation: the slide alone, drawn whole when something about it changed.
    /// `None` when the presentation ended on the way (its tab is gone, or its text has no slides
    /// left), and the frame is the window's.
    pub(super) fn compose_present(&mut self) -> Option<(Vec<(i32, i32, i32, i32)>, FrameTimes)> {
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
