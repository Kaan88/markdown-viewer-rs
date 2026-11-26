#[path = "../../mdv-app/src/update_app.rs"]
mod update_app;

use mdshared_app::updater as shared;
use update_app::APP;

/// The release scripts write the manifest of a release (`build-installer.ps1`,
/// `packaging/write-manifest.sh`); when one has run, what it wrote must read as an installable
/// release, and every file it lists must have the hash it states. Each folder is checked when
/// it exists and skipped when it does not: `target/release-files-all` (release-all.sh, both
/// platforms), `target/dist-cross` (Linux-built Windows files) and `target/release-files`
/// (build-installer.ps1). Runs on Windows and Linux; the platform it runs on is offered the
/// release when the manifest lists an entry for it.
#[test]
fn the_release_scripts_write_a_manifest_this_build_reads() {
    use mdshared_app::update::{self, Version};
    use std::collections::HashMap;
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let current = if cfg!(windows) {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    };
    for name in ["release-files-all", "dist-cross", "release-files"] {
        let dir = target.join(name);
        let Ok(bytes) = std::fs::read(dir.join("mdview-update.txt")) else {
            continue; // The script has not run in this checkout.
        };
        let text = String::from_utf8(bytes.clone()).expect("the manifest is UTF-8");
        let keys: HashMap<&str, &str> = text.lines().filter_map(|l| l.split_once(": ")).collect();
        let version = keys["version"];
        let older = Version::parse("0.0.1").unwrap();
        let offer = shared::offer(&APP, &bytes, &older).expect("the manifest parses");
        match keys.get(format!("{current}.asset").as_str()) {
            Some(asset) => {
                let o = offer.unwrap_or_else(|| panic!("{name}: no offer for {current}"));
                assert_eq!(o.version.to_string(), version);
                assert!(o.app.url.ends_with(&format!("/v{version}/{asset}")));
                if cfg!(windows) {
                    let engine = o.engine.expect("the Windows entry lists the engine");
                    assert!(
                        engine
                            .url
                            .ends_with(&format!("/v{version}/mdview-engine.exe"))
                    );
                } else {
                    assert!(
                        o.engine.is_none(),
                        "Linux has no engine file: it is in the AppImage"
                    );
                }
            }
            None => assert!(
                offer.is_none(),
                "{name}: offered a platform it does not list"
            ),
        }
        // A manifest at the current version offers nothing.
        let same = Version::parse(version).unwrap();
        assert!(shared::offer(&APP, &bytes, &same).unwrap().is_none());
        // The engine keys belong to Windows only.
        assert!(!keys.keys().any(|k| k.starts_with("linux-x86_64.engine")));
        // Every file listed is beside the manifest, with the stated hash.
        let mut listed = Vec::new();
        for (key, hash) in &keys {
            let Some(platform) = key.strip_suffix(".sha256") else {
                continue;
            };
            let file = match keys.get(format!("{platform}.asset").as_str()) {
                Some(asset) => asset.to_string(),
                None => keys[format!("{platform}.url").as_str()]
                    .rsplit('/')
                    .next()
                    .unwrap()
                    .to_string(),
            };
            listed.push((file, *hash));
        }
        assert!(!listed.is_empty(), "{name}: no file hashes in the manifest");
        for (file, hash) in listed {
            let path = dir.join(&file);
            assert!(path.is_file(), "{name}: {file} is listed but missing");
            let ok = update::verify_sha256_file(&path, hash).unwrap();
            assert!(ok, "{name}: {file} matches the hash the manifest lists");
        }
    }
}
