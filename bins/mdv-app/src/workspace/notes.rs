//! Wiki-links and backlinks: where `[[Note]]` leads, which wiki-links lead nowhere, and which
//! other notes link to the one being read.
//!
//! **What is read from disk.** With a folder open (the Files pane's, whether or not the pane
//! shows), `mdshared_app::notes` indexes the Markdown files under it, on its own thread, within
//! the `NOTES_*` limits of `mdview_core::limits`; the watcher's wakes refresh it. Without a
//! folder nothing is indexed and nothing is walked: a wiki-link is looked for in the folder of
//! the file that holds it (one directory listing when the document is read, no file opened), and
//! there are no backlinks. So opening a single file from a home folder or a drive's root lists
//! that one folder and stops.
//!
//! **What is a link.** A `[[name]]` whose note is found is a link. One whose note is not found
//! is a link to a missing note only while a folder is open; with none open it stays text as
//! written, because a file read on its own is ordinary Markdown first (`[[nodiscard]]`). The
//! parser asks [`Decider`] per name; `check_wiki_links` asks again when anything changed.
//!
//! The window sets a wake ([`Workspace::set_notes_wake`]) and the index is then built off the UI
//! thread. Without one (`--shot`, tests) nothing is built until [`Workspace::index_notes_now`].

use super::{LinkOutcome, Reveal, Workspace, read_stable, same_file};
use mdshared_app::notes::{Config, Indexer, NoteIndex, resolve_in_folder};
use mdshared_app::wikilink::parse_target;
use mdview_core::limits::{BACKLINKS_SHOWN, notes_config};
use mdview_core::links::{LinkAction, classify_link};
use mdview_core::model::{SpanKind, WikiAs};
use mdview_core::{Document, parse_bytes_wiki};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What `link_destination` puts before a wiki-link's target, so the destination can travel as a
/// string (a menu item's "Open link") and still be told from a URL.
pub const WIKI_SCHEME: &str = "wiki:";

/// One line of the Backlinks list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklinkEntry {
    /// The linking file.
    pub path: PathBuf,
    /// Its name without the extension.
    pub name: String,
    /// What the list shows for it: the name, with its folder before it when another linking
    /// file has the same name.
    pub shown: String,
    /// The line the link is on, trimmed.
    pub context: String,
    /// The nearest heading above the link.
    pub section: Option<String>,
    /// Line of the file, from 0.
    pub line: u32,
    /// Which of that file's links to this note this is, from 0.
    ordinal: usize,
}

pub(super) struct Notes {
    config: Config,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    indexer: Option<Indexer>,
    index: Option<Arc<NoteIndex>>,
    /// Raised whenever links may resolve differently: a new index, another folder, a watcher
    /// wake. A tab whose `links_checked` differs has its wiki-links looked up again.
    generation: u64,
    /// The active document's backlinks, and what they were computed for.
    backlinks: Vec<BacklinkEntry>,
    backlinks_for: Option<(PathBuf, u64)>,
}

impl Notes {
    pub(super) fn new() -> Notes {
        Notes {
            config: notes_config(),
            wake: None,
            indexer: None,
            index: None,
            generation: 1,
            backlinks: Vec::new(),
            backlinks_for: None,
        }
    }
}

impl Workspace {
    /// Lets the note index be built off this thread: `wake` is called from the indexer's thread
    /// when an index is ready, and the window then calls [`notes_ready`](Self::notes_ready).
    /// Starts indexing the folder already open, if any.
    pub fn set_notes_wake(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.notes.wake = Some(Arc::new(wake));
        self.notes_folder_changed();
    }

    /// The folder changed (or was closed): the old index is dropped, a build under way is
    /// cancelled, and the new folder is indexed if there is a wake to report it with.
    pub(super) fn notes_folder_changed(&mut self) {
        let folder = self.session.folder.clone();
        if self.notes.index.as_ref().map(|i| i.root()) != folder.as_deref() {
            self.notes.index = None;
            self.notes.generation += 1;
            self.dirty = true;
            self.watch_dirty = true;
        }
        let Some(wake) = self.notes.wake.clone() else {
            return;
        };
        if folder.is_none() && self.notes.indexer.is_none() {
            return;
        }
        let config = self.notes.config.clone();
        self.notes
            .indexer
            .get_or_insert_with(|| Indexer::new(config, move || wake()))
            .set_root(folder);
    }

