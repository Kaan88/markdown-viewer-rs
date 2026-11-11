//! The wire format between the viewer and `mdview-engine`.
//!
//! A job is a header of `kind` and payload length as little-endian `u32`s, then the payload. A
//! reply is `status`, `width`, `height`, `baseline`, pixel-buffer length as a `u64`, then
//! straight-alpha RGBA. Both sides are written by hand: the engine is a separate process with a
//! hard deadline, so the format stays small and versionless until there is a second job family.
//!
//! `baseline` is the distance from the top of the raster to the text baseline, in pixels. It is
//! what lets inline math sit on the line instead of hanging below it; images that do not care
//! leave it at the bottom.

use std::io::{self, Read, Write};

/// The SVG job: the payload is the file's bytes, the reply is the raster.
pub const KIND_SVG: u32 = 1;
/// The math job: a small math header (font size, display flag, colour) and then TeX source.
pub const KIND_MATH: u32 = 2;
/// The Mermaid job: a scale factor, a dark-theme flag, then the diagram source.
pub const KIND_MERMAID: u32 = 3;
/// Scale (4 bytes) and the dark flag (1 byte) before the source.
pub const MERMAID_HEADER: usize = 5;
/// An SVG file rasterized at a device scale (4 bytes, `f32`) given before the file's bytes, so a
/// vector image stays sharp at 150% DPI or under zoom instead of being stretched.
pub const KIND_SVG_AT: u32 = 4;

pub const STATUS_OK: u32 = 0;
/// The request or its kind was not understood.
pub const STATUS_BAD_REQUEST: u32 = 1;
/// The job ran but the input could not be rendered.
pub const STATUS_FAILED: u32 = 2;

/// A request payload is a document, and the document cap is 12 MiB; the extra room is for a
/// snippet job rather than a file.
pub const MAX_REQUEST: u32 = 16 * 1024 * 1024;
/// The renderer's image cap: 24 M pixels at four bytes.
pub const MAX_REPLY: u64 = 24_000_000 * 4;

/// The fixed part of a `KIND_MATH` payload: font size in device pixels per em, a display flag, and
/// the RGB text colour. The TeX source is the rest.
pub const MATH_HEADER: usize = 4 + 1 + 3;

pub struct Request {
    pub kind: u32,
    pub payload: Vec<u8>,
}

/// A decoded math job, as the engine reads it.
pub struct MathRequest<'a> {
    pub font_size: f32,
    pub display: bool,
    pub color: [u8; 3],
    pub source: &'a str,
}

pub struct Reply {
    pub status: u32,
    pub width: u32,
    pub height: u32,
    pub baseline: u32,
    pub rgba: Vec<u8>,
}

impl Reply {
    /// A raster that sits at the bottom of its box, which is right for images and wrong for math.
    pub fn ok(width: u32, height: u32, rgba: Vec<u8>) -> Reply {
        Reply {
            status: STATUS_OK,
            width,
            height,
            baseline: height,
            rgba,
        }
    }

    /// A raster with a baseline measured from its top, for inline math.
    pub fn ok_with_baseline(width: u32, height: u32, baseline: u32, rgba: Vec<u8>) -> Reply {
        Reply {
            status: STATUS_OK,
            width,
            height,
            baseline: baseline.min(height),
            rgba,
        }
    }

    pub fn failed(status: u32) -> Reply {
        Reply {
            status,
            width: 0,
            height: 0,
            baseline: 0,
            rgba: Vec::new(),
        }
    }
}

/// Encodes the math payload: the fixed header, then the TeX source.
pub fn math_payload(font_size: f32, display: bool, color: [u8; 3], source: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(MATH_HEADER + source.len());
    payload.extend_from_slice(&font_size.to_le_bytes());
    payload.push(u8::from(display));
    payload.extend_from_slice(&color);
    payload.extend_from_slice(source.as_bytes());
    payload
}

