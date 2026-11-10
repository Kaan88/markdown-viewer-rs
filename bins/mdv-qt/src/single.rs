//! One running window per user. The first instance listens on a loopback port recorded beside the
//! settings file; a later launch sends its file paths there and exits. Plain `std`, no Qt: the
//! listener thread only forwards paths into a channel that the GUI thread drains on its poll tick.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

/// Where the running instance records its port.
pub fn port_file(settings: &Path) -> PathBuf {
    settings.with_file_name("instance.port")
}

/// Hands `paths` to an already running instance. True when one answered, so the caller can exit.
pub fn forward(settings: &Path, paths: &[PathBuf]) -> bool {
    let Some(port) = std::fs::read_to_string(port_file(settings))
        .ok()
        .and_then(|t| t.trim().parse::<u16>().ok())
    else {
        return false;
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(300)) else {
        return false;
    };
    _ = stream.set_read_timeout(Some(Duration::from_millis(1500)));
    let mut message = String::from("mdview\n");
    for path in paths {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.clone());
        message.push_str(&absolute.display().to_string());
        message.push('\n');
    }
    message.push('\n');
    if stream.write_all(message.as_bytes()).is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

/// Starts listening and records the port. `None` when the port cannot be bound or written.
pub fn listen(settings: &Path) -> Option<Receiver<PathBuf>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    if let Some(dir) = settings.parent() {
        _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(port_file(settings), port.to_string()).ok()?;
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut lines = BufReader::new(&stream).lines().map_while(Result::ok);
            if lines.next().as_deref() != Some("mdview") {
                continue;
            }
            for line in lines.take_while(|l| !l.is_empty()) {
                let path = PathBuf::from(line);
                if path.is_file() {
                    _ = sender.send(path);
                }
            }
            _ = (&stream).write_all(b"ok\n");
        }
    });
    Some(receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_launch_reaches_the_first() {
        let dir = std::env::temp_dir().join(format!("mdview-single-{}", std::process::id()));
        let settings = dir.join("settings.ini");
        assert!(!forward(&settings, &[]), "nothing listens yet");
        let receiver = listen(&settings).expect("listen");
        let file = dir.join("a.md");
        std::fs::write(&file, "# a\n").unwrap();
        assert!(forward(&settings, &[file.clone(), dir.join("missing.md")]));
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), file);
        assert!(receiver.try_recv().is_err(), "a missing file is not forwarded");
        _ = std::fs::remove_dir_all(&dir);
    }
}
