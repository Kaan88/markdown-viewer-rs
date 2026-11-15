//! Find in folder: the palette's card as a results list for a search of the Markdown files of the
//! open folder. The walk, the matching lines and the worker thread are
//! `mdshared_app::foldersearch`; the list (a literal one, in the order the walk finds things),
//! its headings and its keys are `mdshared_ui::palette`. This module turns the events of the
//! search into rows and says what choosing a row means.
//!
//! The search runs on a thread of its own that exists only while a search does: it starts when the
//! query changes, is cancelled when the query changes again or the card closes (dropping the
//! handle cancels it), and wakes the window at most every 40 ms while results come in. Nothing
//! here runs until the command is used.

use mdshared_app::foldersearch::{Event, Hit, Options, Search};
use mdshared_ui::glyph;
use mdshared_ui::palette::{Entry, Palette};
use mdview_core::limits::MAX_FILE_BYTES;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the empty box says.
pub const PLACEHOLDER: &str = "Find in the Markdown files of this folder";
/// The last row when the search stopped short.
pub const MORE: &str = "More results are not shown. Narrow the search.";
const SEARCHING: &str = "Searching…";
const NO_MATCHES: &str = "No matches";

/// Tells the window that results are waiting.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

pub struct FolderSearch {
    root: PathBuf,
    wake: Option<Waker>,
    search: Option<Search>,
    /// The query the running or the last search is for.
    query: String,
    /// The files with matches, in order; a row's target names one by index.
    files: Vec<PathBuf>,
    /// By entry id: the file and the line a row opens; `None` for headings and the note.
    targets: Vec<Option<(usize, u32)>>,
    /// Matching lines listed.
    lines: usize,
    /// Files read so far.
    read: usize,
    running: bool,
    /// Files left out: too large, binary or unreadable.
    skipped: usize,
    /// Some file or the total went over its cap.
    more: bool,
}

impl FolderSearch {
    /// A card that searches `root`. `wake` is called from the search thread.
    pub fn new(root: PathBuf, wake: Option<Waker>) -> FolderSearch {
        FolderSearch {
            root,
            wake,
            search: None,
            query: String::new(),
            files: Vec::new(),
            targets: Vec::new(),
            lines: 0,
            read: 0,
            running: false,
            skipped: 0,
            more: false,
        }
    }

    /// The query the list is for.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Starts a search for `query` (the previous one is cancelled and its rows go). An empty query,
    /// or one of spaces only, lists nothing.
    pub fn start(&mut self, query: &str, palette: &mut Palette) {
        self.search = None;
        self.files.clear();
        self.targets.clear();
        self.lines = 0;
        self.read = 0;
        self.more = false;
        self.skipped = 0;
        self.running = false;
        self.query = query.to_string();
        palette.set_entries(Vec::new());
        palette.set_message(None);
        palette.set_footer(None);
        if query.trim().is_empty() {
            return;
        }
        let wake = self.wake.clone();
        self.search = Some(Search::start(
            self.root.clone(),
            Box::new(mdview_core::links::is_markdown_path),
            Box::new(mdview_core::find::line_matcher(query)),
            Options {
                max_file_bytes: MAX_FILE_BYTES as u64,
                ..Options::default()
            },
            move || {
                if let Some(w) = &wake {
                    w();
                }
            },
        ));
        self.running = true;
        self.status(palette);
    }

    /// Takes what the thread has found and adds it to the list. True when the card changed.
    pub fn poll(&mut self, palette: &mut Palette) -> bool {
        let Some(search) = &self.search else {
            return false;
        };
        let events = search.take();
        if events.is_empty() {
            return false;
        }
        let mut rows = Vec::new();
        for event in events {
            match event {
                Event::File {
                    path,
                    rel,
                    hits,
                    more,
                } => self.file(path, rel, hits, more, &mut rows),
                Event::Progress { files } => self.read = files,
                Event::Done {
                    files,
                    skipped,
                    truncated,
                    ..
                } => {
                    self.read = files;
                    self.running = false;
                    self.more |= truncated;
                    if self.more {
                        let id = self.targets.len() as u32;
                        self.targets.push(None);
                        rows.push(Entry::note(id, MORE));
                    }
                    self.skipped = skipped;
                }
            }
        }
        palette.append_entries(rows);
        self.status(palette);
        true
    }

