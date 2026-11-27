//! Presentation mode: the document as slides, one at a time, in the whole window.
//!
//! The slides are cut by `mdview::slides`. Each is parsed and laid out on its own with the
//! normal page renderer (its own, made when a presentation starts and dropped when it ends, so
//! nothing exists outside the mode), at a zoom chosen so the slide fills the window comfortably:
//! as large as the width allows, as small as the height needs, never below what reads as a page
//! and never above [`MAX_ZOOM`]. A slide shorter than the window is centred; a taller one scrolls.
//! The window's page is not touched, so leaving shows the reader exactly where they were.

use crate::gfx::{Face, Painter, Rect, mix};
use crate::panes::{draw_scrollbar, scrollbar};
use crate::theme::Palette;
use mdshared_ui::keys::{KeyName, Mods};
use mdshared_ui::palette::{Entry, Outcome, Palette as List};
use mdview_core::Document;
use mdview_render::{DocumentRenderer, Options, Theme};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The reading column of a slide, in logical px at zoom 1: wider than a page, so that a slide
/// holds a few lines of big text.
pub const SLIDE_WIDTH: f32 = 860.0;
/// The largest zoom a slide gets.
pub const MAX_ZOOM: f32 = 3.0;
/// A slide is zoomed to fill this share of the window's height, when its width leaves room.
const FIT: f32 = 0.9;
/// A tall slide is never zoomed below this share of the zoom the window's width allows: it
/// scrolls instead.
const SMALLEST: f32 = 0.6;
/// How long the slide counter stays, and then takes to fade.
const HOLD: Duration = Duration::from_millis(1800);
const FADE: Duration = Duration::from_millis(400);
/// The frame rate of the fade.
const TICK: Duration = Duration::from_millis(30);

/// The zoom for a slide whose natural height at zoom 1 is `natural` logical px, in a window of
/// `vw` by `vh` logical px whose pages keep a side margin of `margin`.
pub fn fit_zoom(vw: f32, vh: f32, natural: f32, margin: f32) -> f32 {
    let wide = (vw / (SLIDE_WIDTH + 2.0 * margin)).clamp(1.0, MAX_ZOOM);
    let tall = FIT * vh / natural.max(1.0);
    tall.min(wide).max(wide * SMALLEST).clamp(1.0, MAX_ZOOM)
}

