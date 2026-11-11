//! Strict parsing of the only URLs this crate will fetch: absolute `http://` and `https://`.
//!
//! Everything a document can put in an image URL arrives here first, so anything unusual is
//! rejected rather than interpreted: userinfo (`user:pass@`), control characters, backslashes,
//! percent-encoded or non-ASCII host names, bad ports, and anything longer than [`MAX_URL_LEN`].
//! Non-ASCII bytes and the few ASCII characters a browser would escape in a path are
//! percent-encoded, so what goes on the wire is plain ASCII.

/// Longest URL accepted, in bytes, before and after resolving a redirect.
pub const MAX_URL_LEN: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub https: bool,
    /// Lowercase host name, IPv4 address, or bracketed IPv6 literal (`[::1]`).
    pub host: String,
    pub port: u16,
    /// Path and query, starting with `/`, ASCII only. The fragment is dropped.
    pub target: String,
}

impl Url {
    pub fn parse(s: &str) -> Option<Url> {
        if s.len() > MAX_URL_LEN || s.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return None;
        }
        let (https, rest) = match strip_prefix_ci(s, "https://") {
            Some(r) => (true, r),
            None => (false, strip_prefix_ci(s, "http://")?),
        };
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        let (host, port) = parse_authority(authority, https)?;
        let target = encode_target(tail)?;
        let url = Url {
            https,
            host,
            port,
            target,
        };
        (url.to_string().len() <= MAX_URL_LEN).then_some(url)
    }

    /// Resolves a `Location` header against this URL. Returns `None` for anything that does not
    /// lead to an http(s) URL, including other schemes such as `file:` or `javascript:`.
    pub fn join(&self, location: &str) -> Option<Url> {
        let loc = location.trim_matches([' ', '\t']);
        if loc.len() > MAX_URL_LEN || loc.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return None;
        }
        if has_scheme(loc) {
            return Url::parse(loc);
        }
        let scheme = if self.https { "https:" } else { "http:" };
        if loc.starts_with("//") {
            return Url::parse(&format!("{scheme}{loc}"));
        }
        let origin = self.origin();
        let path_only = self.target.split('?').next().unwrap_or("/");
        let joined = if loc.starts_with('/') {
            format!("{origin}{loc}")
        } else if loc.starts_with('?') {
            format!("{origin}{path_only}{loc}")
        } else if loc.is_empty() || loc.starts_with('#') {
            format!("{origin}{}", self.target)
        } else {
            let dir = &path_only[..path_only.rfind('/').map_or(0, |i| i + 1)];
            format!("{origin}{dir}{loc}")
        };
        Url::parse(&joined)
    }

    fn origin(&self) -> String {
        let scheme = if self.https { "https" } else { "http" };
        let default = if self.https { 443 } else { 80 };
        if self.port == default {
            format!("{scheme}://{}", self.host)
        } else {
            format!("{scheme}://{}:{}", self.host, self.port)
        }
    }

    /// The host as WinHTTP wants it: IPv6 literals without their brackets.
    pub fn connect_host(&self) -> &str {
        self.host.trim_start_matches('[').trim_end_matches(']')
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.origin(), self.target)
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// RFC 3986 `scheme ":"`, which is how a `Location` names another scheme.
fn has_scheme(s: &str) -> bool {
    let Some(colon) = s.find(':') else {
        return false;
    };
    let scheme = &s[..colon];
    let mut bytes = scheme.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

fn parse_authority(authority: &str, https: bool) -> Option<(String, u16)> {
    if authority.contains(['@', '\\', '%']) {
        return None;
    }
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let close = inner.find(']')?;
        let addr = &inner[..close];
        if addr.is_empty()
            || !addr.contains(':')
            || !addr
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
        {
            return None;
        }
        (&authority[..close + 2], &inner[close + 1..])
    } else {
        match authority.find(':') {
            Some(i) => (&authority[..i], &authority[i..]),
            None => (authority, ""),
        }
    };
    let port = match port {
        "" | ":" => {
            if https {
                443
            } else {
                80
            }
        }
        p => {
            let digits = p.strip_prefix(':')?;
            if digits.is_empty() || digits.len() > 5 || !digits.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            match digits.parse::<u32>() {
                Ok(n @ 1..=65535) => n as u16,
                _ => return None,
            }
        }
    };
    if !host.starts_with('[') {
        let ok = !host.is_empty()
            && host.len() <= 253
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            && host.split('.').all(|l| l.len() <= 63)
            && !host.starts_with('.')
            && !host.contains("..");
        if !ok {
            return None;
        }
    }
    Some((host.to_ascii_lowercase(), port))
}