    fn file(
        &mut self,
        path: PathBuf,
        rel: String,
        hits: Vec<Hit>,
        more: bool,
        rows: &mut Vec<Entry>,
    ) {
        let n = hits.len();
        let count = match (n, more) {
            (1, false) => "1 match".to_string(),
            (n, false) => format!("{n} matches"),
            (n, true) => format!("{n}+ matches"),
        };
        self.more |= more;
        self.lines += n;
        let file = self.files.len();
        self.files.push(path);
        let id = self.targets.len() as u32;
        self.targets.push(None);
        rows.push(Entry::heading(id, rel).icon(glyph::DOC).hint(count));
        for hit in hits {
            let id = self.targets.len() as u32;
            self.targets.push(Some((file, hit.line)));
            rows.push(
                Entry::new(id, hit.text.clone())
                    .hint(format!("Line {}", hit.line))
                    .marks(char_marks(&hit)),
            );
        }
    }

    /// The message row and the footer for the state the search is in.
    fn status(&self, palette: &mut Palette) {
        palette.set_message(if self.lines > 0 {
            None
        } else if self.running {
            Some(SEARCHING.to_string())
        } else {
            Some(NO_MATCHES.to_string())
        });
        palette.set_footer(Some(self.summary()));
    }

    /// The footer's words: progress while running, then the totals.
    pub fn summary(&self) -> String {
        let files = |n: usize| {
            if n == 1 {
                "1 file".to_string()
            } else {
                format!("{n} files")
            }
        };
        if self.running {
            return format!("Searching… {}", files(self.read));
        }
        let lines = if self.lines == 1 {
            "1 match".to_string()
        } else {
            format!("{} matches", self.lines)
        };
        if self.lines == 0 {
            return format!("Searched {}", files(self.read));
        }
        let mut text = format!("{lines} in {}", files(self.files.len()));
        if self.skipped > 0 {
            text.push_str(&format!(", {} skipped", files(self.skipped)));
        }
        text
    }

    /// What choosing the row with this id opens: the file and the line.
    pub fn hit(&self, id: u32) -> Option<(&Path, u32)> {
        let (file, line) = (*self.targets.get(id as usize)?)?;
        Some((self.files.get(file)?.as_path(), line))
    }

    /// Whether a search is still reading files.
    pub fn running(&self) -> bool {
        self.running
    }
}

/// The char indices of the hit's text that its marks cover.
fn char_marks(hit: &Hit) -> Vec<u32> {
    hit.text
        .char_indices()
        .enumerate()
        .filter(|(_, (b, _))| hit.marks.iter().any(|m| m.contains(b)))
        .map(|(i, _)| i as u32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_char_indices_even_after_wide_chars() {
        let hit = Hit {
            line: 1,
            text: "é needle".to_string(),
            marks: std::iter::once(3..9).collect(),
        };
        assert_eq!(char_marks(&hit), vec![2, 3, 4, 5, 6, 7]);
    }

    fn run(dir: &Path, query: &str) -> (FolderSearch, Palette) {
        let mut palette = Palette::new();
        palette.set_literal(true);
        let mut search = FolderSearch::new(dir.to_path_buf(), None);
        search.start(query, &mut palette);
        while search.running() {
            std::thread::sleep(std::time::Duration::from_millis(2));
            search.poll(&mut palette);
        }
        search.poll(&mut palette);
        (search, palette)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdview-fsm-{name}-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_over_its_share_says_so_and_the_list_ends_with_the_note() {
        let dir = temp("per-file");
        std::fs::write(dir.join("big.md"), "needle\n".repeat(250)).unwrap();
        let (search, palette) = run(&dir, "needle");
        // The heading, 200 lines, the note.
        assert_eq!(palette.len(), 202);
        assert_eq!(palette.label_of(0), Some("big.md"));
        assert_eq!(palette.label_of(201), Some(MORE));
        assert_eq!(search.summary(), "200 matches in 1 file");
        assert_eq!(search.hit(1).map(|h| h.1), Some(1));
        assert_eq!(search.hit(0), None, "a heading opens nothing");
        assert_eq!(search.hit(201), None, "nor does the note");
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_search_that_finds_nothing_counts_the_files_read_and_blank_queries_search_nothing() {
        let dir = temp("none");
        std::fs::write(dir.join("a.md"), "text\n").unwrap();
        std::fs::write(dir.join("b.txt"), "needle\n").unwrap();
        let (search, palette) = run(&dir, "needle");
        assert!(palette.is_empty());
        assert_eq!(search.summary(), "Searched 1 file");
        let (search, palette) = run(&dir, "   ");
        assert!(palette.is_empty() && !search.running());
        _ = std::fs::remove_dir_all(&dir);
    }
}
