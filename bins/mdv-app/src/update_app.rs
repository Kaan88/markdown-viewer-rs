/// mdview as the updater knows it.
pub const APP: mdshared_app::updater::App = mdshared_app::updater::App {
    name: "mdview",
    version: env!("CARGO_PKG_VERSION"),
    manifest_url: "https://github.com/Kaan88/markdown-viewer-rs/releases/latest/download/mdview-update.txt",
    manifest_env: "MDVIEW_UPDATE_MANIFEST",
    engine: Some("mdview-engine"),
    extras: &[],
    // The installer's entry in Windows' list of installed apps (`AppId` in `mdview.iss`).
    uninstall_key: Some(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{B7E2F4A9-3D1C-4C58-A6E0-91D5C8F27B34}_is1",
    ),
};
