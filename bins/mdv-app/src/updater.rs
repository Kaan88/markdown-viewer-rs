//! Updating mdview in place: look for a newer version, download its two executables, check their
//! hashes, and swap them in.
//!
//! Everything here blocks, so the shell calls it from a thread of its own and hears back through
//! a wake. The manifest format, version ordering and SHA-256 are `mdview_core::update`; the swap
//! is `mdview_core::selfupdate`; the transport is `mdview_fetch::fetch_file`. The binaries are
//! not code-signed, so the hash in the manifest, fetched over TLS from GitHub, is the only
//! integrity check there is.

use mdview_core::selfupdate;
use mdview_core::update::{self, Download, Platform, UpdateManifest, Version};
use std::path::PathBuf;
use std::time::Duration;

/// The manifest of the latest release.
pub const MANIFEST_URL: &str =
    "https://github.com/Kaan88/markdown-viewer-rs/releases/latest/download/mdview-update.txt";
/// Seconds between two automatic looks for a newer version.
pub const CHECK_EVERY: u64 = 24 * 60 * 60;
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(10);
/// The engine is about 26 MB; this is for both files on a slow line.
const FILE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_FILE_BYTES: usize = 128 * 1024 * 1024;

/// A newer version this build can install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub version: Version,
    pub notes: Option<String>,
    pub app: Download,
    pub engine: Download,
}

/// Where the reader's update stands, for the bar over the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    None,
    Available(Offer),
    Installing(Offer),
    /// The files are in place; the running process is still the old version.
    Installed(Version),
}

/// What an updater thread reports back.
#[derive(Debug)]
pub enum Event {
    Checked {
        /// The reader asked, so the answer is shown even when there is nothing new.
        manual: bool,
        result: Result<Option<Offer>, String>,
    },
    Installed(Result<(), String>),
}

/// One GET: URL, size cap, time allowed.
pub type Fetch<'a> = &'a dyn Fn(&str, usize, Duration) -> Result<Vec<u8>, String>;

/// The transport the application uses.
pub fn fetch(url: &str, max_bytes: usize, timeout: Duration) -> Result<Vec<u8>, String> {
    mdview_fetch::fetch_file(url, max_bytes, timeout)
}

pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the package version is a valid version")
}

/// `MDVIEW_UPDATE_MANIFEST` names another manifest, for trying a release before it is the latest.
/// It is fetched under the same rules: https only, and the hashes it lists are checked.
pub fn manifest_url() -> String {
    std::env::var("MDVIEW_UPDATE_MANIFEST").unwrap_or_else(|_| MANIFEST_URL.to_string())
}

/// The two files an update replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Files {
    pub app: PathBuf,
    pub engine: PathBuf,
}

impl Files {
    fn targets(&self) -> [PathBuf; 2] {
        [self.app.clone(), self.engine.clone()]
    }
}

/// This process's executable and the engine beside it, when this copy can update itself: both
/// files are there, and the folder is not a cargo build's output.
pub fn installed_files() -> Option<Files> {
    let app = std::env::current_exe().ok()?;
    let dir = app.parent()?;
    let built = dir
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| matches!(n, "release" | "debug" | "deps"));
    let engine = dir.join(format!("mdview-engine{}", std::env::consts::EXE_SUFFIX));
    (!built && engine.is_file()).then_some(Files { app, engine })
}

/// Reads a manifest: the offer if it names a version newer than `current` with both files for
/// this platform.
pub fn offer(manifest: &[u8], current: &Version) -> Result<Option<Offer>, String> {
    let manifest = UpdateManifest::parse(manifest).map_err(|e| e.to_string())?;
    let Some(platform) = Platform::current() else {
        return Ok(None);
    };
    let Some(asset) = update::newer(current, &manifest, platform) else {
        return Ok(None);
    };
    // A release without the engine cannot be installed in place; it is not an offer.
    let Some(engine) = asset.engine else {
        return Ok(None);
    };
    Ok(Some(Offer {
        version: manifest.version,
        notes: manifest.notes,
        app: Download {
            url: asset.url,
            sha256: asset.sha256,
        },
        engine,
    }))
}

