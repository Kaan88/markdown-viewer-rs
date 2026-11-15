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
//!
//! While the card is open and files change, [`FolderSearch::refresh`] searches again for the same
//! query. The list on screen stays as it is while the new search reads (nothing flickers to
//! "Searching…"), and is swapped for the new one when it is done, with the selection on the row
//! of the same file and line, or the nearest one in that file.

use mdshared_app::foldersearch::{Event, Hit, Options, Search};
use mdshared_ui::glyph;
use mdshared_ui::keys::{KeyName, Mods};
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

/// What one search has found so far.
#[derive(Default)]
struct Results {
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

impl Results {
    fn started() -> Results {
        Results {
            running: true,
            ..Results::default()
        }
    }

    /// Adds what the thread reported to the results and the rows to show for it.
    fn absorb(&mut self, events: Vec<Event>, rows: &mut Vec<Entry>) {
        for event in events {
            match event {
                Event::File {
                    path,
                    rel,
                    hits,
                    more,
                } => self.file(path, rel, hits, more, rows),
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

    /// What the row with this id opens: the file and the line.
    fn hit(&self, id: u32) -> Option<(&Path, u32)> {
        let (file, line) = (*self.targets.get(id as usize)?)?;
        Some((self.files.get(file)?.as_path(), line))
    }

    /// The id of the row to select in these results for a row that was on `file` at `line`: the
    /// same line, or the nearest one of that file (the earlier one on a tie), or none.
    fn nearest(&self, file: &Path, line: u32) -> Option<u32> {
        let mut best: Option<(u32, u32)> = None;
        for (id, target) in self.targets.iter().enumerate() {
            let Some((f, l)) = *target else { continue };
            if self.files[f] != file {
                continue;
            }
            let distance = l.abs_diff(line);
            if best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, id as u32));
            }
        }
        best.map(|(_, id)| id)
    }
}

/// A search that replaces the list on screen once it is done.
struct Next {
    search: Search,
    results: Results,
    rows: Vec<Entry>,
}

pub struct FolderSearch {
    root: PathBuf,
    wake: Option<Waker>,
    search: Option<Search>,
    /// The query the running or the last search is for.
    query: String,
    results: Results,
    /// A search for the same query started because files changed.
    next: Option<Next>,
}

impl FolderSearch {
    /// A card that searches `root`. `wake` is called from the search thread.
    pub fn new(root: PathBuf, wake: Option<Waker>) -> FolderSearch {
        FolderSearch {
            root,
            wake,
            search: None,
            query: String::new(),
            results: Results::default(),
            next: None,
        }
    }

    /// The query the list is for.
    pub fn query(&self) -> &str {
        &self.query
    }

    fn begin(&self, query: &str) -> Search {
        let wake = self.wake.clone();
        Search::start(
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
        )
    }

    /// Starts a search for `query` (the previous one is cancelled and its rows go). An empty query,
    /// or one of spaces only, lists nothing.
    pub fn start(&mut self, query: &str, palette: &mut Palette) {
        self.search = None;
        self.next = None;
        self.results = Results::default();
        self.query = query.to_string();
        palette.set_entries(Vec::new());
        palette.set_message(None);
        palette.set_footer(None);
        if query.trim().is_empty() {
            return;
        }
        self.search = Some(self.begin(query));
        self.results = Results::started();
        self.status(palette);
    }

    /// Searches again for the same query, because files changed. The list stays as it is until the
    /// new search is done. False when there is nothing to search again (no query).
    pub fn refresh(&mut self) -> bool {
        if self.query.trim().is_empty() {
            return false;
        }
        self.next = Some(Next {
            search: self.begin(&self.query),
            results: Results::started(),
            rows: Vec::new(),
        });
        true
    }

    /// Whether a search for changed files is still reading.
    #[cfg(test)]
    pub fn refreshing(&self) -> bool {
        self.next.is_some()
    }

    /// Takes what the threads have found and adds it to the list; a finished refresh replaces
    /// it. True when the card changed.
    pub fn poll(&mut self, palette: &mut Palette) -> bool {
        let mut changed = false;
        if let Some(search) = &self.search {
            let events = search.take();
            if !events.is_empty() {
                let mut rows = Vec::new();
                self.results.absorb(events, &mut rows);
                palette.append_entries(rows);
                self.status(palette);
                changed = true;
            }
        }
        if let Some(next) = self.next.as_mut() {
            let events = next.search.take();
            if !events.is_empty() {
                next.results.absorb(events, &mut next.rows);
            }
            if !next.results.running {
                let next = self.next.take().expect("just looked at it");
                self.swap(next, palette);
                changed = true;
            }
        }
        changed
    }

