//! The built exe carries a version resource that names Cargo's version: Hub reads it first to
//! learn which mdview is installed (`bins/mdv-app/build.rs`). Windows only: elsewhere there is no
//! resource. Without a resource compiler the build still succeeds (a warning), so on a machine
//! with neither `rc.exe` nor `llvm-rc` this test is the one that says so.

#![cfg(windows)]

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The index of `needle` in `hay`.
fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// A string value in the StringFileInfo block: the key, its NUL and padding, then the value,
/// all UTF-16. The value must start within a few bytes of the end of the key.
fn has_value(exe: &[u8], key: &str, value: &str) -> bool {
    let (key, value) = (utf16(key), utf16(value));
    let mut from = 0;
    while let Some(at) = find(exe, &key, from) {
        let after = at + key.len();
        let window = &exe[after..(after + 8 + value.len() + 2).min(exe.len())];
        if let Some(i) = find(window, &value, 0)
            && i <= 8
            && window.get(i + value.len()..i + value.len() + 2) == Some(&[0, 0])
        {
            return true;
        }
        from = after;
    }
    false
}

#[test]
fn the_exe_has_a_version_resource_naming_the_cargo_version() {
    let exe = std::fs::read(env!("CARGO_BIN_EXE_mdview")).expect("the built mdview.exe");
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        has_value(&exe, "FileVersion", version),
        "no FileVersion {version} in the exe's version resource"
    );
    assert!(has_value(&exe, "ProductVersion", version));
    assert!(has_value(&exe, "OriginalFilename", "mdview.exe"));
}
