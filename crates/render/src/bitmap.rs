//! A CPU framebuffer in BGRA order, plus the handful of antialiased shapes the page needs.

/// Opaque 8-bit BGRA pixels, top-down, stride `width * 4`. Alpha is always 255.
#[derive(Clone)]
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
        fill(&mut self.pixels, rgb);
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
    ///
    /// Rows whose pixels are all fully covered are stored in one pass, and only the pixels the edges
    /// cut through run the distance formula and blend. This is the hot path for code-block, table and
    /// callout backgrounds: on the 1 MB fixture it was most of a scroll frame.
    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, rgb: [u8; 3]) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let r = radius.clamp(0.0, w.min(h) / 2.0);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (hw, hh) = (w / 2.0, h / 2.0);
        let xs = (x.floor() as i32).max(0);
        let xe = ((x + w).ceil() as i32).min(self.width as i32);
        let ys = (y.floor() as i32).max(0);
        let ye = ((y + h).ceil() as i32).min(self.height as i32);
        if xs >= xe || ys >= ye {
            return;
        }
        // A pixel half a pixel inside an edge is fully covered, so this is the distance at which
        // coverage reaches one.
        let inner = r - 0.5;
        for py in ys..ye {
            let qy = (py as f32 + 0.5 - cy).abs() - (hh - r);
            if qy >= r + 0.5 {
                continue; // the whole row is past the edge
            }
            // How far past the straight edge a pixel can be and still be fully covered on this row.
            let qmax = if qy > 0.0 {
                // In the corner band the distance is diagonal.
                if inner > 0.0 && qy < inner {
                    Some((inner * inner - qy * qy).sqrt())
                } else {
                    None
                }
            } else if qy <= inner {
                Some(inner)
            } else {
                None
            };
            let span = qmax.map(|qmax| {
                let limit = (hw - r) + qmax;
                (
                    ((cx - limit).ceil() as i32).max(xs),
                    (((cx + limit - 1.0).floor() as i32) + 1).min(xe),
                )
            });
            match span {
                Some((lo, hi)) if lo < hi => {
                    let row = py as usize * self.width as usize;
                    let start = (row + lo as usize) * 4;
                    let end = (row + hi as usize) * 4;
                    fill(&mut self.pixels[start..end], rgb);
                    // Only the corner bands on either side of the straight middle need blending.
                    for px in xs..lo {
                        let qx = (px as f32 + 0.5 - cx).abs() - (hw - r);
                        self.blend(px, py, rgb, coverage(qx, qy, r));
                    }
                    for px in hi..xe {
                        let qx = (px as f32 + 0.5 - cx).abs() - (hw - r);
                        self.blend(px, py, rgb, coverage(qx, qy, r));
                    }
                }
                _ => {
                    for px in xs..xe {
                        let qx = (px as f32 + 0.5 - cx).abs() - (hw - r);
                        self.blend(px, py, rgb, coverage(qx, qy, r));
                    }
                }
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

    /// Draws a straight-alpha RGBA image scaled into the rectangle at `(x, y)`: bilinearly, or by
    /// area averaging when it is shrunk below half size on either axis, where bilinear sampling
    /// skips source pixels and turns fine detail into moiré.
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
        let (draw_width, draw_height) = (w.round().max(1.0) as i32, h.round().max(1.0) as i32);
        if (draw_width as u32) * 2 < source_width || (draw_height as u32) * 2 < source_height {
            self.image_area(rgba, source_width, source_height, x, y, w, h);
        } else {
            self.image_bilinear(rgba, source_width, source_height, x, y, w, h);
        }
    }

    /// Box-filtered downscaling: each destination pixel is the coverage-weighted mean of the
    /// source pixels under it, averaged in premultiplied alpha so transparent pixels add no colour
    /// (no dark fringes around a cut-out). Only the rows and columns inside the bitmap are
    /// computed, so a scroll band pays for the source area behind the band alone.
    #[allow(clippy::too_many_arguments)]
    fn image_area(
        &mut self,
        rgba: &[u8],
        source_width: u32,
        source_height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        let (left, top) = (x.round() as i32, y.round() as i32);
        let (draw_width, draw_height) = (w.round().max(1.0) as i32, h.round().max(1.0) as i32);
        let (frame_width, frame_height) = (self.width as i32, self.height as i32);
        let (c0, c1) = ((-left).max(0), (frame_width - left).min(draw_width));
        let (r0, r1) = ((-top).max(0), (frame_height - top).min(draw_height));
        if c0 >= c1 || r0 >= r1 {
            return;
        }
        // The source pixels under destination cell `i` of `n` along an axis `len` long, each
        // weighted by its overlap with the cell in 1/256 px. Integer weights keep the inner loop
        // in integer multiply-adds; the result is divided by the summed weights, so the rounding
        // of a weight cannot brighten or darken a flat area.
        let footprint = |i: i32, n: i32, len: u32, out: &mut Vec<(usize, u64)>| {
            out.clear();
            let ratio = len as f64 / n as f64;
            let (a, b) = (i as f64 * ratio, ((i + 1) as f64 * ratio).min(len as f64));
            let mut s = a.floor() as usize;
            while (s as f64) < b && s < len as usize {
                let overlap = b.min(s as f64 + 1.0) - a.max(s as f64);
                let weight = (overlap * 256.0).round() as u64;
                if weight > 0 {
                    out.push((s, weight));
                }
                s += 1;
            }
        };
        // Per visible column: a range into one flat list of (source byte offset, weight), and
        // the column's total weight.
        let mut columns: Vec<(usize, usize, u64)> = Vec::with_capacity((c1 - c0) as usize);
        let mut taps: Vec<(usize, u64)> = Vec::new();
        let mut scratch = Vec::new();
        for c in c0..c1 {
            footprint(c, draw_width, source_width, &mut scratch);
            let total = scratch.iter().map(|&(_, w)| w).sum();
            columns.push((taps.len(), scratch.len(), total));
            taps.extend(scratch.iter().map(|&(sx, w)| (sx * 4, w)));
        }
        // Premultiplied red, green, blue and alpha, weighted, per visible column.
        let mut sums = vec![[0u64; 4]; (c1 - c0) as usize];
        let mut rows = Vec::new();
        let stride = source_width as usize * 4;
        for r in r0..r1 {
            footprint(r, draw_height, source_height, &mut rows);
            let row_total: u64 = rows.iter().map(|&(_, w)| w).sum();
            sums.fill([0; 4]);
            for &(sy, wy) in &rows {
                let line = &rgba[sy * stride..(sy + 1) * stride];
                for (sum, &(start, len, _)) in sums.iter_mut().zip(&columns) {
                    let mut h = [0u64; 4];
                    for &(sx, wx) in &taps[start..start + len] {
                        let p = &line[sx..sx + 4];
                        let a = p[3] as u64 * wx;
                        h[0] += p[0] as u64 * a;
                        h[1] += p[1] as u64 * a;
                        h[2] += p[2] as u64 * a;
                        h[3] += a;
                    }
                    for (s, h) in sum.iter_mut().zip(h) {
                        *s += h * wy;
                    }
                }
            }
            let py = (top + r) as usize;
            for (i, (sum, &(_, _, column_total))) in sums.iter().zip(&columns).enumerate() {
                let total = column_total * row_total;
                if sum[3] == 0 || total == 0 {
                    continue;
                }
                // Premultiplied colour (0..255) and coverage (0..1) of the cell.
                let scale = 1.0 / (255.0 * total as f64);
                let alpha = (sum[3] as f64 * scale).min(1.0);
                let inverse = 1.0 - alpha;
                let index = (py * self.width as usize + (left + c0) as usize + i) * 4;
                for channel in 0..3 {
                    // RGBA sums, BGRA framebuffer.
                    let value = sum[2 - channel] as f64 * scale
                        + self.pixels[index + channel] as f64 * inverse;
                    self.pixels[index + channel] = (value + 0.5).min(255.0) as u8;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn image_bilinear(
        &mut self,
        rgba: &[u8],
        source_width: u32,
        source_height: u32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
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

/// Fills a BGRX run with one color, two pixels per store where it can.
#[inline]
fn fill(dst: &mut [u8], rgb: [u8; 3]) {
    let px = [rgb[2], rgb[1], rgb[0], 255];
    let pair = [px[0], px[1], px[2], px[3], px[0], px[1], px[2], px[3]];
    let mut chunks = dst.chunks_exact_mut(8);
    for c in &mut chunks {
        c.copy_from_slice(&pair);
    }
    for c in chunks.into_remainder().chunks_exact_mut(4) {
        c.copy_from_slice(&px);
    }
}

/// Antialiasing coverage of one pixel of a rounded rectangle: `qx` and `qy` are the pixel center's
/// distance past the straight edges, and `r` is the corner radius. One is fully inside, zero fully
/// outside.
#[inline]
fn coverage(qx: f32, qy: f32, r: f32) -> f32 {
    let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
    (0.5 - d).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The per-pixel form `rrect` used before it grew its opaque-row fast path: every pixel runs the
    /// distance and blend. The fast path must be pixel-identical to this.
    fn reference(bm: &mut Bitmap, x: f32, y: f32, w: f32, h: f32, radius: f32, rgb: [u8; 3]) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let r = radius.clamp(0.0, w.min(h) / 2.0);
        let (x0, y0) = (x.floor() as i32, y.floor() as i32);
        let (x1, y1) = ((x + w).ceil() as i32, (y + h).ceil() as i32);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        for py in y0.max(0)..y1.min(bm.height as i32) {
            for px in x0.max(0)..x1.min(bm.width as i32) {
                let qx = (px as f32 + 0.5 - cx).abs() - (w / 2.0 - r);
                let qy = (py as f32 + 0.5 - cy).abs() - (h / 2.0 - r);
                let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
                bm.blend(px, py, rgb, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// A straight-alpha RGBA image from a per-pixel function.
    fn synth(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| f(x, y))
            .collect()
    }

    /// The spread (max - min) of the green channel inside a region.
    fn spread(bm: &Bitmap, x0: u32, y0: u32, x1: u32, y1: u32) -> u8 {
        let (mut lo, mut hi) = (255u8, 0u8);
        for y in y0..y1 {
            for x in x0..x1 {
                let v = bm.pixels[(y * bm.width + x) as usize * 4 + 1];
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        hi - lo
    }

    #[test]
    fn a_fine_pattern_shrunk_a_lot_averages_to_flat_grey() {
        // One-pixel checks and one-pixel stripes, shrunk by 4 and by 4.3 so the cells do not line
        // up with the pattern. A 4.3 px cell over stripes holds 2 or 2.3 px of white, so a box
        // filter itself varies by 0.3 / 4.3 of full scale (18 levels); bilinear sampling swings
        // over nearly the whole range.
        let checker = |x: u32, y: u32| {
            let v = if (x + y) % 2 == 0 { 255 } else { 0 };
            [v, v, v, 255]
        };
        let stripes = |x: u32, _| {
            let v = if x % 2 == 0 { 255 } else { 0 };
            [v, v, v, 255]
        };
        for (name, size, pattern, most) in [
            ("checker 4x", 400, synth(400, 400, checker), 1),
            ("stripes 4x", 400, synth(400, 400, stripes), 1),
            ("checker 4.3x", 430, synth(430, 430, checker), 6),
            ("stripes 4.3x", 430, synth(430, 430, stripes), 19),
        ] {
            let mut area = Bitmap::new(120, 120);
            area.clear([255, 0, 0]);
            area.image(&pattern, size, size, 10.0, 10.0, 100.0, 100.0);
            let s = spread(&area, 10, 10, 110, 110);
            assert!(s <= most, "{name}: area-averaged spread {s}");
            let mid = area.pixels[(60 * 120 + 60) * 4 + 1];
            assert!((110..=146).contains(&mid), "{name}: grey {mid}");
            // The border is untouched.
            assert_eq!(&area.pixels[0..4], &[0, 0, 255, 255]);
            assert_eq!(
                &area.pixels[(9 * 120 + 50) * 4..(9 * 120 + 50) * 4 + 4],
                &[0, 0, 255, 255]
            );

            // What this replaces: bilinear sampling of the same image is a moiré. (At exactly 4x
            // it happens to sample halfway between two pixels and come out grey as well.)
            if size == 430 {
                let mut bilinear = Bitmap::new(120, 120);
                bilinear.image_bilinear(&pattern, size, size, 10.0, 10.0, 100.0, 100.0);
                let s = spread(&bilinear, 10, 10, 110, 110);
                assert!(s > 100, "{name}: bilinear spread {s}");
            }
        }
    }

    #[test]
    fn shrinking_a_cut_out_leaves_no_dark_fringe() {
        // Opaque white on the left, fully transparent black on the right: averaging straight
        // alpha would pull the edge towards black.
        let image = synth(400, 64, |x, _| {
            if x < 203 {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let mut on_white = Bitmap::new(100, 16);
        on_white.clear([255, 255, 255]);
        on_white.image(&image, 400, 64, 0.0, 0.0, 100.0, 16.0);
        assert!(on_white.pixels.iter().all(|&v| v == 255), "a dark fringe");

        let mut on_black = Bitmap::new(100, 16);
        on_black.clear([0, 0, 0]);
        on_black.image(&image, 400, 64, 0.0, 0.0, 100.0, 16.0);
        let row: Vec<u8> = (0..100).map(|x| on_black.pixels[x * 4]).collect();
        assert_eq!(&row[..50], &[255; 50]);
        assert_eq!(row[50], 191, "the edge cell is three quarters covered");
        assert_eq!(&row[51..], &[0; 49]);
    }

    #[test]
    fn mild_scaling_keeps_the_bilinear_path_and_clipping_crops() {
        let image = synth(97, 61, |x, y| {
            [
                (x * 5) as u8,
                (y * 9) as u8,
                (x * y) as u8,
                (128 + x + y) as u8,
            ]
        });
        // Half size and up: exactly the bilinear output.
        for (w, h) in [(49.0, 31.0), (97.0, 61.0), (150.0, 90.0)] {
            let mut a = Bitmap::new(160, 100);
            a.clear([30, 60, 90]);
            let mut b = a.clone();
            a.image(&image, 97, 61, 3.0, 4.0, w, h);
            b.image_bilinear(&image, 97, 61, 3.0, 4.0, w, h);
            assert!(a.pixels == b.pixels, "{w}x{h}");
        }
        // A shrunk image cut by the bitmap's edges is the same pixels as the whole draw.
        let mut whole = Bitmap::new(60, 40);
        whole.clear([30, 60, 90]);
        whole.image(&image, 97, 61, 20.0, 15.0, 23.0, 14.0);
        let mut part = Bitmap::new(30, 20);
        part.clear([30, 60, 90]);
        part.image(&image, 97, 61, 20.0 - 30.0, 15.0 - 20.0, 23.0, 14.0);
        for y in 0..20 {
            let a = &whole.pixels[((y + 20) * 60 + 30) * 4..((y + 20) * 60 + 60) * 4];
            let b = &part.pixels[y * 30 * 4..(y + 1) * 30 * 4];
            assert_eq!(a, b, "row {y}");
        }
    }

    #[test]
    fn the_rectangle_fast_paths_match_the_per_pixel_reference() {
        let cases: &[(f32, f32, f32, f32, f32)] = &[
            (10.0, 12.0, 200.0, 40.0, 0.0),
            (10.3, 12.7, 200.0, 40.0, 0.0),
            (10.3, 12.7, 200.0, 40.0, 6.0),
            (0.5, 0.5, 120.0, 90.0, 12.0),
            (-20.0, -15.0, 100.0, 60.0, 4.0),
            (250.0, 180.0, 40.0, 30.0, 4.0),
            (150.0, 70.0, 40.0, 30.0, 100.0),
            (5.0, 5.0, 1.4, 0.6, 0.0),
            (60.0, 60.0, 30.0, 30.0, 15.0),
        ];
        for &(x, y, w, h, r) in cases {
            let mut fast = Bitmap::new(320, 240);
            fast.clear([255, 255, 255]);
            let mut slow = Bitmap::new(320, 240);
            slow.clear([255, 255, 255]);
            fast.rrect(x, y, w, h, r, [20, 40, 200]);
            reference(&mut slow, x, y, w, h, r, [20, 40, 200]);
            assert_eq!(fast.pixels, slow.pixels, "rect {x},{y} {w}x{h} r {r}");
        }
    }
}
