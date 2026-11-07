//! A CPU framebuffer in BGRA order, plus the handful of antialiased shapes the page needs.

/// Opaque 8-bit BGRA pixels, top-down, stride `width * 4`. Alpha is always 255.
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Bitmap {
    pub fn new(width: u32, height: u32) -> Bitmap {
        Bitmap {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn clear(&mut self, rgb: [u8; 3]) {
        let px = [rgb[2], rgb[1], rgb[0], 255];
        for c in self.pixels.chunks_exact_mut(4) {
            c.copy_from_slice(&px);
        }
    }

    /// Copies the pixels into `png`'s RGB order and writes the file.
    pub fn write_png(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut encoder = png::Encoder::new(file, self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut rgb = Vec::with_capacity(self.width as usize * self.height as usize * 3);
        for c in self.pixels.chunks_exact(4) {
            rgb.extend_from_slice(&[c[2], c[1], c[0]]);
        }
        writer
            .write_image_data(&rgb)
            .map_err(|e| std::io::Error::other(e.to_string()))
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, rgb: [u8; 3], a: f32) {
        if a <= 0.0 || x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let o = (y as usize * self.width as usize + x as usize) * 4;
        let d = &mut self.pixels[o..o + 4];
        for (i, v) in [rgb[2], rgb[1], rgb[0]].iter().enumerate() {
            // d is BGRA: index 0 is blue, so the color's channels go in reverse here.
            let dst = d[i] as f32;
            d[i] = (dst + (*v as f32 - dst) * a + 0.5) as u8;
        }
    }

    /// An antialiased rounded rectangle. `radius` 0 draws a plain rectangle.
    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, rgb: [u8; 3]) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let r = radius.clamp(0.0, w.min(h) / 2.0);
        let (x0, y0) = (x.floor() as i32, y.floor() as i32);
        let (x1, y1) = ((x + w).ceil() as i32, (y + h).ceil() as i32);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        for py in y0.max(0)..y1.min(self.height as i32) {
            for px in x0.max(0)..x1.min(self.width as i32) {
                let qx = (px as f32 + 0.5 - cx).abs() - (w / 2.0 - r);
                let qy = (py as f32 + 0.5 - cy).abs() - (h / 2.0 - r);
                let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
                self.blend(px, py, rgb, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// An antialiased thick line segment, for check marks and disclosure chevrons.
    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), thickness: f32, rgb: [u8; 3]) {
        let (x0, x1) = (
            (a.0.min(b.0) - thickness).floor() as i32,
            (a.0.max(b.0) + thickness).ceil() as i32,
        );
        let (y0, y1) = (
            (a.1.min(b.1) - thickness).floor() as i32,
            (a.1.max(b.1) + thickness).ceil() as i32,
        );
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = (dx * dx + dy * dy).max(0.0001);
        for py in y0.max(0)..y1.min(self.height as i32) {
            for px in x0.max(0)..x1.min(self.width as i32) {
                let (qx, qy) = (px as f32 + 0.5 - a.0, py as f32 + 0.5 - a.1);
                let u = ((qx * dx + qy * dy) / len2).clamp(0.0, 1.0);
                let d = (qx - u * dx).hypot(qy - u * dy) - thickness / 2.0;
                self.blend(px, py, rgb, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Draws a straight-alpha RGBA image, bilinearly scaled into the rectangle at `(x, y)`.
    pub fn image(
        &mut self,
        rgba: &[u8],
        source_width: u32,
        source_height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        if source_width == 0 || source_height == 0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        if rgba.len() < source_width as usize * source_height as usize * 4 {
            return;
        }
        let (frame_width, frame_height) = (self.width as i32, self.height as i32);
        let (left, top) = (x.round() as i32, y.round() as i32);
        let (draw_width, draw_height) = (w.round().max(1.0) as i32, h.round().max(1.0) as i32);
        let (sw, sh) = (source_width as f32, source_height as f32);
        let sample = |sx: usize, sy: usize| -> [f32; 4] {
            let i = (sy * source_width as usize + sx) * 4;
            [
                rgba[i] as f32,
                rgba[i + 1] as f32,
                rgba[i + 2] as f32,
                rgba[i + 3] as f32,
            ]
        };
        for row in 0..draw_height {
            let py = top + row;
            if py < 0 || py >= frame_height {
                continue;
            }
            let sy = ((row as f32 + 0.5) / draw_height as f32 * sh - 0.5).clamp(0.0, sh - 1.0);
            let y1 = sy.floor() as usize;
            let y2 = (y1 + 1).min(source_height as usize - 1);
            let fy = sy - y1 as f32;
            for column in 0..draw_width {
                let px = left + column;
                if px < 0 || px >= frame_width {
                    continue;
                }
                let sx =
                    ((column as f32 + 0.5) / draw_width as f32 * sw - 0.5).clamp(0.0, sw - 1.0);
                let x1 = sx.floor() as usize;
                let x2 = (x1 + 1).min(source_width as usize - 1);
                let fx = sx - x1 as f32;
                let (a, b, c, d) = (
                    sample(x1, y1),
                    sample(x2, y1),
                    sample(x1, y2),
                    sample(x2, y2),
                );
                let mut pixel = [0.0f32; 4];
                for channel in 0..4 {
                    let upper = a[channel] + (b[channel] - a[channel]) * fx;
                    let lower = c[channel] + (d[channel] - c[channel]) * fx;
                    pixel[channel] = upper + (lower - upper) * fy;
                }
                let alpha = (pixel[3] / 255.0).clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }
                let index = (py as usize * self.width as usize + px as usize) * 4;
                let inverse = 1.0 - alpha;
                for channel in 0..3 {
                    // RGBA samples, BGRA framebuffer.
                    let value =
                        pixel[2 - channel] * alpha + self.pixels[index + channel] as f32 * inverse;
                    self.pixels[index + channel] = (value + 0.5) as u8;
                }
            }
        }
    }
}
