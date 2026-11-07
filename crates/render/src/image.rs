//! Decodes the images a document points at. PNG and JPEG in-process; SVG through
//! `mdview-engine`, so its rasterizer and fonts never live in the viewer.
//!
//! The plan caps an image at 12 MiB on disk and 24 M pixels once decoded, so a document cannot make
//! the viewer allocate without bound. Anything over a limit, unreadable, or in another format is
//! left to the text placeholder the layout already draws.

use std::path::Path;

pub const MAX_BYTES: u64 = 12 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 24_000_000;

/// Straight-alpha RGBA, row-major.
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    /// Distance from the top to the text baseline; the bottom for images without one.
    pub baseline: u32,
    pub rgba: Vec<u8>,
}

pub fn load(path: &Path) -> Option<DecodedImage> {
    let length = std::fs::metadata(path).ok()?.len();
    if length > MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(&bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        decode_jpeg(&bytes)
    } else if looks_like_svg(&bytes) {
        crate::engine::render_svg(&bytes)
    } else {
        None
    }
}

/// A text file whose first tag is `<svg`: XML declarations, comments and whitespace may come
/// first, and there is no binary signature to rely on.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    let text = String::from_utf8_lossy(head);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    trimmed.starts_with('<') && trimmed.contains("<svg")
}

fn fits(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && width as u64 * height as u64 <= MAX_PIXELS
}

fn decode_png(bytes: &[u8]) -> Option<DecodedImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // 8-bit output with an alpha channel, so everything below has one shape to handle.
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if !fits(width, height) {
        return None;
    }
    let mut buffer = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let (width, height) = (frame.width, frame.height);
    let data = &buffer[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        png::ColorType::Grayscale => data
            .iter()
            .flat_map(|grey| [*grey, *grey, *grey, 255])
            .collect(),
        // EXPAND above means palette images arrive as RGB or RGBA.
        png::ColorType::Indexed => return None,
    };
    Some(DecodedImage {
        width,
        height,
        baseline: height,
        rgba,
    })
}

fn decode_jpeg(bytes: &[u8]) -> Option<DecodedImage> {
    let mut decoder = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(bytes));
    decoder.decode_headers().ok()?;
    let info = decoder.info()?;
    let (width, height) = (info.width as u32, info.height as u32);
    if !fits(width, height) {
        return None;
    }
    let pixels = decoder.decode().ok()?;
    let count = (width as usize) * (height as usize);
    let rgba = if pixels.len() == count * 4 {
        pixels
    } else if pixels.len() == count * 3 {
        pixels
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect()
    } else if pixels.len() == count {
        pixels
            .iter()
            .flat_map(|grey| [*grey, *grey, *grey, 255])
            .collect()
    } else {
        return None;
    };
    Some(DecodedImage {
        width,
        height,
        baseline: height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_are_the_plan_numbers() {
        assert!(fits(4000, 6000));
        assert!(!fits(4001, 6000));
        assert!(!fits(0, 100));
        assert_eq!(MAX_BYTES, 12 * 1024 * 1024);
        assert_eq!(MAX_PIXELS, 24_000_000);
    }
}
