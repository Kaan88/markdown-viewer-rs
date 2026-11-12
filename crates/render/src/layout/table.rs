//! Tables: grid placement with `colspan`/`rowspan`, column widths from natural text widths, row
//! heights, borders and the header band, then each cell's text. The HTML presentation attributes
//! are honoured where they are stated: `width` and `align` on the table, `align`, `valign` and
//! `height` on a row, `width`, `height` and `valign` on a cell, and a `<caption>` above it all.

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
        for caption in captions {
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
            let x = self.aligned_x(left, width, &layout, Align::Center);
            let height = layout.height().round();
            self.emit(&inline, layout, x, self.y, 15.0);
            self.y += height + self.px(8.0);
        }
        // `<table width>` narrows the table; `align` then says where in the column it stands.
        let table_attrs = self.doc.attrs_of(b).copied();
        let (left, width) = {
            let full = width;
            let asked = match table_attrs.and_then(|a| a.width) {
                Some(Length::Px(px)) => self.px(px as f32),
                Some(Length::Percent(p)) => full * f32::from(p) / 100.0,
                None => full,
            };
            let width = asked.min(full).max(self.px(40.0).min(full)).round();
            let left = match table_attrs.map(|a| a.align) {
                Some(Align::Center) => left + ((full - width) / 2.0).round(),
                Some(Align::Right) => left + full - width,
                _ => left,
            };
            (left, width)
        };
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
                for rr in r..r + rowspan {
                    if taken[rr].len() < col + colspan {
                        taken[rr].resize(col + colspan, false);
                    }
                    for c in col..col + colspan {
                        taken[rr][c] = true;
                    }
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
        let spread = |pick: fn(&(f32, f32)) -> f32| -> Vec<f32> {
            let mut column = vec![0f32; columns];
            for (cell, w) in cells.iter().zip(&widths) {
                if cell.colspan == 1 {
                    column[cell.col] = column[cell.col].max(pick(w));
                }
            }
            for (cell, w) in cells.iter().zip(&widths) {
                if cell.colspan > 1 {
                    let w = pick(w);
                    let span = cell.col..cell.col + cell.colspan;
                    let have: f32 = column[span.clone()].iter().sum::<f32>()
                        + 2.0 * pad_x * (cell.colspan - 1) as f32;
                    if w > have {
                        let extra = (w - have) / cell.colspan as f32;
                        for n in &mut column[span] {
                            *n += extra;
                        }
                    }
                }
            }
            column
        };
        let natural = spread(|w| w.0);
        let minimal = spread(|w| w.1);
        let available = width - 2.0 * hr;
        let columns_pad = 2.0 * pad_x * columns as f32;
        let total: f32 = natural.iter().sum::<f32>() + columns_pad;
        let min_total: f32 = minimal.iter().sum::<f32>() + columns_pad;
        let mut columns_w = vec![0f32; columns];
        if total <= available {
            let extra = ((available - total) / columns as f32).floor();
            for (ci, w) in natural.iter().enumerate() {
                columns_w[ci] = w + 2.0 * pad_x + extra;
            }
        } else if min_total <= available {
            // Too wide on one line: every column keeps its longest word whole, and the room
            // left over goes to the columns in proportion to what they would still wrap.
            let share = (available - min_total) / (total - min_total);
            for ci in 0..columns {
                columns_w[ci] = minimal[ci] + 2.0 * pad_x + (natural[ci] - minimal[ci]) * share;
            }
        } else {
            // Not even the longest words fit side by side: the columns shrink together and
            // the words break inside their cells.
            let scale = available / min_total;
            for (ci, w) in minimal.iter().enumerate() {
                columns_w[ci] = (w + 2.0 * pad_x) * scale;
            }
        }
        // A cell's `width` fixes its column (the widest ask wins); the other columns share what
        // is left in the proportions they had.
        let mut fixed = vec![0f32; columns];
        for cell in cells.iter().filter(|cell| cell.colspan == 1) {
            let asked = match cell.width {
                Some(Length::Px(px)) => self.px(px as f32),
                Some(Length::Percent(p)) => available * f32::from(p) / 100.0,
                None => continue,
            };
            fixed[cell.col] = fixed[cell.col].max(asked);
        }
        if fixed.iter().any(|w| *w > 0.0) {
            let narrowest = 2.0 * pad_x + self.px(8.0);
            let free_columns = fixed.iter().filter(|w| **w == 0.0).count();
            // The fixed columns may not squeeze the others below the narrowest column.
            let room = (available - narrowest * free_columns as f32).max(0.0);
            let asked: f32 = fixed.iter().sum();
            let shrink = if asked > room { room / asked } else { 1.0 };
            let free_now: f32 = columns_w
                .iter()
                .zip(&fixed)
                .filter(|(_, f)| **f == 0.0)
                .map(|(w, _)| *w)
                .sum();
            let left_over = available - asked * shrink;
            for (w, f) in columns_w.iter_mut().zip(&fixed) {
                if *f > 0.0 {
                    *w = (*f * shrink).max(narrowest.min(available / columns as f32));
                } else if free_now > 0.0 {
                    *w = *w / free_now * left_over;
                }
            }
            if free_columns == 0 {
                // Every column is fixed: they fill the table in the proportions asked for.
                let total: f32 = columns_w.iter().sum();
                for w in &mut columns_w {
                    *w = *w / total * available;
                }
            }
        }
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
