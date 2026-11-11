//! Tables: grid placement with `colspan`/`rowspan`, column widths from natural text widths, row
//! heights, borders and the header band, then each cell's text.

use super::inline::Inline;
use super::pager::{Pager, Txt};
use super::paint::Paint;
use crate::text::Brush;
use crate::theme::Color;
use mdview_core::model::{Align, BlockKind};
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
        }
        struct Laid {
            cell: usize,
            layout: Layout<Brush>,
        }
        let rows: Vec<u32> = self.doc.children(b).collect();
        let row_count = rows.len();
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
            let mut col = 0usize;
            for cell in self.doc.children(*row) {
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
                cells.push(Cell {
                    inline: self.cell_inline(cell),
                    align,
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
        // Natural width of each cell's text; a spanning cell is spread over its columns afterwards.
        let naturals: Vec<f32> = cells
            .iter()
            .map(|cell| {
                if cell.inline.text.trim().is_empty() && cell.inline.boxes.is_empty() {
                    return 0.0;
                }
                let txt = txt_of(cell, None, self.theme.text);
                self.text_layout(&cell.inline, &txt).width().ceil()
            })
            .collect();
        let mut natural = vec![0f32; columns];
        for (cell, w) in cells.iter().zip(&naturals) {
            if cell.colspan == 1 {
                natural[cell.col] = natural[cell.col].max(*w);
            }
        }
        for (cell, w) in cells.iter().zip(&naturals) {
            if cell.colspan > 1 {
                let span = cell.col..cell.col + cell.colspan;
                let have: f32 = natural[span.clone()].iter().sum::<f32>()
                    + 2.0 * pad_x * (cell.colspan - 1) as f32;
                if *w > have {
                    let extra = (*w - have) / cell.colspan as f32;
                    for n in &mut natural[span] {
                        *n += extra;
                    }
                }
            }
        }
        let available = width - 2.0 * hr;
        let total: f32 = natural.iter().sum::<f32>() + 2.0 * pad_x * columns as f32;
        let mut columns_w = vec![0f32; columns];
        if total <= available {
            let extra = ((available - total) / columns as f32).floor();
            for (ci, w) in natural.iter().enumerate() {
                columns_w[ci] = w + 2.0 * pad_x + extra;
            }
        } else {
            let scale = available / total;
            for (ci, w) in natural.iter().enumerate() {
                columns_w[ci] = ((w + 2.0 * pad_x) * scale).max(2.0 * pad_x + self.px(8.0));
            }
        }
        let span_width =
            |cell: &Cell| -> f32 { columns_w[cell.col..cell.col + cell.colspan].iter().sum() };
        let mut laid: Vec<Laid> = Vec::with_capacity(cells.len());
        for (i, cell) in cells.iter().enumerate() {
            let max = (span_width(cell) - 2.0 * pad_x).max(1.0);
            let txt = txt_of(cell, Some(max), self.theme.text);
            laid.push(Laid {
                cell: i,
                layout: self.text_layout(&cell.inline, &txt),
            });
        }
        // Row heights: single-row cells first, then each spanning cell grows its last row if the
        // rows it covers are too short.
        let mut heights = vec![2.0 * pad_y; row_count];
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
        if header_row[0] {
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
            self.emit(
                &cell.inline,
                l.layout,
                col_x[cell.col] + dx,
                y0 + row_y[cell.row] + pad_y,
                text_size,
            );
        }
        self.y = y0 + total_h + self.px(16.0);
    }
}
