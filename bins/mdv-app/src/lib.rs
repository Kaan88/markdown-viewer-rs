//! Viewer policy and file/session handling, separate from the shared Markdown renderer.

pub mod commands;
pub mod files;
pub mod place;
#[path = "preview_text.rs"]
pub mod preview;
pub mod selfupdate;
pub mod session;
pub mod single;
pub mod slides;
pub mod status;
pub mod update;
pub mod update_app;
pub mod watch;
