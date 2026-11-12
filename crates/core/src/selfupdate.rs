//! Replacing the application's own files with downloaded ones.
//!
//! A new file is first written beside its target as `<name>.new` and checked there. [`swap_in`]
//! then renames each running file aside to `<name>.old` and the new one into its place: Windows
//! lets a running executable be renamed, though not overwritten or deleted, so the process that
//! is running keeps its old image and the next launch gets the new one. If any step fails, the
//! steps already taken are undone, so the folder never holds a mix of two versions.
//! [`clean_up`] removes what an earlier update left behind, once the old process has gone.

use std::io;
use std::path::{Path, PathBuf};

fn with_suffix(target: &Path, suffix: &str) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Where the new copy of `target` is written and checked before it is swapped in.
pub fn staged(target: &Path) -> PathBuf {
    with_suffix(target, ".new")
}

/// Where the replaced copy of `target` waits until it can be deleted.
pub fn retired(target: &Path) -> PathBuf {
    with_suffix(target, ".old")
}

/// Puts every target's staged file in its place, all or none.
///
/// Every staged file must exist. On an error the targets are as they were and the staged files
/// are still there.
pub fn swap_in(targets: &[PathBuf]) -> io::Result<()> {
    for target in targets {
        if !staged(target).is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is missing", staged(target).display()),
            ));
        }
    }
    let mut done: Vec<&PathBuf> = Vec::new();
    for target in targets {
        if let Err(e) = swap_one(target) {
            for earlier in done.into_iter().rev() {
                // Best effort: put the new file back where it was staged, then the old one back.
                _ = std::fs::rename(earlier, staged(earlier));
                _ = std::fs::rename(retired(earlier), earlier);
            }
            return Err(e);
        }
        done.push(target);
    }
    Ok(())
}

fn swap_one(target: &Path) -> io::Result<()> {
    let old = retired(target);
    // A copy retired by an earlier update: gone unless a process of that version still runs, and
    // then this update has to wait for it.
    match std::fs::remove_file(&old) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let had_target = target.exists();
    if had_target {
        std::fs::rename(target, &old)?;
    }
    if let Err(e) = std::fs::rename(staged(target), target) {
        if had_target {
            _ = std::fs::rename(&old, target);
        }
        return Err(e);
    }
    Ok(())
}

/// Deletes the retired and staged copies of `targets`, as far as the system allows. A retired
/// file that a process still runs stays until a later call.
pub fn clean_up(targets: &[PathBuf]) {
    for target in targets {
        _ = std::fs::remove_file(retired(target));
        _ = std::fs::remove_file(staged(target));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("mdview-selfupdate-{name}-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn read(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn both_files_are_replaced_and_the_old_ones_kept_aside() {
        let d = dir("ok");
        let (app, engine) = (d.join("app.exe"), d.join("engine.exe"));
        std::fs::write(&app, "app 1").unwrap();
        std::fs::write(&engine, "engine 1").unwrap();
        std::fs::write(staged(&app), "app 2").unwrap();
        std::fs::write(staged(&engine), "engine 2").unwrap();
        // Left by an earlier update.
        std::fs::write(retired(&app), "app 0").unwrap();

        swap_in(&[app.clone(), engine.clone()]).unwrap();
        assert_eq!(read(&app), "app 2");
        assert_eq!(read(&engine), "engine 2");
        assert_eq!(read(&retired(&app)), "app 1");
        assert_eq!(read(&retired(&engine)), "engine 1");
        assert!(!staged(&app).exists() && !staged(&engine).exists());

        clean_up(&[app.clone(), engine.clone()]);
        assert!(!retired(&app).exists() && !retired(&engine).exists());
        assert_eq!(read(&app), "app 2");
        _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_staged_file_changes_nothing() {
        let d = dir("missing");
        let (app, engine) = (d.join("app.exe"), d.join("engine.exe"));
        std::fs::write(&app, "app 1").unwrap();
        std::fs::write(&engine, "engine 1").unwrap();
        std::fs::write(staged(&app), "app 2").unwrap();

        assert!(swap_in(&[app.clone(), engine.clone()]).is_err());
        assert_eq!(read(&app), "app 1");
        assert_eq!(read(&engine), "engine 1");
        assert_eq!(read(&staged(&app)), "app 2");
        assert!(!retired(&app).exists());
        _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failure_on_the_second_file_puts_the_first_back() {
        let d = dir("rollback");
        let (app, engine) = (d.join("app.exe"), d.join("engine.exe"));
        std::fs::write(&app, "app 1").unwrap();
        std::fs::write(&engine, "engine 1").unwrap();
        std::fs::write(staged(&app), "app 2").unwrap();
        std::fs::write(staged(&engine), "engine 2").unwrap();
        // A folder where the engine's retired copy would go cannot be removed as a file, so the
        // engine's swap fails after the application's has been done.
        std::fs::create_dir_all(retired(&engine).join("in-the-way")).unwrap();

        assert!(swap_in(&[app.clone(), engine.clone()]).is_err());
        assert_eq!(read(&app), "app 1");
        assert_eq!(read(&engine), "engine 1");
        assert_eq!(read(&staged(&app)), "app 2");
        assert_eq!(read(&staged(&engine)), "engine 2");
        assert!(!retired(&app).exists());
        _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn a_running_executable_can_be_swapped() {
        // The test binary itself stands in for the application: copy it, run the copy, and
        // replace the copy's file while it runs.
        let d = dir("running");
        let app = d.join("app.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &app).unwrap();
        let mut child = std::process::Command::new(&app)
            .args([
                "--ignored",
                "--exact",
                "selfupdate::tests::sleeps_for_the_swap_test",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            std::fs::remove_file(&app).is_err(),
            "a running exe cannot be deleted"
        );
        std::fs::write(staged(&app), "new").unwrap();
        let swapped = swap_in(std::slice::from_ref(&app));
        let new = std::fs::read_to_string(&app);
        _ = child.kill();
        _ = child.wait();
        swapped.unwrap();
        assert_eq!(new.unwrap(), "new");
        clean_up(std::slice::from_ref(&app));
        assert!(
            !retired(&app).exists(),
            "deleted once the old process has gone"
        );
        _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    #[ignore = "helper process for a_running_executable_can_be_swapped"]
    fn sleeps_for_the_swap_test() {
        std::thread::sleep(std::time::Duration::from_secs(20));
    }
}
