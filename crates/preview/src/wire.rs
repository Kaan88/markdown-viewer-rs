//! The requests and answers, and how they go over a pipe.
//!
//! Each message is a kind (u32), a length (u32) and that many bytes, little-endian. The
//! server writes [`MAGIC`] first, so an editor that started an mdview too old to know the
//! preview server (which would open a window instead) can tell and stop it.

use std::io::{self, Read, Write};
use std::path::PathBuf;

/// The server's first line: the protocol's name and version.
pub const MAGIC: &[u8; 12] = b"MDVPREVIEW7\n";

/// The largest message either side accepts: a frame of 8K by 8K pixels.
pub const MAX_MESSAGE: usize = 8192 * 8192 * 4 + 64;

/// What the editor asks for. A server applies everything that is waiting and then draws once.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// The document's text, and the folder relative images are read from.
    Text {
        text: String,
        base_dir: Option<PathBuf>,
    },
    /// The pane: its size in device pixels, the device scale and the theme.
    View {
        width: u32,
        height: u32,
        scale: f32,
        dark: bool,
    },
    /// The caret is on source line `line` (from 1), byte `column` of it (from 0): the page
    /// shows it, moving only when it is not in view already.
    Follow { line: u32, column: u32 },
    /// The pointer's wheel: move the page `by` device pixels (down when positive).
    Scroll { by: f32 },
    /// The editor scrolled: source line `line` (from 1; the fraction is how far down that line
    /// the editor's top is) goes to the pane's top. Synced scrolling, the editor leading.
    /// `tail` to `end` are the editor's last screen of scrolling, as tops in the same units
    /// (`end` is its top when scrolled to the bottom): over it the page moves on to its own
    /// bottom, so the two reach their ends together.
    Align { line: f32, tail: f32, end: f32 },
    /// The main button went down over the page at `x, y` (device pixels from the pane's top
    /// left): a selection starts there. `clicks` is 1, 2 for a word, 3 or more for the block.
    Press { x: f32, y: f32, clicks: u32 },
    /// The pointer moved with the button down: the selection goes to `x, y`.
    Drag { x: f32, y: f32 },
    /// The button came up: a click that did not move clears the selection, a drag keeps it.
    Release,
    /// Selects the whole page.
    SelectAll,
    /// Clears the selection.
    Unselect,
    /// Asks for the selection's text: the next update carries it in [`Update::copied`].
    Copy,
    /// The pointer rests at `x, y` (device pixels from the pane's top left), no button down:
    /// the next update carries the cursor for that place in [`Update::pointer`].
    Hover { x: f32, y: f32 },
    /// The editor shows source lines `first` to `last` (from 1, both included) itself, in a
    /// box `height` device pixels tall: the top-level blocks starting on them are left out of
    /// the page and room is kept for the box ([`Update::gap_y`] says where). `None` puts them back.
    Gap { gap: Option<(u32, u32, f32)> },
    /// Asks which top-level block is at `x, y` (device pixels from the pane's top left): the
    /// next update carries its source lines in [`Update::block`].
    BlockAt { x: f32, y: f32 },
    /// Stop: the server ends.
    Quit,
}

/// One drawn frame: what changed since the last one.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Update {
    pub width: u32,
    pub height: u32,
    /// The page row at the top of the pane, and the page's height (device pixels).
    pub top: f32,
    pub total: f32,
    /// Row `r` of this frame was row `r + shift` of the last one (the page moved up `shift`
    /// rows: positive when it was scrolled down).
    pub shift: i32,
    /// The rows that are new, `(first, count)`; `None` is the whole frame.
    pub band: Option<(u32, u32)>,
    /// The new rows' pixels, `width * 4` bytes a row, as `Bitmap` keeps them.
    pub pixels: Vec<u8>,
    /// How long the server took to draw it, in microseconds.
    pub micros: u32,
    /// How many requests this answers: everything that was waiting when it was drawn.
    pub answers: u32,
    /// The source line (from 1, with a fraction) at the pane's top: where the editor goes when
    /// the page leads synced scrolling. The inverse of [`Request::Align`].
    pub line: f32,
    /// The selection, when it changed: its rectangles `[x, y, w, h]` in page pixels (`y` from
    /// the page's top, so the editor places them with `top`); empty when nothing is selected.
    /// `None`: as the last update left it.
    pub selection: Option<Vec<[f32; 4]>>,
    /// The answer to [`Request::Copy`]: the selected text, as mdview copies it.
    pub copied: Option<Copied>,
    /// A click on a task box: the byte offset of its mark (the space or `x` between the
    /// brackets) in the last [`Request::Text`], and whether the click checks it. The page does not
    /// change: the editor changes its text, which comes back as a new text.
    pub task: Option<(u32, bool)>,
    /// The answer to [`Request::Hover`]: the cursor for the place the pointer rests on.
    pub pointer: Option<Pointer>,
    /// The page row (device pixels from the page's top) where the room for the editor's box
    /// starts, while a [`Request::Gap`] is set and that row is known.
    pub gap_y: Option<f32>,
    /// The answer to [`Request::BlockAt`]: `Some(None)` when no block is there, else the block's
    /// first source line and the line the next top-level block starts on (`None` for the last
    /// block), both from 1.
    pub block: Option<Option<(u32, Option<u32>)>>,
}

