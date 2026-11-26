//! Engine setup for Cargo test processes only. The production lookup stays unchanged.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

pub fn require_engine() {
    static READY: OnceLock<()> = OnceLock::new();
    READY.get_or_init(|| {
        let path = engine_path();
        assert!(path.is_absolute() && path.is_file(), "{path:?}");
        mdview_render::set_engine_path(path);
    });
}

fn engine_path() -> PathBuf {
    if let Some(path) = std::env::var_os("MDV_ENGINE") {
        return path.into();
    }
    let exe = std::env::current_exe().expect("locate the Cargo test executable");
    let deps = exe.parent().expect("Cargo test executable directory");
    assert!(
        deps.ends_with("deps"),
        "expected Cargo's deps directory: {exe:?}"
    );
    let profile = deps.parent().expect("Cargo test profile directory");
    let engine = format!("mdview-engine{}", std::env::consts::EXE_SUFFIX);
    let sibling = profile.join(&engine);
    if sibling.is_file() {
        return sibling;
    }

    // Cargo does not build binaries from dependencies for `cargo test`. Use a separate
    // target directory so the nested build cannot wait for the outer build's lock.
    let target = profile.join("test-engine");
    let cargo = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "-j",
            "8",
            "-p",
            "mdshared-md-engine",
            "--bin",
            "mdview-engine",
        ])
        .args(["--config", "build.rustc-wrapper=\"\""])
        .arg("--target-dir")
        .arg(&target)
        .arg("--release")
        .output()
        .expect("run Cargo to build the pinned test engine");
    assert!(
        cargo.status.success(),
        "could not build the pinned test engine:\n{}",
        String::from_utf8_lossy(&cargo.stderr)
    );
    target.join("release").join(engine)
}
