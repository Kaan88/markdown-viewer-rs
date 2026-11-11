//! A single-line text field for the hand-drawn UI. It owns the text, the caret, the selection and
//! the horizontal scroll; the app feeds it keys, pasted text and mouse positions, and draws it with
//! a [`Painter`]. No clipboard or windowing code lives here.

use crate::gfx::{Face, Painter, Rect, Rgb};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditKey {
    Left,
    Right,
    Home,
    End,
    Backspace,
    Delete,
}

/// Colours the field is drawn with.
pub struct FieldColors {
    pub text: Rgb,
    pub placeholder: Rgb,
    /// Drawn at about 0.35 alpha behind the text.
    pub selection: Rgb,
    pub caret: Rgb,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Other,
}

fn class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Other
    }
}

#[derive(Default)]
pub struct TextField {
    text: String,
    /// Byte offset of the caret (always on a char boundary).
    caret: usize,
    /// Byte offset of the other end of the selection; equal to `caret` when nothing is selected.
    anchor: usize,
    /// Horizontal scroll in logical px.
    scroll: f32,
}

impl TextField {
    pub fn new() -> TextField {
        TextField::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Caret and selection anchor, byte offsets, for knowing when a redraw is due.
    pub fn caret(&self) -> (usize, usize) {
        (self.caret, self.anchor)
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Replaces the text, caret at the end, no selection.
    pub fn set_text(&mut self, text: &str) {
        self.text = clean(text);
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.scroll = 0.0;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.anchor = 0;
        self.scroll = 0.0;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    fn sel_range(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    pub fn selected_text(&self) -> &str {
        let (a, b) = self.sel_range();
        &self.text[a..b]
    }

    /// Inserts typed or pasted text at the caret, replacing the selection. Control characters are
    /// dropped; newlines and tabs become spaces. Returns true if the text changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let clean = clean(text);
        if clean.is_empty() && !self.has_selection() {
            return false;
        }
        let (a, b) = self.sel_range();
        self.text.replace_range(a..b, &clean);
        self.caret = a + clean.len();
        self.anchor = self.caret;
        true
    }

    /// Deletes the selection (for Cut). Returns true if the text changed.
    pub fn delete_selection(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        let (a, b) = self.sel_range();
        self.text.replace_range(a..b, "");
        self.caret = a;
        self.anchor = a;
        true
    }

    fn prev_char(&self, i: usize) -> usize {
        self.text[..i]
            .char_indices()
            .next_back()
            .map_or(0, |(p, _)| p)
    }

    fn next_char(&self, i: usize) -> usize {
        self.text[i..]
            .chars()
            .next()
            .map_or(i, |c| i + c.len_utf8())
    }

    /// The start of the word before `i`: skip whitespace, then one run of the same class.
    fn prev_word(&self, i: usize) -> usize {
        let mut pos = i;
        let mut chars = self.text[..i].char_indices().rev().peekable();
        while let Some(&(p, c)) = chars.peek() {
            if class(c) != Class::Space {
                break;
            }
            pos = p;
            chars.next();
        }
        let Some(&(_, first)) = chars.peek() else {
            return pos;
        };
        let k = class(first);
        for (p, c) in chars {
            if class(c) != k {
                break;
            }
            pos = p;
        }
        pos
    }

    /// The end of the word after `i`: skip whitespace, then one run of the same class.
    fn next_word(&self, i: usize) -> usize {
        let mut pos = i;
        let mut chars = self.text[i..].chars().peekable();
        while let Some(&c) = chars.peek() {
            if class(c) != Class::Space {
                break;
            }
            pos += c.len_utf8();
            chars.next();
        }
        let Some(&first) = chars.peek() else {
            return pos;
        };
        let k = class(first);
        for c in chars {
            if class(c) != k {
                break;
            }
            pos += c.len_utf8();
        }
        pos
    }

    /// Caret movement and deletion. `word` is Ctrl held (move/delete by word), `extend` is Shift
    /// held (extend the selection; ignored for Backspace/Delete). Left/Right without Shift collapse
    /// a selection to its start/end. Returns true if the TEXT changed.
    pub fn key(&mut self, key: EditKey, word: bool, extend: bool) -> bool {
        match key {
            EditKey::Left | EditKey::Right | EditKey::Home | EditKey::End => {
                let (a, b) = self.sel_range();
                let to = match key {
                    EditKey::Left if !extend && a != b => a,
                    EditKey::Right if !extend && a != b => b,
                    EditKey::Left if word => self.prev_word(self.caret),
                    EditKey::Left => self.prev_char(self.caret),
                    EditKey::Right if word => self.next_word(self.caret),
                    EditKey::Right => self.next_char(self.caret),
                    EditKey::Home => 0,
                    _ => self.text.len(),
                };
                self.caret = to;
                if !extend {
                    self.anchor = to;
                }
                false
            }
            EditKey::Backspace | EditKey::Delete => {
                if self.has_selection() {
                    return self.delete_selection();
                }
                let to = match (key, word) {
                    (EditKey::Backspace, true) => self.prev_word(self.caret),
                    (EditKey::Backspace, false) => self.prev_char(self.caret),
                    (_, true) => self.next_word(self.caret),
                    _ => self.next_char(self.caret),
                };
                if to == self.caret {
                    return false;
                }
                let (a, b) = (to.min(self.caret), to.max(self.caret));
                self.text.replace_range(a..b, "");
                self.caret = a;
                self.anchor = a;
                true
            }
        }
    }

    /// Byte offset of the char boundary nearest to `x` (text-relative, scroll already applied).
    fn offset_at(&self, x: f32, stops: &[f32]) -> usize {
        let n = self.text.chars().count() + 1;
        let stops = &stops[..stops.len().min(n)];
        let mut best = 0;
        let mut best_d = f32::INFINITY;
        for (i, &s) in stops.iter().enumerate() {
            let d = (s - x).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        self.text
            .char_indices()
            .nth(best)
            .map_or(self.text.len(), |(p, _)| p)
    }

    /// Mouse press at `x` logical px from the left edge of the text area (before scroll; the field
    /// adds its own scroll). `stops` is `Painter::caret_stops` for the current text. `extend` is
    /// Shift.
    pub fn press(&mut self, x: f32, stops: &[f32], extend: bool) {
        let at = self.offset_at(x + self.scroll, stops);
        self.caret = at;
        if !extend {
            self.anchor = at;
        }
    }

    /// Mouse drag to `x`: extends the selection from the press anchor.
    pub fn drag(&mut self, x: f32, stops: &[f32]) {
        self.caret = self.offset_at(x + self.scroll, stops);
    }

    /// Double click: selects the word (or the run of spaces) at `x`.
    pub fn select_word_at(&mut self, x: f32, stops: &[f32]) {
        if self.text.is_empty() {
            return;
        }
        let x = x + self.scroll;
        let count = self.text.chars().count();
        let stops = &stops[..stops.len().min(count + 1)];
        // The char whose span holds x.
        let mut idx = 0;
        for (i, &s) in stops.iter().enumerate().take(count) {
            if s <= x {
                idx = i;
            }
        }
        let (start, c) = self.text.char_indices().nth(idx).unwrap();
        let k = class(c);
        let mut a = start;
        for (p, ch) in self.text[..start].char_indices().rev() {
            if class(ch) != k {
                break;
            }
            a = p;
        }
        let mut b = start;
        for ch in self.text[start..].chars() {
            if class(ch) != k {
                break;
            }
            b += ch.len_utf8();
        }
        self.anchor = a;
        self.caret = b;
    }

    /// Draws text, selection and caret inside `area` (the text area, already inset from the box
    /// the caller drew), clipped to it and vertically centred, scrolling horizontally so the caret
    /// stays visible. A selection and the caret (when `caret_on`) show only when `focused`. When
    /// the text is empty, `placeholder` is drawn in the placeholder colour.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        p: &mut Painter,
        area: Rect,
        face: Face,
        size: f32,
        colors: &FieldColors,
        placeholder: &str,
        focused: bool,
        caret_on: bool,
    ) {
        let stops = p.caret_stops(face, size, &self.text);
        let total = stops.last().copied().unwrap_or(0.0);
        let caret_idx = self.text[..self.caret].chars().count().min(stops.len() - 1);
        let anchor_idx = self.text[..self.anchor]
            .chars()
            .count()
            .min(stops.len() - 1);
        let cx = stops[caret_idx];

        // Keep the caret in view, then keep the scroll within the text.
        let max_scroll = (total + 1.0 - area.w).max(0.0);
        if cx - self.scroll > area.w - 1.0 {
            self.scroll = cx - (area.w - 1.0);
        }
        if cx < self.scroll {
            self.scroll = cx;
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);

        let cy = area.cy();
        let h = size * 1.25;
        let top = cy - h / 2.0;
        let old = p.clip(area);
        let ox = area.x - self.scroll;
        if self.text.is_empty() {
            p.text(
                face,
                size,
                area.x,
                cy,
                placeholder,
                colors.placeholder,
                area.w,
            );
        } else {
            if focused && caret_idx != anchor_idx {
                let (a, b) = (
                    stops[caret_idx.min(anchor_idx)],
                    stops[caret_idx.max(anchor_idx)],
                );
                p.fill_a(Rect::new(ox + a, top, b - a, h), colors.selection, 0.35);
            }
            p.text(face, size, ox, cy, &self.text, colors.text, f32::INFINITY);
        }
        if focused && caret_on {
            let x = if self.text.is_empty() {
                area.x
            } else {
                ox + cx
            };
            p.fill(Rect::new(x, top, 1.0f32.max(p.px()), h), colors.caret);
        }
        p.restore(old);
    }
}

/// Drops control characters; newlines and tabs become spaces.
fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(s: &str) -> TextField {
        let mut f = TextField::new();
        f.set_text(s);
        f
    }

    /// 10 px per char.
    fn stops(s: &str) -> Vec<f32> {
        (0..=s.chars().count()).map(|i| i as f32 * 10.0).collect()
    }

    fn at(f: &mut TextField, caret: usize) {
        f.caret = caret;
        f.anchor = caret;
    }

    #[test]
    fn insert_and_replace_selection() {
        let mut f = TextField::new();
        assert!(f.insert("hello world"));
        assert_eq!(f.text(), "hello world");
        f.select_all();
        assert_eq!(f.selected_text(), "hello world");
        assert!(f.insert("x"));
        assert_eq!(f.text(), "x");
        assert!(!f.has_selection());
        at(&mut f, 0);
        f.insert("ab");
        assert_eq!(f.text(), "abx");
        assert_eq!(f.caret, 2);
        assert!(!f.insert(""));
    }

    #[test]
    fn control_characters_filtered() {
        let mut f = TextField::new();
        f.insert("a\tb\nc\r\nd\u{0}e\u{7f}\u{1b}f");
        assert_eq!(f.text(), "a b c def");
        assert!(!f.insert("\u{0}\u{1}"));
        f.set_text("x\ny");
        assert_eq!(f.text(), "x y");
    }

    #[test]
    fn backspace_delete_at_ends() {
        let mut f = field("ab");
        assert!(!f.key(EditKey::Delete, false, false));
        assert!(f.key(EditKey::Backspace, false, false));
        assert_eq!(f.text(), "a");
        at(&mut f, 0);
        assert!(!f.key(EditKey::Backspace, false, false));
        assert!(f.key(EditKey::Delete, false, false));
        assert_eq!(f.text(), "");
        assert!(!f.key(EditKey::Delete, true, false));
        assert!(!f.key(EditKey::Backspace, true, false));
        assert!(!f.delete_selection());
    }

    #[test]
    fn backspace_deletes_selection() {
        let mut f = field("hello");
        f.key(EditKey::Left, false, true);
        f.key(EditKey::Left, false, true);
        assert!(f.key(EditKey::Backspace, false, false));
        assert_eq!(f.text(), "hel");
        f.select_all();
        assert!(f.key(EditKey::Delete, true, true));
        assert!(f.is_empty());
    }

    #[test]
    fn word_movement() {
        let mut f = field("foo  bar_1 .. baz");
        f.key(EditKey::Left, true, false);
        assert_eq!(f.caret, 14);
        f.key(EditKey::Left, true, false);
        assert_eq!(f.caret, 11);
        f.key(EditKey::Left, true, false);
        assert_eq!(f.caret, 5);
        f.key(EditKey::Left, true, false);
        assert_eq!(f.caret, 0);
        f.key(EditKey::Left, true, false);
        assert_eq!(f.caret, 0);
        f.key(EditKey::Right, true, false);
        assert_eq!(f.caret, 3);
        f.key(EditKey::Right, true, false);
        assert_eq!(f.caret, 10);
        f.key(EditKey::Right, true, false);
        assert_eq!(f.caret, 13);
        f.key(EditKey::Right, true, false);
        assert_eq!(f.caret, 17);
        f.key(EditKey::Right, true, false);
        assert_eq!(f.caret, 17);
    }

    #[test]
    fn word_delete() {
        let mut f = field("one two  three");
        assert!(f.key(EditKey::Backspace, true, false));
        assert_eq!(f.text(), "one two  ");
        assert!(f.key(EditKey::Backspace, true, false));
        assert_eq!(f.text(), "one ");
        at(&mut f, 0);
        assert!(f.key(EditKey::Delete, true, false));
        assert_eq!(f.text(), " ");
        let mut g = field("a  b");
        at(&mut g, 1);
        g.key(EditKey::Delete, true, false);
        assert_eq!(g.text(), "a");
    }

    #[test]
    fn shift_extend_then_collapse() {
        let mut f = field("abcdef");
        at(&mut f, 3);
        f.key(EditKey::Right, false, true);
        f.key(EditKey::Right, false, true);
        assert_eq!(f.selected_text(), "de");
        f.key(EditKey::Right, false, false);
        assert!(!f.has_selection());
        assert_eq!(f.caret, 5);
        f.key(EditKey::Left, false, true);
        f.key(EditKey::Left, false, true);
        assert_eq!(f.selected_text(), "de");
        f.key(EditKey::Left, false, false);
        assert_eq!(f.caret, 3);
        assert!(!f.has_selection());
        f.key(EditKey::End, false, true);
        assert_eq!(f.selected_text(), "def");
        f.key(EditKey::Home, false, true);
        assert_eq!(f.selected_text(), "abc");
        f.key(EditKey::Home, false, false);
        assert!(!f.has_selection());
        assert_eq!(f.caret, 0);
    }

    #[test]
    fn select_word() {
        let s = "foo bar  baz";
        let mut f = field(s);
        let st = stops(s);
        f.select_word_at(45.0, &st);
        assert_eq!(f.selected_text(), "bar");
        f.select_word_at(2.0, &st);
        assert_eq!(f.selected_text(), "foo");
        f.select_word_at(75.0, &st);
        assert_eq!(f.selected_text(), "  ");
        f.select_word_at(500.0, &st);
        assert_eq!(f.selected_text(), "baz");
        f.select_word_at(-50.0, &st);
        assert_eq!(f.selected_text(), "foo");
        TextField::new().select_word_at(5.0, &[0.0]);
    }

    #[test]
    fn multibyte_safety() {
        let s = "İı çay 日本語 😀x";
        let mut f = field(s);
        let st = stops(s);
        for _ in 0..30 {
            f.key(EditKey::Left, false, false);
            assert!(s.is_char_boundary(f.caret));
        }
        assert_eq!(f.caret, 0);
        for _ in 0..30 {
            f.key(EditKey::Right, true, true);
            assert!(s.is_char_boundary(f.caret));
            let _ = f.selected_text();
        }
        for i in 0..=s.chars().count() * 10 + 20 {
            f.press(i as f32, &st, false);
            assert!(s.is_char_boundary(f.caret));
            f.select_word_at(i as f32, &st);
            let _ = f.selected_text();
        }
        f.select_all();
        f.insert("😀");
        f.key(EditKey::Backspace, false, false);
        assert!(f.is_empty());
        let mut g = field("a😀b");
        at(&mut g, 1);
        g.key(EditKey::Delete, false, false);
        assert_eq!(g.text(), "ab");
        g.set_text("日本語");
        g.key(EditKey::Backspace, true, false);
        assert!(g.is_empty());
    }

    #[test]
    fn press_and_drag() {
        let s = "abcdef";
        let mut f = field(s);
        let st = stops(s);
        f.press(24.0, &st, false);
        assert_eq!(f.caret, 2);
        assert!(!f.has_selection());
        f.drag(41.0, &st);
        assert_eq!(f.selected_text(), "cd");
        f.drag(-100.0, &st);
        assert_eq!(f.selected_text(), "ab");
        f.drag(1000.0, &st);
        assert_eq!(f.selected_text(), "cdef");
        f.press(0.0, &st, false);
        f.press(36.0, &st, true);
        assert_eq!(f.selected_text(), "abcd");
        // Mismatched or empty stops do not panic.
        f.press(5.0, &[], false);
        f.drag(5.0, &[0.0]);
        let mut e = TextField::new();
        e.press(10.0, &[0.0], false);
        e.drag(10.0, &[0.0]);
        assert_eq!(e.caret, 0);
    }

    #[test]
    fn scroll_is_added_to_x() {
        let s = "abcdef";
        let mut f = field(s);
        f.scroll = 20.0;
        f.press(0.0, &stops(s), false);
        assert_eq!(f.caret, 2);
    }
}
