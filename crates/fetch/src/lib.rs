//! Remote image downloads for mdview: a small, non-blocking fetcher with a disk cache.
//!
//! Documents are untrusted, and every `http(s)` image URL in one ends up here. So the fetcher does
//! as little as it can: GET only, one fixed `User-Agent: mdview` and `Accept: image/*`, no
//! cookies, no credentials, no `Referer`, no decompression, at most 5 redirects (never from https
//! to http, never to another scheme), one overall deadline, a hard size cap, and only a `200`
//! counts. The bytes are returned exactly as served; sniffing and decoding are the caller's job.
//!
//! # Use
//!
//! ```no_run
//! # use mdview_fetch::{Fetcher, State};
//! let fetcher = Fetcher::new(Some("cache/images".into()), || {
//!     // Runs on a worker thread: post a message to the UI thread, nothing more.
//! });
//! match fetcher.request("https://example.com/a.png") {
//!     State::Ready(bytes) => { /* decode and draw */ }
//!     State::Pending => { /* draw a placeholder; `wake` fires when it finishes */ }
//!     State::Failed => { /* draw the failed placeholder */ }
//! }
//! // Later, on the UI thread, after `wake` posted its message:
//! for url in fetcher.take_completed() {
//!     let _ = fetcher.request(&url); // now Ready or Failed
//! }
//! ```
//!
//! # Threads
//!
//! Nothing runs until the first request that misses both caches. Downloads then run on up to four
//! worker threads, each blocking in synchronous WinHTTP calls. A worker that has had nothing to
//! do for 10 seconds exits, closing its WinHTTP session and any kept-alive connection, so an idle
//! app has no fetch threads at all.
//!
//! # Retries
//!
//! Failures follow [`mdview_core::status::ImageRequests`]: a failed URL reports
//! [`State::Failed`] for 30 seconds, after which the next [`Fetcher::request`] tries again;
//! [`Fetcher::retry_failed`] (the Reload command) clears every failure at once. A URL that is not
//! an acceptable `http(s)` URL is [`State::Failed`] for good and costs no thread.
//!
//! # Platforms
//!
//! Windows uses WinHTTP (see `winhttp.rs`, the only `unsafe` code in the crate). Other platforms
//! build a stub whose downloads always fail; see `stub.rs` for the intended approach.

// The stub transport leaves the parts only a real transport uses (error kinds, header parsing).
#![cfg_attr(not(windows), allow(dead_code))]

mod cache;
mod url;

#[cfg(not(windows))]
mod stub;
#[cfg(windows)]
mod winhttp;

#[cfg(not(windows))]
use stub::Session;
#[cfg(windows)]
use winhttp::Session;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mdview_core::limits::{IMAGE_CACHE_BYTES, IMAGE_TIMEOUT_MS, MAX_IMAGE_BYTES};
use mdview_core::status::ImageRequests;

use cache::{DiskCache, Entry, unix_now};
use url::Url;

/// Bytes of one downloaded image, exactly as served (the caller sniffs and decodes).
pub type Bytes = Arc<[u8]>;

/// Where one URL stands, as returned by [`Fetcher::request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Ready(Bytes),
    Pending,
    Failed,
}

/// The user agent sent with every request. Fixed and generic on purpose.
pub const USER_AGENT: &str = "mdview";
/// Redirects followed before giving up.
pub const MAX_REDIRECTS: usize = 5;
/// How long a cached copy is used without asking the server again.
pub const FRESH_FOR: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_WORKERS: usize = 4;
const IDLE_EXIT: Duration = Duration::from_secs(10);
const RETRY_AFTER: Duration = Duration::from_secs(30);
/// Longest `ETag` or `Last-Modified` value kept for revalidation.
const MAX_VALIDATOR_LEN: usize = 1024;

/// Why a download failed. Internal: callers only see [`State::Failed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Error {
    /// No transport on this platform, or the WinHTTP session could not be opened.
    Unsupported,
    Os(u32),
    Timeout,
    TooLarge,
    Status(u16),
    TooManyRedirects,
    BadRedirect,
    InsecureRedirect,
    BadResponse,
}

