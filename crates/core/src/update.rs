//! Update foundation: version ordering, the update manifest, and SHA-256 verification.
//!
//! This module does no I/O of its own beyond reading a file handed to [`verify_sha256_file`].
//! Downloading is the shells' job; see `docs/UPDATE.md`.
//!
//! # Manifest format (`mdview-update.txt`, version 1)
//!
//! A small, strict, line-based UTF-8 text format. Rules:
//!
//! * At most [`MAX_MANIFEST_BYTES`] bytes and [`MAX_LINES`] lines; anything larger is rejected.
//! * Lines end in `\n` (a trailing `\r` is tolerated). Blank lines and lines starting with `#`
//!   are ignored. Every other line is `key: value`, split at the first `:`, both sides trimmed.
//! * The first meaningful line must be `manifest: 1`.
//! * Keys (all lower-case, no others are accepted; duplicates are rejected):
//!   * `version: MAJOR.MINOR.PATCH[-pre]` (required)
//!   * `notes: text` (optional, at most [`MAX_NOTES_BYTES`] bytes, no control characters)
//!   * per platform `P` in `windows-x86_64`, `linux-x86_64`:
//!     `P.asset: file name` (letters, digits, `.`, `_`, `-`; no path separators; <= 128 bytes),
//!     `P.url: https://...` (https only, <= 512 bytes, no whitespace/control characters, no
//!     userinfo), `P.sha256: 64 hex digits` (either case, stored lower-case).
//!     A platform is present only if all three of its keys are present. These three describe
//!     the application's own executable.
//!     `P.engine.url` and `P.engine.sha256` (same rules; both or neither) describe the engine
//!     executable that ships beside it. A build that updates itself in place needs both files, so
//!     it ignores a platform entry without them.
//! * At least one platform must be present.
//!
//! Example:
//!
//! ```text
//! manifest: 1
//! version: 0.2.0
//! notes: Faster scrolling; fixes a crash on empty files.
//! windows-x86_64.asset: mdview.exe
//! windows-x86_64.url: https://github.com/Kaan88/markdown-viewer-rs/releases/download/v0.2.0/mdview.exe
//! windows-x86_64.sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
//! windows-x86_64.engine.url: https://github.com/Kaan88/markdown-viewer-rs/releases/download/v0.2.0/mdview-engine.exe
//! windows-x86_64.engine.sha256: fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210
//! ```
//!
//! Anything else (unknown key, bad value, http URL, short hash) rejects the whole manifest. The
//! manifest is data: the only thing a valid one can do is name a newer version and an https
//! download whose hash the shell must check before use.
//!
//! # SHA-256
//!
//! `Cargo.lock` carries `sha2` only transitively and `mdview-core` has no dependency on it, so this
//! module includes its own small SHA-256 (FIPS 180-4), checked against the standard test vectors
//! (including the million-`a` vector) in the tests below.

use std::fmt;
use std::io::Read;
use std::path::Path;

pub const MAX_MANIFEST_BYTES: usize = 8 * 1024;
pub const MAX_LINES: usize = 64;
pub const MAX_NOTES_BYTES: usize = 1024;
const MAX_URL_BYTES: usize = 512;
const MAX_ASSET_BYTES: usize = 128;

