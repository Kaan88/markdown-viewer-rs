//! mdview: winit for the window and input, softbuffer for the pixels, every control drawn here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod args;
mod chrome;
mod codebtn;
mod folder_search;
mod frost;
mod keys;
mod notes;
mod overlays;
mod page;
mod palette;
mod panes;
mod platform;
mod present;
mod preview;
mod settings;
mod tree;
mod ui;
mod updater;
mod workspace;
mod zoom;

use mdshared_ui::{gfx, menu, textfield, theme};

fn main() {
    // mdedit's preview pane: the page drawn for another window, over standard input and output.
    // Before anything else, so no window, single-instance hand-over or session is touched.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == "--preview-server")
    {
        std::process::exit(mdview_preview::serve_stdio());
    }
    // A crash from here on leaves a report for the next start.
    if let Some(dir) = app::crash_dir() {
        mdshared_app::crash::install("mdview", env!("CARGO_PKG_VERSION"), dir);
    }
    let code = app::run();
    // Ended normally: a panic caught on the way (a bad image) is not a crash to report.
    mdshared_app::crash::closed_normally();
    std::process::exit(code);
}