    /// Indexes the open folder on this thread, for a shot or a test. Does nothing without one.
    pub fn index_notes_now(&mut self) {
        let Some(folder) = self.session.folder.clone() else {
            return;
        };
        let previous = self.notes.index.take();
        self.notes.index =
            NoteIndex::build(&folder, &self.notes.config, previous.as_deref(), &|| false)
                .map(Arc::new);
        self.notes.generation += 1;
        self.dirty = true;
    }

    /// The indexer woke the window: takes the new index. True if there was one.
    pub fn notes_ready(&mut self) -> bool {
        let Some(index) = self.notes.indexer.as_ref().and_then(Indexer::take) else {
            return false;
        };
        if Some(index.root()) != self.session.folder.as_deref() {
            return false;
        }
        self.notes.index = Some(index);
        self.notes.generation += 1;
        self.dirty = true;
        // The watcher follows the indexed notes too.
        self.watch_dirty = true;
        true
    }

    /// Files changed on disk (a watcher wake, a manual refresh): the index is walked again off
    /// thread, reading only what changed, and without an index the links are looked up again.
    pub fn notes_changed(&mut self) {
        if let Some(indexer) = self.notes.indexer.as_mut() {
            indexer.refresh();
        }
        if self.notes.index.is_none() {
            self.notes.generation += 1;
        }
    }

    /// Files and links the index holds and whether a limit cut it short; `None` without one.
    #[allow(dead_code)] // For traces and tests.
    pub fn notes_stats(&self) -> Option<mdshared_app::notes::Stats> {
        self.notes.index.as_ref().map(|i| i.stats())
    }

