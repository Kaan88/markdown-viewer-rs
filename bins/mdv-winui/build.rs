fn main() {
    println!("cargo:rerun-if-changed=assets/framework.manifest");
    // PerMonitorV2 has to come from the manifest, since it must be set before any code runs.
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{dir}/assets/framework.manifest");
}
