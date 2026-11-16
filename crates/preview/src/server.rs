//! The loop that draws the page for an editor: in mdview's process (`mdview --preview-server`,
//! [`serve_stdio`]) or on a thread of the editor's own ([`serve`]).

use crate::wire::{self, Request, Update};
use mdview_core::model::Document;
use mdview_render::{BlockBox, DARK, DocumentRenderer, Jump, LIGHT, Options};
use std::io::{self, BufReader, BufWriter, Write};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

/// Where a followed line goes: this far below the pane's top (logical px), as mdview puts a
/// line it jumps to.
const MARGIN: f32 = 24.0;

/// The state of one preview: the document, the pane and where the page is.
pub struct Server {
    renderer: DocumentRenderer,
    doc: Option<Document>,
    options: Options,
    /// The pane in device pixels; nothing is drawn until it is known.
    size: Option<(u32, u32)>,
    /// The page row at the pane's top, device pixels.
    top: f32,
    /// The caret's place, to show again after the text changes.
    follow: Option<(u32, u32)>,
    /// The source's line count, for the page's end in [`row_of_line`] and [`line_of_row`].
    lines: u32,
    /// The editor's last screen of scrolling, `(tail, end)` in [`Request::Align`]'s units:
    /// over it the page goes on to its own bottom.
    end: Option<(f32, f32)>,
    /// Something changed since the last frame (or there has been none).
    dirty: bool,
    /// The editor's copy of the frame is gone: the next frame is sent whole.
    whole: bool,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    pub fn new() -> Server {
        Server {
            renderer: DocumentRenderer::new(),
            doc: None,
            options: Options::default(),
            size: None,
            top: 0.0,
            follow: None,
            lines: 1,
            end: None,
            dirty: true,
            whole: true,
        }
    }

    /// Takes a request in. Nothing is drawn until [`frame`](Self::frame). False for
    /// [`Request::Quit`].
    pub fn apply(&mut self, req: Request) -> bool {
        match req {
            Request::Text { text, base_dir } => {
                let doc = mdview_core::parse(&text);
                self.lines = text.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
                if self.options.base_dir != base_dir {
                    self.options.base_dir = base_dir;
                }
                // A new text is laid out anew: the renderer's own check sees only the length.
                if self.size.is_some() {
                    self.renderer.layout(&doc, &self.options);
                }
                self.doc = Some(doc);
                // The page shows the caret in the new text; it stays still while it does.
                if let Some((line, column)) = self.follow {
                    self.go_to(line, column);
                }
            }
            Request::View {
                width,
                height,
                scale,
                dark,
            } => {
                let scale = if scale.is_finite() && scale > 0.1 {
                    scale
                } else {
                    1.0
                };
                let options = Options {
                    width: width as f32 / scale,
                    scale,
                    theme: if dark { DARK } else { LIGHT },
                    max_height: None,
                    base_dir: self.options.base_dir.clone(),
                };
                let new_size = Some((width.max(1), height.max(1)));
                if new_size != self.size || !same(&options, &self.options) {
                    self.size = new_size;
                    self.options = options;
                    if let Some(doc) = &self.doc {
                        self.renderer.layout(doc, &self.options);
                    }
                    self.whole = true;
                    if let Some((line, column)) = self.follow {
                        self.go_to(line, column);
                    }
                }
            }
            Request::Follow { line, column } => {
                self.follow = Some((line, column));
                self.go_to(line, column);
            }
            Request::Scroll { by } => {
                if by.is_finite() {
                    self.top = (self.top + by).max(0.0);
                }
            }
            Request::Align { line, tail, end } => {
                self.end = (tail.is_finite() && end.is_finite()).then_some((tail, end));
                self.align(line);
            }
            Request::Quit => return false,
        }
        self.dirty = true;
        true
    }

