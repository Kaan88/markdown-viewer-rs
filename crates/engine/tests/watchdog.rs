//! The engine ends itself when a job hangs, even if nobody is left to kill it.

use mdview_engine_proto::{KIND_SVG, Request, write_request};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
#[ignore = "waits out the engine's 6 s watchdog"]
fn a_hung_job_ends_the_process() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdview-engine"))
        .env("MDV_ENGINE_STALL", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the engine");
    let mut stdin = child.stdin.take().unwrap();
    write_request(
        &mut stdin,
        &Request {
            kind: KIND_SVG,
            payload: b"<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/>".to_vec(),
        },
    )
    .unwrap();
    stdin.flush().unwrap();
    // Keep stdin open, as a viewer that crashed mid-job would leave it from the engine's side
    // only if something else held the pipe; the watchdog must not depend on seeing EOF.
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(15) {
            child.kill().unwrap();
            panic!("the engine was still running after 15 s");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let waited = started.elapsed();
    drop(stdin);
    assert_eq!(status.code(), Some(3), "the watchdog's exit code");
    assert!(
        waited >= Duration::from_secs(5) && waited < Duration::from_secs(9),
        "the watchdog took {waited:?}"
    );
}