/// Decodes a math payload. `None` when it is too short or its source is not UTF-8.
pub fn read_math_request(payload: &[u8]) -> Option<MathRequest<'_>> {
    if payload.len() < MATH_HEADER {
        return None;
    }
    let font_size = f32::from_le_bytes(payload[0..4].try_into().ok()?);
    if !font_size.is_finite() || font_size <= 0.0 {
        return None;
    }
    let display = payload[4] != 0;
    let color = [payload[5], payload[6], payload[7]];
    let source = std::str::from_utf8(&payload[MATH_HEADER..]).ok()?;
    Some(MathRequest {
        font_size,
        display,
        color,
        source,
    })
}

/// A Mermaid source and the device scale its raster should use.
pub struct MermaidRequest<'a> {
    pub scale: f32,
    pub dark: bool,
    pub source: &'a str,
}

/// Encodes a Mermaid payload: the scale factor, the dark flag, then the source.
pub fn mermaid_payload(scale: f32, dark: bool, source: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(MERMAID_HEADER + source.len());
    payload.extend_from_slice(&scale.to_le_bytes());
    payload.push(u8::from(dark));
    payload.extend_from_slice(source.as_bytes());
    payload
}

/// Decodes a Mermaid payload. `None` when it is too short, the scale is unusable or the source is
/// not UTF-8.
pub fn read_mermaid_request(payload: &[u8]) -> Option<MermaidRequest<'_>> {
    let scale = f32::from_le_bytes(payload.get(0..4)?.try_into().ok()?);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let dark = *payload.get(4)? != 0;
    let source = std::str::from_utf8(payload.get(MERMAID_HEADER..)?).ok()?;
    Some(MermaidRequest {
        scale,
        dark,
        source,
    })
}

/// Encodes a scaled SVG payload: the scale factor, then the file's bytes.
pub fn svg_at_payload(scale: f32, svg: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(4 + svg.len());
    payload.extend_from_slice(&scale.to_le_bytes());
    payload.extend_from_slice(svg);
    payload
}

/// Decodes a scaled SVG payload into its scale and the SVG bytes. `None` when it is too short or
/// the scale is unusable.
pub fn read_svg_at_request(payload: &[u8]) -> Option<(f32, &[u8])> {
    let scale = f32::from_le_bytes(payload.get(0..4)?.try_into().ok()?);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    Some((scale, &payload[4..]))
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
    w.write_all(&reply.baseline.to_le_bytes())?;
    w.write_all(&length.to_le_bytes())?;
    w.write_all(&reply.rgba)?;
    w.flush()
}

pub fn read_reply(r: &mut impl Read) -> io::Result<Reply> {
    let mut header = [0u8; 24];
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
    let baseline = u32::from_le_bytes(header[12..16].try_into().expect("four bytes"));
    let length = u64::from_le_bytes(header[16..24].try_into().expect("eight bytes"));
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
        baseline,
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
        assert_eq!(reply.baseline, 1);
        assert_eq!(reply.rgba, rgba);
    }

    #[test]
    fn a_baseline_round_trips() {
        let mut buffer = Vec::new();
        write_reply(
            &mut buffer,
            &Reply::ok_with_baseline(10, 20, 15, vec![0; 10 * 20 * 4]),
        )
        .unwrap();
        let reply = read_reply(&mut Cursor::new(buffer)).unwrap();
        assert_eq!(reply.baseline, 15);
    }

    #[test]
    fn a_math_payload_round_trips() {
        let payload = math_payload(32.0, true, [1, 2, 3], "x^2");
        let parsed = read_math_request(&payload).expect("the payload parses");
        assert_eq!(parsed.font_size, 32.0);
        assert!(parsed.display);
        assert_eq!(parsed.color, [1, 2, 3]);
        assert_eq!(parsed.source, "x^2");
        assert!(read_math_request(&payload[..MATH_HEADER - 1]).is_none());
    }

    #[test]
    fn a_mermaid_payload_round_trips() {
        let payload = mermaid_payload(2.5, true, "flowchart LR\nA-->B");
        let parsed = read_mermaid_request(&payload).expect("the payload parses");
        assert_eq!(parsed.scale, 2.5);
        assert!(parsed.dark);
        assert_eq!(parsed.source, "flowchart LR\nA-->B");
        assert!(read_mermaid_request(&[1, 2]).is_none());
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