/// One response from the transport. The body is read only for a 200.
#[derive(Debug, Default)]
pub(crate) struct Response {
    pub status: u16,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

/// Limits the tests shrink; the public constructor uses the real ones.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub max_bytes: usize,
    pub deadline: Duration,
    pub cache_cap: u64,
    pub fresh_for: Duration,
    pub retry_after: Duration,
    pub idle_exit: Duration,
    pub max_workers: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            max_bytes: MAX_IMAGE_BYTES,
            deadline: Duration::from_millis(u64::from(IMAGE_TIMEOUT_MS)),
            cache_cap: IMAGE_CACHE_BYTES,
            fresh_for: FRESH_FOR,
            retry_after: RETRY_AFTER,
            idle_exit: IDLE_EXIT,
            max_workers: MAX_WORKERS,
        }
    }
}

/// Downloads images in the background and keeps them in memory and on disk.
///
/// `Fetcher` is `Send + Sync`, but it is meant to be owned by the UI thread: every method returns
/// at once (a fresh disk-cache hit reads one file of at most 12 MiB; nothing else touches the disk
/// or the network on the calling thread). Dropping it lets running downloads finish on their
/// threads, without calling `wake` again, and stops the workers; it does not wait for them.
pub struct Fetcher {
    inner: Arc<Inner>,
}

struct Inner {
    cfg: Config,
    cache: Option<DiskCache>,
    wake: Box<dyn Fn() + Send + Sync>,
    shared: Mutex<Shared>,
    work: Condvar,
}

struct Shared {
    /// Downloaded or disk-loaded bytes, until `trim_memory`.
    ready: HashMap<String, Bytes>,
    /// URLs being fetched; the flag records whether a caller was told `Pending` and so expects
    /// the URL in `take_completed`.
    inflight: HashMap<String, bool>,
    requests: ImageRequests,
    completed: Vec<String>,
    queue: VecDeque<Job>,
    workers: usize,
    idle: usize,
    shutdown: bool,
}

struct Job {
    key: String,
    url: Url,
    /// An expired cache entry: its validators make the request conditional, and its body is the
    /// answer for a 304 or if the server cannot be reached.
    stale: Option<Entry>,
}

impl Fetcher {
    /// `cache_dir`: where files are kept (created on demand; `None` = memory only).
    /// `wake`: called from a worker thread when a request completes, so the UI thread can call
    /// [`take_completed`](Self::take_completed). Calls are coalesced: after one, the next comes
    /// only once `take_completed` has emptied the list. It must not block or call back into the
    /// `Fetcher` beyond cheap work; posting a message to the event loop is the intended use.
    pub fn new(cache_dir: Option<PathBuf>, wake: impl Fn() + Send + Sync + 'static) -> Fetcher {
        Fetcher::with_config(cache_dir, Config::default(), wake)
    }

