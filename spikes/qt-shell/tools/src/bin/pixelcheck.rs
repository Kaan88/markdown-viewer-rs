//! pixelcheck IMAGE.png EXPECTED_W EXPECTED_H
//! Finds the 1 px checkerboard region in a capture and checks it pixel by pixel.
//! Prints PASS only if the region is the expected size (within 2 px, since the edges
//! can round) and has no non-black/white pixel and no parity error.
use std::collections::HashMap;
use std::fs::File;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (ew, eh): (usize, usize) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let mut dec = png::Decoder::new(std::io::BufReader::new(File::open(&a[1]).unwrap()));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut buf).unwrap();
    let (w, h, ch) = (info.width as usize, info.height as usize, info.color_type.samples());
    let px = |x: usize, y: usize| {
        let i = (y * w + x) * ch;
        (buf[i], buf[i + 1], buf[i + 2])
    };
    let bw = |p: (u8, u8, u8)| p == (0, 0, 0) || p == (255, 255, 255);
    let inv = |p: (u8, u8, u8), q: (u8, u8, u8)| bw(p) && bw(q) && p != q;

    // A pixel is checker-consistent if it has an inverse neighbour on both axes.
    let (mut rows, mut cols) = (vec![0usize; h], vec![0usize; w]);
    for y in 0..h {
        for x in 0..w {
            let p = px(x, y);
            if !bw(p) {
                continue;
            }
            let hz = (x > 0 && inv(p, px(x - 1, y))) || (x + 1 < w && inv(p, px(x + 1, y)));
            let vt = (y > 0 && inv(p, px(x, y - 1))) || (y + 1 < h && inv(p, px(x, y + 1)));
            if hz && vt {
                rows[y] += 1;
                cols[x] += 1;
            }
        }
    }
    // Rows or columns that are at least 30% checkerboard belong to the region.
    let span = |v: &[usize], need: usize| {
        let first = v.iter().position(|&c| c >= need)?;
        let last = v.iter().rposition(|&c| c >= need)?;
        Some((first, last))
    };
    let (Some((y0, y1)), Some((x0, x1))) = (span(&rows, ew * 3 / 10), span(&cols, eh * 3 / 10)) else {
        println!("FAIL image={w}x{h} no checkerboard region found (expected {ew}x{eh})");
        return;
    };
    let (rw, rh) = (x1 - x0 + 1, y1 - y0 + 1);
    let (mut non_bw, mut parity) = (0u64, 0u64);
    let mut grey: HashMap<(u8, u8, u8), u64> = HashMap::new();
    let first = px(x0, y0);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = px(x, y);
            if !bw(p) {
                non_bw += 1;
                *grey.entry(p).or_default() += 1;
            } else if (p == first) != ((x - x0 + y - y0) % 2 == 0) {
                parity += 1;
            }
        }
    }
    let mut top: Vec<_> = grey.into_iter().collect();
    top.sort_by_key(|e| std::cmp::Reverse(e.1));
    top.truncate(4);
    let size_ok = rw + 2 >= ew && rw <= ew + 2 && rh + 2 >= eh && rh <= eh + 2;
    let pass = non_bw == 0 && parity == 0 && size_ok;
    println!(
        "{} image={w}x{h} region=({x0},{y0}) {rw}x{rh} expected={ew}x{eh} pixels={} non_bw={non_bw} parity_errors={parity} top_non_bw={top:?}",
        if pass { "PASS" } else { "FAIL" },
        rw * rh,
    );
}
