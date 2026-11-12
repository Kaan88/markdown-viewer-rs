//! One HTTP GET through WinHTTP, the HTTP stack that ships with Windows, so the exe carries no TLS
//! code. Every `unsafe` block of the crate is in this module; the rest sees [`Session::get`].
//!
//! What this module sets on every request: no automatic redirects, no cookies, no authentication
//! or auto-logon, no client certificate, no decompression (WinHTTP sends no `Accept-Encoding`
//! unless asked), `Accept: image/*` (or what [`Session::accepting`] set), no `Referer`, the fixed user agent given to [`Session::new`],
//! TLS 1.2 or later with the system's certificate validation left on.
//!
//! # The deadline
//!
//! Calls are synchronous, on a worker thread. WinHTTP's own timeouts are set from what is left of
//! the deadline before every call, but on Windows 11 they turned out to be coarse (a 1.2 s budget
//! against a silent server returned after 4 s, and `WinHttpSendRequest` reported success). So each
//! request also has a watchdog thread that closes the request handle when the deadline passes,
//! which is WinHTTP's documented way to cancel a synchronous call in progress. To make that safe,
//! the handle is only ever used through [`Request::call`], which refuses to start a call once the
//! deadline has passed, and the watchdog only closes the handle while a call is running; otherwise
//! the handle is closed by the worker when the request is dropped. Either way it is closed once.

use std::ffi::c_void;
use std::ptr::{NonNull, null, null_mut};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Instant;

use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
use windows_sys::Win32::Networking::WinHttp::{
    ERROR_WINHTTP_HEADER_NOT_FOUND, ERROR_WINHTTP_TIMEOUT, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
    WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH, WINHTTP_DISABLE_AUTHENTICATION, WINHTTP_DISABLE_COOKIES,
    WINHTTP_DISABLE_REDIRECTS, WINHTTP_FLAG_SECURE, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2,
    WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3, WINHTTP_OPTION_AUTOLOGON_POLICY,
    WINHTTP_OPTION_CLIENT_CERT_CONTEXT, WINHTTP_OPTION_DISABLE_FEATURE,
    WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT, WINHTTP_OPTION_RECEIVE_TIMEOUT,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_ETAG,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_LAST_MODIFIED, WINHTTP_QUERY_LOCATION,
    WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetOption, WinHttpSetTimeouts,
};

use crate::url::Url;
use crate::{Error, Response};

type Raw = *mut c_void;

fn last_error() -> Error {
    // SAFETY: reads the calling thread's last-error value.
    let code = unsafe { GetLastError() };
    if code == ERROR_WINHTTP_TIMEOUT {
        Error::Timeout
    } else {
        Error::Os(code)
    }
}

fn check(ok: i32) -> Result<(), Error> {
    if ok != 0 { Ok(()) } else { Err(last_error()) }
}

fn remaining_ms(deadline: Instant) -> Result<u32, Error> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(Error::Timeout);
    }
    Ok(left.as_millis().clamp(1, i32::MAX as u128) as u32)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Sets a DWORD option.
///
/// # Safety
/// `h` must be an open WinHTTP handle.
unsafe fn set_u32(h: Raw, option: u32, value: u32) -> Result<(), Error> {
    // SAFETY: `h` is open (caller's contract); the buffer is a live u32 whose size is passed.
    check(unsafe {
        WinHttpSetOption(
            h,
            option,
            (&value as *const u32).cast(),
            size_of::<u32>() as u32,
        )
    })
}

/// Sets every WinHTTP timeout on `h` to what is left before `deadline`.
///
/// # Safety
/// `h` must be an open WinHTTP request handle.
unsafe fn set_timeouts(h: Raw, deadline: Instant) -> Result<(), Error> {
    let ms = remaining_ms(deadline)?;
    let ms_i = ms as i32;
    // SAFETY: `h` is open (caller's contract); the timeouts are plain integers.
    check(unsafe { WinHttpSetTimeouts(h, ms_i, ms_i, ms_i, ms_i) })?;
    // Waiting for the response headers has its own timeout (90 s by default), which
    // WinHttpSetTimeouts does not touch.
    // SAFETY: as above.
    unsafe { set_u32(h, WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT, ms) }
}

