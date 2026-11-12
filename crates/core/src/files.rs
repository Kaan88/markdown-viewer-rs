//! Reading documents from disk, and the small path helpers shells share. The generic parts are in
//! `mdshared_app` and are re-exported here.

use crate::limits::MAX_FILE_BYTES;
use crate::model::Document;
use std::fmt;
use std::path::Path;

pub use mdshared_app::files::{FileStamp, Loaded, decode, stamp};
pub use mdshared_app::paths::{canonical_or_absolute, strip_verbatim, tab_labels};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    TooLarge,
    StillChanging,
    Io(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::TooLarge => f.write_str("File exceeds the 16 MiB viewer limit"),
            LoadError::StillChanging => {
                f.write_str("File is still changing; wait for the save to finish and retry")
            }
            LoadError::Io(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<mdshared_app::files::LoadError> for LoadError {
    fn from(e: mdshared_app::files::LoadError) -> LoadError {
        use mdshared_app::files::LoadError as L;
        match e {
            L::TooLarge => LoadError::TooLarge,
            L::StillChanging => LoadError::StillChanging,
            L::Io(e) => LoadError::Io(e),
        }
    }
}

/// Reads a file, retrying while it changes underneath the read so a half-finished save is not shown.
pub fn read_stable(path: &Path) -> Result<Loaded, LoadError> {
    Ok(mdshared_app::files::read_stable(path, MAX_FILE_BYTES)?)
}

/// Reads and parses a document. Invalid text does not fail: it is replaced and reported as a warning.
pub fn load_document(path: &Path) -> Result<(Document, FileStamp), LoadError> {
    let loaded = read_stable(path)?;
    Ok((crate::parse_bytes(&loaded.bytes), loaded.stamp))
}
