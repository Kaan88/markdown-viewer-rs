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

/// Native "open files" dialog filtered to Markdown plus "All files".
pub fn pick_files(start_dir: Option<&Path>) -> Vec<PathBuf> {
    mdshared_ui::platform::pick_files(start_dir, "Markdown", &["md", "markdown", "mdown", "mkd"])
}