/// The cursor over a place of the page, as mdview shows it there: a hand over a task box (the
/// one thing on the pane a click does), the text cursor over text, the arrow elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    Arrow,
    Text,
    Hand,
}

/// The selection for the clipboard: its text, and the same as an HTML fragment (empty when
/// there is none), as mdview puts both there.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Copied {
    pub text: String,
    pub html: String,
}

impl Update {
    /// An answer that changes nothing on screen: the requests it answers moved nothing.
    pub fn ack(answers: u32) -> Update {
        Update {
            band: Some((0, 0)),
            answers,
            ..Update::default()
        }
    }

    /// Whether this is an [`ack`](Self::ack).
    pub fn is_ack(&self) -> bool {
        self.band == Some((0, 0)) && self.shift == 0 && self.pixels.is_empty()
    }

    /// Applies the update to the editor's copy of the frame, `width * height * 4` bytes; a copy
    /// of another size is replaced by a whole frame. False when the update needs a whole frame
    /// and is only a band (the copy was lost): the caller asks again.
    pub fn apply(&self, frame: &mut Vec<u8>, size: &mut (u32, u32)) -> bool {
        let row = self.width as usize * 4;
        let rows = self.height as usize;
        match self.band {
            None => {
                if self.pixels.len() != row * rows {
                    return false;
                }
                frame.clear();
                frame.extend_from_slice(&self.pixels);
                *size = (self.width, self.height);
                true
            }
            Some((first, count)) => {
                if *size != (self.width, self.height) || frame.len() != row * rows {
                    return false;
                }
                let shift = self.shift;
                if shift != 0 && shift.unsigned_abs() as usize <= rows {
                    let by = shift.unsigned_abs() as usize * row;
                    if shift > 0 {
                        frame.copy_within(by.., 0);
                    } else {
                        let n = frame.len();
                        frame.copy_within(0..n - by, by);
                    }
                }
                let (first, count) = (first as usize, count as usize);
                if first + count > rows || self.pixels.len() != count * row {
                    return false;
                }
                frame[first * row..(first + count) * row].copy_from_slice(&self.pixels);
                true
            }
        }
    }
}

const TEXT: u32 = 1;
const VIEW: u32 = 2;
const FOLLOW: u32 = 3;
const SCROLL: u32 = 4;
const QUIT: u32 = 5;
const ALIGN: u32 = 6;
const PRESS: u32 = 7;
const DRAG: u32 = 8;
const RELEASE: u32 = 9;
const SELECT_ALL: u32 = 10;
const UNSELECT: u32 = 11;
const COPY: u32 = 12;
const HOVER: u32 = 13;
const GAP: u32 = 14;
const BLOCK_AT: u32 = 15;
const UPDATE: u32 = 16;

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_u32(out, b.len() as u32);
    out.extend_from_slice(b);
}

/// Reads a message's fields in order; any shortfall is an error.
struct Fields<'a>(&'a [u8]);

impl Fields<'_> {
    fn u32(&mut self) -> io::Result<u32> {
        let (head, rest) = self.0.split_at_checked(4).ok_or_else(short)?;
        self.0 = rest;
        Ok(u32::from_le_bytes(head.try_into().expect("four bytes")))
    }

    fn f32(&mut self) -> io::Result<f32> {
        self.u32().map(f32::from_bits)
    }

    fn bytes(&mut self) -> io::Result<&[u8]> {
        let n = self.u32()? as usize;
        let (head, rest) = self.0.split_at_checked(n).ok_or_else(short)?;
        self.0 = rest;
        Ok(head)
    }

    fn string(&mut self) -> io::Result<String> {
        String::from_utf8(self.bytes()?.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "text is not UTF-8"))
    }
}

fn short() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "message too short")
}

