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

    /// The release scripts write the manifest of a release (`build-installer.ps1`,
    /// `packaging/write-manifest.sh`); when one has run, what it wrote must read as an installable
    /// release, and every file it lists must have the hash it states. Each folder is checked when
    /// it exists and skipped when it does not: `target/release-files-all` (release-all.sh, both
    /// platforms), `target/dist-cross` (Linux-built Windows files) and `target/release-files`
    /// (build-installer.ps1). Runs on Windows and Linux; the platform it runs on is offered the
    /// release when the manifest lists an entry for it.
    #[test]
    fn the_release_scripts_write_a_manifest_this_build_reads() {
        use mdview_core::update;
        use std::collections::HashMap;
        let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let current = if cfg!(windows) {
            "windows-x86_64"
        } else {
            "linux-x86_64"
        };
        for name in ["release-files-all", "dist-cross", "release-files"] {
            let dir = target.join(name);
            let Ok(bytes) = std::fs::read(dir.join("mdview-update.txt")) else {
                continue; // The script has not run in this checkout.
            };
            let text = String::from_utf8(bytes.clone()).expect("the manifest is UTF-8");
            let keys: HashMap<&str, &str> =
                text.lines().filter_map(|l| l.split_once(": ")).collect();
            let version = keys["version"];
            let older = Version::parse("0.0.1").unwrap();
            let offer = shared::offer(&APP, &bytes, &older).expect("the manifest parses");
            match keys.get(format!("{current}.asset").as_str()) {
                Some(asset) => {
                    let o = offer.unwrap_or_else(|| panic!("{name}: no offer for {current}"));
                    assert_eq!(o.version.to_string(), version);
                    assert!(o.app.url.ends_with(&format!("/v{version}/{asset}")));
                    if cfg!(windows) {
                        let engine = o.engine.expect("the Windows entry lists the engine");
                        assert!(
                            engine
                                .url
                                .ends_with(&format!("/v{version}/mdview-engine.exe"))
                        );
                    } else {
                        assert!(
                            o.engine.is_none(),
                            "Linux has no engine file: it is in the AppImage"
                        );
                    }
                }
                None => assert!(
                    offer.is_none(),
                    "{name}: offered a platform it does not list"
                ),
            }
            // A manifest at the current version offers nothing.
            let same = Version::parse(version).unwrap();
            assert!(shared::offer(&APP, &bytes, &same).unwrap().is_none());
            // The engine keys belong to Windows only.
            assert!(!keys.keys().any(|k| k.starts_with("linux-x86_64.engine")));
            // Every file listed is beside the manifest, with the stated hash.
            let mut listed = Vec::new();
            for (key, hash) in &keys {
                let Some(platform) = key.strip_suffix(".sha256") else {
                    continue;
                };
                let file = match keys.get(format!("{platform}.asset").as_str()) {
                    Some(asset) => asset.to_string(),
                    None => keys[format!("{platform}.url").as_str()]
                        .rsplit('/')
                        .next()
                        .unwrap()
                        .to_string(),
                };
                listed.push((file, *hash));
            }
            assert!(!listed.is_empty(), "{name}: no file hashes in the manifest");
            for (file, hash) in listed {
                let path = dir.join(&file);
                assert!(path.is_file(), "{name}: {file} is listed but missing");
                let ok = update::verify_sha256_file(&path, hash).unwrap();
                assert!(ok, "{name}: {file} matches the hash the manifest lists");
            }
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
