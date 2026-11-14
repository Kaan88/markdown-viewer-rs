//! Export as HTML against the fixture documents: every one makes a page, and the engine's
//! pictures (formulas, diagrams) are in it when the engine can be found.
//!
//! `MDV_EXPORT_DIR=<dir> cargo test -p mdview-render --test export` also writes each page there,
//! to open in a browser.

use mdview_render::{ExportOptions, LIGHT, export_html};
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn engine_available() -> bool {
    if let Some(path) = std::env::var_os("MDV_ENGINE") {
        return PathBuf::from(path).is_file();
    }
    let name = if cfg!(windows) {
        "mdview-engine.exe"
    } else {
        "mdview-engine"
    };
    fixtures().join("../target/release").join(name).is_file()
}

fn page(name: &str, print: bool) -> String {
    let dir = fixtures();
    let doc = mdview_core::parse_bytes(&std::fs::read(dir.join(name)).unwrap());
    let html = export_html(
        &doc,
        &ExportOptions {
            title: "",
            theme: LIGHT,
            base_dir: Some(&dir),
            remote_images: false,
            print,
        },
    );
    if let Some(out) = std::env::var_os("MDV_EXPORT_DIR") {
        let out = Path::new(&out);
        std::fs::create_dir_all(out).unwrap();
        let suffix = if print { "-print" } else { "" };
        std::fs::write(
            out.join(name.replace(".md", &format!("{suffix}.html"))),
            &html,
        )
        .unwrap();
    }
    html
}

#[test]
fn every_fixture_makes_a_complete_page_with_nothing_external() {
    for entry in std::fs::read_dir(fixtures()).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let html = page(&name, false);
        assert!(html.starts_with("<!DOCTYPE html>"), "{name}");
        assert!(html.trim_end().ends_with("</html>"), "{name}");
        assert!(!html.contains("<script"), "{name}");
        // Nothing is fetched by opening the file: every picture is embedded, or is not there
        // (remote images were not allowed, and an image too large would be a file: link).
        for src in html.split("src=\"").skip(1) {
            let src = &src[..src.find('"').unwrap()];
            assert!(
                src.starts_with("data:") || src.starts_with("file://"),
                "{name}: {src}"
            );
        }
        assert!(!html.contains("href=\"javascript"), "{name}");
    }
}

#[test]
fn formulas_and_diagrams_are_pictures_when_the_engine_is_there() {
    if !engine_available() {
        eprintln!("skipping: no mdview-engine (set MDV_ENGINE)");
        return;
    }
    let math = page("math.md", false);
    assert!(math.contains("<img class=\"math\" src=\"data:image/png;base64,"));
    assert!(math.contains("<p class=\"math\"><img src=\"data:image/png;base64,"));
    assert!(
        math.contains("alt=\"E = mc^2\""),
        "the source stays as the alt text"
    );
    let diagrams = page("diagrams.md", false);
    assert!(diagrams.contains("<p class=\"diagram\"><img src=\"data:image/png;base64,"));
}

#[test]
fn the_images_fixture_embeds_its_local_pictures() {
    let html = page("images.md", false);
    assert!(html.contains("src=\"data:image/"), "an image is embedded");
    let print = page("showcase.md", true);
    assert!(print.contains("window.print"));
}