    /// The indexed notes, for the watcher: with them followed, a note saved anywhere in the
    /// folder wakes the window. Nothing when there are more than `max`; the index is then
    /// refreshed only by the wakes the open files and the Files tree cause.
    pub fn note_files(&self, max: usize) -> Vec<&Path> {
        match &self.notes.index {
            Some(index) if index.stats().files <= max => {
                index.notes().map(|n| n.path.as_path()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// The Markdown files of the open folder as the note index lists them (paths and the path
    /// below the folder, `/` between parts), at most `max`, in path order. The index is built on
    /// its own thread, so this reads memory only; it is empty until the index is ready, and with
    /// no folder open.
    pub fn folder_files(&self, max: usize) -> Vec<(&Path, &str)> {
        match &self.notes.index {
            Some(index) => index
                .notes()
                .take(max)
                .map(|n| (n.path.as_path(), n.rel.as_str()))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Where the note part of a wiki-link written in `from` leads, looking at the disk now.
    fn resolve_note(&self, from: &Path, note: &str) -> Option<PathBuf> {
        let config = &self.notes.config;
        let beside = || {
            from.parent()
                .and_then(|dir| resolve_in_folder(dir, note, config))
        };
        match &self.notes.index {
            Some(index) if index.covers(from) => {
                index.resolve(Some(from), note).map(|n| n.path.clone())
            }
            // A file outside the open folder: its own folder first, then the open one.
            Some(index) => beside().or_else(|| index.resolve(None, note).map(|n| n.path.clone())),
            None => beside(),
        }
    }

    /// Looks at the active document's wiki-links again if anything changed since they were last
    /// looked at. A link whose note appeared or went is marked; if a `[[name]]` that is text
    /// should be a link now, or the reverse (a folder was opened or closed, a note appeared
    /// beside the file), the file is read and parsed again, keeping the reading position. True
    /// if the page must be laid out again. Called before every layout check; it does nothing
    /// while nothing changed.
    pub(super) fn check_wiki_links(&mut self) -> bool {
        let generation = self.notes.generation;
        let Some(index) = self.active else {
            return false;
        };
        let tab = &self.tabs[index];
        if tab.links_checked == generation {
            return false;
        }
        let Some(doc) = tab.doc.as_ref() else {
            return false;
        };
        let folder_open = self.session.folder.is_some();
        // The answers now, for every link and for every `[[name]]` that was left as text.
        let mut missing = Vec::new();
        let mut reparse = false;
        if doc.links.iter().any(|l| l.wiki) || !doc.wiki_text.is_empty() {
            let mut decide = Decider::new(&self.notes, folder_open, &tab.path);
            for (i, link) in doc.links.iter().enumerate().filter(|(_, l)| l.wiki) {
                let Some(target) = parse_target(doc.str(link.dest)) else {
                    continue;
                };
                match decide.answer(target.note) {
                    WikiAs::Link => {}
                    WikiAs::Missing => missing.push(i),
                    WikiAs::Text => reparse = true,
                }
            }
            reparse |= doc
                .wiki_text
                .iter()
                .any(|&note| decide.answer(doc.str(note)) != WikiAs::Text);
        }
        if reparse && !tab.missing {
            return self.parse_again(index);
        }
        let tab = &mut self.tabs[index];
        tab.links_checked = generation;
        let mut changed = false;
        if let Some(doc) = tab.doc.as_mut() {
            for (i, link) in doc.links.iter_mut().enumerate() {
                let now = missing.binary_search(&i).is_ok();
                changed |= link.missing != now;
                link.missing = now;
            }
        }
        changed
    }

    /// Reads tab `index` from disk and parses it with today's answers about its wiki-links,
    /// without a message: to the reader nothing was reloaded, some brackets became links or
    /// links became brackets. False, and the page as it was, if the file cannot be read.
    fn parse_again(&mut self, index: usize) -> bool {
        let generation = self.notes.generation;
        let folder_open = self.session.folder.is_some();
        let height = self.viewport.map_or(0.0, |v| v.height as f32);
        let tab = &mut self.tabs[index];
        tab.links_checked = generation;
        let Ok(loaded) = read_stable(&tab.path) else {
            return false;
        };
        let same_file = loaded.stamp == tab.stamp;
        tab.drop_view(self.renderer.page_layout(), height);
        let document = parse_document(&self.notes, folder_open, &tab.path, &loaded.bytes);
        if let (Some(old), Some(pos)) = (tab.doc.as_ref(), tab.keep) {
            tab.keep = Some(document.remap_scroll(old, pos));
        }
        tab.set_document(document, loaded.stamp);
        tab.links_checked = generation;
        // Text offsets moved, so a selection would cover other text.
        tab.selection = None;
        if !same_file {
            // The file changed as well: its blocks are not the ones the toggles were made on.
            tab.details.clear();
            self.renderer.clear_details();
        }
        self.press = None;
        self.refind();
        self.dirty = true;
        true
    }

    /// What hovering link `index` of the active document shows: a wiki-link's file (with its
    /// heading), or that there is no such note; any other link's destination as written.
    pub fn link_hint(&self, index: u32) -> Option<String> {
        let tab = self.active_tab()?;
        let doc = tab.doc.as_ref()?;
        let link = doc.links.get(index as usize)?;
        let dest = doc.str(link.dest);
        if !link.wiki {
            return Some(dest.to_string());
        }
        let target = parse_target(dest)?;
        let heading = target.heading.map(|h| format!("#{h}")).unwrap_or_default();
        if target.note.is_empty() {
            return Some(heading);
        }
        Some(match self.resolve_note(&tab.path, target.note) {
            Some(path) => format!("{}{heading}", path.display()),
            None => no_note(target.note),
        })
    }

    /// Follows a wiki-link's target (`Note`, `Note#Heading`, `#Heading`) from the active
    /// document. A note that is not there is refused with a message; nothing is ever created.
    pub(super) fn open_wiki(&mut self, target: &str) -> LinkOutcome {
        let Some(target) = parse_target(target) else {
            return LinkOutcome::Ignored;
        };
        let fragment = target.last_heading().map(mdview_core::slug::slug_of);
        if target.note.is_empty() {
            return match fragment {
                Some(fragment) => self.jump_fragment(&fragment),
                None => LinkOutcome::Ignored,
            };
        }
        let from = self
            .active_tab()
            .map(|t| t.path.clone())
            .unwrap_or_default();
        match self.resolve_note(&from, target.note) {
            Some(path) => self.open_markdown(&path, fragment),
            None => LinkOutcome::Refused(self.refuse(no_note(target.note))),
        }
    }

    /// The notes that link to the active document, for the Backlinks list. Empty without an
    /// index, for a file outside the open folder, and when nothing links here.
    pub fn backlinks(&self) -> &[BacklinkEntry] {
        let current = self
            .active_tab()
            .is_some_and(|t| match &self.notes.backlinks_for {
                Some((path, generation)) => {
                    *generation == self.notes.generation && same_file(path, &t.path)
                }
                None => false,
            });
        if current { &self.notes.backlinks } else { &[] }
    }

    /// Brings [`backlinks`](Self::backlinks) up to date for the active tab. True if the list
    /// changed. Called once per frame; it does nothing while nothing changed.
    pub fn update_backlinks(&mut self) -> bool {
        let generation = self.notes.generation;
        let Some(path) = self.active_tab().map(|t| t.path.clone()) else {
            let had = !self.notes.backlinks.is_empty();
            self.notes.backlinks.clear();
            self.notes.backlinks_for = None;
            return had;
        };
        if self
            .notes
            .backlinks_for
            .as_ref()
            .is_some_and(|(p, g)| *g == generation && same_file(p, &path))
        {
            return false;
        }
        let mut entries: Vec<BacklinkEntry> = Vec::new();
        if let Some(index) = &self.notes.index {
            for backlink in index.backlinks(&path, BACKLINKS_SHOWN) {
                let from = &backlink.from.path;
                let ordinal = entries.iter().rev().take_while(|e| e.path == *from).count();
                entries.push(BacklinkEntry {
                    path: from.clone(),
                    name: backlink.from.name().to_string(),
                    shown: backlink.from.name().to_string(),
                    context: backlink.link.context.clone(),
                    section: backlink.link.section.clone(),
                    line: backlink.link.line,
                    ordinal,
                });
            }
        }
        // Two linking files of one name are told apart by their folder.
        for i in 0..entries.len() {
            let twin = entries
                .iter()
                .any(|e| e.name == entries[i].name && e.path != entries[i].path);
            let folder = entries[i]
                .path
                .parent()
                .and_then(Path::file_name)
                .map(|f| f.to_string_lossy().into_owned());
            if let (true, Some(folder)) = (twin, folder) {
                entries[i].shown = format!("{folder}/{}", entries[i].name);
            }
        }
        let changed = entries != self.notes.backlinks;
        self.notes.backlinks = entries;
        self.notes.backlinks_for = Some((path, generation));
        changed
    }

    /// Opens the file of backlink `index` at the link, as a jump Back returns from.
    pub fn open_backlink(&mut self, index: usize) -> bool {
        let Some(entry) = self.backlinks().get(index).cloned() else {
            return false;
        };
        let Some(target) = self.active_tab().map(|t| t.path.clone()) else {
            return false;
        };
        let from = self.place();
        let Ok(tab) = self.open(&entry.path) else {
            return false;
        };
        self.load(tab);
        // The link in the opened document: the same one, counted among its links to the note
        // the reader came from. The index counted in the file's text, so this is a best match.
        let spans = self.tabs[tab]
            .doc
            .as_ref()
            .map(|doc| self.links_to(&entry.path, doc, &target))
            .unwrap_or_default();
        if let Some(&(start, end)) = spans.get(entry.ordinal).or(spans.last()) {
            self.tabs[tab].reveal = Some(Reveal::Range(start, end));
            self.tabs[tab].selection = Some((start, end));
        }
        self.ensure_layout();
        self.dirty = true;
        self.remember(from);
        true
    }

    /// The text ranges of `doc`'s links (it is the file at `path`) that lead to `target`.
    fn links_to(&self, path: &Path, doc: &Document, target: &Path) -> Vec<(u32, u32)> {
        let dir = path.parent().unwrap_or(Path::new(""));
        doc.spans
            .iter()
            .filter_map(|span| {
                let SpanKind::Link(i) = span.kind else {
                    return None;
                };
                let link = doc.links.get(i as usize)?;
                let dest = doc.str(link.dest);
                let leads = if link.wiki {
                    let note = parse_target(dest)?.note;
                    !note.is_empty()
                        && self
                            .resolve_note(path, note)
                            .is_some_and(|p| same_file(&p, target))
                } else {
                    matches!(classify_link(dest, dir),
                        LinkAction::OpenMarkdown { path, .. } if same_file(&path, target))
                };
                leads.then_some((span.start, span.end))
            })
            .collect()
    }
}

/// Parses a file's bytes with its wiki-links decided against the notes as they are now.
pub(super) fn parse_document(
    notes: &Notes,
    folder_open: bool,
    path: &Path,
    bytes: &[u8],
) -> Document {
    let mut decide = Decider::new(notes, folder_open, path);
    parse_bytes_wiki(bytes, &mut |note| decide.answer(note))
}

/// Answers what each `[[note]]` of one document is. A note that is found is a link. One that is
/// not is a link to a missing note when a folder is open, and stays text when none is: a file
/// read on its own is ordinary Markdown first, where `[[nodiscard]]` is not a note. A link into
/// the same document (an empty name) follows the same rule, since its heading is not known yet.
struct Decider<'a> {
    notes: &'a Notes,
    folder_open: bool,
    path: &'a Path,
    /// The index when it covers this file.
    covered: Option<&'a NoteIndex>,
    /// Otherwise the names in the file's own folder, listed at the first question (one
    /// directory listing, nothing opened).
    listing: Option<Option<NoteIndex>>,
    answers: HashMap<String, WikiAs>,
}

impl<'a> Decider<'a> {
    fn new(notes: &'a Notes, folder_open: bool, path: &'a Path) -> Decider<'a> {
        Decider {
            notes,
            folder_open,
            path,
            covered: notes.index.as_deref().filter(|i| i.covers(path)),
            listing: None,
            answers: HashMap::new(),
        }
    }

    fn answer(&mut self, note: &str) -> WikiAs {
        if let Some(&known) = self.answers.get(note) {
            return known;
        }
        let found = note.is_empty() && self.folder_open || !note.is_empty() && self.find(note);
        let answer = match (found, self.folder_open) {
            (true, _) => WikiAs::Link,
            (false, true) => WikiAs::Missing,
            (false, false) => WikiAs::Text,
        };
        self.answers.insert(note.to_string(), answer);
        answer
    }

    fn find(&mut self, note: &str) -> bool {
        if let Some(index) = self.covered {
            return index.resolve(Some(self.path), note).is_some();
        }
        let config = &self.notes.config;
        let Some(dir) = self.path.parent() else {
            return false;
        };
        let beside = if note.contains(['/', '\\']) {
            resolve_in_folder(dir, note, config).is_some()
        } else {
            let listing = self.listing.get_or_insert_with(|| {
                let only_names = Config {
                    max_depth: 0,
                    max_file_bytes: 0,
                    ..config.clone()
                };
                NoteIndex::build(dir, &only_names, None, &|| false)
            });
            listing
                .as_ref()
                .is_some_and(|l| l.resolve(Some(self.path), note).is_some())
        };
        beside
            || self
                .notes
                .index
                .as_ref()
                .is_some_and(|index| index.resolve(None, note).is_some())
    }
}

fn no_note(name: &str) -> String {
    format!("No note named {name}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdview_core::files::canonical_or_absolute;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    static NEXT: AtomicU32 = AtomicU32::new(0);

    fn temp_dir(name: &str) -> PathBuf {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("mdview-notes-{name}-{}-{n}", std::process::id()));
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        canonical_or_absolute(&dir)
    }

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    /// The notes of `fixtures/wiki`, with the folder open and indexed.
    fn fixture() -> (Workspace, PathBuf) {
        let dir = canonical_or_absolute(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/wiki"),
        );
        let mut ws = Workspace::new(None);
        ws.set_viewport(800, 600, 1.0);
        ws.set_folder(Some(dir.clone()));
        ws.index_notes_now();
        (ws, dir)
    }

    fn show(ws: &mut Workspace) {
        ws.paint(800, 600, 1.0).unwrap();
        ws.update_backlinks();
    }

    /// The text and the missing flag of each wiki-link in the active document.
    fn wiki_links(ws: &Workspace) -> Vec<(String, bool)> {
        let doc = ws.active_document().unwrap();
        doc.spans
            .iter()
            .filter_map(|s| match s.kind {
                SpanKind::Link(i) if doc.links[i as usize].wiki => Some((
                    doc.text[s.start as usize..s.end as usize].to_string(),
                    doc.links[i as usize].missing,
                )),
                _ => None,
            })
            .collect()
    }

    fn missing(ws: &Workspace) -> Vec<String> {
        wiki_links(ws)
            .into_iter()
            .filter(|l| l.1)
            .map(|l| l.0)
            .collect()
    }

    fn link_named(ws: &Workspace, text: &str) -> u32 {
        let doc = ws.active_document().unwrap();
        doc.spans
            .iter()
            .find_map(|s| match s.kind {
                SpanKind::Link(i) if &doc.text[s.start as usize..s.end as usize] == text => Some(i),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no link shown as {text}"))
    }

    fn active_name(ws: &Workspace) -> String {
        let path = ws.active_tab().unwrap().path();
        path.strip_prefix(ws.folder().unwrap())
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[test]
    fn wiki_links_in_the_fixture_resolve_or_are_marked_missing() {
        let (mut ws, dir) = fixture();
        ws.open(&dir.join("Home.md")).unwrap();
        show(&mut ws);
        assert_eq!(wiki_links(&ws).len(), 12);
        assert_eq!(missing(&ws), ["Shopping list", "the budget", "Nowhere"]);

        // Hovering shows the file a link leads to, or that there is none.
        let hint = |ws: &Workspace, text: &str| ws.link_hint(link_named(ws, text)).unwrap();
        assert_eq!(
            hint(&ws, "Recipes"),
            dir.join("Recipes.md").display().to_string()
        );
        assert_eq!(
            hint(&ws, "the soup"),
            format!("{}#Soup", dir.join("Recipes.md").display())
        );
        assert_eq!(hint(&ws, "recipes"), hint(&ws, "Recipes"));
        assert_eq!(hint(&ws, "Table"), "#Table");
        assert_eq!(hint(&ws, "Shopping list"), "No note named Shopping list");
        assert_eq!(hint(&ws, "the notes"), "projects/Notes.md#open-questions");
        // Two notes named Plan are equally near the root: the first by path.
        assert!(hint(&ws, "Plan").ends_with("Plan.md"));
        assert!(hint(&ws, "Plan").contains("archive"));
        assert!(hint(&ws, "projects/Plan").contains("projects"));
    }

    #[test]
    fn following_a_wiki_link_opens_the_note_and_never_makes_one() {
        let (mut ws, dir) = fixture();
        ws.open(&dir.join("Home.md")).unwrap();
        show(&mut ws);

        let table = link_named(&ws, "Table");
        assert_eq!(ws.follow_link(table), LinkOutcome::Scrolled);

        let lost = link_named(&ws, "Shopping list");
        assert_eq!(
            ws.follow_link(lost),
            LinkOutcome::Refused("No note named Shopping list".into())
        );
        assert_eq!(
            ws.status_line().unwrap().text,
            "No note named Shopping list"
        );
        assert!(!dir.join("Shopping list.md").exists());
        assert_eq!(ws.tabs().len(), 1);

        let soup = link_named(&ws, "the soup");
        assert_eq!(ws.follow_link(soup), LinkOutcome::OpenedTab(1));
        show(&mut ws);
        assert_eq!(active_name(&ws), "Recipes.md");
        // Back returns to where the link was, as for any link.
        assert!(ws.go_back());
        assert_eq!(active_name(&ws), "Home.md");

        // The menu's "Open link" carries the same destination as a string.
        let plan = ws
            .link_destination(link_named(&ws, "projects/Plan"))
            .unwrap();
        assert_eq!(plan, "wiki:projects/Plan");
        assert_eq!(ws.open_link(&plan), LinkOutcome::OpenedTab(2));
        show(&mut ws);
        assert_eq!(active_name(&ws), "projects/Plan.md");
        // From here, Notes is the one beside this file and Recipes is found by name.
        assert!(missing(&ws).is_empty());
        let notes = link_named(&ws, "Notes");
        assert_eq!(ws.follow_link(notes), LinkOutcome::OpenedTab(3));
        show(&mut ws);
        // In projects, Plan is the note in the same folder, not the archived one.
        let hint = ws.link_hint(link_named(&ws, "Plan")).unwrap();
        assert!(hint.contains("projects") && !hint.contains("archive"));
    }

    #[test]
    fn backlinks_list_the_notes_that_link_here_and_open_at_the_link() {
        let (mut ws, dir) = fixture();
        assert_eq!(
            ws.notes_stats().map(|s| (s.files, s.truncated)),
            Some((5, false))
        );
        ws.open(&dir.join("Home.md")).unwrap();
        show(&mut ws);
        let seen = |ws: &Workspace| -> Vec<(String, String)> {
            ws.backlinks()
                .iter()
                .map(|b| (b.name.clone(), b.context.clone()))
                .collect()
        };
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(
            seen(&ws),
            [
                pair(
                    "Plan",
                    "Last year's plan. The current one is [[projects/Plan]]; [[Home]] lists both."
                ),
                pair("Plan", "- [x] Link back to [[Home]]"),
                pair("Recipes", "Back to [[Home]]."),
                pair(
                    "Recipes",
                    "Whatever is left on Friday. See the [[Home#Table|table at home]]."
                ),
            ]
        );
        assert_eq!(ws.backlinks()[3].section.as_deref(), Some("Soup"));

        ws.open(&dir.join("projects/Plan.md")).unwrap();
        show(&mut ws);
        let names: Vec<String> = seen(&ws).into_iter().map(|b| b.0).collect();
        // A wiki-link by path, one by name from the same folder, and a Markdown link.
        assert_eq!(names, ["Plan", "Home", "Notes", "Notes"]);
        // Home is linked from two files named Plan; each shows its folder.
        ws.activate(0);
        show(&mut ws);
        let shown: Vec<&str> = ws.backlinks().iter().map(|b| b.shown.as_str()).collect();
        assert_eq!(
            shown,
            ["archive/Plan", "projects/Plan", "Recipes", "Recipes"]
        );
        ws.activate(1);
        show(&mut ws);

        // The second link from Notes is the Markdown one: the file opens with it selected.
        assert!(ws.open_backlink(3));
        show(&mut ws);
        assert_eq!(active_name(&ws), "projects/Notes.md");
        let (from, to) = ws.selection().expect("the link is selected");
        let doc = ws.active_document().unwrap();
        assert_eq!(&doc.text[from as usize..to as usize], "the plan");
        // The list is now that of the note just opened.
        let names: Vec<String> = seen(&ws).into_iter().map(|b| b.0).collect();
        assert_eq!(names, ["Home", "Plan"]);
        assert!(ws.go_back());
        assert_eq!(active_name(&ws), "projects/Plan.md");
        assert!(!ws.open_backlink(99));
    }

    #[test]
    fn without_a_folder_only_the_files_own_folder_is_looked_at() {
        let dir = temp_dir("single");
        let a = write(
            &dir,
            "A.md",
            "[[Sibling]] [[sub/Deep]] [[Deep]] [[Nobody]]\n",
        );
        write(&dir, "sibling.markdown", "[[A]]\n");
        write(&dir, "sub/Deep.md", "[[A]]\n");
        let mut ws = Workspace::new(None);
        ws.open(&a).unwrap();
        show(&mut ws);
        // A path names its folder; a bare name is not searched for below this one. With no
        // folder open, what is not found is not a link at all: it stays as written.
        let shown: Vec<String> = wiki_links(&ws).into_iter().map(|l| l.0).collect();
        assert_eq!(shown, ["Sibling", "sub/Deep"]);
        assert!(missing(&ws).is_empty());
        assert_eq!(
            ws.active_document().unwrap().text.trim_end(),
            "Sibling sub/Deep [[Deep]] [[Nobody]]"
        );
        assert!(ws.notes_stats().is_none(), "nothing is indexed");
        assert!(ws.backlinks().is_empty());
        assert_eq!(
            ws.open_link("wiki:Deep"),
            LinkOutcome::Refused("No note named Deep".into())
        );
        assert_eq!(ws.open_link("wiki:sibling"), LinkOutcome::OpenedTab(1));
        // Nothing is indexed by asking, either.
        ws.index_notes_now();
        assert!(ws.notes_stats().is_none());
    }

    /// Ordinary Markdown that happens to hold double brackets (the document a review of the
    /// first wiki-link pass was rendered from).
    const ORDINARY: &str = "\
Citation style: see [[1]](https://example.com/one) and [[2]][two].

Array type `int[[3]]` in code, and plain [[not a note]] text. C++ attr: [[nodiscard]] int f();

A ref link [x] and [[x]].

[two]: https://example.com/two
[x]: https://example.com/x
";

    /// Text and destination of every link of the active document.
    fn all_links(ws: &Workspace) -> Vec<(String, String)> {
        let doc = ws.active_document().unwrap();
        doc.spans
            .iter()
            .filter_map(|s| match s.kind {
                SpanKind::Link(i) => Some((
                    doc.text[s.start as usize..s.end as usize].to_string(),
                    doc.str(doc.links[i as usize].dest).to_string(),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn ordinary_markdown_with_double_brackets_reads_as_it_always_did() {
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        let commonmark = [
            pair("[1]", "https://example.com/one"),
            pair("[2]", "https://example.com/two"),
            pair("x", "https://example.com/x"),
            pair("x", "https://example.com/x"),
        ];
        let dir = temp_dir("ordinary");
        let file = write(&dir, "readme.md", ORDINARY);
        let mut ws = Workspace::new(None);
        ws.open(&file).unwrap();
        show(&mut ws);

        // No folder open: nothing is a wiki-link, and every line reads as plain Markdown.
        let plain = mdview_core::parse(ORDINARY);
        let doc = ws.active_document().unwrap();
        assert_eq!((&doc.text, &doc.spans), (&plain.text, &plain.spans));
        assert_eq!(all_links(&ws), commonmark);
        assert_eq!(
            doc.text,
            "Citation style: see [1] and [2].\n\
             Array type int[[3]] in code, and plain [[not a note]] text. C++ attr: [[nodiscard]] int f();\n\
             A ref link x and [x].\n"
        );

        // A folder is opened: the two that mean nothing to CommonMark are links to notes that
        // are missing; the citation, the reference links and the code are untouched.
        ws.set_folder(Some(dir.clone()));
        ws.index_notes_now();
        show(&mut ws);
        assert_eq!(missing(&ws), ["not a note", "nodiscard"]);
        let others: Vec<(String, String)> = all_links(&ws)
            .into_iter()
            .filter(|l| l.1.starts_with("https://"))
            .collect();
        assert_eq!(others, commonmark);
        let text = ws.active_document().unwrap().text.clone();
        assert!(text.starts_with("Citation style: see [1] and [2].\n"));
        assert!(text.contains("int[[3]] in code, and plain not a note text."));
        assert!(text.contains("C++ attr: nodiscard int f();"));
        assert!(text.contains("A ref link x and [x]."));

        // The note appears: found now, and it stays a link when the folder is closed, because
        // it is beside the document. The other goes back to being text.
        write(&dir, "Not a Note.md", "# here\n");
        ws.index_notes_now();
        show(&mut ws);
        assert_eq!(missing(&ws), ["nodiscard"]);
        ws.set_folder(None);
        show(&mut ws);
        assert_eq!(wiki_links(&ws), [("not a note".to_string(), false)]);
        let text = ws.active_document().unwrap().text.clone();
        assert!(text.contains("plain not a note text. C++ attr: [[nodiscard]] int f();"));
        assert_eq!(
            all_links(&ws)
                .into_iter()
                .filter(|l| l.1.starts_with("https://"))
                .collect::<Vec<_>>(),
            commonmark
        );
        assert!(
            ws.status_line().is_none(),
            "none of this is announced as a reload"
        );
    }

    #[test]
    fn a_new_note_and_a_new_link_show_after_the_index_is_refreshed() {
        let dir = temp_dir("refresh");
        let a = write(&dir, "A.md", "[[B]] and [[Later]]\n");
        let b = write(&dir, "B.md", "# B\n");
        let mut ws = Workspace::new(None);
        ws.set_folder(Some(dir.clone()));
        ws.index_notes_now();
        ws.open(&a).unwrap();
        show(&mut ws);
        assert_eq!(missing(&ws), ["Later"]);

        write(&dir, "deep/down/Later.md", "[[B]] from a new note\n");
        ws.index_notes_now();
        show(&mut ws);
        assert!(missing(&ws).is_empty(), "the link is drawn as found now");

        ws.open(&b).unwrap();
        show(&mut ws);
        let names: Vec<&str> = ws.backlinks().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["A", "Later"]);

        // Another folder: the old index answers nothing.
        let other = temp_dir("refresh-other");
        ws.set_folder(Some(other));
        show(&mut ws);
        assert!(ws.backlinks().is_empty());
        ws.set_folder(None);
        ws.activate(0);
        show(&mut ws);
        // Only A's own folder is looked at again, and Later is not in it: text, not a link.
        assert!(missing(&ws).is_empty());
        assert_eq!(
            ws.active_document().unwrap().text.trim_end(),
            "B and [[Later]]"
        );
    }

    #[test]
    fn the_index_is_built_off_thread_and_refreshed_after_a_change() {
        let dir = temp_dir("thread");
        write(&dir, "A.md", "[[B]]\n");
        let b = write(&dir, "B.md", "# B\n");
        let (send, woken) = std::sync::mpsc::channel();
        let mut ws = Workspace::new(None);
        ws.set_notes_wake(move || _ = send.send(()));
        ws.open(&b).unwrap();
        assert!(!ws.notes_ready(), "no folder, no index");

        ws.set_folder(Some(dir.clone()));
        woken.recv_timeout(Duration::from_secs(10)).expect("built");
        assert!(ws.notes_ready());
        show(&mut ws);
        assert_eq!(ws.backlinks().len(), 1);
        assert_eq!(ws.note_files(10).len(), 2);
        assert!(ws.note_files(1).is_empty());

        // What the window does when the watcher wakes it.
        write(&dir, "C.md", "also [[B]]\n");
        ws.notes_changed();
        woken
            .recv_timeout(Duration::from_secs(10))
            .expect("refreshed");
        assert!(ws.notes_ready());
        show(&mut ws);
        assert_eq!(ws.backlinks().len(), 2);
        let stats = ws.notes_stats().unwrap();
        assert_eq!((stats.read_files, stats.reused_files), (1, 2));

        // Closing the folder drops the index at once.
        ws.set_folder(None);
        show(&mut ws);
        assert!(ws.backlinks().is_empty() && ws.notes_stats().is_none());
    }
}
