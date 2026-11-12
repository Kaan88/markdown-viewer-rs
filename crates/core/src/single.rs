//! One running window per user, handing file paths to the first instance: `mdshared_app::single`
//! with this application's name.

use mdshared_app::single as shared;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

pub use shared::port_file;

/// The name that opens each hand-over message. Another build of mdview speaks the same word.
const APP: &str = "mdview";

/// Hands `paths` to an already running instance. True when one answered, so the caller can exit.
pub fn forward(settings: &Path, paths: &[PathBuf]) -> bool {
    shared::forward(APP, settings, paths)
}

/// Starts listening and records the port. `None` when the port cannot be bound or written.
pub fn listen(settings: &Path) -> Option<Receiver<PathBuf>> {
    shared::listen(APP, settings)
}

/// `listen`, calling `wake` from the listener thread after each launch's paths are queued.
pub fn listen_waking(
    settings: &Path,
    wake: impl Fn() + Send + 'static,
) -> Option<Receiver<PathBuf>> {
    shared::listen_waking(APP, settings, wake)
}