/// Path and query for the request line: fragment dropped, unsafe bytes percent-encoded.
fn encode_target(tail: &str) -> Option<String> {
    let tail = tail.split('#').next().unwrap_or("");
    if tail.contains('\\') {
        return None;
    }
    let mut out = String::with_capacity(tail.len() + 1);
    if !tail.starts_with('/') {
        out.push('/');
    }
    for &b in tail.as_bytes() {
        match b {
            b' ' | b'"' | b'<' | b'>' | b'`' | b'{' | b'}' | b'|' | b'^' | 0x80.. => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push('%');
                out.push(HEX[usize::from(b >> 4)] as char);
                out.push(HEX[usize::from(b & 15)] as char);
            }
            _ => out.push(b as char),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_urls() {
        let u = Url::parse("HTTPS://Example.COM/a/b.png?x=1#frag").unwrap();
        assert!(u.https);
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, 443);
        assert_eq!(u.target, "/a/b.png?x=1");
        assert_eq!(u.to_string(), "https://example.com/a/b.png?x=1");
        let u = Url::parse("http://127.0.0.1:8080").unwrap();
        assert_eq!((u.port, u.target.as_str()), (8080, "/"));
        let u = Url::parse("http://[::1]:81/x").unwrap();
        assert_eq!(u.connect_host(), "::1");
        assert_eq!(u.to_string(), "http://[::1]:81/x");
        let u = Url::parse("https://e.com/ä b.png").unwrap();
        assert_eq!(u.target, "/%C3%A4%20b.png");
    }

    #[test]
    fn rejects_unsafe_urls() {
        for bad in [
            "ftp://e.com/x",
            "file:///C:/x.png",
            "javascript:alert(1)",
            "https://user:pass@e.com/x",
            "https://user@e.com/x",
            "https://e.com/x\r\nCookie: a",
            "https://e.com/\tx",
            "https://e.com\\x",
            "https:///x",
            "https://e.com:0/",
            "https://e.com:65536/",
            "https://e.com:-1/",
            "https://e%2ecom/",
            "https://exämple.com/",
            "https://e..com/",
            "https://[zz]/",
            "//e.com/x",
            "e.com/x",
        ] {
            assert_eq!(Url::parse(bad), None, "{bad}");
        }
        let long = format!("https://e.com/{}", "a".repeat(MAX_URL_LEN));
        assert_eq!(Url::parse(&long), None);
    }

    #[test]
    fn joins_locations() {
        let base = Url::parse("https://e.com:444/a/b.png?q").unwrap();
        let j = |l: &str| base.join(l).map(|u| u.to_string());
        assert_eq!(j("c.png").as_deref(), Some("https://e.com:444/a/c.png"));
        assert_eq!(j("/c.png").as_deref(), Some("https://e.com:444/c.png"));
        assert_eq!(j("?z").as_deref(), Some("https://e.com:444/a/b.png?z"));
        assert_eq!(j("//o.org/p").as_deref(), Some("https://o.org/p"));
        assert_eq!(j("http://o.org/p").as_deref(), Some("http://o.org/p"));
        assert_eq!(j("file:///etc/passwd"), None);
        assert_eq!(j("data:image/png;base64,AA"), None);
        assert_eq!(j("javascript:x"), None);
    }
}
