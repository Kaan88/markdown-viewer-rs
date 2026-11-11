//! mdview: winit for the window and input, softbuffer for the pixels, every control drawn here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod args;
mod chrome;
mod gfx;
mod keys;
mod menu;
mod overlays;
mod page;
mod panes;
mod platform;
mod textfield;
mod theme;
mod tree;
mod ui;
mod workspace;

fn main() {
    std::process::exit(app::run());
}