/// Why a version or manifest was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    BadVersion,
    TooLarge,
    NotUtf8,
    TooManyLines,
    BadLine(usize),
    BadHeader,
    UnknownKey(String),
    DuplicateKey(String),
    BadValue(&'static str),
    MissingField(&'static str),
    NoAssets,
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadVersion => write!(f, "malformed version"),
            Self::TooLarge => write!(f, "manifest too large"),
            Self::NotUtf8 => write!(f, "manifest is not UTF-8"),
            Self::TooManyLines => write!(f, "manifest has too many lines"),
            Self::BadLine(n) => write!(f, "malformed line {n}"),
            Self::BadHeader => write!(f, "missing or unsupported `manifest: 1` header"),
            Self::UnknownKey(k) => write!(f, "unknown key `{k}`"),
            Self::DuplicateKey(k) => write!(f, "duplicate key `{k}`"),
            Self::BadValue(k) => write!(f, "invalid value for `{k}`"),
            Self::MissingField(k) => write!(f, "missing field `{k}`"),
            Self::NoAssets => write!(f, "manifest lists no platform assets"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// `MAJOR.MINOR.PATCH[-pre]`. Ordering follows SemVer precedence (build metadata is not accepted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Dot-separated pre-release identifiers; empty for a release.
    pre: Vec<String>,
}

fn parse_num(s: &str) -> Option<u64> {
    if s.is_empty() || s.len() > 9 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    s.parse().ok()
}

impl Version {
    pub fn parse(s: &str) -> Result<Version, UpdateError> {
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (s, None),
        };
        let mut it = core.split('.');
        let (Some(a), Some(b), Some(c), None) = (it.next(), it.next(), it.next(), it.next()) else {
            return Err(UpdateError::BadVersion);
        };
        let mut ids = Vec::new();
        if let Some(p) = pre {
            if p.len() > 32 {
                return Err(UpdateError::BadVersion);
            }
            for id in p.split('.') {
                let ok = !id.is_empty()
                    && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    && (!id.bytes().all(|b| b.is_ascii_digit()) || parse_num(id).is_some());
                if !ok {
                    return Err(UpdateError::BadVersion);
                }
                ids.push(id.to_string());
            }
        }
        match (parse_num(a), parse_num(b), parse_num(c)) {
            (Some(major), Some(minor), Some(patch)) => Ok(Version {
                major,
                minor,
                patch,
                pre: ids,
            }),
            _ => Err(UpdateError::BadVersion),
        }
    }

    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl std::str::FromStr for Version {
    type Err = UpdateError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Version::parse(s)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        let core =
            (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch));
        if core != Equal {
            return core;
        }
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => return Equal,
            (true, false) => return Greater,
            (false, true) => return Less,
            _ => {}
        }
        for (a, b) in self.pre.iter().zip(&other.pre) {
            let an = a
                .parse::<u64>()
                .ok()
                .filter(|_| a.bytes().all(|c| c.is_ascii_digit()));
            let bn = b
                .parse::<u64>()
                .ok()
                .filter(|_| b.bytes().all(|c| c.is_ascii_digit()));
            let o = match (an, bn) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => Less,
                (None, Some(_)) => Greater,
                (None, None) => a.cmp(b),
            };
            if o != Equal {
                return o;
            }
        }
        self.pre.len().cmp(&other.pre.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    WindowsX64,
    LinuxX64,
}

impl Platform {
    /// The platform this binary was built for, if it is one we ship.
    pub fn current() -> Option<Platform> {
        if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Platform::WindowsX64)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Platform::LinuxX64)
        } else {
            None
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Platform::WindowsX64 => "windows-x86_64",
            Platform::LinuxX64 => "linux-x86_64",
        }
    }
}

const PLATFORMS: [Platform; 2] = [Platform::WindowsX64, Platform::LinuxX64];

/// A file to download and the hash it must have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub url: String,
    /// Lower-case 64-digit hex.
    pub sha256: String,
}

/// A platform's release: the application's executable, and the engine's when the manifest lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    /// Lower-case 64-digit hex.
    pub sha256: String,
    pub engine: Option<Download>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateManifest {
    pub version: Version,
    pub notes: Option<String>,
    windows: Option<Asset>,
    linux: Option<Asset>,
}