/// A session or connect handle, closed on drop.
struct Handle(NonNull<c_void>);

impl Handle {
    fn new(raw: Raw) -> Result<Handle, Error> {
        NonNull::new(raw).map(Handle).ok_or_else(last_error)
    }

    fn raw(&self) -> Raw {
        self.0.as_ptr()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from WinHTTP, is open, and is closed exactly once here. A
        // request handle is always dropped before the connect handle it was opened on, and that
        // before the session, because each lives in a narrower scope.
        unsafe { WinHttpCloseHandle(self.raw()) };
    }
}

struct Slot {
    raw: Raw,
    in_call: bool,
    timed_out: bool,
    closed: bool,
    finished: bool,
}

// SAFETY: the raw handle is only an identifier passed to WinHTTP, which is thread-safe; the slot
// is always accessed under its mutex.
unsafe impl Send for Slot {}

struct Watch {
    slot: Mutex<Slot>,
    cv: Condvar,
}

impl Watch {
    fn lock(&self) -> MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A request handle guarded by a deadline watchdog (see the module documentation).
struct Request {
    watch: Arc<Watch>,
}

impl Request {
    fn new(raw: Raw, deadline: Instant) -> Result<Request, Error> {
        if raw.is_null() {
            return Err(last_error());
        }
        let watch = Arc::new(Watch {
            slot: Mutex::new(Slot {
                raw,
                in_call: false,
                timed_out: false,
                closed: false,
                finished: false,
            }),
            cv: Condvar::new(),
        });
        let w = watch.clone();
        // If no thread can be started, WinHTTP's own (coarser) timeouts still end the request.
        let _ = std::thread::Builder::new()
            .name("mdview-fetch-deadline".into())
            .stack_size(64 * 1024)
            .spawn(move || watchdog(&w, deadline));
        Ok(Request { watch })
    }

