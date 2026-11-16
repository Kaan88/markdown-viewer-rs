//! The requests and answers, and how they go over a pipe.
//!
//! Each message is a kind (u32), a length (u32) and that many bytes, little-endian. The
//! server writes [`MAGIC`] first, so an editor that started an mdview too old to know the
//! preview server (which would open a window instead) can tell and stop it.

use std::io::{self, Read, Write};
use std::path::PathBuf;

/// The server's first line: the protocol's name and version.
pub const MAGIC: &[u8; 12] = b"MDVPREVIEW2\n";

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
    Align { line: f32 },
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
        Request::Align { line } => {
            put_u32(&mut b, line.to_bits());
            ALIGN
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
            ALIGN => Request::Align { line: f.f32()? },
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
            Request::Align { line: 41.25 },
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
        };
        let mut buf = Vec::new();
        write_update(&mut buf, &u).unwrap();
        assert_eq!(read_update(&mut &buf[..]).unwrap(), Some(u));
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