    /// Moves the page so that source line `line` shows, only when that line is not in view
    /// already, so a caret moving within the pane leaves the page still.
    fn go_to(&mut self, line: u32, column: u32) {
        let (Some(doc), Some((_, height))) = (&self.doc, self.size) else {
            return;
        };
        let Some(block) = doc.block_at_line(line) else {
            return;
        };
        let text = doc.blocks[block as usize].text;
        let target = if text.is_empty() {
            Jump::Block(block)
        } else {
            Jump::Text(doc.text_at_line(line, column).unwrap_or(text.start))
        };
        let margin = MARGIN * self.options.scale;
        let height = height as f32;
        let Some(top) = self
            .renderer
            .jump_to(doc, &self.options, target, margin, height)
        else {
            return;
        };
        // The line's row is `top + margin`: in view, with a margin, the page stays.
        let row = top + margin;
        if row >= self.top + margin && row <= self.top + height - 2.0 * margin {
            self.renderer.set_top(self.top);
            return;
        }
        self.top = top.round();
    }

    /// Puts source line `line` (fractional) at the pane's top, as the editor's top is; over the
    /// editor's last screen the page goes on to its bottom ([`Sync::row`]).
    fn align(&mut self, line: f32) {
        if !line.is_finite() {
            return;
        }
        let Some(sync) = self.sync() else {
            return;
        };
        self.top = sync.row(line).round();
        self.renderer.set_top(self.top);
    }

    /// The source line at page row `top`: the inverse of [`align`](Self::align), so that the
    /// lead going from one side to the other moves neither.
    fn line_at(&self, top: f32) -> f32 {
        self.sync().map_or(1.0, |s| s.line(top))
    }

    fn sync(&self) -> Option<Sync> {
        let (doc, page) = (self.doc.as_ref()?, self.renderer.page_layout()?);
        let height = self.size.map_or(0.0, |(_, h)| h as f32);
        Some(Sync {
            knots: knots(doc, page.blocks(), self.lines),
            bottom: (page.total_height() - height).max(0.0),
            end: self.end,
        })
    }

    /// Draws what changed since the last frame. `None` when nothing did, or before the pane's
    /// size and a text are known.
    pub fn frame(&mut self) -> Option<Update> {
        if !self.dirty {
            return None;
        }
        let (Some(doc), Some((_, height))) = (&self.doc, self.size) else {
            return None;
        };
        self.dirty = false;
        let started = Instant::now();
        let frame = self
            .renderer
            .view_frame(doc, &self.options, self.top, height as f32);
        let (width, rows) = (frame.bitmap.width, frame.bitmap.height);
        let row = width as usize * 4;
        let band = if self.whole { None } else { frame.band };
        let pixels = match band {
            None => frame.bitmap.pixels.clone(),
            Some((_, 0)) if frame.shift == 0 => {
                self.top = frame.top;
                return None;
            }
            Some((first, count)) => {
                frame.bitmap.pixels[first as usize * row..(first + count) as usize * row].to_vec()
            }
        };
        let (top, shift) = (frame.top, frame.shift);
        self.top = top;
        self.whole = false;
        let total = self
            .renderer
            .page_layout()
            .map_or(0.0, |page| page.total_height());
        let line = self.line_at(top);
        Some(Update {
            width,
            height: rows,
            top,
            total,
            shift,
            band,
            pixels,
            micros: started.elapsed().as_micros().min(u32::MAX as u128) as u32,
            answers: 0,
            line,
        })
    }
}

/// The places where the source and the page are known to line up, `(line, row)`, both
/// increasing: the page's top at line 1, each top-level block's first line at its row, and the
/// end of the source at the last block's end. Synced scrolling goes straight between them, so a
/// block's lines share its height evenly, and the two directions are each other's inverse.
fn knots(doc: &Document, blocks: &[BlockBox], lines: u32) -> Vec<(f32, f32)> {
    let mut k = Vec::with_capacity(blocks.len() + 2);
    k.push((1.0f32, 0.0f32));
    for b in blocks {
        let line = doc.blocks.get(b.block as usize).map_or(0, |d| d.line) as f32;
        let &(l, r) = k.last().unwrap();
        // Out of order (a block drawn away from its source) or not below: skipped.
        if line > l && b.y >= r {
            k.push((line, b.y));
        }
    }
    if let Some(last) = blocks.last() {
        let end = (lines as f32 + 1.0, last.y + last.height);
        let &(l, r) = k.last().unwrap();
        if end.0 > l && end.1 >= r {
            k.push(end);
        }
    }
    k
}

/// How the editor's top and the page's top go together.
struct Sync {
    knots: Vec<(f32, f32)>,
    /// The page's largest top.
    bottom: f32,
    /// The editor's last screen of scrolling, `(tail, end)`.
    end: Option<(f32, f32)>,
}

