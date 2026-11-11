//! Text blocks: paragraphs, tight list text (`Plain`), headings, and the drawn task checkbox.

use super::inline::ImageBox;
use super::pager::{Pager, Txt, Wrap};
use super::paint::Paint;
use crate::theme::Color;
use mdview_core::model::{Align, ImageAlign};
use parley::{Alignment, AlignmentOptions};

impl<'a, 'b> Pager<'a, 'b> {
    /// A task-list box, drawn rather than typeset so it looks the same with any font stack.
    fn draw_checkbox(&mut self, x: f32, baseline: f32, checked: bool) {
        let side = self.px(15.0);
        let top = baseline - self.px(12.0);
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
        // `<img align="left|right">`: the first such image leaves the line and floats at that
        // side of the paragraph. The lines beside it are shorter, and the rest run under it.
        let gap = self.px(16.0);
        let side: Option<ImageBox> = inline
            .boxes
            .iter()
            .position(|image| {
                matches!(image.align, ImageAlign::Left | ImageAlign::Right)
                    && image.math.is_none()
                    && image.width + gap + self.px(120.0) <= width - indent
            })
            .map(|at| inline.boxes.remove(at));
        if side.is_some() {
            inline.trim_start();
        }
        self.wrap = side.map(|image| {
            let beside = image.width + gap;
            Wrap {
                height: image.height + self.px(4.0),
                narrow: width - indent - beside,
                shift: if image.align == ImageAlign::Left {
                    beside
                } else {
                    0.0
                },
            }
        });
        let txt = Txt {
            size: self.theme.body_size,
            line_height: self.theme.body_line_height,
            weight: 400.0,
            mono: false,
            color,
            max: Some(width - indent),
        };
        let mut layout = self.text_layout(&inline, &txt);
        self.wrap = None;
        let align = if align == Align::None {
            self.inherited_align(b)
        } else {
            align
        };
        if align == Align::Justify {
            // Every line but the last is stretched to the column; carets follow the layout.
            layout.align(Alignment::Justify, AlignmentOptions::default());
        }
        self.apply_dir(&mut layout, b, align);
        let has_text = !inline.text.trim().is_empty() || !inline.boxes.is_empty();
        let mut height = if has_text || task.is_some() {
            layout.height().round()
        } else {
            0.0
        }
        .max(if task.is_some() {
            self.theme.body_size * self.theme.body_line_height * self.scale
        } else {
            0.0
        });
        if let Some(image) = side {
            let x = if image.align == ImageAlign::Left {
                left + indent
            } else {
                left + width - image.width
            };
            self.paints.push(if image.reserved {
                Paint::Rect {
                    x,
                    y: self.y,
                    w: image.width,
                    h: image.height,
                    radius: self.px(3.0),
                    color: self.theme.code_bg,
                }
            } else {
                Paint::Image {
                    index: image.id,
                    x,
                    y: self.y,
                    w: image.width,
                    h: image.height,
                }
            });
            height = height.max(image.height.round());
        }
        let x = self.aligned_x(left + indent, width - indent, &layout, align);
        let baseline = layout
            .lines()
            .next()
            .map_or(height * 0.78, |line| line.metrics().baseline);
        if let Some(checked) = task {
            self.draw_checkbox(left, self.y + baseline, checked);
        }
        let size = txt.size;
        if !inline.text.trim().is_empty() || !inline.boxes.is_empty() {
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
        let txt = Txt {
            size: self.theme.body_size,
            line_height: self.theme.body_line_height,
            weight: 400.0,
            mono: false,
            color,
            max: Some(width - indent),
        };
        let mut layout = self.text_layout(&inline, &txt);
        let align = self.inherited_align(b);
        self.apply_dir(&mut layout, b, align);
        let height = layout.height().round().max(if task.is_some() {
            self.theme.body_size * self.theme.body_line_height * self.scale
        } else {
            0.0
        });
        let baseline = layout
            .lines()
            .next()
            .map_or(height * 0.78, |line| line.metrics().baseline);
        if let Some(checked) = task {
            self.draw_checkbox(left, self.y + baseline, checked);
        }
        let size = txt.size;
        if !inline.text.trim().is_empty() || !inline.boxes.is_empty() {
            let x = self.aligned_x(left + indent, width - indent, &layout, align);
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
        let txt = Txt {
            size,
            line_height: 1.25,
            weight: 600.0,
            mono: false,
            color: self.theme.heading,
            max: Some(width),
        };
        let mut layout = self.text_layout(&inline, &txt);
        self.apply_dir(&mut layout, b, align);
        let x = self.aligned_x(left, width, &layout, align);
        let height = layout.height().round();
        self.emit(&inline, layout, x, self.y, size);
        self.y += height + self.px(bottom);
    }
}
