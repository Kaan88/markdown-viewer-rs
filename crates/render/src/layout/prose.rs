//! Text blocks: paragraphs, tight list text (`Plain`), headings, and the drawn task checkbox.

use super::hit::TaskBox;
use super::pager::{Pager, Txt};
use super::paint::Paint;
use crate::theme::Color;
use mdview_core::model::Align;
use parley::{Alignment, AlignmentOptions};

impl<'a, 'b> Pager<'a, 'b> {
    /// A task-list box, drawn rather than typeset so it looks the same with any font stack.
    fn draw_checkbox(&mut self, block: u32, x: f32, baseline: f32, checked: bool) {
        let side = self.px(15.0);
        let top = baseline - self.px(12.0);
        self.tasks.push(TaskBox {
            x,
            y: top,
            w: side,
            h: side,
            block,
        });
        let radius = self.px(3.0);
        if checked {
            self.paints.push(Paint::Rect {
                x,
                y: top,
                w: side,
                h: side,
                radius,
                color: self.theme.accent,
            });
            let check = self.theme.bg;
            let thickness = self.px(1.7).max(1.0);
            self.paints.push(Paint::Line {
                a: (x + side * 0.24, top + side * 0.52),
                b: (x + side * 0.42, top + side * 0.70),
                thickness,
                color: check,
            });
            self.paints.push(Paint::Line {
                a: (x + side * 0.42, top + side * 0.70),
                b: (x + side * 0.76, top + side * 0.32),
                thickness,
                color: check,
            });
        } else {
            let border = self.theme.muted;
            let bg = self.theme.bg;
            let hair = self.hair();
            self.paints.push(Paint::Rect {
                x,
                y: top,
                w: side,
                h: side,
                radius,
                color: border,
            });
            self.paints.push(Paint::Rect {
                x: x + hair,
                y: top + hair,
                w: side - 2.0 * hair,
                h: side - 2.0 * hair,
                radius: (radius - hair).max(0.0),
                color: bg,
            });
        }
    }

    fn task_indent(&self) -> f32 {
        self.px(22.0)
    }

    pub(super) fn paragraph(&mut self, b: u32, left: f32, width: f32, color: Color, align: Align) {
        let mut inline = self.inline_of(b);
        let task = inline.task;
        if inline.text.trim().is_empty() && inline.boxes.is_empty() && task.is_none() {
            return;
        }
        let indent = if task.is_some() {
            self.task_indent()
        } else {
            0.0
        };
        self.mark_dir(&mut inline, b);
        let align = if align == Align::None {
            self.inherited_align(b)
        } else {
            align
        };
        // `<img align="left|right">` leaves the line and floats; the lines beside it, and beside
        // any float still open from an earlier block, are shorter (`floats.rs`).
        let line = self.theme.body_size * self.theme.body_line_height * self.scale;
        let (text_left, text_width, floated) = self.begin_text(
            b,
            &mut inline,
            left + indent,
            width - indent,
            line,
            align,
            true,
        );
        let has_text = !inline.text.trim().is_empty() || !inline.boxes.is_empty();
        if floated && !has_text && task.is_none() {
            // Only the image: it stands at the side and the next block starts level with it.
            self.wrap = None;
            return;
        }
        let txt = Txt {
            size: self.theme.body_size,
            line_height: self.theme.body_line_height,
            weight: 400.0,
            mono: false,
            color,
            max: Some(text_width),
        };
        let mut layout = self.text_layout(&inline, &txt);
        self.wrap = None;
        if align == Align::Justify {
            // Every line but the last is stretched to the column; carets follow the layout.
            layout.align(Alignment::Justify, AlignmentOptions::default());
        }
        self.apply_dir(&mut layout, b, align);
        let height = if has_text || task.is_some() {
            layout.height().round()
        } else {
            0.0
        }
        .max(if task.is_some() { line } else { 0.0 });
        let x = self.aligned_x(text_left, text_width, &layout, align);
        let baseline = layout
            .lines()
            .next()
            .map_or(height * 0.78, |line| line.metrics().baseline);
        if let Some(checked) = task {
            self.draw_checkbox(b, text_left - indent, self.y + baseline, checked);
        }
        let size = txt.size;
        if has_text {
            self.emit(&inline, layout, x, self.y, size);
        }
        self.y += height + self.px(16.0);
    }

    pub(super) fn plain(&mut self, b: u32, left: f32, width: f32, color: Color) {
        let mut inline = self.inline_of(b);
        self.mark_dir(&mut inline, b);
        let task = inline.task;
        if inline.text.trim().is_empty() && inline.boxes.is_empty() && task.is_none() {
            return;
        }
        let indent = if task.is_some() {
            self.task_indent()
        } else {
            0.0
        };
        let align = self.inherited_align(b);
        let line = self.theme.body_size * self.theme.body_line_height * self.scale;
        let (text_left, text_width, floated) = self.begin_text(
            b,
            &mut inline,
            left + indent,
            width - indent,
            line,
            align,
            true,
        );
        let has_text = !inline.text.trim().is_empty() || !inline.boxes.is_empty();
        if floated && !has_text && task.is_none() {
            self.wrap = None;
            return;
        }
        let txt = Txt {
            size: self.theme.body_size,
            line_height: self.theme.body_line_height,
            weight: 400.0,
            mono: false,
            color,
            max: Some(text_width),
        };
        let mut layout = self.text_layout(&inline, &txt);
        self.wrap = None;
        self.apply_dir(&mut layout, b, align);
        let height = layout
            .height()
            .round()
            .max(if task.is_some() { line } else { 0.0 });
        let baseline = layout
            .lines()
            .next()
            .map_or(height * 0.78, |line| line.metrics().baseline);
        if let Some(checked) = task {
            self.draw_checkbox(b, text_left - indent, self.y + baseline, checked);
        }
        let size = txt.size;
        if has_text {
            let x = self.aligned_x(text_left, text_width, &layout, align);
            self.emit(&inline, layout, x, self.y, size);
        }
        self.y += height + self.px(4.0);
    }

    pub(super) fn heading(&mut self, b: u32, left: f32, width: f32, level: u8, align: Align) {
        let (size, top, bottom) = match level {
            1 => (34.0, 0.0, 14.0),
            2 => (24.0, 22.0, 12.0),
            3 => (18.0, 12.0, 8.0),
            4 => (17.0, 10.0, 8.0),
            5 => (16.0, 8.0, 6.0),
            _ => (15.0, 8.0, 6.0),
        };
        let mut inline = self.inline_of(b);
        if inline.text.trim().is_empty() && inline.boxes.is_empty() {
            return;
        }
        self.mark_dir(&mut inline, b);
        self.y += self.px(top);
        // Beside a float a heading wraps like a paragraph. An image floated inside a heading
        // stays in its line.
        let line = size * 1.25 * self.scale;
        let (left, width, _) = self.begin_text(b, &mut inline, left, width, line, align, false);
        let txt = Txt {
            size,
            line_height: 1.25,
            weight: 600.0,
            mono: false,
            color: self.theme.heading,
            max: Some(width),
        };
        let mut layout = self.text_layout(&inline, &txt);
        self.wrap = None;
        self.apply_dir(&mut layout, b, align);
        let x = self.aligned_x(left, width, &layout, align);
        let height = layout.height().round();
        self.emit(&inline, layout, x, self.y, size);
        self.y += height + self.px(bottom);
    }
}
