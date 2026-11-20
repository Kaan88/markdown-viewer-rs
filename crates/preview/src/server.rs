//! The loop that draws the page for an editor: in mdview's process (`mdview --preview-server`,
//! [`serve_stdio`]) or on a thread of the editor's own ([`serve`]).

use crate::wire::{self, BlockLines, Copied, Pointer, Request, Update};
use mdview_core::model::Document;
use mdview_render::{BlockBox, DARK, DocumentRenderer, Gap, Jump, LIGHT, Options};
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
    /// The selection, two offsets into `Document::text`, anchor first, as mdview keeps it.
    selection: Option<(u32, u32)>,
    /// A press in progress: the range a drag extends from, and whether the pointer moved.
    press: Option<((u32, u32), bool)>,
    /// The selection's rectangles as the editor last got them.
    shown: Vec<[f32; 4]>,
    /// The selection (or the layout under it) changed since the editor last got it.
    selection_dirty: bool,
    /// The editor asked for the selection's text.
    copy: bool,
    /// The text the page was parsed from, as the editor sent it: where a task box's mark is.
    source: String,
    /// A task box was clicked: its mark's byte offset in `source` and its new state.
    task: Option<(u32, bool)>,
    /// The next text is the editor's answer to a tick: the page stays where it is, since the
    /// caret did not move to the box (a moved caret still comes as its own `Follow`).
    keep_top: bool,
    /// The answer to the last `Hover`, until it is sent.
    pointer: Option<Pointer>,
    /// The answer to the last `BlockAt`, until it is sent.
    block: Option<Option<BlockLines>>,
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
            selection: None,
            press: None,
            shown: Vec::new(),
            selection_dirty: false,
            copy: false,
            source: String::new(),
            task: None,
            keep_top: false,
            pointer: None,
            block: None,
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
                self.source = text;
                // The offsets were into the old text.
                self.selection = None;
                self.press = None;
                self.selection_dirty = true;
                // The page shows the caret in the new text; it stays still while it does. After a
                // tick the reader is looking at the box, not the caret: the page stays.
                if std::mem::take(&mut self.keep_top) {
                    self.renderer.set_top(self.top);
                } else if let Some((line, column)) = self.follow {
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
                    self.selection_dirty = true;
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
            Request::Press { x, y, clicks } => {
                self.press = None;
                if let Some(flip) = (clicks <= 1).then(|| self.task_at(x, y)).flatten() {
                    // A click on a task box ticks it in the editor's text, not a selection.
                    self.task = Some(flip);
                    self.keep_top = true;
                    if self.selection.take().is_some() {
                        self.selection_dirty = true;
                    }
                } else if let Some((doc, offset)) = self.offset_at(x, y) {
                    let anchor = match clicks {
                        0 | 1 => (offset, offset),
                        2 => mdview_core::select::word_range(doc, offset),
                        _ => mdview_core::select::block_range(doc, offset),
                    };
                    self.selection = Some(anchor);
                    self.press = Some((anchor, clicks > 1));
                    self.selection_dirty = true;
                }
            }
            Request::Drag { x, y } => {
                if let Some(((a, b), _)) = self.press
                    && let Some((_, offset)) = self.offset_at(x, y)
                {
                    let selection = if offset < a {
                        (b, offset)
                    } else if offset > b {
                        (a, offset)
                    } else {
                        (a, b)
                    };
                    if let Some((_, moved)) = self.press.as_mut() {
                        *moved |= offset != a;
                    }
                    if self.selection != Some(selection) {
                        self.selection = Some(selection);
                        self.selection_dirty = true;
                    }
                }
            }
            Request::Hover { x, y } => {
                self.pointer = Some(self.pointer_at(x, y));
            }
            Request::Release => {
                // A click that did not move clears the selection; a drag leaves it.
                if let Some((_, false)) = self.press.take()
                    && self.selection.take().is_some()
                {
                    self.selection_dirty = true;
                }
            }
            Request::SelectAll => {
                if let Some(doc) = &self.doc {
                    self.selection = Some((0, doc.text.len() as u32));
                    self.selection_dirty = true;
                }
            }
            Request::Unselect => {
                self.press = None;
                if self.selection.take().is_some() {
                    self.selection_dirty = true;
                }
            }
            Request::Copy => self.copy = true,
            Request::Gap { gap } => {
                let gap = gap.map(|(first_line, last_line, height)| Gap {
                    first_line,
                    last_line,
                    height: if height.is_finite() {
                        height.max(0.0)
                    } else {
                        0.0
                    },
                });
                let empty = Document::default();
                self.renderer
                    .set_gap(self.doc.as_ref().unwrap_or(&empty), gap);
            }
            Request::BlockAt { x, y } => self.block = Some(self.block_at(x, y)),
            Request::BlockOfLine { line } => self.block = Some(self.block_of_line(line)),
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

    /// The text offset under pane point `x, y` (device pixels), with the document. Finding it
    /// may lay out the block there, which the next frame shows.
    fn offset_at(&mut self, x: f32, y: f32) -> Option<(&Document, u32)> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        let doc = self.doc.as_ref()?;
        let hit = self
            .renderer
            .hit_test(doc, &self.options, x, y + self.top)?;
        Some((doc, hit.offset))
    }

    /// The task box at `x, y` (pane pixels), when there is one and the text agrees with the page
    /// about every box: the byte offset of its mark in the text and the state a click gives it.
    fn task_at(&mut self, x: f32, y: f32) -> Option<(u32, bool)> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        let doc = self.doc.as_ref()?;
        let block = self
            .renderer
            .hit_test(doc, &self.options, x, y + self.top)?
            .task?;
        let boxes = mdview_core::tasks::document_boxes(doc);
        let index = boxes.iter().position(|&(b, _)| b == block)?;
        let states: Vec<bool> = boxes.iter().map(|&(_, checked)| checked).collect();
        let (at, byte) = mdview_core::tasks::toggle(self.source.as_bytes(), index, &states).ok()?;
        Some((at as u32, byte == b'x'))
    }

    /// The cursor for `x, y` (pane pixels): a hand over a task box, the text cursor over text.
    fn pointer_at(&mut self, x: f32, y: f32) -> Pointer {
        if !(x.is_finite() && y.is_finite()) {
            return Pointer::Arrow;
        }
        let Some(doc) = self.doc.as_ref() else {
            return Pointer::Arrow;
        };
        match self.renderer.hit_test(doc, &self.options, x, y + self.top) {
            Some(hit) if hit.task.is_some() => Pointer::Hand,
            Some(hit) if hit.on_text => Pointer::Text,
            _ => Pointer::Arrow,
        }
    }

    /// The top-level block at pane point `x, y` and its source lines, from the layout: a point
    /// in the side margins goes to the block beside it, one below the last block to none.
    fn block_at(&mut self, x: f32, y: f32) -> Option<BlockLines> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        let doc = self.doc.as_ref()?;
        let page = self.renderer.page_layout()?;
        let span = page.block_at(y + self.top)?;
        if y + self.top > span.y + span.height {
            return None;
        }
        let (first, next) = self.renderer.block_lines(doc, span.block)?;
        // The page's text column, as the renderer lays it out (`Pager::new`).
        let o = &self.options;
        let scale = o.scale.max(0.1);
        let width = o.width.max(120.0);
        let column = (width - 2.0 * o.theme.margin)
            .max(80.0)
            .min(o.theme.reading_width);
        let left = (width - column) / 2.0 * scale;
        let down = (y + self.top - span.y) / span.height.max(1.0);
        let across = (x - left) / (column * scale);
        Some(BlockLines {
            first,
            next,
            at: Some((down.clamp(0.0, 1.0), across.clamp(0.0, 1.0))),
        })
    }

    /// The top-level block that holds source line `line` (from 1): the last that starts on it
    /// or before.
    fn block_of_line(&self, line: u32) -> Option<BlockLines> {
        let doc = self.doc.as_ref()?;
        let page = self.renderer.page_layout()?;
        let span = page
            .blocks()
            .iter()
            .take_while(|s| {
                doc.blocks
                    .get(s.block as usize)
                    .is_some_and(|b| b.line <= line)
            })
            .last()?;
        let (first, next) = self.renderer.block_lines(doc, span.block)?;
        Some(BlockLines {
            first,
            next,
            at: None,
        })
    }

    /// The answer to the last [`Request::BlockAt`], for [`Update::block`].
    pub fn take_block(&mut self) -> Option<Option<BlockLines>> {
        self.block.take()
    }

    /// Where the room for the editor's box starts on the page, while there is one.
    pub fn gap_y(&self) -> Option<f32> {
        self.renderer.gap_y(self.doc.as_ref()?)
    }

    /// The answer to the last [`Request::Hover`], for [`Update::pointer`].
    pub fn take_pointer(&mut self) -> Option<Pointer> {
        self.pointer.take()
    }

    /// The task box clicked since the last answer, for [`Update::task`].
    pub fn take_task(&mut self) -> Option<(u32, bool)> {
        self.task.take()
    }

    /// The selected range, ordered, when it is not empty.
    fn selected(&self) -> Option<(u32, u32)> {
        let (a, b) = self.selection?;
        (a != b).then(|| (a.min(b), a.max(b)))
    }

    /// What an answer carries besides the frame: the selection's rectangles when they changed
    /// (`drew`: a frame was drawn, which can move the blocks under them), and the selected text
    /// when the editor asked for it.
    pub fn selection_answer(&mut self, drew: bool) -> (Option<Vec<[f32; 4]>>, Option<Copied>) {
        let copied = std::mem::take(&mut self.copy).then(|| match (self.selected(), &self.doc) {
            (Some((from, to)), Some(doc)) => Copied {
                text: doc
                    .text
                    .get(from as usize..to as usize)
                    .unwrap_or_default()
                    .to_string(),
                html: mdview_core::rich::selection_html(doc, from, to).unwrap_or_default(),
            },
            _ => Copied::default(),
        });
        let look = self.selection_dirty || (drew && self.selected().is_some());
        self.selection_dirty = false;
        if !look {
            return (None, copied);
        }
        let rects: Vec<[f32; 4]> = match (self.selected(), &self.doc) {
            (Some((from, to)), Some(doc)) if self.size.is_some() => self
                .renderer
                .selection_rects(doc, &self.options, from, to)
                .into_iter()
                .map(|r| [r.x, r.y, r.w, r.h])
                .collect(),
            _ => Vec::new(),
        };
        if rects == self.shown {
            return (None, copied);
        }
        self.shown = rects.clone();
        (Some(rects), copied)
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
            selection: None,
            copied: None,
            task: None,
            pointer: None,
            gap_y: None,
            block: None,
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
        let drawn = server.frame();
        let (selection, copied) = server.selection_answer(drawn.is_some());
        let update = Update {
            selection,
            copied,
            task: server.take_task(),
            pointer: server.take_pointer(),
            block: server.take_block(),
            gap_y: server.gap_y(),
            ..match drawn {
                Some(update) => Update { answers, ..update },
                None => Update::ack(answers),
            }
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

    /// A server with `text` on a 400 by 300 pane, its first frame taken.
    fn shown(text: &str) -> Server {
        let mut s = Server::new();
        s.apply(Request::View {
            width: 400,
            height: 300,
            scale: 1.0,
            dark: false,
        });
        s.apply(Request::Text {
            text: text.into(),
            base_dir: None,
        });
        s.frame().expect("a frame");
        s.selection_answer(true);
        s
    }

    #[test]
    fn a_block_is_found_by_its_place_and_a_gap_keeps_room_where_it_was() {
        let mut s = shown(
            "# Title

First.

- one
- two

Last.
",
        );
        let spans: Vec<BlockBox> = s.renderer.page_layout().unwrap().blocks().to_vec();
        let list = spans[2];
        s.apply(Request::BlockAt {
            x: 200.0,
            y: list.y + 2.0,
        });
        let got = s.take_block().flatten().expect("the list");
        assert_eq!((got.first, got.next), (5, Some(8)));
        let (down, across) = got.at.expect("where in it");
        assert!(
            (0.0..0.5).contains(&down) && (0.0..=1.0).contains(&across),
            "{down} {across}"
        );
        s.apply(Request::BlockOfLine { line: 6 });
        let got = s.take_block().flatten().expect("the list again");
        assert_eq!((got.first, got.next, got.at), (5, Some(8), None));
        s.apply(Request::BlockOfLine { line: 7 });
        assert_eq!(
            s.take_block().flatten().map(|b| b.first),
            Some(5),
            "a blank line after it"
        );
        assert_eq!(s.take_block(), None, "once per question");
        s.apply(Request::BlockAt { x: 200.0, y: 299.0 });
        assert_eq!(s.take_block(), Some(None), "below the last block");

        s.apply(Request::Gap {
            gap: Some((5, 6, 90.0)),
        });
        s.frame().expect("a frame");
        assert_eq!(s.gap_y(), Some(list.y));
        let after = s.renderer.page_layout().unwrap().blocks()[3];
        assert_eq!(after.y, list.y + 90.0);
        // A new text keeps the gap; clearing it brings the list back.
        s.apply(Request::Text {
            text: "# Title

First.

- one
- two
- three

Last.
"
            .into(),
            base_dir: None,
        });
        s.frame().expect("a frame");
        assert_eq!(s.gap_y(), Some(list.y));
        s.apply(Request::Gap { gap: None });
        s.frame().expect("a frame");
        assert_eq!(s.gap_y(), None);
        assert!(s.renderer.page_layout().unwrap().blocks()[2].height > 0.0);
    }

    /// Applies `reqs` as one batch and returns what the answer carries.
    fn answer(s: &mut Server, reqs: Vec<Request>) -> (Option<Vec<[f32; 4]>>, Option<Copied>) {
        for r in reqs {
            s.apply(r);
        }
        let drew = s.frame().is_some();
        s.selection_answer(drew)
    }

    fn copy(s: &mut Server) -> String {
        answer(s, vec![Request::Copy]).1.expect("an answer").text
    }

    #[test]
    fn a_click_on_a_task_box_gives_its_mark_and_selects_nothing() {
        let text = "Tasks:\n\n- [ ] one\n- [x] two\n\n```\n- [ ] code\n```\n";
        let mut s = shown(text);
        // Every point of the pane a click would tick, and what it gives.
        let mut found = Vec::new();
        for y in (0..300).step_by(2) {
            for x in (0..200).step_by(2) {
                s.apply(Request::Press {
                    x: x as f32,
                    y: y as f32,
                    clicks: 1,
                });
                s.apply(Request::Release);
                if let Some(t) = s.take_task()
                    && !found.contains(&t)
                {
                    found.push(t);
                }
            }
        }
        // The two boxes, by the bytes between their brackets; the fenced one is not a box.
        assert_eq!(found, [(11, true), (21, false)]);
        assert_eq!(&text[11..12], " ");
        assert_eq!(&text[21..22], "x");
        // A click on a box selects nothing; a double click there selects a word as before.
        let (bx, by) = (0..300)
            .step_by(2)
            .flat_map(|y| (0..200).step_by(2).map(move |x| (x as f32, y as f32)))
            .find(|&(x, y)| s.task_at(x, y).is_some())
            .expect("a box");
        let (rects, _) = answer(&mut s, vec![Request::SelectAll]);
        assert!(rects.is_some_and(|r| !r.is_empty()));
        let (rects, _) = answer(
            &mut s,
            vec![Request::Press {
                x: bx,
                y: by,
                clicks: 1,
            }],
        );
        assert_eq!(rects, Some(vec![]), "the click cleared the selection");
        assert!(s.take_task().is_some());
        answer(
            &mut s,
            vec![Request::Press {
                x: bx,
                y: by,
                clicks: 2,
            }],
        );
        assert!(s.take_task().is_none());
        // Over the box the cursor is a hand; over the words, the text cursor.
        s.apply(Request::Hover { x: bx, y: by });
        assert_eq!(s.take_pointer(), Some(Pointer::Hand));
        assert_eq!(s.take_pointer(), None, "once per hover");
        let on_text = (0..300)
            .step_by(2)
            .flat_map(|y| (0..200).step_by(2).map(move |x| (x as f32, y as f32)))
            .any(|(x, y)| s.pointer_at(x, y) == Pointer::Text);
        assert!(on_text);
        assert_eq!(s.pointer_at(399.0, 299.0), Pointer::Arrow);
        // A text that no longer agrees with the page about its boxes ticks nothing.
        s.source = "- [ ] one\n".into();
        s.apply(Request::Press {
            x: bx,
            y: by,
            clicks: 1,
        });
        assert!(s.take_task().is_none());
    }

    #[test]
    fn the_text_a_tick_brings_leaves_the_page_where_it_is() {
        // A long page with a box far down, the caret (Follow) at the first line.
        let mut text: String = (0..120).map(|i| format!("Line {i}.\n\n")).collect();
        text.push_str("- [ ] far down\n");
        let mut s = shown(&text);
        s.apply(Request::Follow { line: 1, column: 0 });
        s.apply(Request::Scroll { by: 1e6 });
        let bottom = s.frame().expect("scrolled").top;
        assert!(bottom > 0.0);
        let (bx, by) = (0..300)
            .step_by(2)
            .flat_map(|y| (0..200).step_by(2).map(move |x| (x as f32, y as f32)))
            .find(|&(x, y)| s.task_at(x, y).is_some())
            .expect("the box in view");
        s.apply(Request::Press {
            x: bx,
            y: by,
            clicks: 1,
        });
        s.apply(Request::Release);
        let (at, checked) = s.take_task().expect("ticked");
        assert!(checked);
        // The editor's answer: the same text with the mark changed, and no Follow (the caret
        // did not move). The page stays at the box.
        let mut ticked = text.clone();
        ticked.replace_range(at as usize..at as usize + 1, "x");
        s.apply(Request::Text {
            text: ticked,
            base_dir: None,
        });
        let after = s.frame().map_or(s.top, |u| u.top);
        assert_eq!(after, bottom);
        // A text typed after that follows the caret again.
        s.apply(Request::Text {
            text: format!("x{text}"),
            base_dir: None,
        });
        assert!(s.frame().map_or(s.top, |u| u.top) < bottom);
    }

    #[test]
    fn a_drag_selects_the_text_under_it_and_copy_gives_it() {
        let mut s = shown("First paragraph here.\n\nSecond one, a little longer.\n");
        // A click that does not move selects nothing and copies nothing.
        let (rects, _) = answer(
            &mut s,
            vec![Request::Press {
                x: 30.0,
                y: 30.0,
                clicks: 1,
            }],
        );
        assert!(rects.is_none(), "a caret is not a selection");
        answer(&mut s, vec![Request::Release]);
        assert_eq!(copy(&mut s), "");
        // From the start of the first line down past the second paragraph: both, as text.
        let (rects, _) = answer(
            &mut s,
            vec![
                Request::Press {
                    x: 0.0,
                    y: 20.0,
                    clicks: 1,
                },
                Request::Drag { x: 399.0, y: 250.0 },
                Request::Release,
            ],
        );
        let rects = rects.expect("the selection is shown");
        assert!(rects.len() >= 2, "a rectangle a line: {rects:?}");
        let text = copy(&mut s);
        assert!(
            text.contains("First paragraph here.") && text.contains("a little longer."),
            "{text:?}"
        );
        // The same again changes nothing to show.
        assert!(answer(&mut s, vec![]).0.is_none());
        // A click elsewhere clears it.
        let (rects, _) = answer(
            &mut s,
            vec![
                Request::Press {
                    x: 30.0,
                    y: 30.0,
                    clicks: 1,
                },
                Request::Release,
            ],
        );
        assert_eq!(rects, Some(vec![]));
        assert_eq!(copy(&mut s), "");
    }

    #[test]
    fn two_clicks_take_a_word_three_the_paragraph_and_all_takes_everything() {
        let mut s = shown("Alpha beta gamma.\n\nNext paragraph.\n");
        let press = |clicks| Request::Press {
            x: 2.0,
            y: 30.0,
            clicks,
        };
        answer(&mut s, vec![press(2), Request::Release]);
        assert_eq!(copy(&mut s), "Alpha", "a double click keeps its word");
        answer(&mut s, vec![press(3), Request::Release]);
        assert_eq!(copy(&mut s), "Alpha beta gamma.");
        answer(&mut s, vec![Request::SelectAll]);
        let all = copy(&mut s);
        assert!(
            all.starts_with("Alpha") && all.contains("Next paragraph."),
            "{all:?}"
        );
        let html = answer(&mut s, vec![Request::Copy]).1.unwrap().html;
        assert!(html.contains("<p>"), "{html}");
        answer(&mut s, vec![Request::Unselect]);
        assert_eq!(copy(&mut s), "");
    }

    #[test]
    fn a_new_text_clears_the_selection_and_the_rectangles_follow_the_page() {
        let mut s = shown(&text(100));
        let (rects, _) = answer(&mut s, vec![Request::SelectAll]);
        let first = rects.expect("shown")[0];
        // The page scrolled: the rectangles are page positions, so they stay as they were.
        let (rects, _) = answer(&mut s, vec![Request::Scroll { by: 500.0 }]);
        assert!(rects.is_none_or(|r| r[0] == first));
        // A new text: the offsets were into the old one.
        let (rects, _) = answer(
            &mut s,
            vec![Request::Text {
                text: text(99),
                base_dir: None,
            }],
        );
        assert_eq!(rects, Some(vec![]));
        assert_eq!(copy(&mut s), "");
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
