//! Use only the build input selected by Cargo, never a runtime override.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn engine_path() -> &'static Path {
    Path::new(mdview_engine_fixture::PATH)
}

pub fn require_engine() {
    static READY: OnceLock<()> = OnceLock::new();
    READY.get_or_init(|| {
        let path = engine_path();
        if !path.is_absolute() || !path.is_file() {
            if std::env::var_os("MDVIEW_TEST_ENGINE_VERBOSE").is_some() {
                eprintln!("test engine: expected artifact at {}", path.display());
            }
            panic!("test engine: staged binary missing");
        }
        mdview_render::set_engine_path(PathBuf::from(path));
    });
}
