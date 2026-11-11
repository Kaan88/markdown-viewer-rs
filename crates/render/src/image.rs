//! Decodes the images a document points at. PNG, JPEG, GIF and WebP in-process (GIF and animated
//! WebP show their first frame only, so nothing animates and idle CPU stays zero; the format
//! comes from the magic bytes, never the extension); SVG through
//! `mdview-engine`, so its rasterizer and fonts never live in the viewer.
//!
//! The plan caps an image at 12 MiB on disk and 24 M pixels once decoded, so a document cannot make
//! the viewer allocate without bound. Anything over a limit, unreadable, or in another format is
//! left to the text placeholder the layout already draws.

use std::io::Read;
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
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_BYTES {
        return None;
    }
    // Capped again at the read, so a file that grows after the check still cannot exceed the limit.
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    decode(&bytes)
}

/// Picks the decoder from the leading bytes. A decoder that panics on hostile input counts as a
/// failed decode (where the build unwinds), so the page shows the placeholder.
pub(crate) fn decode(bytes: &[u8]) -> Option<DecodedImage> {
    std::panic::catch_unwind(|| decode_unchecked(bytes))
        .ok()
        .flatten()
}

fn decode_unchecked(bytes: &[u8]) -> Option<DecodedImage> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        decode_jpeg(bytes)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        decode_gif(bytes)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        decode_webp(bytes)
    } else if looks_like_svg(bytes) {
        crate::engine::render_svg(bytes)
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

/// First frame of a GIF, composited onto a transparent logical screen: the frame lands at its
/// offset, transparent pixels stay transparent, everything else is the palette colour.
fn decode_gif(bytes: &[u8]) -> Option<DecodedImage> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.set_memory_limit(gif::MemoryLimit::Bytes(std::num::NonZeroU64::new(
        MAX_PIXELS * 4,
    )?));
    let mut decoder = options.read_info(std::io::Cursor::new(bytes)).ok()?;
    let (screen_w, screen_h) = (decoder.width() as u32, decoder.height() as u32);
    // Only headers have been read so far; check before anything is allocated.
    let frame = decoder.next_frame_info().ok()??;
    let (left, top) = (frame.left as u32, frame.top as u32);
    let (frame_w, frame_h) = (frame.width as u32, frame.height as u32);
    if !fits(frame_w, frame_h) {
        return None;
    }
    // A zero-sized screen turns up in the wild; the frame's own extent then defines the image.
    let width = if screen_w == 0 {
        left + frame_w
    } else {
        screen_w
    };
    let height = if screen_h == 0 {
        top + frame_h
    } else {
        screen_h
    };
    if !fits(width, height) {
        return None;
    }
    let mut pixels = vec![0u8; decoder.buffer_size()];
    if pixels.len() != frame_w as usize * frame_h as usize * 4 {
        return None;
    }
    decoder.read_into_buffer(&mut pixels).ok()?;
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    let copy = frame_w.min(width.saturating_sub(left)) as usize * 4;
    for row in 0..frame_h.min(height.saturating_sub(top)) {
        let from = row as usize * frame_w as usize * 4;
        let to = ((top + row) as usize * width as usize + left as usize) * 4;
        rgba[to..to + copy].copy_from_slice(&pixels[from..from + copy]);
    }
    Some(DecodedImage {
        width,
        height,
        baseline: height,
        rgba,
    })
}

