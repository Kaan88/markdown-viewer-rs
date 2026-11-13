//! Blocks that hold other blocks: block quotes and alerts, lists with their markers, and
//! `<details>` with its drawn chevron.

use super::inline::{Inline, Style};
use super::pager::{Pager, Txt};
use super::paint::Paint;
use crate::theme::Color;
use mdview_core::model::{AlertKind, BlockKind, ListMarker};

/// Logical px a `<dd>` is indented, and the extra gap after tight text in one, which with the 4 px
/// tight text already leaves makes the 16 px between groups.
pub(super) const DEFINITION_INDENT: f32 = 16.0;
pub(super) const DEFINITION_GAP: f32 = 12.0;

impl<'a, 'b> Pager<'a, 'b> {
    pub(super) fn quote(
        &mut self,
        b: u32,
        left: f32,
        width: f32,
        alert: Option<AlertKind>,
        color: Color,
        depth: usize,
    ) {
        let _ = color;
        let (accent, tint) = alert.map_or((self.theme.border, self.theme.tint), |kind| {
            self.theme.alert(kind)
        });
        let pad = self.px(if alert.is_some() { 12.0 } else { 2.0 });
        let inset = self.px(18.0);
        let bar = self.px(3.0);
        let y0 = self.y;
        let start = self.paints.len();
        let inner_left = left + inset;
        let inner_width = width - inset - self.px(14.0);
        self.y = y0 + pad;
        if let Some(alert) = alert {
            let mut title = Inline::default();
            title.push(
                alert.label(),
                Style {
                    bold: true,
                    ..Style::default()
                },
            );
            let txt = Txt {
                size: self.theme.body_size,
                line_height: 1.3,
                weight: 600.0,
                mono: false,
                color: accent,
                max: Some(inner_width),
            };
            let layout = self.text_layout(&title, &txt);
            let height = layout.height().round();
            let size = txt.size;
            self.emit(&title, layout, inner_left, self.y, size);
            self.y += height + self.px(4.0);
        }
        let body_color = if alert.is_some() {
            self.theme.text
        } else {
            self.theme.muted
        };
        self.blocks(b, inner_left, inner_width, body_color, depth + 1);
        let height = (self.y - y0).max(self.px(8.0));
        let bottom = self.px(if alert.is_some() { 12.0 } else { 2.0 });
        let full_height = height + bottom;
        if alert.is_some() {
            self.paints.insert(
                start,
                Paint::Rect {
                    x: left,
                    y: y0,
                    w: width,
                    h: full_height,
                    radius: self.px(6.0),
                    color: tint,
                },
            );
        }
        let bar_color = accent;
        // The bar goes over the alert's tint, not under it; both sit behind the text.
        let bar_at = if alert.is_some() { start + 1 } else { start };
        self.paints.insert(
            bar_at,
            Paint::Rect {
                x: left,
                y: y0,
                w: bar,
                h: full_height,
                radius: bar / 2.0,
                color: bar_color,
            },
        );
        self.y = y0 + full_height + self.px(16.0);
    }

    pub(super) fn list(
        &mut self,
        b: u32,
        left: f32,
        width: f32,
        color: Color,
        depth: usize,
        start: Option<u32>,
    ) {
        let items: Vec<u32> = self.doc.children(b).collect();
        // `<ol type="a">` names the marker once, for every item that does not name its own.
        let list_marker = self.doc.attrs_of(b).and_then(|a| a.marker);
        let indent = self.px(26.0);
        let count = items.len();
        for (index, item) in items.into_iter().enumerate() {
            if self.y > self.max_y {
                return;
            }
            // Beside a float at the left the whole item moves over, marker and all; its lines
            // are shortened one by one like any text. With no room for a line, it starts below.
            let (left, width) = if self.floats.is_empty() {
                (left, width)
            } else {
                let line = self.px(self.theme.body_size * self.theme.body_line_height);
                let need = self.px(super::floats::MIN_TEXT);
                self.clear_narrow(left + indent, width - indent, line, need);
                let (lo, _) = self.beside_rows(left, width, line);
                (lo, width - (lo - left))
            };
            let task = matches!(
                self.doc.blocks[item as usize].kind,
                BlockKind::ListItem { task: true }
            );
            if !task {
                let marker = self
                    .doc
                    .attrs_of(item)
                    .and_then(|a| a.marker)
                    .or(list_marker)
                    .unwrap_or(if start.is_some() {
                        ListMarker::Decimal
                    } else {
                        ListMarker::Disc
                    });
                if marker != ListMarker::None {
                    let number = start.unwrap_or(1).saturating_add(index as u32).max(1);
                    self.marker(marker, number, left, self.y);
                }
            }
            self.blocks(item, left + indent, width - indent, color, depth + 1);
            self.y += self.px(if index + 1 < count { 4.0 } else { 16.0 });
        }
    }

