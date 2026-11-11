//! Popup menus drawn inside the window: the main menu, Recent and the right-click menus.

use crate::gfx::{Face, Painter, Rect};
use crate::theme::Palette;

#[derive(Clone, Debug)]
pub struct Item<T> {
    pub id: T,
    pub label: String,
    pub hint: String,
    /// An icon-font glyph, or empty.
    pub icon: &'static str,
    pub enabled: bool,
    pub checked: bool,
    pub separator_before: bool,
}

impl<T> Item<T> {
    pub fn new(id: T, label: impl Into<String>) -> Item<T> {
        Item {
            id,
            label: label.into(),
            hint: String::new(),
            icon: "",
            enabled: true,
            checked: false,
            separator_before: false,
        }
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }
    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }
    pub fn sep(mut self) -> Self {
        self.separator_before = true;
        self
    }
    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
}

const ROW: f32 = 32.0;
const SEP: f32 = 9.0;
const PAD: f32 = 5.0;
const MAX_WIDTH: f32 = 420.0;

pub struct Menu<T> {
    pub items: Vec<Item<T>>,
    /// Where the menu hangs from: its top-left corner, or its top-right when `align_right`.
    anchor: (f32, f32),
    align_right: bool,
    pub hover: Option<usize>,
    rect: Rect,
    rows: Vec<Rect>,
}

impl<T: Clone> Menu<T> {
    pub fn new(items: Vec<Item<T>>, x: f32, y: f32, align_right: bool) -> Menu<T> {
        Menu {
            items,
            anchor: (x, y),
            align_right,
            hover: None,
            rect: Rect::default(),
            rows: Vec::new(),
        }
    }

    /// Places the menu inside a `width` x `height` window. Called by `draw`; call it before to
    /// know [`rect`](Self::rect).
    pub fn place(&mut self, p: &mut Painter, width: f32, height: f32) {
        let icons = self.items.iter().any(|i| !i.icon.is_empty() || i.checked);
        let lead = if icons { 40.0 } else { 14.0 };
        let mut w: f32 = 180.0;
        for item in &self.items {
            let hint = if item.hint.is_empty() {
                0.0
            } else {
                p.measure(Face::Ui, 12.0, &item.hint) + 28.0
            };
            w = w.max(lead + p.measure(Face::Ui, 13.5, &item.label) + hint + 16.0);
        }
        let w = w.min(MAX_WIDTH).min(width - 8.0);
        let h: f32 = 2.0 * PAD
            + self
                .items
                .iter()
                .map(|i| ROW + if i.separator_before { SEP } else { 0.0 })
                .sum::<f32>();
        let mut x = if self.align_right {
            self.anchor.0 - w
        } else {
            self.anchor.0
        };
        let mut y = self.anchor.1;
        x = x.min(width - w - 4.0).max(4.0);
        if y + h > height - 4.0 {
            y = (height - h - 4.0).max(4.0);
        }
        self.rect = Rect::new(x.round(), y.round(), w.round(), h.min(height - 8.0));
        self.rows.clear();
        let mut ry = self.rect.y + PAD;
        for item in &self.items {
            if item.separator_before {
                ry += SEP;
            }
            self.rows.push(Rect::new(
                self.rect.x + PAD,
                ry,
                self.rect.w - 2.0 * PAD,
                ROW,
            ));
            ry += ROW;
        }
    }

    pub fn draw(&mut self, p: &mut Painter, pal: &Palette, width: f32, height: f32) {
        self.place(p, width, height);
        let icons = self.items.iter().any(|i| !i.icon.is_empty() || i.checked);
        p.shadow(self.rect, 8.0, pal.dark);
        p.frame(self.rect, 8.0, pal.popup, pal.popup_line);
        let old = p.clip(self.rect);
        for (i, item) in self.items.iter().enumerate() {
            let r = self.rows[i];
            if item.separator_before {
                let line = Rect::new(
                    self.rect.x + 1.0,
                    r.y - SEP / 2.0 - 0.5,
                    self.rect.w - 2.0,
                    1.0,
                );
                p.fill(line, pal.popup_line);
            }
            if self.hover == Some(i) && item.enabled {
                p.rbox(r, 5.0, pal.hover);
            }
            let color = if item.enabled { pal.text } else { pal.faint };
            let mut x = r.x + 9.0;
            if icons {
                let glyph = if item.checked { "\u{E73E}" } else { item.icon };
                if !glyph.is_empty() {
                    let c = if item.checked {
                        pal.accent
                    } else if item.enabled {
                        pal.muted
                    } else {
                        pal.faint
                    };
                    p.icon(glyph, 14.0, r.x + 18.0, r.cy(), c);
                }
                x = r.x + 36.0;
            }
            let hint_w = if item.hint.is_empty() {
                0.0
            } else {
                p.text_right(
                    Face::Ui,
                    12.0,
                    r.right() - 10.0,
                    r.cy(),
                    &item.hint,
                    pal.faint,
                ) + 18.0
            };
            p.text(
                Face::Ui,
                13.5,
                x,
                r.cy(),
                &item.label,
                color,
                r.right() - x - hint_w - 8.0,
            );
        }
        p.restore(old);
    }

    /// Where the menu was last placed.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.rect.contains(x, y)
    }

    /// Updates the hovered row. Returns true if it changed.
    pub fn pointer(&mut self, x: f32, y: f32) -> bool {
        let hit = self
            .rows
            .iter()
            .position(|r| r.contains(x, y))
            .filter(|&i| self.items[i].enabled);
        let changed = hit != self.hover;
        self.hover = hit;
        changed
    }

    /// Moves the keyboard highlight by one enabled row.
    pub fn step(&mut self, forward: bool) {
        let n = self.items.len();
        if n == 0 {
            return;
        }
        let mut i = self.hover.unwrap_or(if forward { n - 1 } else { 0 });
        for _ in 0..n {
            i = if forward {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            };
            if self.items[i].enabled {
                self.hover = Some(i);
                return;
            }
        }
    }

    /// The highlighted item's id, for a click or Enter.
    pub fn chosen(&self) -> Option<T> {
        self.hover
            .filter(|&i| self.items[i].enabled)
            .map(|i| self.items[i].id.clone())
    }
}
