//! Headless lazy folder-tree model for the "Files" side pane.
//!
//! Only expanded folders are read from disk. Collapsing a folder keeps the expansion state of its
//! nested folders, so expanding it again restores them.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use mdview_core::links::is_markdown_path;

const MAX_ROWS: usize = 4000;
const MAX_DEPTH: u16 = 32;

/// One visible row of the flattened tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: u16,
    pub is_dir: bool,
    pub expanded: bool,
}

pub struct FileTree {
    root: Option<PathBuf>,
    expanded: HashSet<PathBuf>,
    rows: Vec<Row>,
}

impl Default for FileTree {
    fn default() -> Self {
        Self::new()
    }
}

/// A comparison key: no verbatim prefix, no trailing separator, case-folded with `\` on Windows.
fn key(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut s = mdview_core::files::strip_verbatim(&s).unwrap_or_else(|| s.into_owned());
    if cfg!(windows) {
        s = s.replace('/', "\\").to_lowercase();
    }
    while s.len() > 1 && s.ends_with(is_sep) && !s.ends_with(":\\") {
        s.pop();
    }
    s
}

fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

fn is_hidden(_meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        _meta.file_attributes() & 0x2 != 0
    }
    #[cfg(not(windows))]
    false
}

/// Sorted visible children of `dir`: (path, name, is_dir). Unreadable folders give nothing.
fn read_children(dir: &Path) -> Vec<(PathBuf, String, bool)> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if is_hidden(&meta) {
            continue;
        }
        if meta.is_dir() {
            if name == "target" || name == "node_modules" {
                continue;
            }
            dirs.push((path, name, true));
        } else if is_markdown_path(&path) {
            files.push((path, name, false));
        }
    }
    let order = |a: &(PathBuf, String, bool), b: &(PathBuf, String, bool)| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then_with(|| a.1.cmp(&b.1))
    };
    dirs.sort_by(order);
    files.sort_by(order);
    dirs.extend(files);
    dirs
}

impl FileTree {
    pub fn new() -> FileTree {
        FileTree {
            root: None,
            expanded: HashSet::new(),
            rows: Vec::new(),
        }
    }