    pub(crate) fn with_config(
        cache_dir: Option<PathBuf>,
        cfg: Config,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Fetcher {
        let cache = cache_dir.map(|dir| DiskCache::new(dir, cfg.cache_cap, cfg.max_bytes));
        Fetcher {
            inner: Arc::new(Inner {
                shared: Mutex::new(Shared {
                    ready: HashMap::new(),
                    inflight: HashMap::new(),
                    requests: ImageRequests::with_retry_after(cfg.retry_after),
                    completed: Vec::new(),
                    queue: VecDeque::new(),
                    workers: 0,
                    idle: 0,
                    shutdown: false,
                }),
                cfg,
                cache,
                wake: Box::new(wake),
                work: Condvar::new(),
            }),
        }
    }

    /// Non-blocking. `Ready` from memory or a fresh disk entry; otherwise starts (or joins) a
    /// download and returns `Pending`; `Failed` if the URL is unacceptable or a recent attempt
    /// failed and the retry delay has not passed. The URL string is the key: the same string
    /// comes back from [`take_completed`](Self::take_completed).
    pub fn request(&self, url: &str) -> State {
        let Some(parsed) = Url::parse(url) else {
            return State::Failed;
        };
        let now = Instant::now();
        {
            let mut s = self.inner.lock();
            if let Some(bytes) = s.ready.get(url) {
                return State::Ready(bytes.clone());
            }
            if let Some(saw_pending) = s.inflight.get_mut(url) {
                *saw_pending = true;
                return State::Pending;
            }
            if !s.requests.should_fetch(url, now) {
                return State::Failed;
            }
            s.inflight.insert(url.to_string(), false);
        }
        // The disk is read without the lock held, so workers are never held up by it.
        let disk_key = parsed.to_string();
        let stored = self.inner.cache.as_ref().and_then(|c| c.load(&disk_key));
        let fresh = stored.as_ref().is_some_and(|e| self.inner.is_fresh(e));
        let mut s = self.inner.lock();
        if fresh {
            let bytes: Bytes = stored.map(|e| e.body).unwrap_or_default().into();
            let saw_pending = s.inflight.remove(url).unwrap_or(false);
            s.requests.finished(url, true, Instant::now());
            s.ready.insert(url.to_string(), bytes.clone());
            if saw_pending {
                let notify = s.complete(url);
                drop(s);
                self.inner.notify(notify);
            }
            return State::Ready(bytes);
        }
        s.queue.push_back(Job {
            key: url.to_string(),
            url: parsed,
            stale: stored,
        });
        Inner::start_worker(&self.inner, &mut s);
        State::Pending
    }

    /// URLs that finished (successfully or not) since the last call, in completion order. Call
    /// [`request`](Self::request) for each to get its bytes.
    pub fn take_completed(&self) -> Vec<String> {
        std::mem::take(&mut self.inner.lock().completed)
    }

    /// Forget failures so they are retried on the next `request` (the user pressed Reload).
    pub fn retry_failed(&self) {
        self.inner.lock().requests.on_reload();
    }

    /// Drop in-memory copies (the disk cache stays), e.g. when no document shows them any more.
    pub fn trim_memory(&self) {
        let mut s = self.inner.lock();
        s.ready = HashMap::new();
    }

    #[cfg(test)]
    fn worker_count(&self) -> usize {
        self.inner.lock().workers
    }
}

impl Drop for Fetcher {
    fn drop(&mut self) {
        let mut s = self.inner.lock();
        s.shutdown = true;
        s.queue.clear();
        drop(s);
        self.inner.work.notify_all();
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn is_fresh(&self, e: &Entry) -> bool {
        let now = unix_now();
        e.fetched <= now && now - e.fetched < self.cfg.fresh_for.as_secs()
    }

    fn notify(&self, notify: bool) {
        if notify {
            (self.wake)();
        }
    }

    /// Wakes an idle worker for the job just queued, or starts one if none is free.
    fn start_worker(this: &Arc<Inner>, s: &mut Shared) {
        if s.queue.len() <= s.idle {
            this.work.notify_one();
            return;
        }
        if s.workers >= this.cfg.max_workers {
            return;
        }
        s.workers += 1;
        let inner = this.clone();
        let spawned = std::thread::Builder::new()
            .name("mdview-fetch".into())
            .stack_size(256 * 1024)
            .spawn(move || worker(inner));
        if spawned.is_err() {
            s.workers -= 1;
            if s.workers == 0 {
                // No thread will ever take these: fail them now rather than leave them pending.
                let now = Instant::now();
                while let Some(job) = s.queue.pop_front() {
                    s.inflight.remove(&job.key);
                    s.requests.finished(&job.key, false, now);
                    s.completed.push(job.key);
                }
            }
        }
    }

    /// Next job, or `None` when this worker should exit.
    fn next_job(&self) -> Option<Job> {
        let mut s = self.lock();
        loop {
            if let Some(job) = s.queue.pop_front() {
                return Some(job);
            }
            if s.shutdown {
                s.workers -= 1;
                return None;
            }
            s.idle += 1;
            let (guard, timeout) = self
                .work
                .wait_timeout(s, self.cfg.idle_exit)
                .unwrap_or_else(|e| e.into_inner());
            s = guard;
            s.idle -= 1;
            if timeout.timed_out() && s.queue.is_empty() {
                s.workers -= 1;
                return None;
            }
        }
    }

    /// Fetches one job, consulting and updating the disk cache. `None` is a failure.
    fn run(&self, session: &mut Option<Session>, job: Job) -> Option<Bytes> {
        if session.is_none() {
            *session = Session::new(USER_AGENT).ok();
        }
        let validators = job
            .stale
            .as_ref()
            .map(|e| (e.etag.as_str(), e.last_modified.as_str()));
        let result = match session {
            Some(session) => download(session, &job.url, validators, &self.cfg),
            None => Err(Error::Unsupported),
        };
        let disk_key = job.url.to_string();
        let entry = match result {
            Ok(Downloaded::Body(entry)) => entry,
            Ok(Downloaded::NotModified) => {
                let mut entry = job.stale?;
                entry.fetched = unix_now();
                entry
            }
            // Serving an expired copy beats a broken image while the server is unreachable.
            Err(_) => return job.stale.map(|e| e.body.into()),
        };
        if let Some(cache) = &self.cache {
            cache.store(&disk_key, &entry);
        }
        Some(entry.body.into())
    }

    fn finish(&self, key: String, bytes: Option<Bytes>) {
        let mut s = self.lock();
        s.inflight.remove(&key);
        s.requests.finished(&key, bytes.is_some(), Instant::now());
        if let Some(bytes) = bytes {
            s.ready.insert(key.clone(), bytes);
        }
        let notify = s.complete_owned(key);
        drop(s);
        self.notify(notify);
    }
}

impl Shared {
    /// Records a completion; true if `wake` should be called (the list was empty and the fetcher
    /// is still alive).
    fn complete(&mut self, url: &str) -> bool {
        self.complete_owned(url.to_string())
    }

