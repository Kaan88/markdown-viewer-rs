fn main() {
    println!("cargo:rerun-if-changed=assets/self-contained.manifest");
    println!("cargo:rerun-if-changed=assets/framework.manifest");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SELF_CONTAINED");
    // Both flavours embed a manifest for DPI awareness; self-contained also lists WinRT classes.
    let manifest = if std::env::var_os("CARGO_FEATURE_SELF_CONTAINED").is_some() {
        "self-contained"
    } else {
        "framework"
    };
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/assets/{manifest}.manifest");
}
