//! Tables: grid placement with `colspan`/`rowspan`, column widths, row heights, borders and the
//! header band, then each cell's text. The HTML presentation attributes are honoured where they
//! are stated: `width` and `align` on the table, `align`, `valign` and `height` on a row, `width`,
//! `height` and `valign` on a cell, and a `<caption>` above it all.
//!
//! Widths follow a browser's automatic table layout. Every column has a narrowest width (its
//! longest word) and a widest (its longest line). A table is as wide as its widest columns
//! together, so a small table stays small; one that would be wider than the room it has takes
//! that room and its columns give up width in proportion to what they can spare; when even the
//! longest words do not fit side by side, the columns shrink together and words break inside
//! their cells. [`content_width`] and [`column_widths`] hold that arithmetic, which the height
//! estimate uses as well.

use super::inline::Inline;
use super::pager::{Pager, Txt};
use super::paint::Paint;
use crate::text::Brush;
use crate::theme::Color;
use mdview_core::model::{Align, BlockKind, Length, VAlign};
use parley::Layout;

impl<'a, 'b> Pager<'a, 'b> {
    pub(super) fn table(&mut self, b: u32, left: f32, width: f32) {
        /// A table is never wider than this many columns, whatever `colspan` asks for.
        const MAX_COLUMNS: usize = 200;
        struct Cell {
            inline: Inline,
            align: Align,
            header: bool,
            /// Placement on the grid, after `colspan`/`rowspan` and cells pushed down by a rowspan.
            row: usize,
            col: usize,
            colspan: usize,
            rowspan: usize,
            valign: VAlign,
            width: Option<Length>,
        }
        struct Laid {
            cell: usize,
            layout: Layout<Brush>,
        }
        // A `<caption>` reaches the model inside a row and a cell of its own, which the parser
        // gives anything written straight into a table. It is centred text above the grid.
        let caption_of = |row: u32| -> Option<u32> {
            let mut cells = self.doc.children(row);
            let (cell, None) = (cells.next()?, cells.next()) else {
                return None;
            };
            let mut inside = self.doc.children(cell);
            let (child, None) = (inside.next()?, inside.next()) else {
                return None;
            };
            matches!(self.doc.blocks[child as usize].kind, BlockKind::FigCaption).then_some(child)
        };
        let mut rows = Vec::new();
        let mut captions = Vec::new();
        for row in self.doc.children(b) {
            match caption_of(row) {
                Some(caption) => captions.push(caption),
                None => rows.push(row),
            }
        }
        let table_attrs = self.doc.attrs_of(b).copied();
        let (column_left, full) = (left, width);
        let row_count = rows.len();
        // Row heights the document asks for, in device px; a cell's `height` counts for its row.
        let mut asked_heights = vec![0f32; row_count];
        let mut header_row = Vec::with_capacity(row_count);
        let mut cells: Vec<Cell> = Vec::new();
        // `taken[r]` holds the columns of row `r` already covered by a cell from above.
        let mut taken: Vec<Vec<bool>> = vec![Vec::new(); row_count];
        let mut columns = 0usize;
        for (r, row) in rows.iter().enumerate() {
            let header = matches!(
                self.doc.blocks[*row as usize].kind,
                BlockKind::TableRow { header: true }
            );
            header_row.push(header);
            let row_attrs = self.doc.attrs_of(*row).copied();
            if let Some(Length::Px(px)) = row_attrs.and_then(|a| a.height) {
                asked_heights[r] = self.px(px as f32);
            }
            let mut col = 0usize;
            for cell in self.doc.children(*row) {
                let cell_attrs = self.doc.attrs_of(cell).copied();
                let (align, header_cell, colspan, rowspan) =
                    match self.doc.blocks[cell as usize].kind {
                        BlockKind::TableCell {
                            align,
                            header,
                            colspan,
                            rowspan,
                        } => (
                            align,
                            header,
                            usize::from(colspan.max(1)),
                            usize::from(rowspan.max(1)),
                        ),
                        _ => (Align::None, header, 1, 1),
                    };
                while taken[r].get(col).copied().unwrap_or(false) {
                    col += 1;
                }
                if col >= MAX_COLUMNS {
                    break;
                }
                let colspan = colspan.min(MAX_COLUMNS - col);
                let rowspan = rowspan.min(row_count - r);
                for row in taken.iter_mut().skip(r).take(rowspan) {
                    if row.len() < col + colspan {
                        row.resize(col + colspan, false);
                    }
                    row[col..col + colspan].fill(true);
                }
                columns = columns.max(col + colspan);
                if rowspan == 1
                    && let Some(Length::Px(px)) = cell_attrs.and_then(|a| a.height)
                {
                    asked_heights[r] = asked_heights[r].max(self.px(px as f32));
                }
                cells.push(Cell {
                    inline: self.cell_inline(cell),
                    // A cell with no `align` of its own takes its row's.
                    align: if align == Align::None {
                        row_attrs.map_or(Align::None, |a| a.align)
                    } else {
                        align
                    },
                    valign: cell_attrs
                        .and_then(|a| a.valign)
                        .or(row_attrs.and_then(|a| a.valign))
                        .unwrap_or(VAlign::Top),
                    width: cell_attrs.and_then(|a| a.width),
                    header: header_cell,
                    row: r,
                    col,
                    colspan,
                    rowspan,
                });
                col += colspan;
            }
        }
        if columns == 0 || row_count == 0 {
            self.captions(&captions, left, full, left + full / 2.0);
            return;
        }
        let pad_x = self.px(13.0);
        let pad_y = self.px(8.0);
        let hr = self.hair();
        let text_size = 15.0;
        let txt_of = |cell: &Cell, max: Option<f32>, color: Color| Txt {
            size: text_size,
            line_height: 1.45,
            weight: if cell.header { 600.0 } else { 400.0 },
            mono: false,
            color,
            max,
        };
        // Two widths of each cell's text: on one line (natural), and with every line break taken,
        // which is its longest word (minimal). A spanning cell is spread over its columns
        // afterwards.
        let widths: Vec<(f32, f32)> = cells
            .iter()
            .map(|cell| {
                if cell.inline.text.trim().is_empty() && cell.inline.boxes.is_empty() {
                    return (0.0, 0.0);
                }
                let txt = txt_of(cell, None, self.theme.text);
                let layout = self.text_layout(&cell.inline, &txt);
                let natural = layout.width().ceil();
                (
                    natural,
                    layout.calculate_content_widths().min.ceil().min(natural),
                )
            })
            .collect();
        // What each column asks for, padding included: single cells first, then a column's stated
        // width, then each spanning cell shares what it still lacks among its columns, the
        // narrower spans first.
        let mut asks = vec![
            Column {
                min: 2.0 * pad_x,
                max: 2.0 * pad_x,
                stated: Stated::Auto,
            };
            columns
        ];
        for (cell, w) in cells.iter().zip(&widths) {
            if cell.colspan == 1 {
                let ask = &mut asks[cell.col];
                ask.max = ask.max.max(w.0 + 2.0 * pad_x);
                ask.min = ask.min.max(w.1 + 2.0 * pad_x);
            }
        }
        let mut stated_px = vec![0f32; columns];
        for cell in cells.iter().filter(|cell| cell.colspan == 1) {
            match cell.width {
                Some(Length::Px(px)) => {
                    stated_px[cell.col] = stated_px[cell.col].max(self.px(px as f32));
                }
                Some(Length::Percent(p)) if p > 0 => {
                    let share = f32::from(p.min(100)) / 100.0;
                    asks[cell.col].stated = match asks[cell.col].stated {
                        Stated::Percent(other) => Stated::Percent(other.max(share)),
                        _ => Stated::Percent(share),
                    };
                }
                _ => {}
            }
        }
        for (ask, px) in asks.iter_mut().zip(&stated_px) {
            // A stated width stands in for the longest line, but never cuts into a word.
            if *px > 0.0 {
                ask.max = px.max(ask.min);
                if ask.stated == Stated::Auto {
                    ask.stated = Stated::Fixed;
                }
            }
        }
        let mut spanning: Vec<usize> = (0..cells.len()).filter(|i| cells[*i].colspan > 1).collect();
        spanning.sort_by_key(|i| cells[*i].colspan);
        for i in spanning {
            let cell = &cells[i];
            let span = cell.col..cell.col + cell.colspan;
            let min = widths[i].1 + 2.0 * pad_x;
            let max = match cell.width {
                Some(Length::Px(px)) => self.px(px as f32).max(min),
                _ => widths[i].0 + 2.0 * pad_x,
            };
            // The wider columns take more of it, as they already hold more.
            let weights: Vec<f32> = asks[span.clone()].iter().map(|ask| ask.max).collect();
            let weight: f32 = weights.iter().sum();
            let lacking_min = min - asks[span.clone()].iter().map(|a| a.min).sum::<f32>();
            let lacking_max = max - weight;
            for (ask, w) in asks[span].iter_mut().zip(&weights) {
                if lacking_min > 0.0 {
                    ask.min += lacking_min * w / weight;
                }
                if lacking_max > 0.0 {
                    ask.max += lacking_max * w / weight;
                }
                ask.max = ask.max.max(ask.min);
            }
        }
        // The table is as wide as `<table width>` says, else as its content needs, and never
        // wider than the room it stands in; `align`, its own or that of a `<div>` or `<center>`
        // around it, then says where in that room it stands.
        let min_total: f32 = asks.iter().map(|ask| ask.min).sum();
        let width = match table_attrs.and_then(|a| a.width) {
            Some(asked) => {
                let asked = match asked {
                    Length::Px(px) => self.px(px as f32),
                    Length::Percent(p) => full * f32::from(p) / 100.0,
                };
                asked.max(self.px(40.0)).max(min_total + 2.0 * hr).round()
            }
            None => content_width(&asks, full - 2.0 * hr).ceil() + 2.0 * hr,
        }
        .min(full);
        let align = match table_attrs.map_or(Align::None, |a| a.align) {
            Align::None => self.inherited_align(b),
            align => align,
        };
        let left = match align {
            Align::Center => left + ((full - width) / 2.0).round(),
            Align::Right => left + full - width,
            _ => left,
        };
        let columns_w = column_widths(&asks, width - 2.0 * hr);
        self.captions(&captions, column_left, full, left + width / 2.0);
        let span_width =
            |cell: &Cell| -> f32 { columns_w[cell.col..cell.col + cell.colspan].iter().sum() };
        let mut laid: Vec<Laid> = Vec::with_capacity(cells.len());
        // A word wider than its column breaks inside the cell instead of running over the next.
        self.break_words = true;
        for (i, cell) in cells.iter().enumerate() {
            let max = (span_width(cell) - 2.0 * pad_x).max(1.0);
            let txt = txt_of(cell, Some(max), self.theme.text);
            laid.push(Laid {
                cell: i,
                layout: self.text_layout(&cell.inline, &txt),
            });
        }
        self.break_words = false;
        // Row heights: single-row cells first, then each spanning cell grows its last row if the
        // rows it covers are too short.
        let mut heights: Vec<f32> = asked_heights
            .iter()
            .map(|asked| asked.round().max(2.0 * pad_y))
            .collect();
        for l in &laid {
            let cell = &cells[l.cell];
            if cell.rowspan == 1 {
                heights[cell.row] = heights[cell.row].max(l.layout.height().round() + 2.0 * pad_y);
            }
        }
        let mut spanning: Vec<&Laid> = laid.iter().filter(|l| cells[l.cell].rowspan > 1).collect();
        spanning.sort_by_key(|l| cells[l.cell].row + cells[l.cell].rowspan);
        for l in spanning {
            let cell = &cells[l.cell];
            let end = cell.row + cell.rowspan;
            let have: f32 =
                heights[cell.row..end].iter().sum::<f32>() + (cell.rowspan - 1) as f32 * hr;
            let need = l.layout.height().round() + 2.0 * pad_y;
            if need > have {
                heights[end - 1] += need - have;
            }
        }
        let mut row_y = Vec::with_capacity(row_count);
        let mut acc = hr;
        for h in &heights {
            row_y.push(acc);
            acc += h + hr;
        }
        let total_h = acc;
        let radius = self.px(6.0);
        let y0 = self.y;
        let inner_radius = (radius - hr).max(0.0);
        self.paints.push(Paint::Rect {
            x: left,
            y: y0,
            w: width,
            h: total_h,
            radius,
            color: self.theme.border,
        });
        self.paints.push(Paint::Rect {
            x: left + hr,
            y: y0 + hr,
            w: width - 2.0 * hr,
            h: total_h - 2.0 * hr,
            radius: inner_radius,
            color: self.theme.bg,
        });
        // An HTML row is a header row when every cell in it is a `<th>`.
        let first_is_header = header_row[0] || {
            let mut first = cells.iter().filter(|cell| cell.row == 0).peekable();
            first.peek().is_some() && first.all(|cell| cell.header)
        };
        if first_is_header {
            self.paints.push(Paint::Rect {
                x: left + hr,
                y: y0 + hr,
                w: width - 2.0 * hr,
                h: heights[0],
                radius: inner_radius,
                color: self.theme.head_bg,
            });
            if heights[0] > inner_radius * 2.0 {
                self.paints.push(Paint::Rect {
                    x: left + hr,
                    y: y0 + hr + inner_radius,
                    w: width - 2.0 * hr,
                    h: heights[0] - 2.0 * inner_radius,
                    radius: 0.0,
                    color: self.theme.head_bg,
                });
            }
        }
        let mut col_x = Vec::with_capacity(columns + 1);
        let mut cx = left + hr;
        for w in &columns_w {
            col_x.push(cx);
            cx += w;
        }
        col_x.push(cx);
        // Row separators stop where a cell spans past them.
        for ri in 0..row_count.saturating_sub(1) {
            let y = y0 + row_y[ri] + heights[ri];
            let mut c = 0;
            while c < columns {
                let covered = |c: usize| {
                    cells.iter().any(|cell| {
                        cell.col <= c
                            && c < cell.col + cell.colspan
                            && cell.row <= ri
                            && ri + 1 < cell.row + cell.rowspan
                    })
                };
                if covered(c) {
                    c += 1;
                    continue;
                }
                let start = c;
                while c < columns && !covered(c) {
                    c += 1;
                }
                self.paints.push(Paint::Rect {
                    x: col_x[start],
                    y,
                    w: col_x[c] - col_x[start],
                    h: hr,
                    radius: 0.0,
                    color: self.theme.border,
                });
            }
        }
        for l in laid {
            let cell = &cells[l.cell];
            let w = span_width(cell);
            let dx = match cell.align {
                Align::Center => ((w - l.layout.width()) / 2.0).round(),
                Align::Right => (w - pad_x - l.layout.width()).round(),
                _ => pad_x,
            };
            // The room under the text, for a cell taller than its text and `valign` to share out.
            let end = cell.row + cell.rowspan;
            let cell_h: f32 =
                heights[cell.row..end].iter().sum::<f32>() + (cell.rowspan - 1) as f32 * hr;
            let spare = (cell_h - 2.0 * pad_y - l.layout.height().round()).max(0.0);
            let dy = match cell.valign {
                VAlign::Middle => (spare / 2.0).round(),
                VAlign::Bottom => spare,
                VAlign::Top | VAlign::Baseline => 0.0,
            };
            self.emit(
                &cell.inline,
                l.layout,
                col_x[cell.col] + dx,
                y0 + row_y[cell.row] + pad_y + dy,
                text_size,
            );
        }
        self.y = y0 + total_h + self.px(16.0);
    }
}

