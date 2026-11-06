//! What following a link or loading an image means, per the old app's rules.

use std::path::{Component, Path, PathBuf};

pub const MARKDOWN_EXTENSIONS: [&str; 4] = ["md", "markdown", "mdown", "mkd"];

pub fn is_markdown_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        MARKDOWN_EXTENSIONS
            .iter()
            .any(|m| e.eq_ignore_ascii_case(m))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkAction {
    /// Scroll within the current document. Every fragment here is as the link wrote it, still
    /// percent-encoded: `Document::resolve_fragment` is the one place that decodes it, because
    /// decoding twice would make an anchor with a literal `%` unreachable.
    Fragment(String),
    /// Open in a tab, or switch to it, then go to the fragment.
    OpenMarkdown {
        path: PathBuf,
        fragment: Option<String>,
    },
    /// A local file the viewer does not open. The shell shows `strings::ONLY_MARKDOWN_LINKS`.
    OtherLocalFile(PathBuf),
    /// http, https or mailto: hand over to the system.
    External(String),
    /// Every other scheme is silently ignored, so nothing like `javascript:` can run.
    Ignored,
}

pub fn classify_link(href: &str, doc_dir: &Path) -> LinkAction {
    let href = href.trim();
    match scheme_of(href) {
        Some(s)
            if s.eq_ignore_ascii_case("http")
                || s.eq_ignore_ascii_case("https")
                || s.eq_ignore_ascii_case("mailto") =>
        {
            LinkAction::External(href.to_string())
        }
        Some(s) if s.eq_ignore_ascii_case("file") => match file_url_path(&href[s.len() + 1..]) {
            Some((path, fragment)) => local_target(path, fragment),
            None => LinkAction::Ignored,
        },
        Some(s) if s.len() == 1 && Path::new(href).is_absolute() => {
            local_target(PathBuf::from(href), None)
        }
        Some(_) => LinkAction::Ignored,
        None => {
            let (path, fragment) = split_url(href);
            if path.is_empty() {
                return LinkAction::Fragment(fragment.unwrap_or("").to_string());
            }
            let decoded = percent_decode(path);
            if names_a_host(&decoded) {
                return LinkAction::Ignored;
            }
            let target = doc_dir.join(decoded);
            local_target(normalize(&target), fragment.map(str::to_string))
        }
    }
}

fn local_target(path: PathBuf, fragment: Option<String>) -> LinkAction {
    if is_markdown_path(&path) {
        LinkAction::OpenMarkdown { path, fragment }
    } else {
        LinkAction::OtherLocalFile(path)
    }
}

fn scheme_of(url: &str) -> Option<&str> {
    let colon = url.find(':')?;
    let s = &url[..colon];
    let valid = s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'));
    valid.then_some(s)
}

fn split_url(url: &str) -> (&str, Option<&str>) {
    let (rest, fragment) = match url.split_once('#') {
        Some((a, f)) => (a, Some(f)),
        None => (url, None),
    };
    (rest.split('?').next().unwrap_or(rest), fragment)
}

fn file_url_path(after_scheme: &str) -> Option<(PathBuf, Option<String>)> {
    let (rest, fragment) = split_url(after_scheme);
    let path = match rest.strip_prefix("//") {
        Some(authority) => {
            let (host, path) = authority.split_at(authority.find('/')?);
            if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
                return None;
            }
            path
        }
        None => rest,
    };
    let decoded = percent_decode(path);
    if names_a_host(&decoded) {
        return None;
    }
    // file:///C:/x has a leading slash that is not part of a Windows path.
    let trimmed = match decoded.as_bytes() {
        [b'/', d, b':', ..] if d.is_ascii_alphabetic() && cfg!(windows) => decoded[1..].to_string(),
        _ => decoded,
    };
    Some((PathBuf::from(trimmed), fragment.map(str::to_string)))
}

/// True for `//host/x`, `\\host\x` and mixes of them. On Windows these are UNC or device paths, and
/// opening a document must never make the machine contact another host, which would hand it the
/// user's NTLM credentials. Control characters are skipped because a browser strips them from URLs.
fn names_a_host(path: &str) -> bool {
    let mut leading = path.chars().filter(|c| !c.is_control());
    leading.next().is_some_and(|c| c == '/' || c == '\\')
        && leading.next().is_some_and(|c| c == '/' || c == '\\')
}

/// Resolves `.` and `..` lexically, as URL resolution does. The file need not exist.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%')
            .then(|| b.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| {
                Some((
                    char::from(h[0]).to_digit(16)?,
                    char::from(h[1]).to_digit(16)?,
                ))
            });
        match hex {
            Some((hi, lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    Local(PathBuf),
    Remote(String),
    /// Base64 `data:` payload, which is all the old app accepted.
    InlineData,
    Unsupported,
}

pub fn classify_image(src: &str, doc_dir: &Path) -> ImageSource {
    let src = src.trim();
    match scheme_of(src) {
        Some(s) if s.eq_ignore_ascii_case("http") || s.eq_ignore_ascii_case("https") => {
            ImageSource::Remote(src.to_string())
        }
        Some(s) if s.eq_ignore_ascii_case("data") => {
            let meta = src[5..].split(',').next().unwrap_or("");
            if meta.to_ascii_lowercase().ends_with(";base64") {
                ImageSource::InlineData
            } else {
                ImageSource::Unsupported
            }
        }
        Some(s) if s.eq_ignore_ascii_case("file") => match file_url_path(&src[5..]) {
            Some((path, _)) => ImageSource::Local(path),
            None => ImageSource::Unsupported,
        },
        Some(s) if s.len() == 1 && Path::new(src).is_absolute() => {
            ImageSource::Local(PathBuf::from(src))
        }
        Some(_) => ImageSource::Unsupported,
        None => {
            let (path, _) = split_url(src);
            if path.is_empty() {
                return ImageSource::Unsupported;
            }
            let decoded = percent_decode(path);
            if names_a_host(&decoded) {
                return ImageSource::Unsupported;
            }
            ImageSource::Local(normalize(&doc_dir.join(decoded)))
        }
    }
}
