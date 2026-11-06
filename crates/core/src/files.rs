//! Reading documents from disk, and the small path helpers shells share.

use crate::limits::MAX_FILE_BYTES;
use crate::model::Document;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// What the change detector compares: a missing file is a state of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStamp {
    Missing,
    Present {
        modified: Option<SystemTime>,
        size: u64,
    },
}

pub fn stamp(path: &Path) -> FileStamp {
    match std::fs::metadata(path) {
        Ok(m) => FileStamp::Present {
            modified: m.modified().ok(),
            size: m.len(),
        },
        Err(_) => FileStamp::Missing,
    }
}

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

const READ_ATTEMPTS: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_millis(40);

pub struct Loaded {
    pub bytes: Vec<u8>,
    pub stamp: FileStamp,
}

/// Reads a file, retrying while it changes underneath the read so a half-finished save is not shown.
pub fn read_stable(path: &Path) -> Result<Loaded, LoadError> {
    let mut last_valid = None;
    for attempt in 0..READ_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(RETRY_DELAY);
        }
        let before = stamp(path);
        let bytes = read_limited(path)?;
        let after = stamp(path);
        let unchanged = before == after
            && matches!(after, FileStamp::Present { size, .. } if size == bytes.len() as u64);
        if !unchanged {
            continue;
        }
        // Invalid UTF-8 can also mean a save caught mid-write, so it gets the same retries.
        let valid = has_valid_encoding(&bytes);
        last_valid = Some(Loaded {
            bytes,
            stamp: after,
        });
        if valid {
            break;
        }
    }
    last_valid.ok_or(LoadError::StillChanging)
}

fn read_limited(path: &Path) -> Result<Vec<u8>, LoadError> {
    let io_error = |e: io::Error| LoadError::Io(e.to_string());
    let file = std::fs::File::open(path).map_err(io_error)?;
    let len = file.metadata().map_err(io_error)?.len();
    if len > MAX_FILE_BYTES as u64 {
        return Err(LoadError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(LoadError::TooLarge);
    }
    Ok(bytes)
}

/// Reads and parses a document. Invalid text does not fail: it is replaced and reported as a warning.
pub fn load_document(path: &Path) -> Result<(Document, FileStamp), LoadError> {
    let loaded = read_stable(path)?;
    Ok((crate::parse_bytes(&loaded.bytes), loaded.stamp))
}

fn has_valid_encoding(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xFE])
        || bytes.starts_with(&[0xFE, 0xFF])
        || std::str::from_utf8(bytes).is_ok()
}

/// Decodes file bytes to text with any BOM removed. The flag reports replaced invalid input.
pub fn decode(bytes: &[u8]) -> (std::borrow::Cow<'_, str>, bool) {
    use std::borrow::Cow;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, u16::from_be_bytes);
    }
    match String::from_utf8_lossy(bytes) {
        Cow::Borrowed(s) => (Cow::Borrowed(s), false),
        Cow::Owned(s) => (Cow::Owned(s), true),
    }
}

fn decode_utf16(bytes: &[u8], read: fn([u8; 2]) -> u16) -> (std::borrow::Cow<'_, str>, bool) {
    let units = bytes.as_chunks::<2>().0.iter().map(|c| read(*c));
    let mut invalid = !bytes.len().is_multiple_of(2);
    let text: String = char::decode_utf16(units)
        .map(|r| {
            r.unwrap_or_else(|_| {
                invalid = true;
                char::REPLACEMENT_CHARACTER
            })
        })
        .collect();
    (text.into(), invalid)
}

/// Canonical form used to tell whether a path is already open. A missing file falls back to its
/// absolute path, so a restored tab for a deleted file keeps a stable identity.
pub fn canonical_or_absolute(path: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    if cfg!(windows)
        && let Some(plain) = resolved.to_str().and_then(strip_verbatim)
    {
        return PathBuf::from(plain);
    }
    resolved
}

/// On Windows, `canonicalize` returns `\\?\C:\x`, which `absolute` does not, and which reads badly in
/// the UI. This gives the ordinary spelling, or `None` if the path only works in verbatim form or is
/// not verbatim at all, so a file keeps one identity whether or not it exists right now.
pub fn strip_verbatim(path: &str) -> Option<String> {
    let (plain, rest) = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        (format!(r"\\{rest}"), rest)
    } else {
        let rest = path.strip_prefix(r"\\?\")?;
        let b = rest.as_bytes();
        if !(b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\') {
            return None;
        }
        (rest.to_string(), &rest[3..])
    };
    // MAX_PATH, including the terminator.
    (plain.len() < 260 && rest.split('\\').all(usable_without_verbatim)).then_some(plain)
}

/// Verbatim paths skip the Win32 name rules, so a name can exist there and not be reachable otherwise.
fn usable_without_verbatim(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
        .iter()
        .any(|r| stem.eq_ignore_ascii_case(r))
        || (stem.len() == 4
            && (stem[..3].eq_ignore_ascii_case("COM") || stem[..3].eq_ignore_ascii_case("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    !reserved
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.contains(|c: char| c < ' ' || "<>:\"/|?*".contains(c))
}

/// Tab titles. Filenames that repeat (ignoring case) gain as many parent folders as it takes to tell
/// them apart, so `a/docs/README.md` and `b/docs/README.md` never read the same.
pub fn tab_labels(paths: &[PathBuf]) -> Vec<String> {
    let name_of = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string())
    };
    let names: Vec<String> = paths.iter().map(|p| name_of(p)).collect();
    let folded: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let sep = std::path::MAIN_SEPARATOR_STR;
    paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let others: Vec<&PathBuf> = paths
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i && folded[*j] == folded[i])
                .map(|(_, p)| p)
                .collect();
            if others.is_empty() {
                return names[i].clone();
            }
            let parents = |p: &Path| -> Vec<String> {
                p.parent()
                    .map(|d| {
                        d.components()
                            .rev()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let own = parents(path);
            let rest: Vec<Vec<String>> = others.iter().map(|p| parents(p)).collect();
            for depth in 1..=own.len() {
                let suffix = |v: &[String]| -> Vec<String> {
                    v.iter().take(depth).map(|s| s.to_lowercase()).collect()
                };
                let mine = suffix(&own);
                if rest.iter().all(|o| suffix(o) != mine) {
                    let mut shown: Vec<&str> = own.iter().take(depth).map(String::as_str).collect();
                    shown.reverse();
                    let joined = shown.join(sep).replacen(&format!("{sep}{sep}"), sep, 1);
                    return format!("{} — {}", names[i], joined);
                }
            }
            let full = path
                .parent()
                .map(|d| d.display().to_string())
                .unwrap_or_default();
            format!("{} — {}", names[i], full)
        })
        .collect()
}
