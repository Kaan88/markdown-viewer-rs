//! The hover card: when it is wanted, when it shows, and where it is laid out.

use super::*;
use crate::preview::{self, Card, Lookup, Target};

/// How long the pointer rests on a footnote or a link before its card shows; the tooltips' own
/// delay, so every hover helper waits alike.
const CARD_DELAY: Duration = TIP_DELAY;

impl App {
    /// The pointer is on `target` (a footnote mark or a link with something to show), or on
    /// nothing. The same target keeps its card; moving on it before the card shows starts the
    /// wait again, so the card is for a pointer at rest; another target or none drops it.
    pub(super) fn update_card(&mut self, target: Option<Target>) {
        let (x, y) = self.ui.mouse;
        let Some(target) = target else {
            self.ui.card = None;
            return;
        };
        match self.ui.card.as_mut() {
            Some(c) if c.target == target => {
                if !c.shown {
                    c.at = (x, y);
                    c.due = Instant::now() + CARD_DELAY;
                }
            }
            _ => {
                self.ui.card = Some(Card {
                    target,
                    at: (x, y),
                    due: Instant::now() + CARD_DELAY,
                    loading: false,
                    content: None,
                    shown: false,
                });
            }
        }
    }

    /// Makes the card show now (the screenshots), reading a file in place.
    pub(super) fn show_card_now(&mut self) {
        let Some(c) = self.ui.card.as_mut() else {
            return;
        };
        c.due = Instant::now();
        if let Target::File { path, fragment } = &c.target {
            c.content = preview::load(path, fragment.as_deref());
            c.shown = c.content.is_some();
        } else {
            self.resolve_card(Instant::now());
        }
    }

    /// Gets the text of a card that is due: from the page at once, from a file's cache or a
    /// thread. A card with nothing to show is dropped.
    pub(super) fn resolve_card(&mut self, now: Instant) {
        let Some(c) = self.ui.card.as_ref() else {
            return;
        };
        let content = match &c.target {
            Target::File { path, fragment } => match self.ui.previews.get(path, fragment.as_deref())
            {
                Lookup::Ready(p) => p,
                Lookup::Loading => {
                    if let Some(c) = self.ui.card.as_mut() {
                        c.loading = true;
                    }
                    return;
                }
            },
            t => self.ws.preview_text(t),
        };
        match (content, self.ui.card.as_mut()) {
            (Some(p), Some(c)) => {
                c.content = Some(p);
                c.loading = false;
                c.shown = now >= c.due;
                self.wants_frame = true;
            }
            _ => self.ui.card = None,
        }
    }

    /// When the card next needs attention: its rest is up, or nothing while it waits on a thread
    /// or shows.
    pub(super) fn card_timer(&mut self, now: Instant) -> Option<Instant> {
        let c = self.ui.card.as_ref().filter(|c| !c.shown && !c.loading)?;
        if now >= c.due {
            self.resolve_card(now);
            return None;
        }
        Some(c.due)
    }

    /// The card as it is drawn, in a `w` x `h` window.
    pub(super) fn card_layout(&mut self, w: f32, h: f32) -> Option<preview::Layout> {
        let c = self.ui.card.as_ref().filter(|c| c.shown)?;
        let content = c.content.as_ref()?;
        let mut p = Painter::new(&mut self.screen, &mut self.gfx, self.scale);
        Some(preview::layout(&mut p, content, c.at, w, h))
    }
}
