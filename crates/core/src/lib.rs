//! Platform-neutral core of mdview: document model, parsing, HTML mapping, find, files and session.

pub mod autolink;
pub mod commands;
pub mod emoji;
pub mod files;
pub mod find;
pub mod frontmatter;
pub mod html;
pub mod limits;
pub mod links;
pub mod model;
mod parse;
pub mod place;
pub mod selfupdate;
pub mod session;
pub mod single;
pub mod slug;
pub mod status;
pub mod tasks;
pub mod update;
pub mod watch;

pub use model::{Document, parse_bytes, parse_bytes_wiki};
pub use parse::{parse, parse_wiki};