impl UpdateManifest {
    pub fn asset(&self, platform: Platform) -> Option<&Asset> {
        match platform {
            Platform::WindowsX64 => self.windows.as_ref(),
            Platform::LinuxX64 => self.linux.as_ref(),
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<UpdateManifest, UpdateError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(UpdateError::TooLarge);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| UpdateError::NotUtf8)?;
        if text.lines().count() > MAX_LINES {
            return Err(UpdateError::TooManyLines);
        }
        let mut seen: Vec<&str> = Vec::new();
        let mut header = false;
        let mut version = None;
        let mut notes = None;
        let mut fields: [[Option<String>; 5]; 2] = Default::default();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                return Err(UpdateError::BadLine(i + 1));
            };
            let (k, v) = (k.trim(), v.trim());
            if !header {
                if k != "manifest" || v != "1" {
                    return Err(UpdateError::BadHeader);
                }
                header = true;
                continue;
            }
            if seen.contains(&k) {
                return Err(UpdateError::DuplicateKey(k.chars().take(40).collect()));
            }
            match k {
                "version" => version = Some(Version::parse(v)?),
                "notes" => {
                    if v.len() > MAX_NOTES_BYTES || v.chars().any(char::is_control) {
                        return Err(UpdateError::BadValue("notes"));
                    }
                    notes = Some(v.to_string());
                }
                _ => {
                    let Some((pi, field)) = PLATFORMS.iter().enumerate().find_map(|(pi, p)| {
                        let rest = k.strip_prefix(p.prefix())?.strip_prefix('.')?;
                        Some((pi, rest))
                    }) else {
                        return Err(UpdateError::UnknownKey(k.chars().take(40).collect()));
                    };
                    let (slot, val) = match field {
                        "asset" if valid_asset(v) => (0, v.to_string()),
                        "asset" => return Err(UpdateError::BadValue("asset")),
                        "url" if valid_url(v) => (1, v.to_string()),
                        "url" => return Err(UpdateError::BadValue("url")),
                        "sha256" if valid_sha(v) => (2, v.to_ascii_lowercase()),
                        "sha256" => return Err(UpdateError::BadValue("sha256")),
                        "engine.url" if valid_url(v) => (3, v.to_string()),
                        "engine.url" => return Err(UpdateError::BadValue("engine.url")),
                        "engine.sha256" if valid_sha(v) => (4, v.to_ascii_lowercase()),
                        "engine.sha256" => return Err(UpdateError::BadValue("engine.sha256")),
                        _ => return Err(UpdateError::UnknownKey(k.chars().take(40).collect())),
                    };
                    fields[pi][slot] = Some(val);
                }
            }
            seen.push(k);
        }
        if !header {
            return Err(UpdateError::BadHeader);
        }
        let version = version.ok_or(UpdateError::MissingField("version"))?;
        let mut assets: [Option<Asset>; 2] = [None, None];
        for (pi, f) in fields.into_iter().enumerate() {
            let [name, url, sha256, engine_url, engine_sha256] = f;
            let engine = match (engine_url, engine_sha256) {
                (None, None) => None,
                (Some(url), Some(sha256)) => Some(Download { url, sha256 }),
                _ => {
                    return Err(UpdateError::MissingField(
                        "platform engine.url/engine.sha256",
                    ));
                }
            };
            match (name, url, sha256) {
                (None, None, None) if engine.is_none() => {}
                (Some(name), Some(url), Some(sha256)) => {
                    assets[pi] = Some(Asset {
                        name,
                        url,
                        sha256,
                        engine,
                    })
                }
                _ => return Err(UpdateError::MissingField("platform asset/url/sha256")),
            }
        }
        let [windows, linux] = assets;
        if windows.is_none() && linux.is_none() {
            return Err(UpdateError::NoAssets);
        }
        Ok(UpdateManifest {
            version,
            notes,
            windows,
            linux,
        })
    }
}

fn valid_asset(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= MAX_ASSET_BYTES
        && !v.starts_with('.')
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn valid_url(v: &str) -> bool {
    let Some(rest) = v.strip_prefix("https://") else {
        return false;
    };
    if v.len() > MAX_URL_BYTES || !v.bytes().all(|b| b.is_ascii_graphic()) {
        return false;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.is_empty() && !host.contains('@') && !host.starts_with('.')
}

fn valid_sha(v: &str) -> bool {
    v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The asset to offer, if the manifest's version is strictly newer than `current` and has a build
/// for `platform`.
pub fn newer(current: &Version, manifest: &UpdateManifest, platform: Platform) -> Option<Asset> {
    if manifest.version > *current {
        manifest.asset(platform).cloned()
    } else {
        None
    }
}

// ---- SHA-256 ----

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Incremental SHA-256.
pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0; 64],
            buf_len: 0,
            total: 0,
        }
    }

    fn block(&mut self, b: &[u8]) {
        let mut w = [0u32; 64];
        for (i, c) in b.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b_, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b_) ^ (a & c) ^ (b_ & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b_;
            b_ = a;
            a = t1.wrapping_add(t2);
        }
        for (s, v) in self.h.iter_mut().zip([a, b_, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.total = self.total.wrapping_add(data.len() as u64);
        if self.buf_len > 0 {
            let n = (64 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + n].copy_from_slice(&data[..n]);
            self.buf_len += n;
            data = &data[n..];
            if self.buf_len < 64 {
                return;
            }
            let b = self.buf;
            self.block(&b);
            self.buf_len = 0;
        }
        let mut chunks = data.chunks_exact(64);
        for c in &mut chunks {
            self.block(c);
        }
        let rem = chunks.remainder();
        self.buf[..rem.len()].copy_from_slice(rem);
        self.buf_len = rem.len();
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.total.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        let used = (self.buf_len + 1) % 64;
        pad.resize(1 + (if used <= 56 { 56 - used } else { 120 - used }), 0);
        pad.extend_from_slice(&bits.to_be_bytes());
        self.update(&pad);
        debug_assert_eq!(self.buf_len, 0);
        let mut out = [0u8; 32];
        for (o, v) in out.chunks_exact_mut(4).zip(self.h) {
            o.copy_from_slice(&v.to_be_bytes());
        }
        out
    }
}

