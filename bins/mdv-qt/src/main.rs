// No console window on Windows (release builds; debug keeps it for logs).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    std::process::exit(mdview_qt::shell::run());
}
