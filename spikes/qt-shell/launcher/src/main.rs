//! Qt-free entry: no Qt in DT_NEEDED. Tries the single-instance socket, and only
//! if nobody answers does it dlopen the Qt shell library.
//!   mdview-launcher --socket NAME --so PATH [shell args]
//!   mdview-launcher --listen NAME --secs N    (stands in for a running first instance)
use std::ffi::{c_char, c_int, c_void, CString};
use std::io::{Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};

extern "C" {
    fn dlopen(file: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(h: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}
const RTLD_NOW: c_int = 2;

fn opt(args: &[String], name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).cloned()
}

fn listen(name: &str, secs: u64) -> i32 {
    let addr = SocketAddr::from_abstract_name(name).unwrap();
    let l = UnixListener::bind_addr(&addr).unwrap();
    println!("LISTENING");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(secs));
        std::process::exit(0);
    });
    for s in l.incoming().flatten() {
        let mut s = s;
        let mut b = [0u8; 64];
        let _ = s.read(&mut b);
    }
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(name) = opt(&args, "--listen") {
        let secs = opt(&args, "--secs").unwrap().parse().unwrap();
        std::process::exit(listen(&name, secs));
    }
    if let Some(name) = opt(&args, "--socket") {
        let addr = SocketAddr::from_abstract_name(&name).unwrap();
        if let Ok(mut s) = UnixStream::connect_addr(&addr) {
            let _ = s.write_all(b"open\n");
            return; // Second launch: Qt never loaded.
        }
    }
    let so = CString::new(opt(&args, "--so").expect("--so")).unwrap();
    let entry = CString::new("mdv_shell_main").unwrap();
    unsafe {
        let h = dlopen(so.as_ptr(), RTLD_NOW);
        if h.is_null() {
            panic!("dlopen: {:?}", std::ffi::CStr::from_ptr(dlerror()));
        }
        let f = dlsym(h, entry.as_ptr());
        assert!(!f.is_null());
        let f: extern "C" fn() -> c_int = std::mem::transmute(f);
        std::process::exit(f());
    }
}