    fn complete_owned(&mut self, url: String) -> bool {
        let was_empty = self.completed.is_empty();
        self.completed.push(url);
        was_empty && !self.shutdown
    }
}

fn worker(inner: Arc<Inner>) {
    let mut session = None;
    while let Some(job) = inner.next_job() {
        let key = job.key.clone();
        let bytes = catch_unwind(AssertUnwindSafe(|| inner.run(&mut session, job))).unwrap_or(None);
        inner.finish(key, bytes);
    }
}

enum Downloaded {
    Body(Entry),
    NotModified,
}

/// GETs `url`, following up to [`MAX_REDIRECTS`] redirects, all within one deadline. With
/// `validators` (ETag, Last-Modified; empty when absent) the request is conditional and a 304 is
/// accepted.
fn download(
    session: &Session,
    url: &Url,
    validators: Option<(&str, &str)>,
    cfg: &Config,
) -> Result<Downloaded, Error> {
    let deadline = Instant::now() + cfg.deadline;
    let mut headers = Vec::new();
    if let Some((etag, last_modified)) = validators {
        if header_safe(etag) {
            headers.push(format!("If-None-Match: {etag}"));
        }
        if header_safe(last_modified) {
            headers.push(format!("If-Modified-Since: {last_modified}"));
        }
    }
    let mut current = url.clone();
    for hop in 0..=MAX_REDIRECTS {
        let response = session.get(&current, &headers, deadline, cfg.max_bytes)?;
        match response.status {
            200 => {
                let keep = |v: Option<String>| v.filter(|v| header_safe(v)).unwrap_or_default();
                return Ok(Downloaded::Body(Entry {
                    fetched: unix_now(),
                    etag: keep(response.etag),
                    last_modified: keep(response.last_modified),
                    body: response.body,
                }));
            }
            304 if !headers.is_empty() => return Ok(Downloaded::NotModified),
            301 | 302 | 303 | 307 | 308 => {
                if hop == MAX_REDIRECTS {
                    return Err(Error::TooManyRedirects);
                }
                current = redirect_target(&current, response.location.as_deref())?;
            }
            status => return Err(Error::Status(status)),
        }
    }
    Err(Error::TooManyRedirects)
}

/// Where a redirect from `current` leads: an http(s) URL, and never from https down to http.
fn redirect_target(current: &Url, location: Option<&str>) -> Result<Url, Error> {
    let next = current
        .join(location.ok_or(Error::BadRedirect)?)
        .ok_or(Error::BadRedirect)?;
    if current.https && !next.https {
        return Err(Error::InsecureRedirect);
    }
    Ok(next)
}

/// A server-supplied value that may be sent back in a header: printable ASCII, bounded.
fn header_safe(v: &str) -> bool {
    !v.is_empty() && v.len() <= MAX_VALIDATOR_LEN && v.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// `Content-Length` as WinHTTP reports it; anything but plain digits is a bad response.
pub(crate) fn parse_content_length(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s.len() > 19 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}