    pub(super) fn marker(&mut self, marker: ListMarker, number: u32, left: f32, y: f32) {
        match marker {
            ListMarker::Disc | ListMarker::Circle | ListMarker::Square => {
                let d = self.px(5.0);
                let radius = if marker == ListMarker::Square {
                    0.0
                } else {
                    d / 2.0
                };
                self.paints.push(Paint::Rect {
                    x: left + self.px(8.0),
                    y: y + self.px(10.0),
                    w: d,
                    h: d,
                    radius,
                    color: self.theme.muted,
                });
            }
            other => {
                let label = match other {
                    ListMarker::LowerAlpha => alpha(number, true),
                    ListMarker::UpperAlpha => alpha(number, false),
                    ListMarker::LowerRoman => roman(number).to_lowercase(),
                    ListMarker::UpperRoman => roman(number),
                    _ => number.to_string(),
                };
                let mut inline = Inline::default();
                inline.push(
                    &format!("{label}."),
                    Style {
                        muted: true,
                        ..Style::default()
                    },
                );
                let txt = Txt {
                    size: self.theme.body_size,
                    line_height: self.theme.body_line_height,
                    weight: 400.0,
                    mono: false,
                    color: self.theme.muted,
                    max: None,
                };
                let layout = self.text_layout(&inline, &txt);
                let size = txt.size;
                self.emit(&inline, layout, left, y + self.px(1.0), size);
            }
        }
    }

    /// A `<dd>`: indented under its term, with a gap after it before the next group.
    pub(super) fn definition(&mut self, b: u32, left: f32, width: f32, color: Color, depth: usize) {
        let indent = self.px(DEFINITION_INDENT);
        self.blocks(b, left + indent, width - indent, color, depth + 1);
        if self.ends_in_plain(b) {
            self.y += self.px(DEFINITION_GAP);
        }
    }

    /// Whether the last child of `b` is tight text, which leaves 4 px where a paragraph leaves 16.
    pub(super) fn ends_in_plain(&self, b: u32) -> bool {
        self.doc
            .children(b)
            .last()
            .is_some_and(|c| matches!(self.doc.blocks[c as usize].kind, BlockKind::Plain))
    }

    pub(super) fn details(&mut self, b: u32, left: f32, width: f32, color: Color, depth: usize) {
        let children: Vec<u32> = self.doc.children(b).collect();
        let summary = children
            .iter()
            .copied()
            .find(|c| matches!(self.doc.blocks[*c as usize].kind, BlockKind::Summary));
        let open = self.details_open(b);
        if let Some(summary) = summary {
            let inline = self.inline_of(summary);
            let txt = Txt {
                size: self.theme.body_size,
                line_height: self.theme.body_line_height,
                weight: 600.0,
                mono: false,
                color: self.theme.heading,
                max: Some(width - self.px(18.0)),
            };
            let layout = self.text_layout(&inline, &txt);
            let height = layout.height().round();
            let baseline = layout
                .lines()
                .next()
                .map_or(height * 0.8, |line| line.metrics().baseline);
            // A drawn chevron, because a symbol glyph is not in every font stack.
            let cx = left + self.px(5.0);
            let cy = self.y + baseline - self.px(5.0);
            let w = self.px(7.0);
            let h = self.px(4.5);
            let stroke = self.px(1.6).max(1.0);
            let color = self.theme.muted;
            if open {
                self.paints.push(Paint::Line {
                    a: (cx, cy),
                    b: (cx + w / 2.0, cy + h),
                    thickness: stroke,
                    color,
                });
                self.paints.push(Paint::Line {
                    a: (cx + w / 2.0, cy + h),
                    b: (cx + w, cy),
                    thickness: stroke,
                    color,
                });
            } else {
                self.paints.push(Paint::Line {
                    a: (cx, cy - h / 2.0),
                    b: (cx + h / 2.0, cy),
                    thickness: stroke,
                    color,
                });
                self.paints.push(Paint::Line {
                    a: (cx + h / 2.0, cy),
                    b: (cx, cy + h / 2.0),
                    thickness: stroke,
                    color,
                });
            }
            let size = txt.size;
            self.emit(&inline, layout, left + self.px(18.0), self.y, size);
            self.y += height + self.px(4.0);
        }
        if open {
            let (mark, base) = (self.floats.len(), self.float_base);
            self.float_base = mark;
            for child in children {
                if Some(child) == summary {
                    continue;
                }
                if self.y > self.max_y {
                    break;
                }
                self.block(child, left, width, color, depth + 1);
            }
            self.end_floats(mark);
            self.float_base = base;
            self.y += self.px(12.0);
        }
    }
}

fn alpha(mut n: u32, lower: bool) -> String {
    let mut out = String::new();
    while n > 0 {
        n -= 1;
        out.insert(0, (b'a' + (n % 26) as u8) as char);
        n /= 26;
    }
    if out.is_empty() {
        out.push('a');
    }
    if !lower {
        out.make_ascii_uppercase();
    }
    out
}

fn roman(mut n: u32) -> String {
    const TABLE: &[(u32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for &(value, glyph) in TABLE {
        while n >= value {
            out.push_str(glyph);
            n -= value;
        }
    }
    out
}
