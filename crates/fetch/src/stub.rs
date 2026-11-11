//! Transport for platforms other than Windows: every download fails.
//!
//! TODO(linux): fetch through the system libcurl loaded with `dlopen` at the first request, so the
//! exe still carries no TLS stack and a system without libcurl just shows failed images. Mirror
//! the WinHTTP settings: `CURLOPT_PROTOCOLS_STR`/`CURLOPT_REDIR_PROTOCOLS_STR` = "http,https",
//! `CURLOPT_FOLLOWLOCATION` off (redirects stay in `download`), `CURLOPT_TIMEOUT_MS` from the
//! deadline, `CURLOPT_MAXFILESIZE_LARGE` plus a write callback that aborts past the cap, no
//! `CURLOPT_ACCEPT_ENCODING`, no cookie engine, no netrc, `CURLOPT_USERAGENT` = "mdview".

use std::time::Instant;

use crate::url::Url;
use crate::{Error, Response};

pub struct Session;

impl Session {
    pub fn new(_user_agent: &str) -> Result<Session, Error> {
        Err(Error::Unsupported)
    }

    pub fn get(
        &self,
        _url: &Url,
        _extra_headers: &[String],
        _deadline: Instant,
        _max_bytes: usize,
    ) -> Result<Response, Error> {
        Err(Error::Unsupported)
    }
}
