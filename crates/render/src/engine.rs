//! The client side of `mdview-engine`: one shared, lazy engine process.
//!
//! The first SVG starts the engine; later jobs reuse it, so the process keeps its font database
//! warm. The engine exits by itself after 10 s without a job, and a failed write or a per-job
//! deadline kills the process and forgets it, so the next job starts a fresh one. A broken or slow
//! rasterizer therefore costs a placeholder, never the viewer.
//!
//! The binary is looked for beside this executable or in a workspace `target/release` above it;
//! `MDV_ENGINE` overrides the search.

use crate::image::{DecodedImage, MAX_PIXELS};
use mdview_engine_proto::{
    KIND_MATH, KIND_MERMAID, KIND_SVG, MAX_REPLY, Request, STATUS_OK, math_payload,
    mermaid_payload, read_reply, write_request,
};
use std::io;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// The plan's per-job deadline. A slow SVG is a placeholder, not a frozen window.
const DEADLINE: Duration = Duration::from_secs(5);

/// One retry covers the engine having exited between jobs (its idle timeout): the write fails
/// immediately and a fresh process takes the job. A deadline does not retry, because a file that
/// hangs the rasterizer would hang it again.
const ATTEMPTS: usize = 2;

/// The live engine, if there is one. The worker thread owns the pipes; this keeps the sender and a
/// handle for killing the process.
static SHARED: Mutex<Option<Handle>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static STARTS: AtomicU64 = AtomicU64::new(0);

struct Job {
    kind: u32,
    payload: Vec<u8>,
    reply: Sender<Option<Reply>>,
}

type Reply = mdview_engine_proto::Reply;

#[derive(Clone)]
struct Handle {
    id: u64,
    tx: Sender<Job>,
    child: Arc<Mutex<Child>>,
}

fn shared() -> MutexGuard<'static, Option<Handle>> {
    SHARED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts one engine and its worker thread.
fn connect() -> Option<Handle> {
    let engine = engine_path()?;
    let mut command = Command::new(engine);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // The engine is a console program. Started from a GUI shell without this, Windows gives it a
    // new console, and with Windows Terminal as the default terminal that handoff can take longer
    // than the 5 s deadline, so the first diagram on a page became a placeholder.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().ok()?;
    let stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let child = Arc::new(Mutex::new(child));
    let worker_child = child.clone();
    let (tx, jobs) = mpsc::channel();
    std::thread::spawn(move || worker(stdin, stdout, worker_child, jobs));
    STARTS.fetch_add(1, Ordering::Relaxed);
    Some(Handle {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        tx,
        child,
    })
}

/// Serves jobs in order over one pipe pair. On the first protocol or pipe error it answers `None`
/// and exits, which also closes stdin so the engine can go.
fn worker(
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    child: Arc<Mutex<Child>>,
    jobs: mpsc::Receiver<Job>,
) {
    let mut stdout = std::io::BufReader::new(stdout);
    while let Ok(job) = jobs.recv() {
        let result = (|| -> io::Result<Reply> {
            write_request(
                &mut stdin,
                &Request {
                    kind: job.kind,
                    payload: job.payload,
                },
            )?;
            read_reply(&mut stdout)
        })();
        let ok = result.is_ok();
        let _ = job.reply.send(result.ok());
        if !ok {
            break;
        }
    }
    drop(stdin);
    // Reap the process we were talking to, whether it exited on its own or was killed.
    if let Ok(mut child) = child.lock() {
        let _ = child.wait();
    }
}

