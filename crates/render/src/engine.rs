//! The client side of `mdview-engine`. One SVG starts one engine process and closes its stdin as
//! soon as the reply arrives, so a broken or slow rasterizer costs a placeholder, never the
//! viewer. The process is looked for beside this executable or in the workspace's `target/release`,
//! and `MDV_ENGINE` overrides the search.

use crate::image::{DecodedImage, MAX_PIXELS};
use mdview_engine_proto::{KIND_SVG, MAX_REPLY, Request, STATUS_OK, read_reply, write_request};
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// The plan's per-job deadline. A slow SVG is a placeholder, not a frozen window.
const DEADLINE: Duration = Duration::from_secs(5);

/// Renders SVG bytes through the engine. `None` when the engine is missing, times out, fails or
/// returns something the image cache cannot use.
pub fn render_svg(bytes: &[u8]) -> Option<DecodedImage> {
    let engine = engine_path()?;
    let mut child = Command::new(engine)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let payload = bytes.to_vec();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result: io::Result<_> = (|| {
            write_request(
                &mut stdin,
                &Request {
                    kind: KIND_SVG,
                    payload,
                },
            )?;
            // Closing stdin is the engine's cue to exit once it has answered.
            drop(stdin);
            read_reply(&mut stdout)
        })();
        let _ = sender.send(result);
    });
    let reply = match receiver.recv_timeout(DEADLINE) {
        Ok(Ok(reply)) => reply,
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    let _ = child.wait();
    if reply.status != STATUS_OK
        || reply.width == 0
        || reply.height == 0
        || reply.rgba.len() as u64 > MAX_REPLY
        || reply.rgba.len() != reply.width as usize * reply.height as usize * 4
        || reply.width as u64 * reply.height as u64 > MAX_PIXELS
    {
        return None;
    }
    Some(DecodedImage {
        width: reply.width,
        height: reply.height,
        rgba: reply.rgba,
    })
}

/// The engine binary: `MDV_ENGINE`, a sibling of this executable, or the workspace target beside
/// an ancestor (the Qt shell builds into its own `bins/mdv-qt/target/release`).
fn engine_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MDV_ENGINE") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let mut directory = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let name = if cfg!(windows) {
        "mdview-engine.exe"
    } else {
        "mdview-engine"
    };
    for _ in 0..6 {
        let sibling = directory.join(name);
        if sibling.is_file() {
            return Some(sibling);
        }
        let target = directory.join("target").join("release").join(name);
        if target.is_file() {
            return Some(target);
        }
        if !directory.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_engine_is_not_an_error() {
        // Unless a test asks for the real binary, the search either finds the workspace build (and
        // then a garbage job must still fail cleanly) or finds nothing.
        assert!(render_svg(b"not an svg").is_none());
    }
}
