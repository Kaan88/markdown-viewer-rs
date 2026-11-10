//! Watches the folders that hold the open files, so a change on disk reaches the viewer without a
//! poll. Parent folders rather than the files themselves, because editors save by writing a new
//! file and renaming it over the old one, which ends a watch on the file itself.
//!
//! The watcher thread never touches the UI: it calls `wake`, and the shell checks the files on its
//! own thread. Wakes are coalesced, so a burst of events costs one check.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

/// Shared by the watcher thread and the UI thread.
struct Shared {
    /// The open files' folders and names, compared against each event's paths.
    files: std::sync::Mutex<BTreeSet<PathBuf>>,
    /// A wake has been sent and the shell has not called `woken` yet.
    pending: AtomicBool,
}

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    shared: Arc<Shared>,
    folders: BTreeSet<PathBuf>,
}

impl FileWatcher {
    /// `None` when the OS watcher cannot start (no inotify instances left, for example); the shell
    /// then falls back to a slow poll.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Option<FileWatcher> {
        let shared = Arc::new(Shared {
            files: Default::default(),
            pending: AtomicBool::new(false),
        });
        let seen = shared.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else {
                return;
            };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            let ours = {
                let files = seen.files.lock().unwrap_or_else(|e| e.into_inner());
                event.paths.iter().any(|path| files.contains(path))
            };
            if ours && !seen.pending.swap(true, Ordering::AcqRel) {
                wake();
            }
        })
        .ok()?;
        Some(FileWatcher {
            watcher,
            shared,
            folders: BTreeSet::new(),
        })
    }

    /// Follows exactly these files from now on. Folders no longer needed are released.
    pub fn set_files<'a>(&mut self, paths: impl IntoIterator<Item = &'a Path>) {
        let files: BTreeSet<PathBuf> = paths.into_iter().map(Path::to_path_buf).collect();
        let folders: BTreeSet<PathBuf> = files
            .iter()
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .collect();
        for gone in self.folders.difference(&folders) {
            _ = self.watcher.unwatch(gone);
        }
        let mut watched = BTreeSet::new();
        for folder in &folders {
            if self.folders.contains(folder)
                || self
                    .watcher
                    .watch(folder, RecursiveMode::NonRecursive)
                    .is_ok()
            {
                watched.insert(folder.clone());
            }
        }
        self.folders = watched;
        *self.shared.files.lock().unwrap_or_else(|e| e.into_inner()) = files;
    }

    /// The shell calls this before it checks the files, so a change during the check wakes again.
    pub fn woken(&self) {
        self.shared.pending.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn a_save_by_rename_wakes_and_other_files_do_not() {
        let dir = std::env::temp_dir().join(format!("mdview-watch-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let file = dir.join("a.md");
        std::fs::write(&file, "# a\n").unwrap();
        let (send, receive) = channel();
        let mut watcher = FileWatcher::new(move || _ = send.send(())).expect("watcher");
        watcher.set_files([file.as_path()]);

        std::fs::write(dir.join("other.md"), "x").unwrap();
        assert!(
            receive.recv_timeout(Duration::from_millis(300)).is_err(),
            "a file that is not open does not wake"
        );

        // An editor's atomic save: write a temporary file, then rename it over the open one.
        let temp = dir.join(".a.md.tmp");
        std::fs::write(&temp, "# b\n").unwrap();
        std::fs::rename(&temp, &file).unwrap();
        receive
            .recv_timeout(Duration::from_secs(2))
            .expect("the rename wakes");
        // Until the shell says it has looked, further events do not wake again.
        std::fs::write(&file, "# c\n").unwrap();
        assert!(receive.recv_timeout(Duration::from_millis(300)).is_err());
        watcher.woken();
        std::fs::remove_file(&file).unwrap();
        receive
            .recv_timeout(Duration::from_secs(2))
            .expect("a delete wakes");

        watcher.woken();
        watcher.set_files([]);
        std::fs::write(&file, "# d\n").unwrap();
        assert!(
            receive.recv_timeout(Duration::from_millis(300)).is_err(),
            "nothing is watched after set_files([])"
        );
        _ = std::fs::remove_dir_all(&dir);
    }
}