/// Static lossy or lossless WebP, with alpha when the file has it; the first frame (already
/// composited by the decoder) for an animated one.
fn decode_webp(bytes: &[u8]) -> Option<DecodedImage> {
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let (width, height) = decoder.dimensions();
    if !fits(width, height) {
        return None;
    }
    decoder.set_memory_limit(MAX_PIXELS as usize * 8);
    let mut buffer = vec![0u8; decoder.output_buffer_size()?];
    decoder.read_image(&mut buffer).ok()?;
    let rgba = if decoder.has_alpha() {
        buffer
    } else {
        buffer
            .chunks_exact(3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect()
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

    fn asset(name: &str) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/assets");
        std::fs::read(path.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn pixel(image: &DecodedImage, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width + x) * 4) as usize;
        image.rgba[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn gif_offset_frame_and_transparency() {
        let image = decode(&asset("offset.gif")).expect("gif decodes");
        assert_eq!((image.width, image.height), (24, 16));
        assert_eq!(image.rgba.len(), 24 * 16 * 4);
        // Outside the frame: transparent.
        assert_eq!(pixel(&image, 0, 0)[3], 0);
        assert_eq!(pixel(&image, 5, 4)[3], 0);
        assert_eq!(pixel(&image, 6, 3)[3], 0);
        assert_eq!(pixel(&image, 18, 12)[3], 0);
        // The frame's border sits at its offset (6, 4), in palette colour 1.
        assert_eq!(pixel(&image, 6, 4), [230, 120, 20, 255]);
        assert_eq!(pixel(&image, 17, 11), [230, 120, 20, 255]);
        // Transparent index inside the frame stays transparent; white elsewhere.
        assert_eq!(pixel(&image, 7, 5)[3], 0);
        assert_eq!(pixel(&image, 10, 6), [255, 255, 255, 255]);
    }

    #[test]
    fn animated_gif_uses_first_frame() {
        let image = decode(&asset("anim.gif")).expect("gif decodes");
        assert_eq!((image.width, image.height), (16, 16));
        let [r, g, b, a] = pixel(&image, 8, 8);
        assert!(r > 150 && g < 100 && b < 100 && a == 255, "{r} {g} {b} {a}");
    }

    #[test]
    fn lossy_webp() {
        let image = decode(&asset("lossy.webp")).expect("webp decodes");
        assert_eq!((image.width, image.height), (32, 32));
        assert_eq!(image.rgba.len(), 32 * 32 * 4);
        assert_eq!(pixel(&image, 16, 16)[3], 255);
        // The source is a red/green gradient: the far corner is redder and greener than the near.
        let (near, far) = (pixel(&image, 1, 1), pixel(&image, 30, 30));
        assert!(far[0] > near[0] + 100 && far[1] > near[1] + 100);
    }

    #[test]
    fn lossless_webp_with_alpha() {
        let image = decode(&asset("alpha.webp")).expect("webp decodes");
        assert_eq!((image.width, image.height), (32, 32));
        assert_eq!(pixel(&image, 0, 0)[3], 0);
        assert_eq!(pixel(&image, 6, 16), [40, 180, 90, 255]);
        assert_eq!(pixel(&image, 15, 15)[3], 128);
    }

    #[test]
    fn animated_webp_uses_first_frame() {
        let image = decode(&asset("anim.webp")).expect("webp decodes");
        assert_eq!((image.width, image.height), (16, 16));
        let [r, g, b, _] = pixel(&image, 8, 8);
        assert!(r > 200 && g > 150 && b < 80, "{r} {g} {b}");
    }

    #[test]
    fn truncated_files_give_none() {
        for name in [
            "offset.gif",
            "anim.gif",
            "lossy.webp",
            "alpha.webp",
            "anim.webp",
        ] {
            let bytes = asset(name);
            for cut in [bytes.len() / 2, bytes.len() - 1, 14, 12, 6, 0] {
                // Must not panic or hang; a cut that removes pixel data must not decode.
                let result = decode(&bytes[..cut]);
                if cut < bytes.len() / 2 + 1 {
                    assert!(result.is_none(), "{name} cut at {cut}");
                }
            }
        }
    }

    #[test]
    fn oversized_headers_are_rejected_before_allocating() {
        // GIF: a 65535 x 65535 screen and frame, with no pixel data behind them.
        let mut gif = b"GIF89a".to_vec();
        gif.extend([0xFF, 0xFF, 0xFF, 0xFF, 0x80, 0, 0]);
        gif.extend([0, 0, 0, 255, 255, 255]);
        gif.push(0x2C);
        gif.extend([0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        gif.push(2);
        assert!(decode(&gif).is_none());

        // WebP: an extended header claiming 100000 x 100000 (24-bit fields hold width - 1).
        let canvas = 100_000u32 - 1;
        let mut vp8x = vec![0u8; 10];
        vp8x[4..7].copy_from_slice(&canvas.to_le_bytes()[..3]);
        vp8x[7..10].copy_from_slice(&canvas.to_le_bytes()[..3]);
        let mut webp = b"RIFF".to_vec();
        webp.extend((4u32 + 8 + 10).to_le_bytes());
        webp.extend(b"WEBPVP8X");
        webp.extend(10u32.to_le_bytes());
        webp.extend(vp8x);
        assert!(decode(&webp).is_none());
        // The same claim in the plain header must be refused by the pixel limit itself.
        assert!(!fits(100_000, 100_000));
    }

    #[test]
    fn format_comes_from_bytes_not_extension() {
        let dir = std::env::temp_dir().join("mdv-image-sniff");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("really-a-gif.png");
        std::fs::write(&path, asset("offset.gif")).unwrap();
        assert!(load(&path).is_some());
        let path = dir.join("really-a-webp.jpg");
        std::fs::write(&path, asset("lossy.webp")).unwrap();
        assert!(load(&path).is_some());
    }
}
