//! One running window per user. The first instance listens on a loopback port recorded beside the
//! settings file; a later launch sends its file paths there and exits. Plain `std`, no Qt: the
//! listener thread only forwards paths into a channel that the GUI thread drains on its poll tick.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

/// Where the running instance records its port.
pub fn port_file(settings: &Path) -> PathBuf {
    settings.with_file_name("instance.port")
}

/// The file a running instance keeps open for as long as it lives.
fn lock_file(settings: &Path) -> PathBuf {
    settings.with_file_name("instance.lock")
}

/// Whether an instance is running, known at once. The port file outlives the instance that wrote
/// it, and on Windows a connection to a loopback port nobody listens on is not refused: it waits
/// out the whole timeout, which was 300 ms added to every ordinary launch. The running instance
/// holds the lock file open with no sharing, so opening it fails while that instance lives and
/// succeeds the moment it is gone, however it ended.
#[cfg(windows)]
fn instance_alive(settings: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    match std::fs::OpenOptions::new()
        .write(true)
        .share_mode(0)
        .open(lock_file(settings))
    {
        Ok(_) => false,
        Err(e) => e.raw_os_error() == Some(ERROR_SHARING_VIOLATION),
    }
}

/// Elsewhere a refused connection returns at once, so the port is asked directly.
#[cfg(not(windows))]
fn instance_alive(_settings: &Path) -> bool {
    true
}

/// Takes the lock [`instance_alive`] looks for. `None` if it cannot be taken, which only costs a
/// later launch its hand-over.
fn hold_lock(settings: &Path) -> Option<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(lock_file(settings)).ok()
}

/// Hands `paths` to an already running instance. True when one answered, so the caller can exit.
pub fn forward(settings: &Path, paths: &[PathBuf]) -> bool {
    let Some(port) = std::fs::read_to_string(port_file(settings))
        .ok()
        .and_then(|t| t.trim().parse::<u16>().ok())
    else {
        return false;
    };
    if !instance_alive(settings) {
        return false;
    }
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
    listen_waking(settings, || {})
}

/// `listen`, calling `wake` from the listener thread after each launch's paths are queued, so the
/// shell can drain them at once instead of on a timer.
pub fn listen_waking(
    settings: &Path,
    wake: impl Fn() + Send + 'static,
) -> Option<Receiver<PathBuf>> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    if let Some(dir) = settings.parent() {
        _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(port_file(settings), port.to_string()).ok()?;
    let lock = hold_lock(settings);
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        // Held until the process ends: this thread never returns.
        let _lock = lock;
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
            wake();
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
        assert!(
            receiver.try_recv().is_err(),
            "a missing file is not forwarded"
        );
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stale_port_file_does_not_cost_the_connection_timeout() {
        let dir = std::env::temp_dir().join(format!("mdview-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let settings = dir.join("settings.ini");
        // What an instance that has exited leaves: its port, and a lock nobody holds.
        let free = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        std::fs::write(port_file(&settings), port.to_string()).unwrap();
        std::fs::write(lock_file(&settings), "").unwrap();
        let started = std::time::Instant::now();
        assert!(!forward(&settings, &[]));
        if cfg!(windows) {
            assert!(
                started.elapsed() < Duration::from_millis(100),
                "{:?}",
                started.elapsed()
            );
        }
        _ = std::fs::remove_dir_all(&dir);
    }
}