fn kill(handle: &Handle) {
    if let Ok(mut child) = handle.child.lock() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Forgets the shared engine, but only if it is still the one this caller was using: another
/// thread may already have replaced it.
fn invalidate(handle: &Handle) {
    let mut slot = shared();
    if slot.as_ref().is_some_and(|current| current.id == handle.id) {
        *slot = None;
    }
}

/// Renders SVG bytes through the shared engine. `None` when the engine is missing, times out,
/// fails, or returns something the image cache cannot use.
pub fn render_svg(bytes: &[u8]) -> Option<DecodedImage> {
    let reply = submit(KIND_SVG, bytes.to_vec())?;
    decode(reply)
}

/// Renders a TeX formula. `font_size` is device pixels per em; `display` asks for display style,
/// otherwise inline text style. `color` is RGB, because the theme's text colour is the formula's.
pub fn render_math(
    source: &str,
    display: bool,
    font_size: f32,
    color: [u8; 3],
) -> Option<DecodedImage> {
    if !font_size.is_finite() || font_size < 4.0 || font_size > 512.0 {
        return None;
    }
    let reply = submit(KIND_MATH, math_payload(font_size, display, color, source))?;
    decode(reply)
}

/// Renders a Mermaid diagram. `scale` is device pixels per SVG unit; the renderer asks for twice
/// its own scale and halves the box, as it does for math.
pub fn render_mermaid(source: &str, scale: f32, dark: bool) -> Option<DecodedImage> {
    if !scale.is_finite() || scale < 0.1 || scale > 8.0 {
        return None;
    }
    let reply = submit(KIND_MERMAID, mermaid_payload(scale, dark, source))?;
    decode(reply)
}

fn submit(kind: u32, payload: Vec<u8>) -> Option<mdview_engine_proto::Reply> {
    for _ in 0..ATTEMPTS {
        let handle = {
            let mut slot = shared();
            if slot.is_none() {
                *slot = connect();
            }
            slot.clone()?
        };
        let (sender, receiver) = mpsc::channel();
        let job = Job {
            kind,
            payload: payload.clone(),
            reply: sender,
        };
        if handle.tx.send(job).is_err() {
            // The worker is gone; the next attempt starts a new engine.
            invalidate(&handle);
            continue;
        }
        match receiver.recv_timeout(DEADLINE) {
            Ok(Some(reply)) => return Some(reply),
            // An engine that died between jobs, or one the worker lost: retry once.
            Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                invalidate(&handle);
            }
            Err(RecvTimeoutError::Timeout) => {
                kill(&handle);
                invalidate(&handle);
                return None;
            }
        }
    }
    None
}

fn decode(reply: Reply) -> Option<DecodedImage> {
    if reply.status != STATUS_OK
        || reply.width == 0
        || reply.height == 0
        || reply.rgba.len() as u64 > MAX_REPLY
        || reply.rgba.len() != reply.width as usize * reply.height as usize * 4
        || reply.width as u64 * reply.height as u64 > MAX_PIXELS
    {
        return None;
    }
    Some(DecodedImage {
        width: reply.width,
        height: reply.height,
        baseline: reply.baseline,
        rgba: reply.rgba,
    })
}

