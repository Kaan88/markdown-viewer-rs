//! Updating mdview in place: `mdshared_app::updater` with this application's name, manifest and
//! files, and `mdshared_fetch` as the transport. `docs/UPDATE.md` describes the whole flow.

use mdshared_app::updater as shared;
use mdview::update::Version;
use std::time::Duration;

pub use shared::{
    Event, Fetch, Files, NOT_INSTALLED, Offer, Stage, Updates, at_launch_label, check_due,
    clean_up, install,
};

pub use mdview::update_app::APP;

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
