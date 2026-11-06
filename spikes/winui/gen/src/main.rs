//! Regenerates ../src/bindings.rs from the Windows App SDK 2.6.4 winmds in ../.cache/nuget264.
//! The winmds are only parsed; nothing from the package is executed on Linux.
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let nuget = root.join(".cache/nuget264");
    windows_bindgen::builder()
        .input(nuget.join("winui/metadata"))
        .input(nuget.join("interactiveexperiences/metadata/10.0.18362.0"))
        .input(nuget.join("foundation/metadata"))
        .input_default()
        .output(root.join("src/bindings.rs"))
        .implements([
            "Microsoft.UI.Xaml.IApplicationOverrides",
            "Microsoft.UI.Xaml.Markup.IXamlMetadataProvider",
        ])
        .compose("Microsoft.UI.Xaml.Application")
        .minimal()
        .flat()
        .filter_file(root.join("filter.txt"))
        .write();
}