    /// Runs `f` with the open handle, unless the deadline has already passed. If the watchdog
    /// closed the handle during the call, the call's own result is replaced by a timeout.
    fn call<T>(&self, f: impl FnOnce(Raw) -> Result<T, Error>) -> Result<T, Error> {
        let raw = {
            let mut s = self.watch.lock();
            if s.timed_out || s.closed {
                return Err(Error::Timeout);
            }
            s.in_call = true;
            s.raw
        };
        let result = f(raw);
        let mut s = self.watch.lock();
        s.in_call = false;
        if s.timed_out {
            return Err(Error::Timeout);
        }
        result
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        let mut s = self.watch.lock();
        s.finished = true;
        if !s.closed {
            s.closed = true;
            // SAFETY: the handle is open (not closed by the watchdog, checked under the lock) and
            // no call is running on it, since calls only run inside `call` on this thread.
            unsafe { WinHttpCloseHandle(s.raw) };
        }
        drop(s);
        self.watch.cv.notify_all();
    }
}

fn watchdog(watch: &Watch, deadline: Instant) {
    let mut s = watch.lock();
    loop {
        if s.finished {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            s.timed_out = true;
            if s.in_call && !s.closed {
                s.closed = true;
                // SAFETY: the handle is open (checked under the lock) and the worker is blocked in
                // a WinHTTP call on it; closing it is WinHTTP's documented way to cancel that call.
                // The worker will not touch the handle again: `call` sees `timed_out`, and `Drop`
                // sees `closed`.
                unsafe { WinHttpCloseHandle(s.raw) };
            }
            return;
        }
        s = watch
            .cv
            .wait_timeout(s, deadline - now)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}

/// A WinHTTP session: one per worker thread, closed when the worker exits, which also drops any
/// kept-alive connections so nothing lingers while the app is idle.
pub struct Session {
    handle: Handle,
    /// The `Accept` header's value.
    accept: &'static str,
}

impl Session {
    pub fn new(user_agent: &str) -> Result<Session, Error> {
        let agent = wide(user_agent);
        // SAFETY: `agent` is NUL-terminated and outlives the call; null proxy strings are allowed
        // with the default access type; flags 0 selects synchronous mode.
        let raw = unsafe {
            WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                null(),
                null(),
                0,
            )
        };
        let handle = Handle::new(raw)?;
        let h = handle.raw();
        // SAFETY: `h` is the session handle just opened.
        unsafe {
            set_u32(
                h,
                WINHTTP_OPTION_REDIRECT_POLICY,
                WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
            )?;
            // TLS 1.3 is refused by older Windows 10 builds; 1.2 alone is then the floor.
            let tls = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3;
            if set_u32(h, WINHTTP_OPTION_SECURE_PROTOCOLS, tls).is_err() {
                set_u32(
                    h,
                    WINHTTP_OPTION_SECURE_PROTOCOLS,
                    WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2,
                )?;
            }
        }
        Ok(Session {
            handle,
            accept: "image/*",
        })
    }

    /// The same session asking for `accept` instead of images.
    pub fn accepting(mut self, accept: &'static str) -> Session {
        self.accept = accept;
        self
    }

    /// One GET of `url`, no redirects followed, finished or abandoned by `deadline`.
    /// `extra_headers` are complete `Name: value` lines the caller has checked for CR/LF. The body
    /// is read only for status 200 and fails with [`Error::TooLarge`] as soon as it would exceed
    /// `max_bytes`.
    pub fn get(
        &self,
        url: &Url,
        extra_headers: &[String],
        deadline: Instant,
        max_bytes: usize,
    ) -> Result<Response, Error> {
        let host = wide(url.connect_host());
        // SAFETY: the session handle is open; `host` is NUL-terminated and outlives the call.
        // WinHttpConnect only records the server; nothing goes on the network yet.
        let connect =
            Handle::new(unsafe { WinHttpConnect(self.handle.raw(), host.as_ptr(), url.port, 0) })?;

        let verb = wide("GET");
        let target = wide(&url.target);
        let accept = wide(self.accept);
        let accept_types = [accept.as_ptr(), null()];
        let flags = if url.https { WINHTTP_FLAG_SECURE } else { 0 };
        // SAFETY: the connect handle is open; every string is NUL-terminated and outlives the
        // call; a null version means HTTP/1.1, a null referrer sends no Referer; the accept-type
        // array is NULL-terminated.
        let request = Request::new(
            unsafe {
                WinHttpOpenRequest(
                    connect.raw(),
                    verb.as_ptr(),
                    target.as_ptr(),
                    null(),
                    null(),
                    accept_types.as_ptr(),
                    flags,
                )
            },
            deadline,
        )?;

        let headers = wide(&extra_headers.join("\r\n"));
        request.call(|h| {
            // SAFETY (whole block): `call` hands out the request handle only while it is open.
            unsafe {
                set_u32(
                    h,
                    WINHTTP_OPTION_DISABLE_FEATURE,
                    WINHTTP_DISABLE_COOKIES
                        | WINHTTP_DISABLE_REDIRECTS
                        | WINHTTP_DISABLE_AUTHENTICATION,
                )?;
                set_u32(
                    h,
                    WINHTTP_OPTION_AUTOLOGON_POLICY,
                    WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH,
                )?;
                if url.https {
                    // A null buffer of length 0 is WINHTTP_NO_CLIENT_CERT_CONTEXT: send none.
                    check(WinHttpSetOption(
                        h,
                        WINHTTP_OPTION_CLIENT_CERT_CONTEXT,
                        null(),
                        0,
                    ))?;
                }
                set_timeouts(h, deadline)?;
                // `headers` is NUL-terminated (length -1 says so) and outlives the call; there is
                // no request body.
                let (ptr, len) = if extra_headers.is_empty() {
                    (null(), 0)
                } else {
                    (headers.as_ptr(), u32::MAX)
                };
                check(WinHttpSendRequest(h, ptr, len, null(), 0, 0, 0))
            }
        })?;
        request.call(|h| {
            // SAFETY: the handle is open and the request was sent; the reserved argument is null.
            unsafe {
                set_timeouts(h, deadline)?;
                check(WinHttpReceiveResponse(h, null_mut()))
            }
        })?;

        let mut response = request.call(|h| {
            // SAFETY: the handle is open and its response headers have been received.
            unsafe {
                Ok(Response {
                    status: query_status(h)?,
                    etag: query_string(h, WINHTTP_QUERY_ETAG)?,
                    last_modified: query_string(h, WINHTTP_QUERY_LAST_MODIFIED)?,
                    location: query_string(h, WINHTTP_QUERY_LOCATION)?,
                    body: Vec::new(),
                })
            }
        })?;
        if response.status != 200 {
            return Ok(response);
        }
        // SAFETY: as above.
        let declared =
            match request.call(|h| unsafe { query_string(h, WINHTTP_QUERY_CONTENT_LENGTH) })? {
                Some(s) => Some(crate::parse_content_length(&s).ok_or(Error::BadResponse)?),
                None => None,
            };
        if declared.is_some_and(|n| n > max_bytes as u64) {
            return Err(Error::TooLarge);
        }
        let mut body = Vec::with_capacity(declared.unwrap_or(0) as usize);
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            let read = request.call(|h| {
                let mut read = 0u32;
                // SAFETY: the handle is open with a response received; `chunk` is writable for
                // the length passed and `read` is a live u32.
                unsafe {
                    set_u32(h, WINHTTP_OPTION_RECEIVE_TIMEOUT, remaining_ms(deadline)?)?;
                    check(WinHttpReadData(
                        h,
                        chunk.as_mut_ptr().cast(),
                        chunk.len() as u32,
                        &mut read,
                    ))?;
                }
                Ok((read as usize).min(chunk.len()))
            })?;
            if read == 0 {
                break;
            }
            if body.len() + read > max_bytes {
                return Err(Error::TooLarge);
            }
            body.extend_from_slice(&chunk[..read]);
        }
        response.body = body;
        Ok(response)
    }
}

