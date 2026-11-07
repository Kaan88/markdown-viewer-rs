//! The wire format between the viewer and `mdview-engine`.
//!
//! A job is a header of `kind` and payload length as little-endian `u32`s, then the payload. A
//! reply is `status`, `width`, `height`, pixel-buffer length as a `u64`, then straight-alpha RGBA.
//! Both sides are written by hand: the engine is a separate process with a hard deadline, so the
//! format stays small and versionless until there is a second job family.

use std::io::{self, Read, Write};

/// The SVG job: the payload is the file's bytes, the reply is the raster.
pub const KIND_SVG: u32 = 1;

pub const STATUS_OK: u32 = 0;
/// The request or its kind was not understood.
pub const STATUS_BAD_REQUEST: u32 = 1;
/// The job ran but the input could not be rendered.
pub const STATUS_FAILED: u32 = 2;

/// A request payload is a document, and the document cap is 12 MiB; the extra room is for a future
/// job that carries a snippet rather than a file.
pub const MAX_REQUEST: u32 = 16 * 1024 * 1024;
/// The renderer's image cap: 24 M pixels at four bytes.
pub const MAX_REPLY: u64 = 24_000_000 * 4;

pub struct Request {
    pub kind: u32,
    pub payload: Vec<u8>,
}

pub struct Reply {
    pub status: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Reply {
    pub fn ok(width: u32, height: u32, rgba: Vec<u8>) -> Reply {
        Reply {
            status: STATUS_OK,
            width,
            height,
            rgba,
        }
    }

    pub fn failed(status: u32) -> Reply {
        Reply {
            status,
            width: 0,
            height: 0,
            rgba: Vec::new(),
        }
    }
}

pub fn write_request(w: &mut impl Write, request: &Request) -> io::Result<()> {
    let length = u32::try_from(request.payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "payload too large"))?;
    if length > MAX_REQUEST {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "payload too large",
        ));
    }
    w.write_all(&request.kind.to_le_bytes())?;
    w.write_all(&length.to_le_bytes())?;
    w.write_all(&request.payload)?;
    w.flush()
}

/// Reads one request. `Ok(None)` is a clean end of stream before a header; a short header is an
/// error, because the other side is then gone mid-frame.
pub fn read_request(r: &mut impl Read) -> io::Result<Option<Request>> {
    let mut header = [0u8; 8];
    let read = read_frame_start(r, &mut header)?;
    if read == 0 {
        return Ok(None);
    }
    let kind = u32::from_le_bytes(header[0..4].try_into().expect("four bytes"));
    let length = u32::from_le_bytes(header[4..8].try_into().expect("four bytes"));
    if length > MAX_REQUEST {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request payload too large",
        ));
    }
    let mut payload = vec![0u8; length as usize];
    r.read_exact(&mut payload)?;
    Ok(Some(Request { kind, payload }))
}

pub fn write_reply(w: &mut impl Write, reply: &Reply) -> io::Result<()> {
    let length = reply.rgba.len() as u64;
    if length > MAX_REPLY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "reply pixels too large",
        ));
    }
    w.write_all(&reply.status.to_le_bytes())?;
    w.write_all(&reply.width.to_le_bytes())?;
    w.write_all(&reply.height.to_le_bytes())?;
    w.write_all(&length.to_le_bytes())?;
    w.write_all(&reply.rgba)?;
    w.flush()
}

pub fn read_reply(r: &mut impl Read) -> io::Result<Reply> {
    let mut header = [0u8; 20];
    let read = read_frame_start(r, &mut header)?;
    if read != header.len() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "engine reply is incomplete",
        ));
    }
    let status = u32::from_le_bytes(header[0..4].try_into().expect("four bytes"));
    let width = u32::from_le_bytes(header[4..8].try_into().expect("four bytes"));
    let height = u32::from_le_bytes(header[8..12].try_into().expect("four bytes"));
    let length = u64::from_le_bytes(header[12..20].try_into().expect("eight bytes"));
    if length > MAX_REPLY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "engine reply pixels too large",
        ));
    }
    let mut rgba = vec![0u8; length as usize];
    r.read_exact(&mut rgba)?;
    Ok(Reply {
        status,
        width,
        height,
        rgba,
    })
}

/// Fills `header` from a possibly empty stream and returns how many bytes arrived.
fn read_frame_start(r: &mut impl Read, header: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < header.len() {
        match r.read(&mut header[filled..])? {
            0 if filled == 0 => return Ok(0),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "frame ended before its header",
                ));
            }
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn a_request_round_trips() {
        let mut buffer = Vec::new();
        write_request(
            &mut buffer,
            &Request {
                kind: KIND_SVG,
                payload: b"<svg/>".to_vec(),
            },
        )
        .unwrap();
        let request = read_request(&mut Cursor::new(buffer)).unwrap().unwrap();
        assert_eq!(request.kind, KIND_SVG);
        assert_eq!(request.payload, b"<svg/>");
    }

    #[test]
    fn a_reply_round_trips() {
        let mut buffer = Vec::new();
        let rgba = vec![1u8, 2, 3, 255, 4, 5, 6, 255];
        write_reply(&mut buffer, &Reply::ok(2, 1, rgba.clone())).unwrap();
        let reply = read_reply(&mut Cursor::new(buffer)).unwrap();
        assert_eq!(reply.status, STATUS_OK);
        assert_eq!((reply.width, reply.height), (2, 1));
        assert_eq!(reply.rgba, rgba);
    }

    #[test]
    fn a_clean_end_of_stream_is_not_an_error() {
        assert!(
            read_request(&mut Cursor::new(Vec::new()))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_short_header_is_an_error() {
        assert!(read_request(&mut Cursor::new(vec![1, 0, 0])).is_err());
    }
}
