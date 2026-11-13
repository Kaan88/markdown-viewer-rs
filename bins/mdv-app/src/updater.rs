//! Updating mdview in place: `mdshared_app::updater` with this application's name, manifest and
//! files, and `mdshared_fetch` as the transport. `docs/UPDATE.md` describes the whole flow.

use mdshared_app::updater as shared;
use mdview_core::update::Version;
use std::time::Duration;

pub use shared::{
    Event, Fetch, Files, NOT_INSTALLED, Offer, Stage, Updates, at_launch_label, check_due,
    clean_up, install,
};

/// mdview as the updater knows it.
pub const APP: shared::App = shared::App {
    name: "mdview",
    version: env!("CARGO_PKG_VERSION"),
    manifest_url: "https://github.com/Kaan88/markdown-viewer-rs/releases/latest/download/mdview-update.txt",
    manifest_env: "MDVIEW_UPDATE_MANIFEST",
    engine: Some("mdview-engine"),
    // The installer's entry in Windows' list of installed apps (`AppId` in `mdview.iss`).
    uninstall_key: Some(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{B7E2F4A9-3D1C-4C58-A6E0-91D5C8F27B34}_is1",
    ),
};

/// The transport the application uses.
pub fn fetch(url: &str, max_bytes: usize, timeout: Duration) -> Result<Vec<u8>, String> {
    mdshared_fetch::fetch_file(crate::workspace::USER_AGENT, url, max_bytes, timeout)
}

pub fn current_version() -> Version {
    APP.current_version()
}

pub fn manifest_url() -> String {
    APP.manifest_url()
}

/// This process's executable and the engine beside it, when this copy can update itself.
pub fn installed_files() -> Option<Files> {
    shared::installed_files(&APP)
}

/// Fetches the manifest and reads it.
pub fn check(fetch: Fetch) -> Result<Option<Offer>, String> {
    shared::check(&APP, fetch)
}

/// Writes the new version into the installed-apps entry, if the installer made one.
pub fn record_version(version: &Version) {
    shared::record_version(&APP, version);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `build-installer.ps1` writes the manifest of a release; when it has run, what it wrote
    /// must read as an installable release of this very version.
    #[cfg(windows)]
    #[test]
    fn the_release_script_writes_a_manifest_this_build_reads() {
        use mdview_core::update;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release-files/mdview-update.txt");
        let Ok(bytes) = std::fs::read(&path) else {
            return; // The script has not run in this checkout.
        };
        let older = Version::parse("0.0.1").unwrap();
        let o = shared::offer(&APP, &bytes, &older)
            .expect("the manifest parses")
            .expect("it offers a release with both files");
        let engine = o.engine.expect("the engine is listed");
        assert!(o.app.url.ends_with(&format!("/v{}/mdview.exe", o.version)));
        assert!(
            engine
                .url
                .ends_with(&format!("/v{}/mdview-engine.exe", o.version))
        );
        let dir = path.parent().unwrap();
        for (file, download) in [("mdview.exe", &o.app), ("mdview-engine.exe", &engine)] {
            let ok = update::verify_sha256_file(&dir.join(file), &download.sha256).unwrap();
            assert!(ok, "{file} matches the hash the manifest lists");
        }
    }

    #[test]
    fn a_cargo_build_does_not_update_itself() {
        // The test binary runs from `target/release/deps`.
        assert_eq!(installed_files(), None);
    }

    /// The fetcher's limits are its own now; the renderer's image cache still follows core's.
    #[test]
    fn the_fetcher_and_the_renderer_agree_on_image_limits() {
        use mdview_core::limits;
        assert_eq!(mdshared_fetch::MAX_IMAGE_BYTES, limits::MAX_IMAGE_BYTES);
        assert_eq!(mdshared_fetch::IMAGE_CACHE_BYTES, limits::IMAGE_CACHE_BYTES);
        assert_eq!(mdshared_fetch::IMAGE_TIMEOUT_MS, limits::IMAGE_TIMEOUT_MS);
    }
}