/// # Safety
/// `h` must be an open request handle whose response headers have been received.
unsafe fn query_status(h: Raw) -> Result<u16, Error> {
    let mut status = 0u32;
    let mut len = size_of::<u32>() as u32;
    // SAFETY: `h` is valid (caller's contract); the buffer is a live u32 of the size given, and
    // the header-name and index pointers may be null.
    check(unsafe {
        WinHttpQueryHeaders(
            h,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut len,
            null_mut(),
        )
    })?;
    u16::try_from(status).map_err(|_| Error::BadResponse)
}

/// A response header as text, `None` when absent. Values longer than 8 KiB are refused.
///
/// # Safety
/// `h` must be an open request handle whose response headers have been received.
unsafe fn query_string(h: Raw, info: u32) -> Result<Option<String>, Error> {
    const MAX_CHARS: usize = 8 * 1024;
    let mut buf = vec![0u16; 256];
    for _ in 0..2 {
        let mut len_bytes = (buf.len() * 2) as u32;
        // SAFETY: `h` is valid (caller's contract); `buf` is writable for `len_bytes` bytes and
        // the header-name and index pointers may be null.
        let ok = unsafe {
            WinHttpQueryHeaders(
                h,
                info,
                null(),
                buf.as_mut_ptr().cast::<c_void>(),
                &mut len_bytes,
                null_mut(),
            )
        };
        if ok != 0 {
            let chars = (len_bytes as usize / 2).min(buf.len());
            return Ok(Some(String::from_utf16_lossy(&buf[..chars])));
        }
        // SAFETY: reads the calling thread's last-error value.
        match unsafe { GetLastError() } {
            ERROR_WINHTTP_HEADER_NOT_FOUND => return Ok(None),
            ERROR_INSUFFICIENT_BUFFER => {
                let need = len_bytes as usize / 2 + 1;
                if need > MAX_CHARS {
                    return Err(Error::BadResponse);
                }
                buf = vec![0u16; need];
            }
            code => return Err(Error::Os(code)),
        }
    }
    Err(Error::BadResponse)
}