    /// Sets (or clears) the root folder: reads its first level only, forgets expansions.
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        self.root = root;
        self.expanded.clear();
        self.rebuild();
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// The visible rows, top to bottom. The root itself is not a row.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Expands or collapses the folder at `index`. No-op for files or a bad index.
    pub fn toggle(&mut self, index: usize) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        if !row.is_dir {
            return false;
        }
        let path = row.path.clone();
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        self.rebuild_changed()
    }

    /// Re-reads the root and every expanded folder, keeping expansions that still exist.
    pub fn refresh(&mut self) -> bool {
        self.expanded.retain(|p| p.is_dir());
        self.rebuild_changed()
    }

    /// Expands the ancestors of `path` (if under the root) and returns its row index.
    pub fn reveal(&mut self, path: &Path) -> Option<usize> {
        let root_key = key(self.root.as_deref()?);
        let target = key(path);
        let rest = target.strip_prefix(&root_key)?;
        let rest = if root_key.ends_with(is_sep) {
            rest
        } else {
            rest.strip_prefix(is_sep)?
        };
        if rest.is_empty() {
            return None;
        }
        let mut prefix = root_key.clone();
        let parts: Vec<&str> = rest.split(is_sep).filter(|p| !p.is_empty()).collect();
        for part in &parts[..parts.len() - 1] {
            if !prefix.ends_with(is_sep) {
                prefix.push(if cfg!(windows) { '\\' } else { '/' });
            }
            prefix.push_str(part);
            let idx = self.index_of_key(&prefix)?;
            if !self.rows[idx].is_dir {
                return None;
            }
            if !self.rows[idx].expanded {
                self.toggle(idx);
            }
        }
        self.index_of_key(&target)
    }

    /// The row index of `path` if it is currently visible.
    #[cfg(test)]
    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.index_of_key(&key(path))
    }

    /// The root and every visible expanded folder.
    #[cfg(test)]
    pub fn watched_dirs(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.root.iter().cloned().collect();
        out.extend(
            self.rows
                .iter()
                .filter(|r| r.is_dir && r.expanded)
                .map(|r| r.path.clone()),
        );
        out
    }

    fn index_of_key(&self, k: &str) -> Option<usize> {
        self.rows.iter().position(|r| key(&r.path) == k)
    }

    fn rebuild_changed(&mut self) -> bool {
        let old = std::mem::take(&mut self.rows);
        self.rebuild();
        self.rows != old
    }

    fn rebuild(&mut self) {
        let mut rows = Vec::new();
        if let Some(root) = &self.root {
            self.collect(root, 0, &mut rows);
        }
        self.rows = rows;
    }

    fn collect(&self, dir: &Path, depth: u16, rows: &mut Vec<Row>) {
        if depth > MAX_DEPTH {
            return;
        }
        for (path, name, is_dir) in read_children(dir) {
            if rows.len() >= MAX_ROWS {
                return;
            }
            let expanded = is_dir && self.expanded.contains(&path);
            rows.push(Row {
                path: path.clone(),
                name,
                depth,
                is_dir,
                expanded,
            });
            if expanded {
                self.collect(&path, depth + 1, rows);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    struct Tmp(PathBuf);
    impl Tmp {
        fn new() -> Tmp {
            let p = std::env::temp_dir().join(format!(
                "mdview-tree-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
        fn file(&self, rel: &str) {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        fn dir(&self, rel: &str) {
            fs::create_dir_all(self.0.join(rel)).unwrap();
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn names(t: &FileTree) -> Vec<String> {
        t.rows()
            .iter()
            .map(|r| format!("{}{}", "  ".repeat(r.depth as usize), r.name))
            .collect()
    }

    fn tree(tmp: &Tmp) -> FileTree {
        let mut t = FileTree::new();
        t.set_root(Some(tmp.0.clone()));
        t
    }

    #[test]
    fn ordering_and_filtering() {
        let tmp = Tmp::new();
        tmp.file("b.md");
        tmp.file("A.md");
        tmp.file("c.markdown");
        tmp.file("notes.txt");
        tmp.file(".hidden.md");
        tmp.dir("zeta");
        tmp.dir("Alpha");
        tmp.dir(".git");
        tmp.dir("target");
        tmp.dir("node_modules");
        let t = tree(&tmp);
        assert_eq!(names(&t), ["Alpha", "zeta", "A.md", "b.md", "c.markdown"]);
        assert!(t.rows()[0].is_dir && !t.rows()[2].is_dir);
    }

    #[test]
    fn lazy_expand_collapse() {
        let tmp = Tmp::new();
        tmp.file("d/inner/deep.md");
        tmp.file("d/a.md");
        let mut t = tree(&tmp);
        assert_eq!(names(&t), ["d"]);
        assert!(t.toggle(0));
        assert_eq!(names(&t), ["d", "  inner", "  a.md"]);
        assert!(t.toggle(1));
        assert_eq!(names(&t), ["d", "  inner", "    deep.md", "  a.md"]);
        assert!(!t.toggle(2), "files are a no-op");
        assert!(!t.toggle(99));
        // Collapsing hides nested rows; re-expanding restores nested expansion.
        assert!(t.toggle(0));
        assert_eq!(names(&t), ["d"]);
        assert!(t.toggle(0));
        assert_eq!(names(&t), ["d", "  inner", "    deep.md", "  a.md"]);
        assert_eq!(t.watched_dirs().len(), 3);
    }

    #[test]
    fn refresh_add_remove() {
        let tmp = Tmp::new();
        tmp.file("a.md");
        tmp.file("d/x.md");
        let mut t = tree(&tmp);
        t.toggle(0);
        assert!(!t.refresh());
        tmp.file("d/y.md");
        tmp.file("b.md");
        assert!(t.refresh());
        assert_eq!(names(&t), ["d", "  x.md", "  y.md", "a.md", "b.md"]);
        fs::remove_file(tmp.0.join("a.md")).unwrap();
        assert!(t.refresh());
        assert_eq!(names(&t), ["d", "  x.md", "  y.md", "b.md"]);
        fs::remove_dir_all(tmp.0.join("d")).unwrap();
        assert!(t.refresh());
        assert_eq!(names(&t), ["b.md"]);
        assert_eq!(t.watched_dirs(), vec![tmp.0.clone()]);
    }

    #[test]
    fn reveal_nested() {
        let tmp = Tmp::new();
        tmp.file("a/b/c/note.md");
        tmp.file("a/other.md");
        let mut t = tree(&tmp);
        let target = tmp.0.join("a").join("b").join("c").join("note.md");
        assert_eq!(t.index_of(&target), None);
        let i = t.reveal(&target).unwrap();
        assert_eq!(t.rows()[i].name, "note.md");
        assert_eq!(t.rows()[i].depth, 3);
        assert_eq!(t.index_of(&target), Some(i));
        assert_eq!(t.reveal(&tmp.0.join("a").join("missing.md")), None);
        assert_eq!(t.reveal(Path::new("Z:\\elsewhere\\x.md")), None);
    }

    #[cfg(windows)]
    #[test]
    fn index_of_case_and_verbatim() {
        let tmp = Tmp::new();
        tmp.file("Docs/Readme.md");
        let mut t = tree(&tmp);
        t.toggle(0);
        let file = tmp.0.join("Docs").join("Readme.md");
        let upper = PathBuf::from(file.to_string_lossy().to_uppercase());
        assert_eq!(t.index_of(&upper), Some(1));
        let verbatim = PathBuf::from(format!("{}{}", "\\\\?\\", file.display()));
        assert_eq!(t.index_of(&verbatim), Some(1));
        assert_eq!(t.reveal(&upper), Some(1));
    }

    #[test]
    fn clear_root() {
        let tmp = Tmp::new();
        tmp.file("a.md");
        let mut t = tree(&tmp);
        assert_eq!(t.rows().len(), 1);
        t.set_root(None);
        assert!(t.rows().is_empty() && t.root().is_none());
        assert_eq!(t.reveal(&tmp.0.join("a.md")), None);
    }
}