impl<'a, 'b> Pager<'a, 'b> {
    /// A table's captions, above its grid: centred on `centre`, the middle of the table, as far
    /// as the room from `left` over `width` allows, and wrapped only at that room's width.
    fn captions(&mut self, captions: &[u32], left: f32, width: f32, centre: f32) {
        for &caption in captions {
            let inline = self.cell_inline(caption);
            if inline.text.trim().is_empty() && inline.boxes.is_empty() {
                continue;
            }
            let txt = Txt {
                size: 15.0,
                line_height: 1.45,
                weight: 400.0,
                mono: false,
                color: self.theme.text,
                max: Some(width),
            };
            let layout = self.text_layout(&inline, &txt);
            let x = (centre - layout.width() / 2.0)
                .round()
                .min(left + width - layout.width())
                .max(left);
            let height = layout.height().round();
            self.emit(&inline, layout, x, self.y, 15.0);
            self.y += height + self.px(8.0);
        }
    }
}

/// How a column's width is stated in the document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Stated {
    /// Not at all: the content decides.
    Auto,
    /// In px, which [`Column::max`] then holds.
    Fixed,
    /// As this share of the table's width, over 0 and at most 1.
    Percent(f32),
}

/// What one column asks for, in device px with the cell padding included.
#[derive(Debug, Clone, Copy)]
pub(super) struct Column {
    /// The narrowest it goes without breaking a word.
    pub min: f32,
    /// The width at which nothing in it wraps, or its stated width. Never under `min`.
    pub max: f32,
    pub stated: Stated,
}

