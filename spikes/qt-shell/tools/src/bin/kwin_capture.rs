//! kwin_capture OUT.png
//! True compositor capture: asks the session's KWin for the active screen through
//! org.kde.KWin.ScreenShot2 at native resolution and writes the raw pixels to a PNG.
//! Needs KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 on KWin, so use it on a nested KWin only.
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use zbus::zvariant::{Fd, OwnedValue, Value};

fn main() {
    let out = std::env::args().nth(1).expect("OUT.png");
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
    let read_end = unsafe { File::from_raw_fd(fds[0]) };
    let write_end = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    let reader = std::thread::spawn(move || {
        let mut v = Vec::new();
        let mut f = read_end;
        f.read_to_end(&mut v).unwrap();
        v
    });

    let bus = zbus::blocking::Connection::session().unwrap();
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("native-resolution", Value::from(true));
    opts.insert("include-cursor", Value::from(false));
    let reply = bus
        .call_method(
            Some("org.kde.KWin"),
            "/org/kde/KWin/ScreenShot2",
            Some("org.kde.KWin.ScreenShot2"),
            "CaptureActiveScreen",
            &(opts, Fd::from(write_end.as_fd())),
        )
        .expect("CaptureActiveScreen");
    drop(write_end);
    let meta: HashMap<String, OwnedValue> = reply.body().deserialize().unwrap();
    let get = |k: &str| u32::try_from(&meta[k]).unwrap() as usize;
    let (w, h, stride, fmt) = (get("width"), get("height"), get("stride"), get("format"));
    let raw = reader.join().unwrap();
    assert!(raw.len() >= stride * h, "short read {} < {}", raw.len(), stride * h);
    // QImage::Format: 4,5,6 are BGRA in memory on little endian; 16,17,18 are RGBA.
    let bgra = matches!(fmt, 4 | 5 | 6);
    assert!(bgra || matches!(fmt, 16 | 17 | 18), "unhandled QImage format {fmt}");
    let mut rgb = Vec::with_capacity(w * h * 3);
    for row in raw.chunks_exact(stride).take(h) {
        for p in row[..w * 4].chunks_exact(4) {
            if bgra {
                rgb.extend_from_slice(&[p[2], p[1], p[0]]);
            } else {
                rgb.extend_from_slice(&p[..3]);
            }
        }
    }
    let mut enc = png::Encoder::new(std::io::BufWriter::new(File::create(&out).unwrap()), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    println!("captured {w}x{h} format={fmt} stride={stride}");
}