    /// Puts a finished refresh on screen, the selection on the row of the same place.
    fn swap(&mut self, next: Next, palette: &mut Palette) {
        let was = palette
            .selected_id()
            .and_then(|id| self.results.hit(id))
            .map(|(file, line)| (file.to_path_buf(), line));
        self.search = Some(next.search);
        self.results = next.results;
        palette.set_entries(next.rows);
        self.status(palette);
        let want = was.and_then(|(file, line)| self.results.nearest(&file, line));
        let ids = palette.listed_ids();
        let (Some(want), Some(at)) = (want, palette.selected_index()) else {
            return;
        };
        let Some(to) = ids.iter().position(|&id| id == want) else {
            return;
        };
        let key = if to > at { KeyName::Down } else { KeyName::Up };
        for _ in 0..to.abs_diff(at).min(ids.len()) {
            // Headings are not chosen: a step skips them, so this may land past the row at once.
            if palette.selected_id() == Some(want) {
                break;
            }
            palette.key(&key, Mods::default(), None);
        }
    }

    /// The message row and the footer for the state the search is in.
    fn status(&self, palette: &mut Palette) {
        palette.set_message(if self.results.lines > 0 {
            None
        } else if self.results.running {
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
        let r = &self.results;
        if r.running {
            return format!("Searching… {}", files(r.read));
        }
        let lines = if r.lines == 1 {
            "1 match".to_string()
        } else {
            format!("{} matches", r.lines)
        };
        if r.lines == 0 {
            return format!("Searched {}", files(r.read));
        }
        let mut text = format!("{lines} in {}", files(r.files.len()));
        if r.skipped > 0 {
            text.push_str(&format!(", {} skipped", files(r.skipped)));
        }
        text
    }

    /// What choosing the row with this id opens: the file and the line.
    pub fn hit(&self, id: u32) -> Option<(&Path, u32)> {
        self.results.hit(id)
    }

    /// Whether a search is still reading files.
    pub fn running(&self) -> bool {
        self.results.running
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

    fn refreshed(search: &mut FolderSearch, palette: &mut Palette) {
        assert!(search.refresh());
        while search.refreshing() {
            std::thread::sleep(std::time::Duration::from_millis(2));
            search.poll(palette);
        }
    }

    fn selected(search: &FolderSearch, palette: &Palette) -> Option<(String, u32)> {
        let (path, line) = search.hit(palette.selected_id()?)?;
        Some((path.file_name()?.to_string_lossy().into_owned(), line))
    }

    #[test]
    fn a_refresh_shows_new_results_and_keeps_the_selected_row_where_it_can() {
        let dir = temp("refresh");
        std::fs::write(dir.join("a.md"), "needle one\nplain\nneedle two\n").unwrap();
        std::fs::write(dir.join("b.md"), "needle b\n").unwrap();
        let (mut search, mut palette) = run(&dir, "needle");
        assert_eq!(search.summary(), "3 matches in 2 files");
        // Select a.md line 3 (the keyboard: first row is line 1).
        palette.key(&KeyName::Down, Mods::default(), None);
        assert_eq!(selected(&search, &palette), Some(("a.md".into(), 3)));

        // A new file appears first in the list: every id moves, the selection stays on the
        // same file and line.
        std::fs::write(dir.join("0.md"), "needle first\n").unwrap();
        refreshed(&mut search, &mut palette);
        assert_eq!(search.summary(), "4 matches in 3 files");
        assert_eq!(palette.label_of(0), Some("0.md"));
        assert_eq!(selected(&search, &palette), Some(("a.md".into(), 3)));

        // The selected line goes away: the nearest of that file is selected.
        std::fs::write(dir.join("a.md"), "needle zero\nneedle one\n").unwrap();
        refreshed(&mut search, &mut palette);
        assert_eq!(selected(&search, &palette), Some(("a.md".into(), 2)));

        // The file goes away: the selection falls back to the first row, as a new list does.
        std::fs::remove_file(dir.join("a.md")).unwrap();
        refreshed(&mut search, &mut palette);
        assert_eq!(search.summary(), "2 matches in 2 files");
        assert_eq!(selected(&search, &palette), Some(("0.md".into(), 1)));
        assert!(selected(&search, &palette).is_some());
        // Nothing left: "No matches", and a refresh with no query does nothing.
        std::fs::remove_file(dir.join("0.md")).unwrap();
        std::fs::remove_file(dir.join("b.md")).unwrap();
        refreshed(&mut search, &mut palette);
        assert!(palette.is_empty());
        let mut blank = FolderSearch::new(dir.clone(), None);
        assert!(!blank.refresh());
        _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_list_stays_on_screen_while_a_refresh_reads() {
        let dir = temp("refresh-hold");
        std::fs::write(dir.join("a.md"), "needle\n").unwrap();
        let (mut search, mut palette) = run(&dir, "needle");
        assert_eq!(palette.len(), 2);
        assert!(search.refresh());
        // Not polled yet: the old rows and the old footer are what is shown, and a new query
        // drops the pending refresh.
        assert_eq!(palette.len(), 2);
        assert_eq!(search.summary(), "1 match in 1 file");
        search.start("other", &mut palette);
        assert!(!search.refreshing());
        _ = std::fs::remove_dir_all(&dir);
    }
}
