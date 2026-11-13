//! OS glue is `mdshared_ui::platform`; the Markdown file filter of the open dialog is supplied here.

pub use mdshared_ui::platform::*;
use std::path::{Path, PathBuf};

/// Whether the system shows animations (Windows: Settings, Accessibility, Visual effects,
/// "Animation effects"). When it does not, animated images stay on their first frame.
#[cfg(windows)]
pub fn shows_animations() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut on: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL (4 bytes) to `pvparam`.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut on as *mut i32).cast(),
            0,
        )
    };
    ok == 0 || on != 0
}

#[cfg(not(windows))]
pub fn shows_animations() -> bool {
    true
}

/// The extensions the open dialog's Markdown filter lists.
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd"];

/// Native "open files" dialog filtered to Markdown plus "All files".
#[cfg(windows)]
pub fn pick_files(start_dir: Option<&Path>) -> Vec<PathBuf> {
    mdshared_ui::platform::pick_files(start_dir, "Markdown", MARKDOWN_EXTENSIONS)
}

/// [`pick_files`] without blocking the window: `done` runs on a helper thread with the chosen
/// files. Returns whether the dialog opened.
#[cfg(not(windows))]
pub fn pick_files_async(
    start_dir: Option<&Path>,
    done: impl FnOnce(Vec<PathBuf>) + Send + 'static,
) -> bool {
    mdshared_ui::platform::pick_files_async(start_dir, "Markdown", MARKDOWN_EXTENSIONS, done)
}

/// The file types the desktop entry and the default-app command cover.
#[cfg(not(windows))]
pub const MIME_TYPES: &[&str] = mdshared_ui::desktop_entry::VIEWER_MIME_TYPES;

/// Whether Settings offers "Make mdview the default for Markdown files": only off Windows,
/// only from an AppImage (the one place that writes a desktop entry), and not once it is the
/// default already.
#[cfg(not(windows))]
pub fn can_offer_default_app() -> bool {
    mdshared_app::appimage::path().is_some()
        && !mdshared_ui::desktop_entry::is_default("mdview", MIME_TYPES)
}

#[cfg(windows)]
pub fn can_offer_default_app() -> bool {
    false
}

/// Makes mdview the default app for Markdown files (Linux: `mimeapps.list`).
pub fn make_mdview_default() -> Result<(), String> {
    #[cfg(not(windows))]
    {
        mdshared_ui::desktop_entry::make_default("mdview", MIME_TYPES)
    }
    #[cfg(windows)]
    {
        Err("not supported on Windows".to_string())
    }
}
