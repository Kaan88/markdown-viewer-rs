//! Tests against a plain HTTP/1.1 server on 127.0.0.1, run inside each test. The network tests
//! need the WinHTTP transport, so they are Windows-only; the stub is checked elsewhere.

use super::*;

#[test]
fn redirect_rules() {
    let https = Url::parse("https://a.org/x/y.png").unwrap();
    assert_eq!(
        redirect_target(&https, Some("http://a.org/y.png")),
        Err(Error::InsecureRedirect)
    );
    assert_eq!(
        redirect_target(&https, Some("file:///C:/Windows/win.ini")),
        Err(Error::BadRedirect)
    );
    assert_eq!(redirect_target(&https, None), Err(Error::BadRedirect));
    assert_eq!(
        redirect_target(&https, Some("z.png")).unwrap().to_string(),
        "https://a.org/x/z.png"
    );
    let http = Url::parse("http://a.org/").unwrap();
    assert!(
        redirect_target(&http, Some("https://b.org/"))
            .unwrap()
            .https
    );
}

#[test]
fn validators_must_be_header_safe() {
    assert!(header_safe("\"abc\""));
    assert!(header_safe("Wed, 21 Oct 2015 07:28:00 GMT"));
    assert!(!header_safe(""));
    assert!(!header_safe("a\r\nCookie: x"));
    assert!(!header_safe(&"a".repeat(MAX_VALIDATOR_LEN + 1)));
    assert_eq!(parse_content_length("123"), Some(123));
    assert_eq!(parse_content_length("-1"), None);
    assert_eq!(parse_content_length("1 2"), None);
}

#[test]
fn unacceptable_urls_fail_without_a_thread() {
    let f = Fetcher::new(None, || {});
    for bad in [
        "file:///C:/x.png",
        "https://u:p@e.com/x.png",
        "data:image/png;base64,AA",
    ] {
        assert_eq!(f.request(bad), State::Failed);
    }
    assert_eq!(f.worker_count(), 0);
}

#[cfg(windows)]
mod net {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    type Handler = dyn Fn(&str, &str, &mut TcpStream) + Send + Sync;

    /// A one-file HTTP server: each connection gets its own thread, reads one request head and
    /// hands the path, the whole head and the stream to the handler.
    struct Server {
        port: u16,
        connections: Arc<AtomicUsize>,
        heads: Arc<Mutex<Vec<String>>>,
    }

    impl Server {
        fn start(handler: impl Fn(&str, &str, &mut TcpStream) + Send + Sync + 'static) -> Server {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let connections = Arc::new(AtomicUsize::new(0));
            let heads = Arc::new(Mutex::new(Vec::new()));
            let handler: Arc<Handler> = Arc::new(handler);
            let (c, h) = (connections.clone(), heads.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    c.fetch_add(1, Ordering::SeqCst);
                    let (handler, heads) = (handler.clone(), h.clone());
                    std::thread::spawn(move || {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let mut head = Vec::new();
                        let mut byte = [0u8; 1];
                        while !head.ends_with(b"\r\n\r\n") && head.len() < 16 * 1024 {
                            match stream.read(&mut byte) {
                                Ok(1) => head.push(byte[0]),
                                _ => return,
                            }
                        }
                        let head = String::from_utf8_lossy(&head).into_owned();
                        let path = head.split(' ').nth(1).unwrap_or("").to_string();
                        heads.lock().unwrap().push(head.clone());
                        handler(&path, &head, &mut stream);
                    });
                }
            });
            Server {
                port,
                connections,
                heads,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }

        fn connections(&self) -> usize {
            self.connections.load(Ordering::SeqCst)
        }

