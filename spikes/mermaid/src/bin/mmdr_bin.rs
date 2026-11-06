#[path = "../common.rs"]
mod common;
use mermaid_rs_renderer::{render_with_options, RenderOptions};

fn render(src: &str) -> Result<String, String> {
    let mut o = RenderOptions::default();
    o.layout.fast_text_metrics = true;
    render_with_options(src, o).map_err(|e| e.to_string())
}

fn render_default(src: &str) -> Result<String, String> {
    render_with_options(src, RenderOptions::default()).map_err(|e| e.to_string())
}

fn main() {
    // Default uses real font metrics, which scan system fonts and write a cache under
    // $XDG_CACHE_HOME/mmdr. MMDR_FAST=1 selects its estimated-metrics mode.
    if std::env::var_os("MMDR_FAST").is_some() {
        common::run("mmdr-fast", render);
    } else {
        common::run("mmdr", render_default);
    }
}
