//! mdview: winit for the window and input, softbuffer for the pixels, every control drawn here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod args;
mod chrome;
mod codebtn;
mod folder_search;
mod frost;
mod keys;
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
    std::process::exit(app::run());
}
