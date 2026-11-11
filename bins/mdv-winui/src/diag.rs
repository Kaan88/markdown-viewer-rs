//! Startup checkpoints, `--measure` output and the message box for failures.
use crate::bindings::*;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use windows_core::*;

struct Options {
    measure: Option<PathBuf>,
    trace: Option<Mutex<std::fs::File>>,
}

static OPTIONS: OnceLock<Options> = OnceLock::new();

fn beside_exe(name: &str) -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(name));
    path.set_file_name(name);
    path
}

pub fn init() {
    let mut measure = None;
    let mut want_trace = false;
    for arg in std::env::args().skip(1) {
        if arg == "--measure" {
            measure = Some(beside_exe("measure.log"));
        } else if let Some(file) = arg.strip_prefix("--measure=") {
            measure = Some(PathBuf::from(file));
        } else if arg == "--trace" {
            want_trace = true;
        }
    }
    let want_trace = want_trace || measure.is_some();
    let trace_file = want_trace
        .then(|| std::fs::File::create(beside_exe("mdview.trace.log")).ok())
        .flatten()
        .map(Mutex::new);
    _ = OPTIONS.set(Options {
        measure,
        trace: trace_file,
    });
    std::panic::set_hook(Box::new(|info| {
        // Release builds abort after this hook, so this is the only chance to say anything.
        fatal_text(&format!("panic: {info}"));
    }));
    if measuring() {
        // A build that never reaches a frame must still end the process and say so.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(45));
            measure_line("result=timeout");
            std::process::exit(2);
        });
    }
}

pub fn measuring() -> bool {
    OPTIONS.get().is_some_and(|o| o.measure.is_some())
}

/// Milliseconds since the OS created this process, so DLL loading before main is included.
pub fn since_process_start_ms() -> f64 {
    let filetime = |t: FILETIME| (t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64;
    unsafe {
        let (mut created, mut exit, mut kernel, mut user, mut now) = Default::default();
        _ = GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exit,
            &mut kernel,
            &mut user,
        );
        GetSystemTimePreciseAsFileTime(&mut now);
        (filetime(now).wrapping_sub(filetime(created))) as f64 / 10_000.0
    }
}

pub fn trace(message: &str) {
    if let Some(Options {
        trace: Some(file), ..
    }) = OPTIONS.get()
    {
        if let Ok(mut file) = file.lock() {
            _ = writeln!(file, "{:10.1} ms  {message}", since_process_start_ms());
        }
    }
}

pub fn fail(what: &str, error: &Error) {
    trace(&format!("FAILED {what}: {:#010x} {error}", error.code().0));
}

/// Reports a failure the app cannot continue from, then ends the process with exit code 1, so a
/// startup failure never leaves an invisible process behind.
pub fn fatal(what: &str, error: &Error) -> ! {
    fatal_text(&format!("{what}: {:#010x} {error}", error.code().0))
}

pub fn fatal_text(text: &str) -> ! {
    trace(&format!("FATAL {text}"));
    if measuring() {
        let flat: String = text
            .chars()
            .map(|c| if c.is_whitespace() { '_' } else { c })
            .collect();
        measure_line(&format!("result=failed reason={flat}"));
    } else {
        message_box(&format!("mdview failed: {text}"));
    }
    std::process::exit(1);
}

/// Reports a failure the user can retry from (a file that will not open, say) without exiting.
pub fn error_box(text: &str) {
    trace(&format!("ERROR {text}"));
    message_box(&format!("mdview: {text}"));
}

/// Records what the manifest actually gave us: 2 means per-monitor.
pub fn trace_dpi_awareness() {
    let awareness = unsafe { GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) };
    trace(&format!("DPI awareness={awareness}"));
}

pub fn measure_line(fields: &str) {
    trace(&format!("measure: {fields}"));
    let Some(path) = OPTIONS.get().and_then(|o| o.measure.as_ref()) else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        _ = writeln!(file, "mode=framework {fields}");
    }
}

/// An information dialog (About, Keyboard shortcuts), owned by the app's window.
pub fn info_box(title: &str, text: &str) {
    let text = HSTRING::from(text);
    let title = HSTRING::from(title);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            0x40, // MB_ICONINFORMATION
        );
    }
}

fn message_box(text: &str) {
    let text = HSTRING::from(text);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            PCWSTR(text.as_ptr()),
            w!("mdview"),
            MB_ICONERROR as u32,
        );
    }
}
