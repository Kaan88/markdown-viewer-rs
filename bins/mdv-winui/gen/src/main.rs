//! Regenerates `../src/bar_bindings.rs`: the controls the command bar needs (Button, StackPanel,
//! TextBlock, FontIcon, the find box's TextBox events, the title bar calls). It is a separate,
//! self-contained module so `bindings.rs` and its hand-written additions are left alone; objects
//! cross between the two with `cast()` (QueryInterface), never by Rust type.
//!
//! Run on Windows with the Windows App SDK 2.6.4 NuGet package restored:
//! `cargo run --manifest-path bins/mdv-winui/gen/Cargo.toml`
//! (`NUGET` overrides the packages folder, default `%USERPROFILE%\.nuget\packages`).
use std::path::{Path, PathBuf};

fn main() {
    let nuget = std::env::var_os("NUGET").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("USERPROFILE").expect("USERPROFILE"))
            .join(".nuget")
            .join("packages")
    });
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    windows_bindgen::builder()
        .input(nuget.join("microsoft.windowsappsdk.winui/2.6.4/metadata"))
        .input(nuget.join("microsoft.windowsappsdk.interactiveexperiences/2.6.4/metadata/10.0.18362.0"))
        .input(nuget.join("microsoft.windowsappsdk.foundation/2.6.4/metadata"))
        .input_default()
        .output(root.join("src/bar_bindings.rs"))
        .minimal()
        .flat()
        .filter_file(root.join("gen/filter.txt"))
        .write();
}
