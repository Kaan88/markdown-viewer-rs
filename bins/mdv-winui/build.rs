fn main() {
    println!("cargo:rerun-if-changed=assets/framework.manifest");
    println!("cargo:rerun-if-changed=assets/icon.rc");
    println!("cargo:rerun-if-changed=assets/mdview.ico");
    // PerMonitorV2 has to come from the manifest, since it must be set before any code runs.
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/assets/framework.manifest");
    embed_icon(&dir);
}

/// Compiles the application icon with the Windows SDK's `rc.exe` and links it in, so Explorer, the
/// taskbar and the title bar show it. `rc` is on PATH inside a Visual Studio developer prompt; when it
/// is not, the build still works and the exe just has no icon.
fn embed_icon(dir: &str) {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::env::var("OUT_DIR").unwrap();
    let res = format!("{out}/icon.res");
    let status = std::process::Command::new("rc.exe")
        .args(["/nologo", "/fo", &res, &format!("{dir}/assets/icon.rc")])
        .current_dir(format!("{dir}/assets"))
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={res}"),
        _ => println!("cargo:warning=rc.exe not found or failed; building without an icon"),
    }
}