fn frame(kind: u32, body: &[u8], w: &mut impl Write) -> io::Result<()> {
    let mut head = Vec::with_capacity(8);
    put_u32(&mut head, kind);
    put_u32(&mut head, body.len() as u32);
    w.write_all(&head)?;
    w.write_all(body)?;
    w.flush()
}

/// Reads one message: its kind and body. `None` at the end of the stream.
fn read_message(r: &mut impl Read) -> io::Result<Option<(u32, Vec<u8>)>> {
    let mut head = [0u8; 8];
    match r.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let kind = u32::from_le_bytes(head[..4].try_into().expect("four"));
    let len = u32::from_le_bytes(head[4..].try_into().expect("four")) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message too long",
        ));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(Some((kind, body)))
}

pub fn write_request(w: &mut impl Write, req: &Request) -> io::Result<()> {
    let mut b = Vec::new();
    let kind = match req {
        Request::Text { text, base_dir } => {
            put_bytes(&mut b, text.as_bytes());
            let dir = base_dir
                .as_ref()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
            put_bytes(&mut b, dir.as_bytes());
            TEXT
        }
        Request::View {
            width,
            height,
            scale,
            dark,
        } => {
            put_u32(&mut b, *width);
            put_u32(&mut b, *height);
            put_u32(&mut b, scale.to_bits());
            put_u32(&mut b, *dark as u32);
            VIEW
        }
        Request::Follow { line, column } => {
            put_u32(&mut b, *line);
            put_u32(&mut b, *column);
            FOLLOW
        }
        Request::Scroll { by } => {
            put_u32(&mut b, by.to_bits());
            SCROLL
        }
        Request::Align { line, tail, end } => {
            put_u32(&mut b, line.to_bits());
            put_u32(&mut b, tail.to_bits());
            put_u32(&mut b, end.to_bits());
            ALIGN
        }
        Request::Press { x, y, clicks } => {
            put_u32(&mut b, x.to_bits());
            put_u32(&mut b, y.to_bits());
            put_u32(&mut b, *clicks);
            PRESS
        }
        Request::Drag { x, y } => {
            put_u32(&mut b, x.to_bits());
            put_u32(&mut b, y.to_bits());
            DRAG
        }
        Request::Release => RELEASE,
        Request::SelectAll => SELECT_ALL,
        Request::Unselect => UNSELECT,
        Request::Copy => COPY,
        Request::Hover { x, y } => {
            put_u32(&mut b, x.to_bits());
            put_u32(&mut b, y.to_bits());
            HOVER
        }
        Request::Gap { gap } => {
            let (has, (first, last, height)) = gap.map_or((0, (0, 0, 0.0)), |g| (1, g));
            put_u32(&mut b, has);
            put_u32(&mut b, first);
            put_u32(&mut b, last);
            put_u32(&mut b, height.to_bits());
            GAP
        }
        Request::BlockAt { x, y } => {
            put_u32(&mut b, x.to_bits());
            put_u32(&mut b, y.to_bits());
            BLOCK_AT
        }
        Request::Quit => QUIT,
    };
    frame(kind, &b, w)
}

/// Reads one request; `None` at the end of the stream. An unknown kind is skipped as `None`
/// would end the server, so it is read past instead.
pub fn read_request(r: &mut impl Read) -> io::Result<Option<Request>> {
    loop {
        let Some((kind, body)) = read_message(r)? else {
            return Ok(None);
        };
        let mut f = Fields(&body);
        let req = match kind {
            TEXT => {
                let text = f.string()?;
                let dir = f.string()?;
                Request::Text {
                    text,
                    base_dir: (!dir.is_empty()).then(|| PathBuf::from(dir)),
                }
            }
            VIEW => Request::View {
                width: f.u32()?,
                height: f.u32()?,
                scale: f.f32()?,
                dark: f.u32()? != 0,
            },
            FOLLOW => Request::Follow {
                line: f.u32()?,
                column: f.u32()?,
            },
            SCROLL => Request::Scroll { by: f.f32()? },
            ALIGN => Request::Align {
                line: f.f32()?,
                tail: f.f32()?,
                end: f.f32()?,
            },
            PRESS => Request::Press {
                x: f.f32()?,
                y: f.f32()?,
                clicks: f.u32()?,
            },
            DRAG => Request::Drag {
                x: f.f32()?,
                y: f.f32()?,
            },
            RELEASE => Request::Release,
            SELECT_ALL => Request::SelectAll,
            UNSELECT => Request::Unselect,
            COPY => Request::Copy,
            HOVER => Request::Hover {
                x: f.f32()?,
                y: f.f32()?,
            },
            GAP => {
                let has = f.u32()? != 0;
                let g = (f.u32()?, f.u32()?, f.f32()?);
                Request::Gap {
                    gap: has.then_some(g),
                }
            }
            BLOCK_AT => Request::BlockAt {
                x: f.f32()?,
                y: f.f32()?,
            },
            QUIT => Request::Quit,
            _ => continue,
        };
        return Ok(Some(req));
    }
}