impl Sync {
    /// The page's top for the editor's top `line`: the same source line at both tops, except
    /// over the editor's last screen, where the page goes on straight to its bottom. The page
    /// is mostly the longer, so its top line alone would leave its end unreachable; when it is
    /// the shorter, it stops at its bottom as before.
    fn row(&self, line: f32) -> f32 {
        let same = row_of_line(&self.knots, line).clamp(0.0, self.bottom);
        match self.end {
            Some((tail, end)) if end > tail && self.bottom > 0.0 => {
                let t = ((line - tail) / (end - tail)).clamp(0.0, 1.0);
                same + (self.bottom - same) * t
            }
            _ => same,
        }
    }

    /// The editor's top for page row `row`: [`row`](Self::row)'s inverse. The page at its
    /// bottom is the editor at its end.
    fn line(&self, row: f32) -> f32 {
        let Some((_, end)) = self.end.filter(|&(t, e)| e > t && self.bottom > 0.0) else {
            return line_of_row(&self.knots, row);
        };
        if row >= self.bottom - 0.5 {
            return end;
        }
        // `row` never goes down as the line goes on: halve the interval to the line.
        let (mut lo, mut hi) = (1.0f32, end.max(1.0));
        for _ in 0..40 {
            let mid = (lo + hi) / 2.0;
            if self.row(mid) < row {
                lo = mid;
            } else {
                hi = mid;
            }
            if hi - lo < 1e-4 {
                break;
            }
        }
        (lo + hi) / 2.0
    }
}

/// Straight between the two knots around `x` (in `.0` when `by_line`, else in `.1`).
fn between(k: &[(f32, f32)], x: f32, by_line: bool) -> f32 {
    let key = |p: &(f32, f32)| if by_line { p.0 } else { p.1 };
    let val = |p: &(f32, f32)| if by_line { p.1 } else { p.0 };
    let i = k.partition_point(|p| key(p) <= x);
    match (i.checked_sub(1).map(|i| &k[i]), k.get(i)) {
        (Some(a), Some(b)) if key(b) > key(a) => {
            let t = (x - key(a)) / (key(b) - key(a));
            val(a) + t * (val(b) - val(a))
        }
        (Some(a), _) => val(a),
        (None, Some(b)) => val(b),
        (None, None) => 0.0,
    }
}

fn row_of_line(k: &[(f32, f32)], line: f32) -> f32 {
    between(k, line, true)
}

fn line_of_row(k: &[(f32, f32)], row: f32) -> f32 {
    between(k, row, false)
}

fn same(a: &Options, b: &Options) -> bool {
    a.width == b.width
        && (a.scale - b.scale).abs() < 0.001
        && a.theme.key() == b.theme.key()
        && a.base_dir == b.base_dir
}

/// Runs a preview until `requests` ends or asks to quit: each turn takes every request that is
/// waiting, then draws once and hands the update to `send` (false from `send` ends it too).
pub fn serve(requests: Receiver<Request>, mut send: impl FnMut(Update) -> bool) {
    let mut server = Server::new();
    while let Ok(first) = requests.recv() {
        if !server.apply(first) {
            return;
        }
        let mut answers = 1;
        while let Ok(more) = requests.try_recv() {
            if !server.apply(more) {
                return;
            }
            answers += 1;
        }
        // Every batch is answered, a frame or an ack, so the editor knows nothing is pending.
        let update = match server.frame() {
            Some(update) => Update { answers, ..update },
            None => Update::ack(answers),
        };
        if !send(update) {
            return;
        }
    }
}

