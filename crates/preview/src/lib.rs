//! mdview's page, drawn for another window.
//!
//! An editor that shows a preview beside its text sends [`Request`]s (the text, the size of
//! its pane, the caret's line, a scroll) and gets [`Update`]s back: the pixels of the page
//! that changed. The same requests and answers serve two ways of running the drawing:
//!
//! - in mdview's own process, `mdview --preview-server`, over its standard input and output
//!   ([`wire`]); the editor carries no renderer;
//! - in the editor's process, on a thread (feature `server`, [`serve`]); the editor carries
//!   the renderer.

pub mod wire;

#[cfg(feature = "server")]
mod server;

#[cfg(feature = "server")]
pub use server::{Server, serve, serve_stdio};

pub use wire::{Copied, Request, Update};