/// The width the columns take together when no `<table width>` says: every column at its widest,
/// and wide enough that a column stated in percent is that share of it, within `available`.
pub(super) fn content_width(columns: &[Column], available: f32) -> f32 {
    let min_total: f32 = columns.iter().map(|c| c.min).sum();
    let (mut rest, mut percent, mut by_percent) = (0f32, 0f32, 0f32);
    for column in columns {
        match column.stated {
            Stated::Percent(share) => {
                percent += share;
                by_percent = by_percent.max(column.max / share);
            }
            _ => rest += column.max,
        }
    }
    let wanted = if rest == 0.0 {
        by_percent
    } else if percent >= 1.0 {
        f32::INFINITY
    } else {
        (rest / (1.0 - percent)).max(by_percent)
    };
    wanted.max(min_total).min(available)
}

/// Shares `inner`, the table's width inside its border, among the columns.
///
/// Each column starts at its narrowest. The room left goes first to columns stated in percent,
/// then to those stated in px, then to the rest, each group up to what it asks for and, when the
/// room runs out inside a group, in proportion to what each column still lacks. Room left after
/// that (a stated table width) widens the unstated columns in proportion to their widest.
///
/// Without room for the narrowest widths, the widest columns are cut down to one width that
/// makes the table fit, and the columns narrower than that keep their words whole.
pub(super) fn column_widths(columns: &[Column], inner: f32) -> Vec<f32> {
    let min_total: f32 = columns.iter().map(|c| c.min).sum();
    if inner <= min_total {
        let mut sorted: Vec<f32> = columns.iter().map(|c| c.min).collect();
        sorted.sort_by(f32::total_cmp);
        let mut left = inner.max(0.0);
        let mut level = f32::INFINITY;
        for (i, min) in sorted.iter().enumerate() {
            let wider = (sorted.len() - i) as f32;
            if min * wider >= left {
                level = left / wider;
                break;
            }
            left -= min;
        }
        return columns.iter().map(|c| c.min.min(level)).collect();
    }
    let mut widths: Vec<f32> = columns.iter().map(|c| c.min).collect();
    let mut room = inner - min_total;
    let target = |column: &Column, pass: usize| match (column.stated, pass) {
        (Stated::Percent(share), 0) => Some(share * inner),
        (Stated::Fixed, 1) | (Stated::Auto, 2) => Some(column.max),
        _ => None,
    };
    for pass in 0..3 {
        let lacking = |width: f32, column: &Column| {
            target(column, pass).map_or(0.0, |target| (target - width).max(0.0))
        };
        let total: f32 = widths
            .iter()
            .zip(columns)
            .map(|(w, column)| lacking(*w, column))
            .sum();
        if total <= 0.0 {
            continue;
        }
        // The slack keeps a rounding error from wrapping a column that fits exactly.
        let share = if room + 0.01 >= total {
            1.0
        } else {
            room / total
        };
        for (w, column) in widths.iter_mut().zip(columns) {
            *w += lacking(*w, column) * share;
        }
        room = (room - total * share).max(0.0);
    }
    if room > 0.01 {
        // A table stated wider than its content: the unstated columns take the rest, or the
        // px ones when every column is stated, or all of them.
        let kind = [Stated::Auto, Stated::Fixed]
            .into_iter()
            .find(|kind| columns.iter().any(|c| c.stated == *kind));
        let takes = |column: &Column| kind.is_none_or(|kind| column.stated == kind);
        let weight: f32 = columns.iter().filter(|c| takes(c)).map(|c| c.max).sum();
        for (w, column) in widths.iter_mut().zip(columns) {
            if takes(column) {
                *w += room * column.max / weight;
            }
        }
    }
    widths
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto(min: f32, max: f32) -> Column {
        Column {
            min,
            max,
            stated: Stated::Auto,
        }
    }

    fn close(got: &[f32], want: &[f32]) {
        assert_eq!(got.len(), want.len(), "{got:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 0.01, "{got:?} is not {want:?}");
        }
    }

    #[test]
    fn content_that_fits_keeps_its_widest_widths() {
        let columns = [auto(30.0, 80.0), auto(40.0, 120.0)];
        let inner = content_width(&columns, 700.0);
        assert_eq!(inner, 200.0);
        close(&column_widths(&columns, inner), &[80.0, 120.0]);
    }

    #[test]
    fn too_wide_gives_up_width_in_proportion_to_what_can_be_spared() {
        // 300 over: the columns can spare 100 and 500, and give up a sixth and five sixths.
        let columns = [auto(100.0, 200.0), auto(100.0, 600.0)];
        assert_eq!(content_width(&columns, 500.0), 500.0);
        close(&column_widths(&columns, 500.0), &[150.0, 350.0]);
    }

    #[test]
    fn without_room_for_the_longest_words_only_the_widest_columns_are_cut() {
        let columns = [auto(50.0, 50.0), auto(400.0, 900.0), auto(300.0, 300.0)];
        assert_eq!(content_width(&columns, 450.0), 450.0);
        close(&column_widths(&columns, 450.0), &[50.0, 200.0, 200.0]);
        close(&column_widths(&columns, 60.0), &[20.0, 20.0, 20.0]);
    }

    #[test]
    fn stated_widths_come_before_content() {
        let fixed = Column {
            min: 40.0,
            max: 200.0,
            stated: Stated::Fixed,
        };
        // The px column keeps its width while the other one wraps.
        let columns = [fixed, auto(50.0, 900.0)];
        close(&column_widths(&columns, 500.0), &[200.0, 300.0]);
        // A percent column is its share of a table that grows until it can be.
        let half = Column {
            min: 30.0,
            max: 60.0,
            stated: Stated::Percent(0.5),
        };
        let columns = [half, auto(50.0, 150.0)];
        assert_eq!(content_width(&columns, 700.0), 300.0);
        close(&column_widths(&columns, 300.0), &[150.0, 150.0]);
        // In a table stated wider than its content, the unstated columns take the rest.
        let columns = [fixed, auto(50.0, 100.0), auto(50.0, 300.0)];
        close(&column_widths(&columns, 1000.0), &[200.0, 200.0, 600.0]);
    }
}