/// `mdview --preview-server`: [`serve`] over standard input and output, after
/// [`MAGIC`](wire::MAGIC). Ends with the input. Returns the exit code.
pub fn serve_stdio() -> i32 {
    let mut out = BufWriter::with_capacity(1 << 20, io::stdout().lock());
    if out
        .write_all(wire::MAGIC)
        .and_then(|_| out.flush())
        .is_err()
    {
        return 1;
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut input = BufReader::new(io::stdin().lock());
        while let Ok(Some(req)) = wire::read_request(&mut input) {
            if tx.send(req).is_err() {
                break;
            }
        }
    });
    serve(rx, |update| wire::write_update(&mut out, &update).is_ok());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(n: usize) -> String {
        (0..n)
            .map(|i| format!("Paragraph number {i}, with some words to wrap.\n\n"))
            .collect()
    }

    #[test]
    fn a_frame_comes_once_the_size_and_text_are_known_and_only_when_something_changed() {
        let mut s = Server::new();
        s.apply(Request::Text {
            text: text(50),
            base_dir: None,
        });
        assert!(s.frame().is_none(), "no size yet");
        s.apply(Request::View {
            width: 400,
            height: 300,
            scale: 1.0,
            dark: false,
        });
        let first = s.frame().expect("a frame");
        assert_eq!((first.width, first.height), (400, 300));
        assert_eq!(first.band, None, "the first frame is whole");
        assert_eq!(first.pixels.len(), 400 * 300 * 4);
        assert!(first.total > 300.0);
        assert!(s.frame().is_none(), "nothing changed");

        // A scroll sends only the rows that came into view.
        s.apply(Request::Scroll { by: 40.0 });
        let moved = s.frame().expect("a frame");
        assert_eq!(moved.top, 40.0);
        assert_eq!(moved.shift, 40);
        assert_eq!(moved.band, Some((260, 40)));
        let mut copy = first.pixels.clone();
        let mut size = (400, 300);
        assert!(moved.apply(&mut copy, &mut size));
    }

    #[test]
    fn the_caret_line_is_brought_into_view_and_a_move_inside_it_keeps_the_page() {
        let mut s = Server::new();
        s.apply(Request::View {
            width: 400,
            height: 300,
            scale: 1.0,
            dark: true,
        });
        s.apply(Request::Text {
            text: text(200),
            base_dir: None,
        });
        s.frame();
        // Paragraph 100 is on line 201.
        s.apply(Request::Follow {
            line: 201,
            column: 0,
        });
        let far = s.frame().expect("moved").top;
        assert!(far > 1000.0, "{far}");
        // The next paragraph is in view: the page stays.
        s.apply(Request::Follow {
            line: 203,
            column: 0,
        });
        assert!(s.frame().is_none_or(|u| u.top == far));
        // Typing keeps the caret's place in view.
        s.apply(Request::Text {
            text: format!("# Title\n\n{}", text(200)),
            base_dir: None,
        });
        let after = s.frame().expect("a frame").top;
        assert!((after - far).abs() < 200.0, "{far} {after}");
    }

    #[test]
    fn lines_and_rows_map_straight_between_the_blocks_and_back() {
        // Blocks on lines 3 and 10, at rows 24 and 200 (the second 100 high), 20 lines.
        let k = vec![(1.0, 0.0), (3.0, 24.0), (10.0, 200.0), (21.0, 300.0)];
        assert_eq!(row_of_line(&k, 3.0), 24.0);
        assert_eq!(row_of_line(&k, 6.5), 112.0);
        assert_eq!(row_of_line(&k, 1.0), 0.0);
        assert_eq!(row_of_line(&k, 99.0), 300.0, "past the end: the end");
        for line in [1.0, 2.0, 3.0, 4.25, 9.9, 15.0, 21.0] {
            let back = line_of_row(&k, row_of_line(&k, line));
            assert!((back - line).abs() < 1e-3, "{line} came back as {back}");
        }
    }

    #[test]
    fn the_editors_top_line_goes_to_the_panes_top_and_the_update_says_where_it_is() {
        let mut s = Server::new();
        s.apply(Request::View {
            width: 400,
            height: 300,
            scale: 1.0,
            dark: false,
        });
        s.apply(Request::Text {
            text: text(200),
            base_dir: None,
        });
        let first = s.frame().expect("a frame");
        assert!(first.line < 2.0, "{}", first.line);
        // Paragraph 100 is on line 201: aligned, its block is at the top.
        s.apply(Request::Align {
            line: 201.0,
            tail: 0.0,
            end: 0.0,
        });
        let at = s.frame().expect("moved");
        assert!(at.top > 1000.0, "{}", at.top);
        assert!((at.line - 201.0).abs() < 0.5, "{}", at.line);
        // Half a line further: the page moves down a little, not a block.
        s.apply(Request::Align {
            line: 201.5,
            tail: 0.0,
            end: 0.0,
        });
        let half = s.frame().expect("moved");
        assert!(
            half.top > at.top && half.top - at.top < 40.0,
            "{} {}",
            at.top,
            half.top
        );
        // The wheel moves the page; the update gives the line now at the top.
        s.apply(Request::Scroll { by: 400.0 });
        let wheel = s.frame().expect("moved");
        assert!(wheel.line > 205.0, "{}", wheel.line);
    }

    #[test]
    fn over_the_editors_last_screen_the_page_goes_on_to_its_bottom_and_back() {
        // 20 lines, the page 3000 rows, its bottom at 2700; the editor ends at line 15, a
        // screen being 5 lines.
        let sync = Sync {
            knots: vec![(1.0, 0.0), (10.0, 1000.0), (21.0, 3000.0)],
            bottom: 2700.0,
            end: Some((10.0, 15.0)),
        };
        assert_eq!(sync.row(1.0), 0.0);
        assert_eq!(
            sync.row(10.0),
            1000.0,
            "before the last screen: the same line"
        );
        assert_eq!(
            sync.row(15.0),
            2700.0,
            "the editor at its end: the page at its own"
        );
        assert_eq!(sync.row(19.0), 2700.0);
        let mut last = -1.0;
        for i in 0..=140 {
            let line = 1.0 + i as f32 * 0.1;
            let row = sync.row(line);
            assert!(row >= last, "the page goes back up at line {line}");
            last = row;
            let back = sync.line(row);
            assert!((back - line).abs() < 0.01, "{line} came back as {back}");
        }
        // The page shorter than the editor's end: it stops at its bottom, and the page at its
        // bottom is the editor at its end.
        let short = Sync {
            bottom: 900.0,
            ..sync
        };
        assert_eq!(short.row(12.0), 900.0);
        assert_eq!(short.line(900.0), 15.0);
        assert!((short.line(short.row(5.0)) - 5.0).abs() < 0.01);
        // No last screen (the editor does not scroll): the same line, the page at its top.
        let still = Sync {
            end: Some((1.0, 1.0)),
            ..short
        };
        assert_eq!(still.row(1.0), 0.0);
    }

    #[test]
    fn the_editor_at_its_end_puts_the_page_at_its_end() {
        let mut s = Server::new();
        s.apply(Request::View {
            width: 400,
            height: 300,
            scale: 1.0,
            dark: false,
        });
        s.apply(Request::Text {
            text: text(200),
            base_dir: None,
        });
        s.frame().expect("a frame");
        // The editor shows 30 lines of the 400: its end is line 371, its last screen from 341.
        let at = |s: &mut Server, line: f32| {
            s.apply(Request::Align {
                line,
                tail: 341.0,
                end: 371.0,
            });
            s.frame().expect("moved")
        };
        let end = at(&mut s, 371.0);
        assert_eq!(end.top, end.total - 300.0, "the page at its bottom");
        assert_eq!(end.line, 371.0);
        let before = at(&mut s, 341.0);
        assert!(before.top < end.top && (before.line - 341.0).abs() < 0.05);
        // The wheel over the page from there: the editor's line goes down with it, to its end.
        s.apply(Request::Scroll { by: 100.0 });
        let wheel = s.frame().expect("moved");
        assert!(wheel.line > 341.0 && wheel.line < 371.0, "{}", wheel.line);
        s.apply(Request::Scroll { by: 1e6 });
        assert_eq!(s.frame().expect("moved").line, 371.0);
    }

    #[test]
    fn the_loop_draws_once_for_everything_that_was_waiting() {
        let (tx, rx) = mpsc::channel();
        tx.send(Request::View {
            width: 200,
            height: 100,
            scale: 1.0,
            dark: false,
        })
        .unwrap();
        tx.send(Request::Text {
            text: "# One\n".into(),
            base_dir: None,
        })
        .unwrap();
        tx.send(Request::Text {
            text: "# Two\n".into(),
            base_dir: None,
        })
        .unwrap();
        tx.send(Request::Quit).unwrap();
        let mut answers = Vec::new();
        serve(rx, |u| {
            answers.push(u.answers);
            true
        });
        // Quit came with the rest, so nothing was drawn; a batch is answered once.
        assert!(answers.len() <= 1, "{answers:?}");
    }
}