/// What a laid-out slide was made for, and what came of it.
struct Laid {
    key: (
        u32,
        u32,
        u32,
        (&'static str, &'static str, &'static str),
        usize,
        u32,
    ),
    options: Options,
    /// The slide's height, device px.
    total: f32,
    /// The window's height, device px.
    height: f32,
}

/// A button that went down on a slide: where the selection starts, and the link under it.
#[derive(Clone, Copy)]
struct Press {
    anchor: u32,
    link: Option<u32>,
    moved: bool,
}

/// What a button coming up on a slide meant.
#[derive(Debug, PartialEq, Eq)]
pub enum Release {
    /// Nothing was pressed on the slide.
    Nothing,
    /// A drag selected text; it stays selected.
    Selected,
    /// A plain click: the slide goes on.
    Advance,
    /// A click on a link: its destination as written (a wiki-link after `wiki:`).
    Link(String),
}

/// What a key or button did in the slide overview.
#[derive(Debug, PartialEq, Eq)]
pub enum Overview {
    /// The overview is not open.
    Closed,
    /// The list stays open.
    Open,
    /// A slide was chosen; the overview closed and the slide is showing.
    Went,
    /// The overview closed without a choice.
    Dismissed,
}

pub struct Show {
    /// The tab the slides come from, and the revision of its text they were cut from.
    pub tab: usize,
    revision: u64,
    slides: Vec<String>,
    index: usize,
    /// Device px from the slide's top.
    scroll: f32,
    base_dir: Option<PathBuf>,
    renderer: DocumentRenderer,
    /// The slide `index` parsed, and the number of the slide it is.
    parsed: Option<(usize, Document)>,
    laid: Option<Laid>,
    /// Counts the texts the slides were cut from, so a layout of an old one is not reused.
    generation: u32,
    /// The last navigation or pointer movement: the counter shows from then.
    active: Instant,
    /// The pointer is hidden (it comes back when it moves).
    pub cursor_hidden: bool,
    /// The window was fullscreen before the presentation.
    pub was_fullscreen: bool,
    /// The signature of what is on screen.
    drawn: Option<u64>,
    /// The selected text of the slide: offsets into its document's text, in order.
    selection: Option<(u32, u32)>,
    press: Option<Press>,
    /// Where the slide's bitmap is on screen, device px (set by every draw).
    origin: (i32, i32),
    /// The list of slides, while it is open: the shell's palette widget, a slide per row.
    overview: Option<List>,
    /// Counts what the overview did, so a changed list is drawn.
    overview_rev: u32,
}

impl Show {
    /// A presentation of `source`; `None` when it has no slides.
    pub fn new(tab: usize, revision: u64, source: &str, base_dir: Option<PathBuf>) -> Option<Show> {
        let slides = mdview::slides::split(source);
        if slides.is_empty() {
            return None;
        }
        #[cfg(test)]
        crate::test_engine::require_engine();
        Some(Show {
            tab,
            revision,
            slides,
            index: 0,
            scroll: 0.0,
            base_dir,
            renderer: DocumentRenderer::new(),
            parsed: None,
            laid: None,
            generation: 0,
            active: Instant::now(),
            cursor_hidden: false,
            was_fullscreen: false,
            drawn: None,
            selection: None,
            press: None,
            origin: (0, 0),
            overview: None,
            overview_rev: 0,
        })
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The text changed: cuts it again and stays on the same slide number (the last one when
    /// there are fewer now). False, and nothing changed, when it has no slides any more.
    pub fn set_source(&mut self, revision: u64, source: &str) -> bool {
        let slides = mdview::slides::split(source);
        if slides.is_empty() {
            return false;
        }
        self.revision = revision;
        self.slides = slides;
        self.index = self.index.min(self.slides.len() - 1);
        self.parsed = None;
        self.laid = None;
        self.selection = None;
        self.press = None;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// The renderer is told the display's subpixel order, as the page's is.
    pub fn set_subpixel(&mut self, subpixel: mdview_render::Subpixel) {
        self.renderer.set_subpixel(subpixel);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.slides.len()
    }

    #[cfg(test)]
    pub fn index(&self) -> usize {
        self.index
    }

    /// The Markdown of slide `i`, for tests.
    #[cfg(test)]
    pub fn slide(&self, i: usize) -> &str {
        &self.slides[i]
    }

    /// The scroll offset of the slide, device px.
    #[cfg(test)]
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    // ---- moving --------------------------------------------------------------------------------

    /// Shows slide `i` (clamped), from its top. True when that is another slide.
    pub fn go(&mut self, i: usize) -> bool {
        let i = i.min(self.slides.len() - 1);
        if i == self.index {
            return false;
        }
        self.index = i;
        self.scroll = 0.0;
        self.selection = None;
        self.press = None;
        true
    }

    pub fn next(&mut self) -> bool {
        self.go(self.index + 1)
    }

    pub fn prev(&mut self) -> bool {
        self.go(self.index.saturating_sub(1))
    }

    pub fn first(&mut self) -> bool {
        self.go(0)
    }

    pub fn last(&mut self) -> bool {
        self.go(self.slides.len() - 1)
    }

    /// The farthest the slide scrolls, device px; 0 until it is laid out or when it fits.
    pub fn max_scroll(&self) -> f32 {
        self.laid
            .as_ref()
            .map_or(0.0, |l| (l.total - l.height).max(0.0))
    }

    /// Scrolls the slide by `dy` device px. True when it moved.
    pub fn scroll_by(&mut self, dy: f32) -> bool {
        let next = (self.scroll + dy).clamp(0.0, self.max_scroll()).round();
        let moved = next != self.scroll;
        self.scroll = next;
        moved
    }

    /// Space, Page Down and a click go forward as a reader would: a slide taller than the window
    /// first scrolls a page, and the next slide comes when its end is showing. `down` false goes
    /// back the same way, to the top and then to the previous slide.
    pub fn page(&mut self, down: bool) -> bool {
        let page = self.laid.as_ref().map_or(0.0, |l| l.height * 0.9);
        if down {
            if self.scroll < self.max_scroll() - 0.5 {
                return self.scroll_by(page);
            }
            self.next()
        } else {
            if self.scroll > 0.0 {
                return self.scroll_by(-page);
            }
            self.prev()
        }
    }

    // ---- the counter ---------------------------------------------------------------------------

    /// Something happened that shows the counter: a slide changed, a key was pressed, the pointer
    /// moved.
    pub fn touch(&mut self, now: Instant) {
        self.active = now;
        self.cursor_hidden = false;
    }

    /// How visible the counter is: 1, fading to 0. Without animation it goes at once.
    pub fn counter_alpha(&self, now: Instant, animate: bool) -> f32 {
        let since = now.saturating_duration_since(self.active);
        if since < HOLD {
            1.0
        } else if animate && since < HOLD + FADE {
            1.0 - (since - HOLD).as_secs_f32() / FADE.as_secs_f32()
        } else {
            0.0
        }
    }

    /// When the counter next changes by itself: at the end of its hold, then at each step of the
    /// fade. `None` once it is gone.
    pub fn next_tick(&self, now: Instant, animate: bool) -> Option<Instant> {
        let since = now.saturating_duration_since(self.active);
        if since < HOLD {
            Some(self.active + HOLD)
        } else if animate && since < HOLD + FADE {
            Some(now + TICK)
        } else {
            None
        }
    }

    // ---- laying out and drawing ----------------------------------------------------------------

    /// Lays the slide out for a window of `w` by `h` device px at display `scale` in `theme`,
    /// unless it already is. False for an empty window.
    pub fn prepare(&mut self, w: u32, h: u32, scale: f32, theme: &Theme) -> bool {
        if w == 0 || h == 0 || !(scale.is_finite() && scale > 0.0) {
            return false;
        }
        let key = (
            w,
            h,
            scale.to_bits(),
            theme.key(),
            self.index,
            self.generation,
        );
        if self.laid.as_ref().is_some_and(|l| l.key == key) {
            return true;
        }
        if self.parsed.as_ref().map(|p| p.0) != Some(self.index) {
            self.parsed = Some((self.index, mdview_core::parse(&self.slides[self.index])));
        }
        let Some((_, doc)) = self.parsed.as_ref() else {
            return false;
        };
        let mut theme = *theme;
        theme.reading_width = SLIDE_WIDTH;
        let (vw, vh) = (w as f32 / scale, h as f32 / scale);
        let mut options = Options {
            width: vw,
            scale,
            theme,
            max_height: None,
            base_dir: self.base_dir.clone(),
        };
        // The slide as it is at zoom 1 says how big it can be made.
        let natural = self.renderer.layout_exact(doc, &options).total_height() / scale;
        let zoom = fit_zoom(vw, vh, natural, theme.margin);
        if (zoom - 1.0).abs() > 0.005 {
            options.width = vw / zoom;
            options.scale = scale * zoom;
            self.renderer.layout_exact(doc, &options);
        }
        let total = self
            .renderer
            .page_layout()
            .map_or(0.0, |l| l.total_height());
        self.laid = Some(Laid {
            key,
            options,
            total,
            height: h as f32,
        });
        self.scroll = self.scroll.clamp(0.0, (total - h as f32).max(0.0)).round();
        true
    }

    /// A signature of what [`draw`](Self::draw) would put on screen, so an unchanged slide is
    /// not drawn again.
    pub fn signature(&self, now: Instant, animate: bool, pal: &Palette) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let laid = self.laid.as_ref().map(|l| l.key);
        (
            laid,
            self.scroll.to_bits(),
            (self.counter_alpha(now, animate) * 16.0).round() as u32,
            self.slides.len(),
            pal.dark,
            self.selection,
            self.overview.is_some().then_some(self.overview_rev),
        )
            .hash(&mut h);
        h.finish()
    }

    /// Whether `signature` is the one drawn last; remembers it when it is not.
    pub fn is_drawn(&mut self, signature: u64) -> bool {
        let same = self.drawn == Some(signature);
        self.drawn = Some(signature);
        same
    }

    /// Forgets what is on screen, so the next frame draws.
    pub fn forget(&mut self) {
        self.drawn = None;
    }

    /// Draws the slide, centred when it is shorter than the window, its scroll bar when it is
    /// taller, and the counter over its bottom right corner.
    pub fn draw(&mut self, p: &mut Painter, pal: &Palette, now: Instant, animate: bool) {
        let alpha = self.counter_alpha(now, animate);
        let (count, total) = (self.index + 1, self.slides.len());
        let (Some((_, doc)), Some(laid)) = (self.parsed.as_ref(), self.laid.as_ref()) else {
            return;
        };
        let bg = laid.options.theme.bg;
        let (w, h) = (p.bmp.width as i32, p.bmp.height as i32);
        p.fill_dev(0, 0, w, h, bg, 1.0);
        let rows = laid.total.min(laid.height).max(1.0);
        let frame = self
            .renderer
            .view_frame(doc, &laid.options, self.scroll, rows);
        self.scroll = frame.top;
        let bmp = &frame.bitmap;
        let x = ((w - bmp.width as i32) / 2).max(0);
        let y = ((h as f32 - rows) / 2.0).floor().max(0.0) as i32;
        p.blit(bmp, 0, bmp.height as i32, x, y);
        self.origin = (x, y);
        if let Some((from, to)) = self.selection {
            let (lo, hi) = (y, y + rows as i32);
            let a = if pal.dark { 0.42 } else { 0.30 };
            for r in self.renderer.selection_rects(doc, &laid.options, from, to) {
                let top = (y as f32 + r.y - self.scroll).round() as i32;
                let bottom = (y as f32 + r.y + r.h - self.scroll).round() as i32;
                let left = (x as f32 + r.x).round() as i32;
                let right = (x as f32 + r.x + r.w).round() as i32;
                p.fill_dev(left, top.max(lo), right, bottom.min(hi), pal.selection, a);
            }
        }
        let scale = p.scale;
        let (lw, lh) = (w as f32 / scale, h as f32 / scale);
        let area = Rect::new(0.0, 0.0, lw, lh);
        if let Some((track, thumb)) = scrollbar(area, laid.total, laid.height, self.scroll) {
            draw_scrollbar(p, pal, track, thumb, false);
        }
        if alpha > 0.0 {
            let label = format!("{count} / {total}");
            let tw = p.measure(Face::Ui, 13.0, &label);
            let pill = Rect::new(lw - tw - 28.0 - 20.0, lh - 28.0 - 18.0, tw + 28.0, 28.0);
            p.rbox_a(pill, 14.0, pal.popup, 0.94 * alpha);
            p.text_center(
                Face::Ui,
                13.0,
                pill,
                &label,
                mix(pal.popup, pal.text, alpha),
            );
        }
        if let Some(list) = self.overview.as_mut() {
            // The caret is drawn solid: a blinking one would need a timer.
            list.draw(p, pal, area, 0.0, true);
        }
    }

    // ---- selecting and clicking ----------------------------------------------------------------

    /// What is under the pointer, `x, y` in device px of the window.
    pub fn hit(&mut self, x: f32, y: f32) -> Option<mdview_render::Hit> {
        let (Some((_, doc)), Some(laid)) = (self.parsed.as_ref(), self.laid.as_ref()) else {
            return None;
        };
        let (px, py) = (
            x - self.origin.0 as f32,
            y - self.origin.1 as f32 + self.scroll,
        );
        self.renderer
            .hit_test(doc, &laid.options, px, py.clamp(0.0, laid.total))
    }

    /// Whether a link is under the pointer.
    pub fn link_at(&mut self, x: f32, y: f32) -> bool {
        self.hit(x, y).is_some_and(|h| h.link.is_some())
    }

    /// The left button went down at `x, y` (device px).
    pub fn press_at(&mut self, x: f32, y: f32) {
        self.selection = None;
        let hit = self.hit(x, y);
        self.press = Some(Press {
            anchor: hit.map_or(0, |h| h.offset),
            link: hit.and_then(|h| h.link),
            moved: false,
        });
    }

    /// The pointer moved with the button down: extends the selection. True when it changed.
    pub fn drag_to(&mut self, x: f32, y: f32) -> bool {
        let Some(press) = self.press else {
            return false;
        };
        let Some(hit) = self.hit(x, y) else {
            return false;
        };
        if hit.offset == press.anchor && !press.moved {
            return false;
        }
        if let Some(press) = self.press.as_mut() {
            press.moved = true;
        }
        let selection = Some((press.anchor.min(hit.offset), press.anchor.max(hit.offset)));
        let changed = self.selection != selection;
        self.selection = selection;
        changed
    }

    /// Whether the button is down on the slide.
    pub fn pressing(&self) -> bool {
        self.press.is_some()
    }

    /// The press is let go without a release (the window lost the focus with the button down):
    /// nothing is followed or advanced, and a selection already dragged out stays.
    pub fn cancel_press(&mut self) {
        self.press = None;
    }

    /// The left button came up.
    pub fn release(&mut self) -> Release {
        let Some(press) = self.press.take() else {
            return Release::Nothing;
        };
        if press.moved {
            return Release::Selected;
        }
        self.selection = None;
        let link = press.link.and_then(|i| {
            let (_, doc) = self.parsed.as_ref()?;
            let l = doc.links.get(i as usize)?;
            let dest = doc.str(l.dest);
            Some(if l.wiki {
                format!("{}{dest}", crate::workspace::WIKI_SCHEME)
            } else {
                dest.to_string()
            })
        });
        match link {
            Some(dest) => Release::Link(dest),
            None => Release::Advance,
        }
    }

    /// The selected text of the slide, as the page copies it.
    pub fn selected_text(&self) -> Option<String> {
        let (from, to) = self.selection?;
        let (_, doc) = self.parsed.as_ref()?;
        doc.text.get(from as usize..to as usize).map(str::to_string)
    }

    /// The selection as an HTML fragment, to go beside the text on the clipboard.
    pub fn selected_html(&self) -> Option<String> {
        let (from, to) = self.selection?;
        let (_, doc) = self.parsed.as_ref()?;
        mdview_core::rich::selection_html(doc, from, to)
    }

    /// Selects the whole text of the slide (Ctrl+A). False when it has none.
    pub fn select_all(&mut self) -> bool {
        if self.parsed.as_ref().map(|p| p.0) != Some(self.index) {
            self.parsed = Some((self.index, mdview_core::parse(&self.slides[self.index])));
        }
        let len = self
            .parsed
            .as_ref()
            .map_or(0, |(_, doc)| doc.text.len() as u32);
        self.selection = (len > 0).then_some((0, len));
        self.selection.is_some()
    }

    #[cfg(test)]
    pub fn selection(&self) -> Option<(u32, u32)> {
        self.selection
    }

    /// Goes to the slide that holds the anchor `fragment` (as a link wrote it): this one first,
    /// then the others in order. False when no slide has it.
    pub fn go_to_fragment(&mut self, fragment: &str) -> bool {
        let n = self.slides.len();
        for k in 0..n {
            let i = (self.index + k) % n;
            let found = match self.parsed.as_ref() {
                Some((at, doc)) if *at == i => doc.resolve_fragment(fragment).is_some(),
                _ => mdview_core::parse(&self.slides[i])
                    .resolve_fragment(fragment)
                    .is_some(),
            };
            if found {
                self.go(i);
                return true;
            }
        }
        false
    }

    // ---- the overview --------------------------------------------------------------------------

    /// The title of each slide: its first heading, else its first line of text.
    pub fn titles(&self) -> Vec<String> {
        self.slides
            .iter()
            .map(|slide| {
                let doc = mdview_core::parse(slide);
                let title = match doc.headings.first() {
                    Some(h) => doc.str(h.title).to_string(),
                    None => doc
                        .text
                        .lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty())
                        .unwrap_or_default()
                        .to_string(),
                };
                title.chars().take(120).collect()
            })
            .collect()
    }

    pub fn overview_open(&self) -> bool {
        self.overview.is_some()
    }

    /// Opens the list of slides, on the one showing.
    pub fn open_overview(&mut self) {
        let mut list = List::new();
        let entries = self
            .titles()
            .into_iter()
            .enumerate()
            .map(|(i, title)| {
                let title = if title.is_empty() {
                    "(no title)".to_string()
                } else {
                    title
                };
                Entry::new(i as u32, format!("{}. {title}", i + 1))
            })
            .collect();
        list.set_placeholder("Go to slide: a number or a word of its title");
        list.set_entries(entries);
        list.open("");
        for _ in 0..self.index {
            list.key(&KeyName::Down, Mods::default(), None);
        }
        self.overview = Some(list);
        self.overview_rev = self.overview_rev.wrapping_add(1);
    }

    fn overview_outcome(&mut self, out: Outcome) -> Overview {
        match out {
            Outcome::Chosen(id) => {
                self.overview = None;
                self.go(id as usize);
                Overview::Went
            }
            Outcome::Closed => {
                self.overview = None;
                Overview::Dismissed
            }
            Outcome::None | Outcome::Unused => Overview::Open,
            _ => {
                self.overview_rev = self.overview_rev.wrapping_add(1);
                Overview::Open
            }
        }
    }

    /// A key while the overview is open: Up and Down move, Enter goes to the slide, Escape closes
    /// the list, anything typed narrows it.
    pub fn overview_key(&mut self, name: &KeyName, mods: Mods, typed: Option<&str>) -> Overview {
        let Some(list) = self.overview.as_mut() else {
            return Overview::Closed;
        };
        let out = list.key(name, mods, typed);
        self.overview_outcome(out)
    }

    /// The pointer moved, `x, y` in logical px.
    pub fn overview_move(&mut self, x: f32, y: f32) {
        if let Some(list) = self.overview.as_mut()
            && list.pointer_move(x, y)
        {
            self.overview_rev = self.overview_rev.wrapping_add(1);
        }
    }

    pub fn overview_press(&mut self, x: f32, y: f32, mods: Mods) -> Overview {
        let Some(list) = self.overview.as_mut() else {
            return Overview::Closed;
        };
        let out = list.press(x, y, mods, 1);
        self.overview_outcome(out)
    }

    pub fn overview_release(&mut self, x: f32, y: f32) -> Overview {
        let Some(list) = self.overview.as_mut() else {
            return Overview::Closed;
        };
        let out = list.release(x, y);
        self.overview_outcome(out)
    }

    /// The wheel, `dy` logical px towards the end.
    pub fn overview_wheel(&mut self, x: f32, y: f32, dy: f32) {
        if let Some(list) = self.overview.as_mut()
            && list.contains(x, y)
            && list.wheel(dy)
        {
            self.overview_rev = self.overview_rev.wrapping_add(1);
        }
    }

    /// The slide number (0-based) the overview's row is on, for tests.
    #[cfg(test)]
    pub fn overview_selected(&self) -> Option<usize> {
        let id = self.overview.as_ref()?.selected_id()?;
        Some(id as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_fits_the_window_and_stays_in_bounds() {
        let m = 40.0;
        // A short slide in a wide window: as large as the width allows.
        let z = fit_zoom(1280.0, 800.0, 200.0, m);
        assert!((z - 1280.0 / 940.0).abs() < 0.001, "{z}");
        // One that is tall for the window shrinks to fit, but not below 60% of that.
        let z = fit_zoom(1280.0, 800.0, 700.0, m);
        assert!((z - 0.9 * 800.0 / 700.0).abs() < 0.001, "{z}");
        let z = fit_zoom(1280.0, 800.0, 5000.0, m);
        assert_eq!(z, 1.0);
        // A narrow window never zooms below the page's own size.
        assert_eq!(fit_zoom(600.0, 900.0, 300.0, m), 1.0);
        // A huge one stops at the largest.
        assert_eq!(fit_zoom(7000.0, 4000.0, 300.0, m), MAX_ZOOM);
    }

    fn show(src: &str) -> Show {
        Show::new(0, 1, src, None).expect("slides")
    }

    #[test]
    fn navigation_is_clamped() {
        let mut s = show("one\n\n---\n\ntwo\n\n---\n\nthree\n");
        assert_eq!((s.len(), s.index()), (3, 0));
        assert!(!s.prev());
        assert!(s.next() && s.next());
        assert!(!s.next());
        assert_eq!(s.index(), 2);
        assert!(s.first() && s.index() == 0);
        assert!(s.last() && s.index() == 2);
        assert!(!s.go(99) && s.index() == 2);
    }

    #[test]
    fn a_changed_text_keeps_the_slide_number() {
        let mut s = show("a\n\n---\n\nb\n\n---\n\nc\n");
        s.go(1);
        assert!(s.set_source(2, "x\n\n---\n\ny\n\n---\n\nz\n\n---\n\nw\n"));
        assert_eq!((s.index(), s.len(), s.revision()), (1, 4, 2));
        assert_eq!(s.slide(1).trim(), "y");
        // Fewer slides: the last one.
        s.go(3);
        assert!(s.set_source(3, "p\n\n---\n\nq\n"));
        assert_eq!(s.index(), 1);
        // Nothing left to show: refused, and the old slides stay.
        assert!(!s.set_source(4, "---\n\n---\n"));
        assert_eq!((s.index(), s.len(), s.revision()), (1, 2, 3));
    }

    #[test]
    fn the_counter_holds_then_fades() {
        let s = show("a\n");
        let t = s.active;
        assert_eq!(s.counter_alpha(t, true), 1.0);
        assert_eq!(
            s.counter_alpha(t + HOLD - Duration::from_millis(1), true),
            1.0
        );
        let mid = s.counter_alpha(t + HOLD + FADE / 2, true);
        assert!((mid - 0.5).abs() < 0.01, "{mid}");
        assert_eq!(s.counter_alpha(t + HOLD + FADE, true), 0.0);
        // Without animation it goes at once.
        assert_eq!(s.counter_alpha(t + HOLD, false), 0.0);
        assert_eq!(s.next_tick(t, true), Some(t + HOLD));
        assert!(s.next_tick(t + HOLD, true).is_some());
        assert_eq!(s.next_tick(t + HOLD + FADE, true), None);
        assert_eq!(s.next_tick(t + HOLD, false), None);
    }

    #[test]
    fn a_tall_slide_scrolls_before_the_next_comes() {
        let tall: String = (0..60).map(|i| format!("line {i}\n\n")).collect();
        let mut s = show(&format!("{tall}\n---\n\nnext\n"));
        assert!(s.prepare(1000, 600, 1.0, &mdview_render::LIGHT));
        let max = s.max_scroll();
        assert!(max > 0.0, "the slide is taller than the window");
        assert!(s.scroll_by(100.0) && s.scroll() == 100.0);
        assert!(s.scroll_by(-500.0) && s.scroll() == 0.0);
        assert!(!s.scroll_by(-1.0));
        // Paging goes down the slide, then to the next.
        let mut pages = 0;
        while s.index() == 0 {
            assert!(s.page(true));
            pages += 1;
            assert!(pages < 100);
        }
        assert!(pages > 1);
        assert_eq!(s.scroll(), 0.0, "the next slide starts at its top");
        assert!(s.page(false) && s.index() == 0);
    }
}