/// Fetches the manifest and reads it.
pub fn check(fetch: Fetch) -> Result<Option<Offer>, String> {
    let bytes = fetch(
        &manifest_url(),
        update::MAX_MANIFEST_BYTES,
        MANIFEST_TIMEOUT,
    )?;
    offer(&bytes, &current_version())
}

/// Downloads both files beside their targets, checks each against its hash, and swaps them in.
/// On any failure nothing is replaced and the downloads are removed.
pub fn install(offer: &Offer, files: &Files, fetch: Fetch) -> Result<(), String> {
    let result = stage_and_swap(offer, files, fetch);
    if result.is_err() {
        for target in files.targets() {
            _ = std::fs::remove_file(selfupdate::staged(&target));
        }
    }
    result
}

fn stage_and_swap(offer: &Offer, files: &Files, fetch: Fetch) -> Result<(), String> {
    for (download, target) in [(&offer.app, &files.app), (&offer.engine, &files.engine)] {
        let bytes = fetch(&download.url, MAX_FILE_BYTES, FILE_TIMEOUT)?;
        if !update::verify_sha256(&bytes, &download.sha256) {
            return Err(format!(
                "{} does not match its SHA-256",
                target.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        std::fs::write(selfupdate::staged(target), &bytes).map_err(|e| e.to_string())?;
    }
    selfupdate::swap_in(&files.targets()).map_err(|e| e.to_string())
}

/// Removes what an earlier update left beside the executables.
pub fn clean_up(files: &Files) {
    selfupdate::clean_up(&files.targets());
}

/// The installer's entry in Windows' list of installed apps (`AppId` in `mdview.iss`).
#[cfg(windows)]
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{B7E2F4A9-3D1C-4C58-A6E0-91D5C8F27B34}_is1";

/// Writes the new version into the installed-apps entry, if the installer made one. Best effort:
/// the entry only says what Settings shows.
#[cfg(windows)]
pub fn record_version(version: &Version) {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let reg = |args: &[&str]| {
        Command::new("reg.exe")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .is_ok_and(|s| s.success())
    };
    if !reg(&["query", UNINSTALL_KEY, "/v", "DisplayVersion"]) {
        return;
    }
    let version = version.to_string();
    reg(&[
        "add",
        UNINSTALL_KEY,
        "/v",
        "DisplayVersion",
        "/t",
        "REG_SZ",
        "/d",
        &version,
        "/f",
    ]);
}

#[cfg(not(windows))]
pub fn record_version(_version: &Version) {}

#[cfg(test)]
mod tests {
    use super::*;
    use mdview_core::update::sha256_hex;
    use std::cell::RefCell;

    fn manifest(version: &str, app: &[u8], engine: Option<&[u8]>) -> String {
        let p = if cfg!(windows) {
            "windows-x86_64"
        } else {
            "linux-x86_64"
        };
        let mut m = format!(
            "manifest: 1\nversion: {version}\nnotes: Faster.\n{p}.asset: mdview.exe\n\
             {p}.url: https://example.com/app\n{p}.sha256: {}\n",
            sha256_hex(app)
        );
        if let Some(engine) = engine {
            m.push_str(&format!(
                "{p}.engine.url: https://example.com/engine\n{p}.engine.sha256: {}\n",
                sha256_hex(engine)
            ));
        }
        m
    }

    fn files(name: &str) -> Files {
        let dir =
            std::env::temp_dir().join(format!("mdview-updater-{name}-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let files = Files {
            app: dir.join("mdview.exe"),
            engine: dir.join("mdview-engine.exe"),
        };
        std::fs::write(&files.app, "app 1").unwrap();
        std::fs::write(&files.engine, "engine 1").unwrap();
        files
    }

    fn read(p: &std::path::Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn only_a_newer_version_with_both_files_is_an_offer() {
        let current = Version::parse("0.2.0").unwrap();
        let newer = manifest("0.3.0", b"a", Some(b"e"));
        let o = offer(newer.as_bytes(), &current).unwrap().unwrap();
        assert_eq!(o.version.to_string(), "0.3.0");
        assert_eq!(o.notes.as_deref(), Some("Faster."));
        assert_eq!(o.app.url, "https://example.com/app");
        assert_eq!(o.engine.sha256, sha256_hex(b"e"));

        for not_offered in [
            manifest("0.2.0", b"a", Some(b"e")),
            manifest("0.1.9", b"a", Some(b"e")),
            manifest("0.3.0", b"a", None),
        ] {
            assert_eq!(offer(not_offered.as_bytes(), &current), Ok(None));
        }
        assert!(offer(b"<html>Not Found</html>", &current).is_err());
    }

    #[test]
    fn install_checks_both_hashes_before_anything_is_replaced() {
        let files = files("install");
        let o = offer(
            manifest("9.0.0", b"app 2", Some(b"engine 2")).as_bytes(),
            &current_version(),
        )
        .unwrap()
        .unwrap();

        // The engine arrives damaged: nothing changes, and no download is left behind.
        let asked = RefCell::new(Vec::new());
        let damaged = |url: &str, _: usize, _: Duration| {
            asked.borrow_mut().push(url.to_string());
            Ok(if url.ends_with("/app") {
                b"app 2".to_vec()
            } else {
                b"engine 2, damaged".to_vec()
            })
        };
        let err = install(&o, &files, &damaged).unwrap_err();
        assert!(err.contains("mdview-engine.exe"), "{err}");
        assert_eq!(asked.borrow().len(), 2);
        assert_eq!(read(&files.app), "app 1");
        assert_eq!(read(&files.engine), "engine 1");
        assert!(!selfupdate::staged(&files.app).exists());
        assert!(!selfupdate::staged(&files.engine).exists());

        // A failed download likewise.
        let offline = |_: &str, _: usize, _: Duration| Err("Timeout".to_string());
        assert_eq!(install(&o, &files, &offline), Err("Timeout".to_string()));
        assert_eq!(read(&files.app), "app 1");

        let good = |url: &str, _: usize, _: Duration| {
            Ok(if url.ends_with("/app") {
                b"app 2".to_vec()
            } else {
                b"engine 2".to_vec()
            })
        };
        install(&o, &files, &good).unwrap();
        assert_eq!(read(&files.app), "app 2");
        assert_eq!(read(&files.engine), "engine 2");
        assert_eq!(read(&selfupdate::retired(&files.app)), "app 1");
        clean_up(&files);
        assert!(!selfupdate::retired(&files.app).exists());
        assert!(!selfupdate::retired(&files.engine).exists());
        _ = std::fs::remove_dir_all(files.app.parent().unwrap());
    }

    /// `build-installer.ps1` writes the manifest of a release; when it has run, what it wrote
    /// must read as an installable release of this very version.
    #[cfg(windows)]
    #[test]
    fn the_release_script_writes_a_manifest_this_build_reads() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release-files/mdview-update.txt");
        let Ok(bytes) = std::fs::read(&path) else {
            return; // The script has not run in this checkout.
        };
        let older = Version::parse("0.0.1").unwrap();
        let o = offer(&bytes, &older)
            .expect("the manifest parses")
            .expect("it offers a release with both files");
        assert!(o.app.url.ends_with(&format!("/v{}/mdview.exe", o.version)));
        assert!(
            o.engine
                .url
                .ends_with(&format!("/v{}/mdview-engine.exe", o.version))
        );
        let dir = path.parent().unwrap();
        for (file, download) in [("mdview.exe", &o.app), ("mdview-engine.exe", &o.engine)] {
            let ok = update::verify_sha256_file(&dir.join(file), &download.sha256).unwrap();
            assert!(ok, "{file} matches the hash the manifest lists");
        }
    }

    #[test]
    fn a_cargo_build_does_not_update_itself() {
        // The test binary runs from `target/release/deps`.
        assert_eq!(installed_files(), None);
    }
}
