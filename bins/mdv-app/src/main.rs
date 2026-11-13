//! mdview: winit for the window and input, softbuffer for the pixels, every control drawn here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod args;
mod chrome;
mod keys;
mod overlays;
mod page;
mod palette;
mod panes;
mod platform;
mod settings;
mod tree;
mod ui;
mod updater;
mod workspace;

use mdshared_ui::{gfx, menu, textfield, theme};

fn main() {
    std::process::exit(app::run());
}
