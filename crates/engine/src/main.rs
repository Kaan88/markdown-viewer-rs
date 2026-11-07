//! `mdview-engine`: one job at a time over stdin/stdout, started by the viewer only when a block
//! needs it and left alone when the viewer stops asking. It exits after 10 s without a request, so
//! its memory returns to the system.

use mdview_engine_proto::{
    KIND_MATH, KIND_SVG, Reply, Request, STATUS_BAD_REQUEST, STATUS_FAILED, read_math_request,
    read_request, write_reply,
};
use std::io::{self, BufReader, BufWriter};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// The plan's idle exit: the viewer may keep the process between blocks, but not forever.
const IDLE: Duration = Duration::from_secs(10);
/// The plan's job deadline is 5 s; the engine starts a watchdog for the one job it is given.
const MAX_PIXELS: u64 = 24_000_000;

fn main() {
    // A deadline self-test hook: the client's timeout test starts the engine with this set, so it
    // accepts no jobs. It is otherwise inert and costs one environment lookup.
    if std::env::var_os("MDV_ENGINE_STALL").is_some() {
        std::thread::sleep(Duration::from_secs(60));
    }
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

    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    let options = mdview_engine::options();
    loop {
        let request = match receiver.recv_timeout(IDLE) {
            Ok(Some(request)) => request,
            Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => break,
        };
        let reply = handle(&request, &options);
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
        _ => Reply::failed(STATUS_BAD_REQUEST),
    }
}
