//! Platform-neutral core of mdview: document model, parsing, HTML mapping, find, files and session.

pub mod autolink;
pub mod commands;
pub mod files;
pub mod find;
pub mod html;
pub mod limits;
pub mod links;
pub mod model;
mod parse;
pub mod session;
pub mod slug;
pub mod status;

pub use model::{Document, parse_bytes};
pub use parse::parse;
