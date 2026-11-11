//! `mdview-engine`: one job at a time over stdin/stdout, started by the viewer only when a block
//! needs it and left alone when the viewer stops asking. It exits after 10 s without a request, so
//! its memory returns to the system.

use mdview_engine_proto::{
    KIND_MATH, KIND_MERMAID, KIND_SVG, KIND_SVG_AT, Reply, Request, STATUS_BAD_REQUEST,
    STATUS_FAILED, read_math_request, read_mermaid_request, read_request, read_svg_at_request,
    write_reply,
};
use std::io::{self, BufReader, BufWriter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// The plan's idle exit: the viewer may keep the process between blocks, but not forever.
const IDLE: Duration = Duration::from_secs(10);
/// The plan's job deadline is 5 s and the viewer kills the engine when it passes. The engine also
/// ends itself a little later, so a job that hangs after the viewer itself died (and so can no
/// longer kill it) does not leave an orphan process behind. Between jobs, a closed stdin ends it.
const DEADLINE: Duration = Duration::from_secs(6);
const MAX_PIXELS: u64 = 24_000_000;

fn main() {
    // A deadline self-test hook: with this set, every job hangs, which is how the client's timeout
    // test and the watchdog test stand in for a parser stuck on bad input. It is otherwise inert
    // and costs one environment lookup.
    let stall = std::env::var_os("MDV_ENGINE_STALL").is_some();
    let (sender, receiver) = mpsc::channel::<Option<Request>>();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = BufReader::new(stdin.lock());
        loop {
            match read_request(&mut input) {
                Ok(Some(request)) => {
                    if sender.send(Some(request)).is_err() {
                        break;
                    }
                }
                Ok(None) | Err(_) => {
                    let _ = sender.send(None);
                    break;
                }
            }
        }
    });

    // When the current job started, in ms since `epoch` plus one; 0 means idle.
    static BUSY_SINCE: AtomicU64 = AtomicU64::new(0);
    let epoch = Instant::now();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let since = BUSY_SINCE.load(Ordering::Relaxed);
            if since != 0
                && epoch.elapsed().as_millis() as u64 >= since - 1 + DEADLINE.as_millis() as u64
            {
                std::process::exit(3);
            }
        }
    });

    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    let options = mdview_engine::options();
    loop {
        let request = match receiver.recv_timeout(IDLE) {
            Ok(Some(request)) => request,
            Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => break,
        };
        BUSY_SINCE.store(epoch.elapsed().as_millis() as u64 + 1, Ordering::Relaxed);
        if stall {
            std::thread::sleep(Duration::from_secs(60));
        }
        let reply = handle(&request, &options);
        BUSY_SINCE.store(0, Ordering::Relaxed);
        if write_reply(&mut output, &reply).is_err() {
            break;
        }
    }
}

fn handle(request: &Request, options: &resvg::usvg::Options) -> Reply {
    match request.kind {
        KIND_SVG => match mdview_engine::render_svg(&request.payload, MAX_PIXELS, options) {
            Ok(image) => Reply::ok(image.width, image.height, image.rgba),
            Err(_) => Reply::failed(STATUS_FAILED),
        },
        KIND_SVG_AT => {
            let Some((scale, svg)) = read_svg_at_request(&request.payload) else {
                return Reply::failed(STATUS_BAD_REQUEST);
            };
            match mdview_engine::render_svg_at(svg, scale, MAX_PIXELS, options) {
                Ok(image) => Reply::ok(image.width, image.height, image.rgba),
                Err(_) => Reply::failed(STATUS_FAILED),
            }
        }
        KIND_MATH => {
            let Some(math) = read_math_request(&request.payload) else {
                return Reply::failed(STATUS_BAD_REQUEST);
            };
            match mdview_engine::render_math(
                math.source,
                math.display,
                math.font_size,
                math.color,
                MAX_PIXELS,
                options,
            ) {
                Ok(image) => {
                    Reply::ok_with_baseline(image.width, image.height, image.baseline, image.rgba)
                }
                Err(_) => Reply::failed(STATUS_FAILED),
            }
        }
        KIND_MERMAID => {
            let Some(mermaid) = read_mermaid_request(&request.payload) else {
                return Reply::failed(STATUS_BAD_REQUEST);
            };
            match mdview_engine::render_mermaid(
                mermaid.source,
                mermaid.scale,
                mermaid.dark,
                MAX_PIXELS,
                options,
            ) {
                Ok(image) => Reply::ok(image.width, image.height, image.rgba),
                Err(_) => Reply::failed(STATUS_FAILED),
            }
        }
        _ => Reply::failed(STATUS_BAD_REQUEST),
    }
}