        fn last_head(&self) -> String {
            self.heads
                .lock()
                .unwrap()
                .last()
                .cloned()
                .unwrap_or_default()
        }
    }

    fn respond(stream: &mut TcpStream, status: &str, headers: &[&str], body: &[u8]) {
        let mut out = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
        for h in headers {
            out.push_str(h);
            out.push_str("\r\n");
        }
        out.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
        let _ = stream.write_all(out.as_bytes());
        let _ = stream.write_all(body);
    }

    fn png(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 7 + 3) as u8).collect()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mdview-fetch-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A fetcher whose `wake` counts calls and signals a channel.
    fn fetcher(
        dir: Option<PathBuf>,
        cfg: Config,
    ) -> (Fetcher, mpsc::Receiver<()>, Arc<AtomicUsize>) {
        let (tx, rx) = mpsc::channel();
        let wakes = Arc::new(AtomicUsize::new(0));
        let w = wakes.clone();
        let tx = Mutex::new(tx);
        let f = Fetcher::with_config(dir, cfg, move || {
            w.fetch_add(1, Ordering::SeqCst);
            let _ = tx.lock().unwrap().send(());
        });
        (f, rx, wakes)
    }

    /// Waits for `wake`, then returns `take_completed`.
    fn completed(f: &Fetcher, rx: &mpsc::Receiver<()>) -> Vec<String> {
        rx.recv_timeout(Duration::from_secs(20))
            .expect("wake was not called");
        f.take_completed()
    }

    fn get(url: &str, cfg: &Config) -> Result<Downloaded, Error> {
        let session = Session::new(USER_AGENT).unwrap();
        download(&session, &Url::parse(url).unwrap(), None, cfg)
    }

    #[test]
    fn success_sends_only_safe_headers_and_wakes() {
        let body = png(3000);
        let b = body.clone();
        let server =
            Server::start(move |_, _, s| respond(s, "200 OK", &["Content-Type: image/png"], &b));
        let (f, rx, wakes) = fetcher(None, Config::default());
        let url = server.url("/a.png");
        assert_eq!(f.request(&url), State::Pending);
        assert_eq!(completed(&f, &rx), vec![url.clone()]);
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert_eq!(f.request(&url), State::Ready(body.into()));
        assert!(f.take_completed().is_empty());

        let head = server.last_head().to_ascii_lowercase();
        assert!(head.starts_with("get /a.png http/1.1\r\n"), "{head}");
        assert!(head.contains("\r\nuser-agent: mdview\r\n"), "{head}");
        assert!(head.contains("\r\naccept: image/*\r\n"), "{head}");
        for absent in ["cookie", "referer", "authorization", "accept-encoding"] {
            assert!(
                !head.contains(&format!("\r\n{absent}:")),
                "{absent} sent: {head}"
            );
        }
        assert_eq!(server.connections(), 1);
    }

    #[test]
    fn not_found_fails_and_is_not_hammered_until_retry() {
        let server = Server::start(|_, _, s| respond(s, "404 Not Found", &[], b"no"));
        let (f, rx, _) = fetcher(None, Config::default());
        let url = server.url("/missing.png");
        assert_eq!(f.request(&url), State::Pending);
        assert_eq!(completed(&f, &rx), vec![url.clone()]);
        assert_eq!(f.request(&url), State::Failed);
        assert_eq!(f.request(&url), State::Failed);
        assert_eq!(server.connections(), 1);
        f.retry_failed();
        assert_eq!(f.request(&url), State::Pending);
        assert_eq!(completed(&f, &rx), vec![url.clone()]);
        assert_eq!(f.request(&url), State::Failed);
        assert_eq!(server.connections(), 2);
    }

    #[test]
    fn failure_is_retried_after_the_delay() {
        let server = Server::start(|_, _, s| respond(s, "500 Oops", &[], b""));
        let cfg = Config {
            retry_after: Duration::from_millis(200),
            ..Config::default()
        };
        let (f, rx, _) = fetcher(None, cfg);
        let url = server.url("/x.png");
        f.request(&url);
        completed(&f, &rx);
        assert_eq!(f.request(&url), State::Failed);
        std::thread::sleep(Duration::from_millis(250));
        assert_eq!(f.request(&url), State::Pending);
        completed(&f, &rx);
        assert_eq!(server.connections(), 2);
    }

    /// `/r/N` redirects to `/r/N-1` until `/r/0`, which is the image.
    fn redirect_server() -> Server {
        Server::start(|path, _, s| {
            let n: usize = path.trim_start_matches("/r/").parse().unwrap_or(0);
            match path {
                "/loop" => respond(s, "302 Found", &["Location: /loop"], b""),
                "/file" => respond(
                    s,
                    "302 Found",
                    &["Location: file:///C:/Windows/win.ini"],
                    b"",
                ),
                "/ftp" => respond(s, "301 Moved", &["Location: ftp://127.0.0.1/x"], b""),
                "/none" => respond(s, "302 Found", &[], b""),
                _ if n == 0 => respond(s, "200 OK", &[], b"img"),
                _ => {
                    let loc = format!("Location: /r/{}", n - 1);
                    let status = [
                        "301 Moved",
                        "302 Found",
                        "303 See Other",
                        "307 Temporary",
                        "308 Permanent",
                    ][n % 5];
                    respond(s, status, &[&loc], b"");
                }
            }
        })
    }

    #[test]
    fn follows_up_to_five_redirects() {
        let server = redirect_server();
        let cfg = Config::default();
        match get(&server.url("/r/5"), &cfg) {
            Ok(Downloaded::Body(e)) => assert_eq!(e.body, b"img"),
            _ => panic!("five redirects should be followed"),
        }
        assert_eq!(server.connections(), 6);
        assert!(matches!(
            get(&server.url("/r/6"), &cfg),
            Err(Error::TooManyRedirects)
        ));
        assert!(matches!(
            get(&server.url("/loop"), &cfg),
            Err(Error::TooManyRedirects)
        ));
        assert_eq!(server.connections(), 6 + 6 + 6);
    }

    #[test]
    fn redirects_to_other_schemes_fail() {
        let server = redirect_server();
        let cfg = Config::default();
        assert!(matches!(
            get(&server.url("/file"), &cfg),
            Err(Error::BadRedirect)
        ));
        assert!(matches!(
            get(&server.url("/ftp"), &cfg),
            Err(Error::BadRedirect)
        ));
        assert!(matches!(
            get(&server.url("/none"), &cfg),
            Err(Error::BadRedirect)
        ));
        assert_eq!(server.connections(), 3);
        let (f, rx, _) = fetcher(None, Config::default());
        let url = server.url("/file");
        assert_eq!(f.request(&url), State::Pending);
        completed(&f, &rx);
        assert_eq!(f.request(&url), State::Failed);
    }

    #[test]
    fn oversize_bodies_fail_with_and_without_content_length() {
        let server = Server::start(|path, _, s| {
            let body = png(5000);
            if path == "/declared" {
                respond(s, "200 OK", &[], &body);
            } else {
                // No Content-Length: the body runs until the connection closes.
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
                for chunk in body.chunks(500) {
                    let _ = s.write_all(chunk);
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        });
        let cfg = Config {
            max_bytes: 1000,
            ..Config::default()
        };
        assert!(matches!(
            get(&server.url("/declared"), &cfg),
            Err(Error::TooLarge)
        ));
        assert!(matches!(
            get(&server.url("/streamed"), &cfg),
            Err(Error::TooLarge)
        ));
        let roomy = Config {
            max_bytes: 5000,
            ..Config::default()
        };
        assert!(matches!(
            get(&server.url("/declared"), &roomy),
            Ok(Downloaded::Body(_))
        ));
        assert!(matches!(
            get(&server.url("/streamed"), &roomy),
            Ok(Downloaded::Body(_))
        ));
    }

    #[test]
    fn deadline_covers_the_whole_request() {
        let server = Server::start(|path, _, s| {
            if path == "/silent" {
                std::thread::sleep(Duration::from_secs(5));
                respond(s, "200 OK", &[], b"late");
            } else {
                // Headers at once, then one byte every 100 ms: no single read times out.
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n");
                for _ in 0..1000 {
                    if s.write_all(b"x").is_err() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });
        let cfg = Config {
            deadline: Duration::from_millis(400),
            ..Config::default()
        };
        for path in ["/silent", "/drip"] {
            let start = Instant::now();
            let r = get(&server.url(path), &cfg);
            let took = start.elapsed();
            assert!(matches!(r, Err(Error::Timeout)), "{path}: {:?}", r.err());
            assert!(took < Duration::from_millis(1000), "{path} took {took:?}");
        }
    }

    #[test]
    fn concurrent_requests_share_one_download() {
        let server = Server::start(|_, _, s| {
            std::thread::sleep(Duration::from_millis(300));
            respond(s, "200 OK", &[], b"shared");
        });
        let (f, rx, _) = fetcher(None, Config::default());
        let f = Arc::new(f);
        let url = server.url("/same.png");
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let (f, url) = (f.clone(), url.clone());
                std::thread::spawn(move || (0..8).map(|_| f.request(&url)).collect::<Vec<_>>())
            })
            .collect();
        for t in threads {
            assert!(t.join().unwrap().iter().all(|s| *s == State::Pending));
        }
        assert_eq!(completed(&f, &rx), vec![url.clone()]);
        assert_eq!(f.request(&url), State::Ready(Arc::from(&b"shared"[..])));
        assert_eq!(server.connections(), 1);
    }

    #[test]
    fn workers_exit_when_idle() {
        let server = Server::start(|_, _, s| respond(s, "200 OK", &[], b"i"));
        let cfg = Config {
            idle_exit: Duration::from_millis(200),
            ..Config::default()
        };
        let (f, rx, _) = fetcher(None, cfg);
        for i in 0..6 {
            f.request(&server.url(&format!("/{i}")));
        }
        assert!(f.worker_count() >= 1 && f.worker_count() <= MAX_WORKERS);
        let mut done = 0;
        while done < 6 {
            done += completed(&f, &rx).len();
        }
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(f.worker_count(), 0);
        // And a new request starts one again.
        assert_eq!(f.request(&server.url("/again")), State::Pending);
        assert_eq!(completed(&f, &rx).len(), 1);
    }

    #[test]
    fn disk_cache_serves_a_new_fetcher_without_a_connection() {
        let dir = temp_dir("hit");
        let server = Server::start(|_, _, s| respond(s, "200 OK", &[], b"cached-bytes"));
        let url = server.url("/c.png");
        {
            let (f, rx, _) = fetcher(Some(dir.clone()), Config::default());
            f.request(&url);
            completed(&f, &rx);
        }
        let (f, _rx, wakes) = fetcher(Some(dir.clone()), Config::default());
        assert_eq!(
            f.request(&url),
            State::Ready(Arc::from(&b"cached-bytes"[..]))
        );
        assert_eq!(server.connections(), 1);
        assert_eq!(wakes.load(Ordering::SeqCst), 0);
        // trim_memory drops the copy; the disk still has it.
        f.trim_memory();
        assert_eq!(
            f.request(&url),
            State::Ready(Arc::from(&b"cached-bytes"[..]))
        );
        assert_eq!(server.connections(), 1);
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .count();
        assert_eq!(leftovers, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn eviction_drops_the_least_recently_used() {
        let dir = temp_dir("evict");
        let server = Server::start(|_, _, s| respond(s, "200 OK", &[], &png(1000)));
        let urls: Vec<String> = ["/a", "/b", "/c", "/d"]
            .iter()
            .map(|p| server.url(p))
            .collect();
        let entry_size = {
            let (f, rx, _) = fetcher(Some(dir.clone()), Config::default());
            f.request(&urls[0]);
            completed(&f, &rx);
            let cache = f.inner.cache.as_ref().unwrap();
            std::fs::metadata(cache.file_for(&Url::parse(&urls[0]).unwrap().to_string()))
                .unwrap()
                .len()
        };
        // Room for three entries.
        let cfg = || Config {
            cache_cap: entry_size * 3 + entry_size / 2,
            ..Config::default()
        };
        let (f, rx, _) = fetcher(Some(dir.clone()), cfg());
        for url in &urls[1..3] {
            f.request(url);
            completed(&f, &rx);
            std::thread::sleep(Duration::from_millis(20));
        }
        // A new fetcher reads /a from disk, which makes it the most recently used.
        let (f, rx, _) = fetcher(Some(dir.clone()), cfg());
        assert!(matches!(f.request(&urls[0]), State::Ready(_)));
        std::thread::sleep(Duration::from_millis(20));
        f.request(&urls[3]);
        completed(&f, &rx);
        assert_eq!(server.connections(), 4);

        let (f, _rx, _) = fetcher(Some(dir.clone()), cfg());
        for i in [0, 2, 3] {
            assert!(
                matches!(f.request(&urls[i]), State::Ready(_)),
                "{i} should be cached"
            );
        }
        assert_eq!(server.connections(), 4);
        assert_eq!(
            f.request(&urls[1]),
            State::Pending,
            "/b was the least recently used"
        );
        let files = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(files, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_entries_are_ignored() {
        let dir = temp_dir("corrupt");
        let server = Server::start(|_, _, s| respond(s, "200 OK", &[], b"good-bytes"));
        let url = server.url("/x.png");
        let path = {
            let (f, rx, _) = fetcher(Some(dir.clone()), Config::default());
            f.request(&url);
            completed(&f, &rx);
            f.inner
                .cache
                .as_ref()
                .unwrap()
                .file_for(&Url::parse(&url).unwrap().to_string())
        };
        // Flip one body byte: lengths still agree, the checksum does not.
        let mut data = std::fs::read(&path).unwrap();
        let at = data.len() - 9;
        data[at] ^= 0xff;
        std::fs::write(&path, &data).unwrap();
        let (f, rx, _) = fetcher(Some(dir.clone()), Config::default());
        assert_eq!(f.request(&url), State::Pending);
        completed(&f, &rx);
        assert_eq!(f.request(&url), State::Ready(Arc::from(&b"good-bytes"[..])));
        assert_eq!(server.connections(), 2);
        // A truncated entry, as a crash without the rename would leave, is ignored the same way.
        let data = std::fs::read(&path).unwrap();
        std::fs::write(&path, &data[..data.len() / 2]).unwrap();
        let (f, rx, _) = fetcher(Some(dir.clone()), Config::default());
        assert_eq!(f.request(&url), State::Pending);
        completed(&f, &rx);
        assert_eq!(server.connections(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn expired_entries_are_revalidated() {
        let dir = temp_dir("revalidate");
        let server = Server::start(|path, head, s| {
            let lower = head.to_ascii_lowercase();
            let unchanged = match path {
                "/etag" => lower.contains("\r\nif-none-match: \"v1\"\r\n"),
                "/date" => {
                    lower.contains("\r\nif-modified-since: wed, 21 oct 2015 07:28:00 gmt\r\n")
                }
                _ => false,
            };
            if unchanged {
                respond(s, "304 Not Modified", &[], b"");
            } else {
                respond(
                    s,
                    "200 OK",
                    &[
                        "ETag: \"v1\"",
                        "Last-Modified: Wed, 21 Oct 2015 07:28:00 GMT",
                    ],
                    format!("body{path}").as_bytes(),
                );
            }
        });
        let stale = || Config {
            fresh_for: Duration::ZERO,
            ..Config::default()
        };
        for path in ["/etag", "/date"] {
            let url = server.url(path);
            let before = server.connections();
            let (f, rx, _) = fetcher(Some(dir.clone()), stale());
            f.request(&url);
            completed(&f, &rx);
            // Every entry is expired at once, so a new fetcher must ask again: 304 keeps it.
            let (f, rx, _) = fetcher(Some(dir.clone()), stale());
            assert_eq!(f.request(&url), State::Pending);
            completed(&f, &rx);
            let expected = format!("body{path}");
            assert_eq!(
                f.request(&url),
                State::Ready(Arc::from(expected.as_bytes()))
            );
            assert_eq!(server.connections(), before + 2);
            assert!(server.last_head().to_ascii_lowercase().contains("if-"));
        }
        // An expired entry is still served when the server is gone.
        let url = server.url("/etag");
        let dead = format!("http://127.0.0.1:{}/etag", {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        });
        let cache = DiskCache::new(dir.clone(), IMAGE_CACHE_BYTES, MAX_IMAGE_BYTES);
        let entry = cache.load(&Url::parse(&url).unwrap().to_string()).unwrap();
        cache.store(&Url::parse(&dead).unwrap().to_string(), &entry);
        let (f, rx, _) = fetcher(Some(dir.clone()), stale());
        assert_eq!(f.request(&dead), State::Pending);
        completed(&f, &rx);
        assert_eq!(f.request(&dead), State::Ready(Arc::from(&b"body/etag"[..])));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fresh_entry_is_not_revalidated() {
        let dir = temp_dir("fresh");
        let server = Server::start(|_, _, s| respond(s, "200 OK", &["ETag: \"v\""], b"f"));
        let url = server.url("/f.png");
        let (f, rx, _) = fetcher(Some(dir.clone()), Config::default());
        f.request(&url);
        completed(&f, &rx);
        drop(f);
        let (f, _, _) = fetcher(Some(dir.clone()), Config::default());
        assert!(matches!(f.request(&url), State::Ready(_)));
        assert_eq!(server.connections(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Proves TLS works through WinHTTP. Needs the internet: `cargo test -p mdview-fetch -- --ignored`.
    #[test]
    #[ignore]
    fn live_https_badge() {
        let (f, rx, _) = fetcher(None, Config::default());
        let url = "https://img.shields.io/badge/mdview-ok-green";
        assert_eq!(f.request(url), State::Pending);
        assert_eq!(completed(&f, &rx), vec![url.to_string()]);
        match f.request(url) {
            State::Ready(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                assert!(text.contains("<svg"), "not an SVG: {text:.200}");
                println!("live: {} bytes of SVG over https", bytes.len());
            }
            other => panic!("live fetch failed: {other:?}"),
        }
    }
}
