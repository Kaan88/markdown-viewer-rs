//! The Files pane's folder tree is `mdshared_ui::tree`; the viewer lists Markdown files only.

pub use mdshared_ui::tree::{FileTree, Rules};

/// A tree that lists Markdown files.
pub fn new_tree() -> FileTree {
    FileTree::new(Rules {
        shows_file: Box::new(mdview_core::links::is_markdown_path),
    })
}