fn hex(bytes: &[u8]) -> String {
    use fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Lower-case hex SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex(&h.finish())
}

/// Constant-shape comparison of the digest of `data` with a 64-hex `expected` (either case).
pub fn verify_sha256(data: &[u8], expected: &str) -> bool {
    valid_sha(expected) && sha256_hex(data).eq_ignore_ascii_case(expected)
}

/// Streams a file through SHA-256 and compares with `expected`. I/O errors are returned.
pub fn verify_sha256_file(path: &Path, expected: &str) -> std::io::Result<bool> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(valid_sha(expected) && hex(&h.finish()).eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn manifest(extra: &str) -> String {
        format!(
            "manifest: 1\nversion: 0.2.0\nwindows-x86_64.asset: mdview-setup-0.2.0.exe\n\
             windows-x86_64.url: https://example.com/a/mdview-setup-0.2.0.exe\n\
             windows-x86_64.sha256: {SHA}\n{extra}"
        )
    }

    #[test]
    fn version_parse_and_display() {
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert_eq!(v("1.2.3-rc.1").to_string(), "1.2.3-rc.1");
        assert!(v("1.2.3-rc.1").is_prerelease());
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.2.x",
            "1.2.3-",
            "1.2.3-a..b",
            "v1.2.3",
            "1.2.3+b",
            "-1.2.3",
            "1.2.3-01",
            "1.2.99999999999",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn version_ordering() {
        let order = [
            "0.9.9",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.2.0",
            "2.0.0",
            "10.0.0",
        ];
        for w in order.windows(2) {
            assert!(v(w[0]) < v(w[1]), "{} < {}", w[0], w[1]);
            assert!(v(w[1]) > v(w[0]));
        }
        assert_eq!(v("1.2.3"), v("1.2.3"));
        assert_eq!(v("1.2.3").cmp(&v("1.2.3")), std::cmp::Ordering::Equal);
        assert!(v("1.10.0") > v("1.9.0"));
    }

    #[test]
    fn manifest_valid() {
        let m = UpdateManifest::parse(manifest("# c\n\nnotes: hello\r\n").as_bytes()).unwrap();
        assert_eq!(m.version, v("0.2.0"));
        assert_eq!(m.notes.as_deref(), Some("hello"));
        let a = m.asset(Platform::WindowsX64).unwrap();
        assert_eq!(a.name, "mdview-setup-0.2.0.exe");
        assert_eq!(a.sha256, SHA);
        assert!(m.asset(Platform::LinuxX64).is_none());
    }

    #[test]
    fn manifest_engine_is_both_keys_or_neither() {
        let plain = UpdateManifest::parse(manifest("").as_bytes()).unwrap();
        assert!(plain.asset(Platform::WindowsX64).unwrap().engine.is_none());
        let url = "windows-x86_64.engine.url: https://example.com/a/mdview-engine.exe
";
        let sha = format!(
            "windows-x86_64.engine.sha256: {}
",
            SHA.to_uppercase()
        );
        let m = UpdateManifest::parse(manifest(&format!("{url}{sha}")).as_bytes()).unwrap();
        let engine = m
            .asset(Platform::WindowsX64)
            .unwrap()
            .engine
            .clone()
            .unwrap();
        assert_eq!(engine.url, "https://example.com/a/mdview-engine.exe");
        assert_eq!(engine.sha256, SHA);
        for half in [url.to_string(), sha.clone()] {
            assert!(UpdateManifest::parse(manifest(&half).as_bytes()).is_err());
        }
        let http = "windows-x86_64.engine.url: http://example.com/e.exe
";
        assert!(UpdateManifest::parse(manifest(&format!("{http}{sha}")).as_bytes()).is_err());
        // An engine alone does not make a platform.
        let lone = format!(
            "linux-x86_64.engine.url: https://example.com/e
linux-x86_64.engine.sha256: {SHA}
"
        );
        assert!(UpdateManifest::parse(manifest(&lone).as_bytes()).is_err());
    }

    #[test]
    fn manifest_uppercase_hash_normalised() {
        let t = manifest("").replace(SHA, &SHA.to_uppercase());
        assert_eq!(
            UpdateManifest::parse(t.as_bytes())
                .unwrap()
                .asset(Platform::WindowsX64)
                .unwrap()
                .sha256,
            SHA
        );
    }

    #[test]
    fn manifest_rejections() {
        let ok = manifest("");
        assert!(UpdateManifest::parse(ok.as_bytes()).is_ok());
        let cases: Vec<(String, UpdateError)> = vec![
            (
                ok.replace("https://", "http://"),
                UpdateError::BadValue("url"),
            ),
            (
                ok.replace("https://", "file://"),
                UpdateError::BadValue("url"),
            ),
            (
                ok.replace("https://example.com", "https://u:p@example.com"),
                UpdateError::BadValue("url"),
            ),
            (
                ok.replace("https://example.com", "https://"),
                UpdateError::BadValue("url"),
            ),
            (
                ok.replace("exe\nwindows-x86_64.sha", "exe x\nwindows-x86_64.sha"),
                UpdateError::BadValue("url"),
            ),
            (ok.replace(SHA, &SHA[..63]), UpdateError::BadValue("sha256")),
            (
                ok.replace(SHA, &format!("{SHA}0")),
                UpdateError::BadValue("sha256"),
            ),
            (
                ok.replace(SHA, &SHA.replace('0', "g")),
                UpdateError::BadValue("sha256"),
            ),
            (
                ok.replace(
                    "mdview-setup-0.2.0.exe\nwindows-x86_64.url",
                    "../evil.exe\nwindows-x86_64.url",
                ),
                UpdateError::BadValue("asset"),
            ),
            (ok.replace("manifest: 1\n", ""), UpdateError::BadHeader),
            (
                ok.replace("manifest: 1", "manifest: 2"),
                UpdateError::BadHeader,
            ),
            (
                ok.replace("version: 0.2.0", "version: nope"),
                UpdateError::BadVersion,
            ),
            (
                ok.replace("version: 0.2.0\n", ""),
                UpdateError::MissingField("version"),
            ),
            (
                format!("{ok}version: 0.3.0\n"),
                UpdateError::DuplicateKey("version".into()),
            ),
            (
                format!("{ok}bogus: 1\n"),
                UpdateError::UnknownKey("bogus".into()),
            ),
            (
                format!("{ok}windows-x86_64.exec: 1\n"),
                UpdateError::UnknownKey("windows-x86_64.exec".into()),
            ),
            (format!("{ok}garbage\n"), UpdateError::BadLine(6)),
            (
                format!("{ok}notes: a\x07b\n"),
                UpdateError::BadValue("notes"),
            ),
            (
                format!("{ok}notes: {}\n", "x".repeat(MAX_NOTES_BYTES + 1)),
                UpdateError::BadValue("notes"),
            ),
            (
                "manifest: 1\nversion: 1.0.0\n".into(),
                UpdateError::NoAssets,
            ),
            (
                ok.replace("windows-x86_64.asset: mdview-setup-0.2.0.exe\n", ""),
                UpdateError::MissingField("platform asset/url/sha256"),
            ),
        ];
        for (text, want) in cases {
            assert_eq!(
                UpdateManifest::parse(text.as_bytes()).unwrap_err(),
                want,
                "{text}"
            );
        }
    }

    #[test]
    fn manifest_size_and_encoding_limits() {
        let big = format!("{}{}", manifest(""), "# x\n".repeat(MAX_MANIFEST_BYTES));
        assert_eq!(
            UpdateManifest::parse(big.as_bytes()).unwrap_err(),
            UpdateError::TooLarge
        );
        let many = format!("{}{}", manifest(""), "#\n".repeat(MAX_LINES));
        assert_eq!(
            UpdateManifest::parse(many.as_bytes()).unwrap_err(),
            UpdateError::TooManyLines
        );
        assert_eq!(
            UpdateManifest::parse(&[0xff, 0xfe]).unwrap_err(),
            UpdateError::NotUtf8
        );
        assert!(UpdateManifest::parse(b"").is_err());
        let long_url = manifest("").replace(
            "example.com/a/",
            &format!("example.com/{}/", "a".repeat(600)),
        );
        assert_eq!(
            UpdateManifest::parse(long_url.as_bytes()).unwrap_err(),
            UpdateError::BadValue("url")
        );
    }

    #[test]
    fn newer_and_platform_selection() {
        let both = format!(
            "{}linux-x86_64.asset: mdview.AppImage\nlinux-x86_64.url: https://example.com/mdview.AppImage\nlinux-x86_64.sha256: {SHA}\n",
            manifest("")
        );
        let m = UpdateManifest::parse(both.as_bytes()).unwrap();
        assert_eq!(
            newer(&v("0.1.0"), &m, Platform::WindowsX64).unwrap().name,
            "mdview-setup-0.2.0.exe"
        );
        assert_eq!(
            newer(&v("0.1.0"), &m, Platform::LinuxX64).unwrap().name,
            "mdview.AppImage"
        );
        assert!(newer(&v("0.2.0"), &m, Platform::WindowsX64).is_none());
        assert!(newer(&v("0.3.0"), &m, Platform::WindowsX64).is_none());
        assert!(newer(&v("0.2.0-rc.1"), &m, Platform::WindowsX64).is_some());
        let win_only = UpdateManifest::parse(manifest("").as_bytes()).unwrap();
        assert!(newer(&v("0.1.0"), &win_only, Platform::LinuxX64).is_none());
        let pre =
            UpdateManifest::parse(manifest("").replace("0.2.0\n", "0.2.0-beta.1\n").as_bytes())
                .unwrap();
        assert!(newer(&v("0.2.0"), &pre, Platform::WindowsX64).is_none());
    }

    #[test]
    fn sha256_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256_hex(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
        let mut h = Sha256::new();
        for _ in 0..1000 {
            h.update(&[b'a'; 1000]);
        }
        assert_eq!(
            hex(&h.finish()),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn sha256_boundaries_and_chunking() {
        // Padding edge lengths (55/56/63/64/65) hashed whole and byte-by-byte must agree.
        for n in [0usize, 1, 55, 56, 57, 63, 64, 65, 119, 120, 128, 1000] {
            let data: Vec<u8> = (0..n).map(|i| (i * 7) as u8).collect();
            let whole = sha256_hex(&data);
            let mut h = Sha256::new();
            for b in &data {
                h.update(std::slice::from_ref(b));
            }
            assert_eq!(hex(&h.finish()), whole, "len {n}");
        }
        assert_eq!(
            sha256_hex(&[b'a'; 55]),
            "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"
        );
        assert_eq!(
            sha256_hex(&[b'a'; 64]),
            "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"
        );
    }

    #[test]
    fn verify_bytes_and_file() {
        let good = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(b"abc", good));
        assert!(verify_sha256(b"abc", &good.to_uppercase()));
        assert!(!verify_sha256(b"abd", good));
        assert!(!verify_sha256(b"abc", &good[..63]));
        assert!(!verify_sha256(b"abc", ""));
        let p = std::env::temp_dir().join(format!("mdview-update-test-{}", std::process::id()));
        std::fs::write(&p, b"abc").unwrap();
        assert!(verify_sha256_file(&p, good).unwrap());
        assert!(!verify_sha256_file(&p, SHA).unwrap());
        std::fs::remove_file(&p).unwrap();
        assert!(verify_sha256_file(&p, good).is_err());
    }
}
