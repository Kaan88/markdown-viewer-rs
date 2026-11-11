//! The on-disk image cache.
//!
//! # Layout
//!
//! ```text
//! <cache_dir>/
//!   <16 hex digits>.img          one entry per URL; the name is FNV-1a 64 of the URL
//!   <16 hex digits>.<pid>.<n>.tmp   an entry being written, renamed over the .img when complete
//! ```
//!
//! An entry file is a header followed by the body exactly as served:
//!
//! ```text
//! magic        8 bytes   "mdvimg01"
//! fetched      u64 LE    Unix seconds of the last download or successful revalidation
//! url_len      u16 LE    then the URL (normalized, see Url's Display), UTF-8
//! etag_len     u16 LE    then the ETag header value, ASCII, may be empty
//! lm_len       u16 LE    then the Last-Modified header value, ASCII, may be empty
//! body_len     u64 LE    then the body
//! checksum     u64 LE    FNV-1a 64 of the body
//! ```
//!
//! A file is served only if every length agrees with the file size, the URL matches the one asked
//! for (two URLs may share a name; the later download replaces the earlier) and the checksum
//! matches. Writes go to a unique temp file that is renamed into place, so a crash or a second
//! process never exposes a half-written entry. The file's modification time is its last use, which
//! eviction sorts by; it is bumped on every read. Temp files older than an hour are left-overs of a
//! crashed writer and are deleted by the next eviction pass.
//!
//! Two processes may share the directory: each entry is replaced atomically, reads take the whole
//! file at once, and both sides tolerate an entry vanishing under them (it is just a miss).

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

const MAGIC: &[u8; 8] = b"mdvimg01";
const MAX_HEADER_FIELD: usize = 4096;
const STALE_TEMP: Duration = Duration::from_secs(3600);

pub fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Unix seconds.
    pub fetched: u64,
    pub etag: String,
    pub last_modified: String,
    pub body: Vec<u8>,
}

pub struct DiskCache {
    dir: PathBuf,
    cap: u64,
    max_body: usize,
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl DiskCache {
    pub fn new(dir: PathBuf, cap: u64, max_body: usize) -> DiskCache {
        DiskCache { dir, cap, max_body }
    }

    fn path_for(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.img", fnv1a(url.as_bytes())))
    }

    /// The entry for `url` if a valid one is stored. Marks it as just used.
    pub fn load(&self, url: &str) -> Option<Entry> {
        let path = self.path_for(url);
        let mut file = fs::File::open(&path).ok()?;
        let len = file.metadata().ok()?.len();
        let limit = (self.max_body + 3 * MAX_HEADER_FIELD + 64) as u64;
        if len > limit {
            drop(file);
            let _ = fs::remove_file(&path);
            return None;
        }
        let mut data = Vec::with_capacity(len as usize);
        file.by_ref().take(limit).read_to_end(&mut data).ok()?;
        drop(file);
        match decode(&data) {
            Some((stored_url, entry)) if stored_url == url => {
                // Best effort: the modification time is the LRU clock.
                if let Ok(f) = fs::OpenOptions::new().write(true).open(&path) {
                    let _ = f.set_modified(SystemTime::now());
                }
                Some(entry)
            }
            Some(_) => None, // another URL with the same hash; leave it
            None => {
                let _ = fs::remove_file(&path);
                None
            }
        }
    }

    /// Stores `entry` for `url`, then evicts least recently used entries over the cap. Failures
    /// are ignored: the cache is an optimisation.
    pub fn store(&self, url: &str, entry: &Entry) {
        let data = encode(url, entry);
        if data.len() as u64 > self.cap || fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let path = self.path_for(url);
        let temp = self.dir.join(format!(
            "{:016x}.{}.{}.tmp",
            fnv1a(url.as_bytes()),
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let written = (|| {
            use std::io::Write;
            let mut f = fs::File::create(&temp)?;
            f.write_all(&data)?;
            f.sync_all()
        })();
        if written.is_err() || fs::rename(&temp, &path).is_err() {
            let _ = fs::remove_file(&temp);
            return;
        }
        self.evict();
    }

    fn evict(&self) {
        let Ok(dir) = fs::read_dir(&self.dir) else {
            return;
        };
        let now = SystemTime::now();
        let mut entries = Vec::new();
        let mut total = 0u64;
        for item in dir.flatten() {
            let path = item.path();
            let Ok(meta) = item.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            match path.extension().and_then(|e| e.to_str()) {
                Some("img") => {
                    total += meta.len();
                    entries.push((modified, meta.len(), path));
                }
                Some("tmp")
                    if now
                        .duration_since(modified)
                        .is_ok_and(|age| age > STALE_TEMP) =>
                {
                    let _ = fs::remove_file(&path);
                }
                _ => {}
            }
        }
        if total <= self.cap {
            return;
        }
        entries.sort_by_key(|e| e.0);
        for (_, len, path) in entries {
            if total <= self.cap {
                break;
            }
            if fs::remove_file(&path).is_ok() || !path.exists() {
                total = total.saturating_sub(len);
            }
        }
    }

    #[cfg(test)]
    pub fn file_for(&self, url: &str) -> PathBuf {
        self.path_for(url)
    }
}

fn push_field(out: &mut Vec<u8>, s: &str) {
    let s = &s.as_bytes()[..s.len().min(MAX_HEADER_FIELD)];
    out.extend_from_slice(&(s.len() as u16).to_le_bytes());
    out.extend_from_slice(s);
}

fn encode(url: &str, e: &Entry) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + url.len() + e.body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&e.fetched.to_le_bytes());
    push_field(&mut out, url);
    push_field(&mut out, &e.etag);
    push_field(&mut out, &e.last_modified);
    out.extend_from_slice(&(e.body.len() as u64).to_le_bytes());
    out.extend_from_slice(&e.body);
    out.extend_from_slice(&fnv1a(&e.body).to_le_bytes());
    out
}

fn decode(data: &[u8]) -> Option<(String, Entry)> {
    let mut r = Reader(data);
    if r.take(8)? != MAGIC {
        return None;
    }
    let fetched = u64::from_le_bytes(r.take(8)?.try_into().ok()?);
    let url = r.field()?;
    let etag = r.field()?;
    let last_modified = r.field()?;
    let body_len = usize::try_from(u64::from_le_bytes(r.take(8)?.try_into().ok()?)).ok()?;
    let body = r.take(body_len)?.to_vec();
    let sum = u64::from_le_bytes(r.take(8)?.try_into().ok()?);
    if !r.0.is_empty() || sum != fnv1a(&body) {
        return None;
    }
    Some((
        url,
        Entry {
            fetched,
            etag,
            last_modified,
            body,
        },
    ))
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(head)
    }

    fn field(&mut self) -> Option<String> {
        let len = u16::from_le_bytes(self.take(2)?.try_into().ok()?);
        String::from_utf8(self.take(usize::from(len))?.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_rejects_damage() {
        let e = Entry {
            fetched: 42,
            etag: "\"abc\"".into(),
            last_modified: String::new(),
            body: vec![1, 2, 3, 4],
        };
        let data = encode("http://a/x", &e);
        assert_eq!(decode(&data), Some(("http://a/x".to_string(), e)));
        assert_eq!(decode(&data[..data.len() - 1]), None);
        let mut flipped = data.clone();
        let body_at = data.len() - 8 - 2;
        flipped[body_at] ^= 1;
        assert_eq!(decode(&flipped), None);
        let mut longer = data.clone();
        longer.push(0);
        assert_eq!(decode(&longer), None);
    }
}
