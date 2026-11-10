//! AppImage self-update, std only. The AppImage runtime sets `APPIMAGE` to the running file; the
//! embedded update information is what `appimageupdatetool` reads, so this module only decides
//! *whether* to check, runs the tool off the UI thread, and turns its exit code into a status.
//! The contract is `packaging/appimage/README.md`: `--check-for-update` exits 0 (current), 1 (an
//! update exists) or something else (error); a plain run applies the update as a zsync delta.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, SystemTime};

/// At most one automatic check per day.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    UpToDate,
    Available,
    Updated,
    Failed(String),
}

/// The running AppImage, when this process is one.
pub fn appimage() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("APPIMAGE")?);
    path.is_file().then_some(path)
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|d| d.join("mdview"))
}

fn state_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|d| d.join("mdview"))
}

/// The opt-out the helper script and the shell share: `MDVIEW_NO_UPDATE_CHECK` or a marker file.
pub fn opted_out() -> bool {
    std::env::var_os("MDVIEW_NO_UPDATE_CHECK").is_some()
        || config_dir().is_some_and(|d| d.join("no-update-check").exists())
}

/// Turns the automatic check off or on by writing or removing the marker file.
pub fn set_opted_out(out: bool) -> std::io::Result<()> {
    let Some(dir) = config_dir() else {
        return Ok(());
    };
    let marker = dir.join("no-update-check");
    if out {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(marker, "")
    } else {
        match std::fs::remove_file(marker) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// True when `stamp` is missing or older than a day.
pub fn due(stamp: &Path, now: SystemTime) -> bool {
    std::fs::metadata(stamp)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| now.duration_since(t).ok())
        .is_none_or(|age| age >= CHECK_EVERY)
}

/// Maps `appimageupdatetool --check-for-update`'s exit code.
pub fn check_status(code: Option<i32>) -> Status {
    match code {
        Some(0) => Status::UpToDate,
        Some(1) => Status::Available,
        Some(n) => Status::Failed(format!("exit code {n}")),
        None => Status::Failed("the update tool was stopped".into()),
    }
}

fn tool() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    ["appimageupdatetool", "AppImageUpdate"]
        .iter()
        .flat_map(|name| std::env::split_paths(&path).map(move |dir| dir.join(name)))
        .find(|p| p.is_file())
}

/// Starts the once-a-day check on a worker thread. `None` when this is not an AppImage, the user
/// opted out, a check ran in the last 24 hours, or the tool is not installed.
pub fn spawn_check() -> Option<Receiver<Status>> {
    let image = appimage()?;
    if opted_out() {
        return None;
    }
    let stamp = state_dir()?.join("update-check.stamp");
    if !due(&stamp, SystemTime::now()) {
        return None;
    }
    let tool = tool()?;
    if let Some(dir) = stamp.parent() {
        _ = std::fs::create_dir_all(dir);
    }
    _ = std::fs::write(&stamp, "");
    let (send, receive) = channel();
    std::thread::spawn(move || {
        let code = Command::new(tool)
            .arg("--check-for-update")
            .arg(&image)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.code());
        _ = send.send(match code {
            Ok(code) => check_status(code),
            Err(e) => Status::Failed(e.to_string()),
        });
    });
    Some(receive)
}

/// Applies the update now (a zsync delta), on a worker thread. The running file is replaced on
/// disk; the user restarts to use it.
pub fn spawn_apply() -> Option<Receiver<Status>> {
    let image = appimage()?;
    let tool = tool()?;
    let (send, receive) = channel();
    std::thread::spawn(move || {
        let result = Command::new(tool)
            .arg(&image)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        _ = send.send(match result {
            Ok(s) if s.success() => Status::Updated,
            Ok(s) => Status::Failed(format!("exit code {}", s.code().unwrap_or(-1))),
            Err(e) => Status::Failed(e.to_string()),
        });
    });
    Some(receive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_map_to_statuses() {
        assert_eq!(check_status(Some(0)), Status::UpToDate);
        assert_eq!(check_status(Some(1)), Status::Available);
        assert!(matches!(check_status(Some(2)), Status::Failed(_)));
        assert!(matches!(check_status(None), Status::Failed(_)));
    }

    #[test]
    fn a_check_is_due_once_a_day() {
        let dir = std::env::temp_dir().join(format!("mdview-stamp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let stamp = dir.join("s");
        assert!(due(&stamp, SystemTime::now()), "no stamp yet");
        std::fs::write(&stamp, "").unwrap();
        let now = SystemTime::now();
        assert!(!due(&stamp, now), "just checked");
        assert!(due(&stamp, now + CHECK_EVERY + Duration::from_secs(5)), "a day later");
        _ = std::fs::remove_dir_all(&dir);
    }
}
