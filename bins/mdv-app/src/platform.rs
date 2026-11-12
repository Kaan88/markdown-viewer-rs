//! OS glue is `mdshared_ui::platform`; the Markdown file filter of the open dialog is supplied here.

pub use mdshared_ui::platform::*;
use std::path::{Path, PathBuf};

/// Native "open files" dialog filtered to Markdown plus "All files".
pub fn pick_files(start_dir: Option<&Path>) -> Vec<PathBuf> {
    mdshared_ui::platform::pick_files(start_dir, "Markdown", &["md", "markdown", "mdown", "mkd"])
}