pub fn write_update(w: &mut impl Write, u: &Update) -> io::Result<()> {
    let mut b = Vec::with_capacity(u.pixels.len() + 48);
    put_u32(&mut b, u.width);
    put_u32(&mut b, u.height);
    put_u32(&mut b, u.top.to_bits());
    put_u32(&mut b, u.total.to_bits());
    put_u32(&mut b, u.shift as u32);
    let (has, first, count) = u.band.map_or((0, 0, 0), |(a, n)| (1, a, n));
    put_u32(&mut b, has);
    put_u32(&mut b, first);
    put_u32(&mut b, count);
    put_u32(&mut b, u.micros);
    put_u32(&mut b, u.answers);
    put_u32(&mut b, u.line.to_bits());
    put_bytes(&mut b, &u.pixels);
    match &u.selection {
        None => put_u32(&mut b, 0),
        Some(rects) => {
            put_u32(&mut b, 1);
            put_u32(&mut b, rects.len() as u32);
            for r in rects {
                for v in r {
                    put_u32(&mut b, v.to_bits());
                }
            }
        }
    }
    match &u.copied {
        None => put_u32(&mut b, 0),
        Some(c) => {
            put_u32(&mut b, 1);
            put_bytes(&mut b, c.text.as_bytes());
            put_bytes(&mut b, c.html.as_bytes());
        }
    }
    match u.task {
        None => put_u32(&mut b, 0),
        Some((at, checked)) => {
            put_u32(&mut b, 1);
            put_u32(&mut b, at);
            put_u32(&mut b, checked as u32);
        }
    }
    put_u32(
        &mut b,
        match u.pointer {
            None => 0,
            Some(Pointer::Arrow) => 1,
            Some(Pointer::Text) => 2,
            Some(Pointer::Hand) => 3,
        },
    );
    match u.gap_y {
        None => put_u32(&mut b, 0),
        Some(y) => {
            put_u32(&mut b, 1);
            put_u32(&mut b, y.to_bits());
        }
    }
    // 0: no answer; 1: no block there; 2: a block, its first line, then the next block's
    // line (0 for none).
    match u.block {
        None => put_u32(&mut b, 0),
        Some(None) => put_u32(&mut b, 1),
        Some(Some((first, next))) => {
            put_u32(&mut b, 2);
            put_u32(&mut b, first);
            put_u32(&mut b, next.unwrap_or(0));
        }
    }
    frame(UPDATE, &b, w)
}

/// Reads one update; `None` at the end of the stream (the server ended).
pub fn read_update(r: &mut impl Read) -> io::Result<Option<Update>> {
    loop {
        let Some((kind, body)) = read_message(r)? else {
            return Ok(None);
        };
        if kind != UPDATE {
            continue;
        }
        let mut f = Fields(&body);
        let width = f.u32()?;
        let height = f.u32()?;
        let top = f.f32()?;
        let total = f.f32()?;
        let shift = f.u32()? as i32;
        let has = f.u32()?;
        let (first, count) = (f.u32()?, f.u32()?);
        let micros = f.u32()?;
        let answers = f.u32()?;
        let line = f.f32()?;
        let pixels = f.bytes()?.to_vec();
        let selection = if f.u32()? != 0 {
            let n = f.u32()? as usize;
            if n > MAX_MESSAGE / 16 {
                return Err(short());
            }
            let mut rects = Vec::with_capacity(n);
            for _ in 0..n {
                rects.push([f.f32()?, f.f32()?, f.f32()?, f.f32()?]);
            }
            Some(rects)
        } else {
            None
        };
        let copied = if f.u32()? != 0 {
            Some(Copied {
                text: f.string()?,
                html: f.string()?,
            })
        } else {
            None
        };
        let task = if f.u32()? != 0 {
            Some((f.u32()?, f.u32()? != 0))
        } else {
            None
        };
        let pointer = match f.u32()? {
            1 => Some(Pointer::Arrow),
            2 => Some(Pointer::Text),
            3 => Some(Pointer::Hand),
            _ => None,
        };
        let gap_y = if f.u32()? != 0 { Some(f.f32()?) } else { None };
        let block = match f.u32()? {
            0 => None,
            1 => Some(None),
            _ => {
                let first = f.u32()?;
                let next = f.u32()?;
                Some(Some((first, (next != 0).then_some(next))))
            }
        };
        return Ok(Some(Update {
            width,
            height,
            top,
            total,
            shift,
            band: (has != 0).then_some((first, count)),
            pixels,
            micros,
            answers,
            line,
            selection,
            copied,
            task,
            pointer,
            gap_y,
            block,
        }));
    }
}