/// The engine binary: `MDV_ENGINE`, a sibling of this executable, or the workspace target beside
/// an ancestor (the Qt shell builds into its own `bins/mdv-qt/target/release`).
fn engine_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MDV_ENGINE") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let mut directory = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let name = if cfg!(windows) {
        "mdview-engine.exe"
    } else {
        "mdview-engine"
    };
    for _ in 0..6 {
        let sibling = directory.join(name);
        if sibling.is_file() {
            return Some(sibling);
        }
        let target = directory.join("target").join("release").join(name);
        if target.is_file() {
            return Some(target);
        }
        if !directory.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
pub(crate) fn engine_starts() -> u64 {
    STARTS.load(Ordering::Relaxed)
}

/// Stops the shared engine, for tests that need a known start.
#[cfg(test)]
fn reset_engine() {
    let handle = shared().take();
    if let Some(handle) = handle {
        kill(&handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Engine tests share a process and a global slot, so they cannot run concurrently.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn badge() -> &'static [u8] {
        include_bytes!("../../../fixtures/assets/badge.svg")
    }

    #[test]
    fn a_missing_engine_is_not_an_error() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // When the workspace build is present a garbage job fails cleanly; when it is absent the
        // search returns nothing. Either way there is no panic and no image.
        assert!(render_svg(b"not an svg").is_none());
    }

    #[test]
    fn a_formula_comes_back_with_a_baseline() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if engine_path().is_none() {
            eprintln!("skipping: no mdview-engine beside the test binary");
            return;
        }
        let image = render_math("x_i^2 + 1", false, 32.0, [0, 0, 0]).expect("the formula renders");
        assert!(image.width > 10 && image.height > 10);
        assert_eq!(
            image.rgba.len(),
            image.width as usize * image.height as usize * 4
        );
        assert!(
            image.baseline > 0 && image.baseline <= image.height,
            "baseline {} in {} px",
            image.baseline,
            image.height
        );
        assert!(
            image.rgba.chunks_exact(4).any(|p| p[3] > 0),
            "the raster has ink"
        );
    }

    #[test]
    fn a_diagram_comes_back() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if engine_path().is_none() {
            eprintln!("skipping: no mdview-engine beside the test binary");
            return;
        }
        let image = render_mermaid("flowchart LR\n A[Start] --> B[Done]", 2.0, false)
            .expect("the diagram renders");
        assert!(image.width > 100 && image.height > 30);
        assert!(
            image.rgba.chunks_exact(4).any(|p| p[3] > 0),
            "the raster has ink"
        );
    }

    #[test]
    fn two_jobs_share_one_engine() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if engine_path().is_none() {
            eprintln!("skipping: no mdview-engine beside the test binary");
            return;
        }
        reset_engine();
        let before = engine_starts();
        let first = Instant::now();
        let image = render_svg(badge()).expect("the first job renders");
        let first_ms = first.elapsed().as_secs_f64() * 1000.0;
        assert_eq!((image.width, image.height), (180, 28));
        let second = Instant::now();
        let image = render_svg(badge()).expect("the second job renders");
        let second_ms = second.elapsed().as_secs_f64() * 1000.0;
        assert_eq!((image.width, image.height), (180, 28));
        assert_eq!(
            engine_starts() - before,
            1,
            "both jobs used one engine process"
        );
        eprintln!("engine: first job {first_ms:.1} ms, second {second_ms:.1} ms, one start");
        assert!(
            second_ms < first_ms,
            "the reused engine should skip startup: {first_ms:.1} vs {second_ms:.1} ms"
        );
    }

    #[test]
    #[ignore = "waits out the engine's 10 s idle window"]
    fn an_idle_engine_is_replaced_by_the_next_job() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if engine_path().is_none() {
            eprintln!("skipping: no mdview-engine beside the test binary");
            return;
        }
        reset_engine();
        render_svg(badge()).expect("the engine starts for the first job");
        let before = engine_starts();
        // The engine exits after ten seconds without a request; its stdin closes under the worker.
        std::thread::sleep(Duration::from_secs(11));
        let started = Instant::now();
        let image = render_svg(badge()).expect("a fresh engine serves the job after idle");
        assert_eq!((image.width, image.height), (180, 28));
        assert_eq!(
            engine_starts() - before,
            1,
            "the idle process was replaced exactly once"
        );
        eprintln!(
            "engine: after 11 s idle the next job took {:.1} ms and started one process",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }

    #[test]
    #[ignore = "waits out the 5 s deadline"]
    fn a_stalled_engine_is_killed_and_the_next_job_starts_fresh() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if engine_path().is_none() {
            eprintln!("skipping: no mdview-engine beside the test binary");
            return;
        }
        reset_engine();
        // The engine honours MDV_ENGINE_STALL before it reads a job, which is how this test hangs
        // one without a special job kind in the protocol.
        unsafe { std::env::set_var("MDV_ENGINE_STALL", "1") };
        let started = Instant::now();
        let stalled = render_svg(badge());
        let waited = started.elapsed();
        unsafe { std::env::remove_var("MDV_ENGINE_STALL") };
        assert!(stalled.is_none(), "a stalled job becomes a placeholder");
        assert!(
            waited >= Duration::from_secs(4) && waited < Duration::from_secs(8),
            "the deadline took {waited:?}"
        );
        let started = Instant::now();
        let image = render_svg(badge()).expect("a fresh engine serves the next job");
        let respawned = started.elapsed();
        assert_eq!((image.width, image.height), (180, 28));
        eprintln!(
            "engine: stalled job gave up after {:.1} ms, respawn took {:.1} ms",
            waited.as_secs_f64() * 1000.0,
            respawned.as_secs_f64() * 1000.0
        );
    }
}