/// Reads the server's first line; false when it is not [`MAGIC`].
pub fn read_magic(r: &mut impl Read) -> bool {
    let mut got = [0u8; 12];
    r.read_exact(&mut got).is_ok() && &got == MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_updates_go_over_a_pipe_unchanged() {
        let reqs = [
            Request::Text {
                text: "# Başlık\n".into(),
                base_dir: Some(PathBuf::from("/notes")),
            },
            Request::Text {
                text: String::new(),
                base_dir: None,
            },
            Request::View {
                width: 640,
                height: 480,
                scale: 1.5,
                dark: true,
            },
            Request::Follow {
                line: 12,
                column: 3,
            },
            Request::Scroll { by: -48.0 },
            Request::Align {
                line: 41.25,
                tail: 300.0,
                end: 340.5,
            },
            Request::Press {
                x: 12.5,
                y: 300.0,
                clicks: 2,
            },
            Request::Drag { x: 40.0, y: -3.0 },
            Request::Release,
            Request::SelectAll,
            Request::Unselect,
            Request::Copy,
            Request::Hover { x: 3.5, y: 70.0 },
            Request::Gap {
                gap: Some((5, 9, 120.5)),
            },
            Request::Gap { gap: None },
            Request::BlockAt { x: 3.0, y: 44.5 },
            Request::Quit,
        ];
        let mut buf = Vec::new();
        for r in &reqs {
            write_request(&mut buf, r).unwrap();
        }
        let mut rd = &buf[..];
        for r in &reqs {
            assert_eq!(read_request(&mut rd).unwrap().as_ref(), Some(r));
        }
        assert_eq!(read_request(&mut rd).unwrap(), None);

        let u = Update {
            width: 2,
            height: 3,
            top: 10.0,
            total: 900.0,
            shift: -1,
            band: Some((2, 1)),
            pixels: vec![7; 8],
            micros: 1234,
            answers: 3,
            line: 17.5,
            selection: Some(vec![[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.5]]),
            copied: Some(Copied {
                text: "Başlık".into(),
                html: "<b>Başlık</b>".into(),
            }),
            task: Some((40, true)),
            pointer: Some(Pointer::Hand),
            gap_y: Some(310.0),
            block: Some(Some((5, None))),
        };
        let mut buf = Vec::new();
        write_update(&mut buf, &u).unwrap();
        assert_eq!(read_update(&mut &buf[..]).unwrap(), Some(u));
        // No block at the point is not the same as no answer; a next block's line comes back.
        for block in [Some(None), Some(Some((3, Some(8)))), None] {
            let u = Update {
                block,
                ..Update::ack(1)
            };
            let mut buf = Vec::new();
            write_update(&mut buf, &u).unwrap();
            assert_eq!(read_update(&mut &buf[..]).unwrap(), Some(u));
        }
        // Nothing selected is not the same as the selection unchanged.
        for selection in [Some(vec![]), None] {
            let u = Update {
                selection,
                ..Update::ack(1)
            };
            let mut buf = Vec::new();
            write_update(&mut buf, &u).unwrap();
            assert_eq!(read_update(&mut &buf[..]).unwrap(), Some(u));
        }
    }

    #[test]
    fn a_band_moves_the_kept_rows_and_fills_the_new_ones() {
        // Three rows of one pixel: rows 0, 1, 2 hold 0, 1, 2.
        let mut frame: Vec<u8> = [0u8, 1, 2].iter().flat_map(|&v| [v; 4]).collect();
        let mut size = (1, 3);
        // Scrolled down one row: the kept rows move up, the last row is new.
        let u = Update {
            width: 1,
            height: 3,
            shift: 1,
            band: Some((2, 1)),
            pixels: vec![9; 4],
            ..Update::default()
        };
        assert!(u.apply(&mut frame, &mut size));
        assert_eq!(frame.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), [1, 2, 9]);
        // A band for a copy of another size is refused.
        let mut other = (2, 3);
        assert!(!u.apply(&mut frame, &mut other));
    }
}
