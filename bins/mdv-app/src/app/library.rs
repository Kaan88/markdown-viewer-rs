//! The notes library in the window: the Notes face's clicks, and "Add this file to the notes
//! library…" (the viewer's one write to it; see `crate::notes`).

use super::*;
use crate::menu::{Item, Menu};
use crate::notes::{MISSING, Pending};
use crate::ui::{Act, DialogKind, Popup, PopupKind};

impl App {
    /// Looks for the notes library on a helper thread: mdedit's settings file is read and the
    /// folder it names looked at there, and the answer comes back as [`Wake::Library`]. Until
    /// then the head offers no Notes face and there is no add (audit 4, E12: a folder on a share
    /// that does not answer held the first frame).
    pub(super) fn look_for_library(&mut self) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        let settings = self.library_settings.clone();
        std::thread::spawn(move || {
            let found = crate::notes::find_library(settings.as_deref());
            let _ = proxy.send_event(Wake::Library(found));
        });
    }

    /// The loop is about to run (`run`): other threads can wake the window from now on, and it
    /// looks for the notes library. One call, so that a window is not given its proxy without
    /// the look; the test of the look drives this call, as `run` makes it (review of the audit 4
    /// fixes, W2).
    pub(super) fn loop_ready(&mut self, proxy: LoopProxy) {
        self.proxy = Some(proxy);
        self.look_for_library();
    }

    /// Where the notes library is, from [`look_for_library`](Self::look_for_library).
    pub(super) fn library_found(&mut self, found: crate::notes::Found) {
        if self.trace {
            let ms = self.started.elapsed().as_secs_f64() * 1e3;
            eprintln!("mdview: notes library looked for at {ms:.1} ms: {found:?}");
        }
        self.ui.notes.found(found);
        self.ws.set_library_offered(self.ui.notes.available);
        if self.args.notes_face {
            self.ui.notes.show(true);
        }
        self.wants_frame = true;
    }

    /// A switch of the Files pane's head was clicked: 0 the files, 1 the notes library.
    pub(super) fn pane_face(&mut self, face: usize) {
        let on = face == 1;
        if self.ui.notes.on != on {
            self.ui.panes.files_scroll = 0.0;
        }
        self.ui.notes.show(on);
    }

    /// A click on row `i` of the Notes face: a group or section folds, a note opens.
    pub(super) fn notes_row(&mut self, i: usize) {
        let Some(row) = self.ui.notes.rows.get(i).cloned() else {
            return;
        };
        match row.kind {
            mdshared_app::library::tree::Kind::Note { path, missing, .. } => {
                if missing || !path.exists() {
                    self.ws.set_status(MISSING);
                } else {
                    let _ = self.ws.open(&path);
                }
            }
            _ => self.ui.notes.toggle(i),
        }
    }

    /// The Refresh button: the library is read again.
    pub(super) fn notes_refresh(&mut self) {
        self.ui.notes.load();
    }

    /// "Add this file to the notes library…" for the tab at `index`.
    pub(super) fn library_add(&mut self, index: usize) {
        let Some(tab) = self.ws.tab(index) else {
            return;
        };
        if tab.is_untitled() || tab.is_missing() {
            self.ws.set_status(crate::notes::NEEDS_FILE);
            return;
        }
        let note = tab.path().to_path_buf();
        let Some(dir) = self.ui.notes.dir.clone() else {
            self.ws.set_status(crate::notes::NO_LIBRARY);
            return;
        };
        if !dir.is_dir() {
            self.ws.set_status(crate::notes::NO_FOLDER);
            return;
        }
        self.ui.notes.load();
        if let Some(title) = self.ui.notes.listed(&note) {
            self.ws.set_status(&crate::notes::already(&title));
            return;
        }
        let targets = self.ui.notes.targets();
        match targets.len() {
            0 => {
                let Some(file) = self.ui.notes.default_file() else {
                    return;
                };
                self.library_ask(file, "Notes".into(), note);
            }
            1 => {
                let (name, file) = targets.into_iter().next().unwrap_or_default();
                self.library_ask(file, name, note);
            }
            _ => {
                self.ui.notes.pending = Some(Pending {
                    file: PathBuf::new(),
                    note,
                    title: String::new(),
                    group: String::new(),
                });
                self.open_library_groups(&targets);
            }
        }
    }

    /// A group was picked in the menu of groups.
    pub(super) fn library_put_in(&mut self, i: usize) {
        let Some(p) = self.ui.notes.pending.take() else {
            return;
        };
        if let Some((name, file)) = self.ui.notes.targets().into_iter().nth(i) {
            self.library_ask(file, name, p.note);
        }
    }

    /// Asks the first time, writes at once after it.
    fn library_ask(&mut self, file: PathBuf, group: String, note: PathBuf) {
        let title = mdshared_app::library::title_of(&note);
        self.ui.notes.pending = Some(Pending {
            file,
            note,
            title,
            group,
        });
        if self.ws.library_ok() {
            self.library_write();
        } else {
            self.open_dialog(DialogKind::AddToLibrary);
        }
    }

    /// Writes the file that waited for the answer.
    pub(super) fn library_write(&mut self) {
        let Some(p) = self.ui.notes.pending.take() else {
            return;
        };
        match self.ui.notes.put(&p.file, &p.note) {
            Ok((title, group)) => {
                self.ws.set_library_ok();
                self.ws.set_status(&crate::notes::added(&title, &group));
            }
            Err(why) => self.ws.set_status(&why),
        }
    }

    /// The menu of groups, under the pointer.
    fn open_library_groups(&mut self, targets: &[(String, PathBuf)]) {
        let (x, y) = self.ui.mouse;
        let items: Vec<Item<Act>> = targets
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                Item::new(Act::LibraryPut(i), name).icon(crate::chrome::glyph::BOOKMARK)
            })
            .collect();
        let menu = Menu::new(items, x.max(8.0), y.max(8.0), false);
        self.ui.popup = Some(Popup::new(PopupKind::Library, menu));
        self.ui.tips.hide();
    }

    /// For a shot: the question, as asked of the first document.
    pub(super) fn library_add_for_shot(&mut self) {
        if let Some(i) = self.ws.active() {
            self.library_add(i);
        }
    }

    /// For a shot: the menu of groups under the first tab.
    pub(super) fn library_groups_for_shot(&mut self) {
        if let Some(r) = self.hit_rect(Hit::Tab(0)) {
            self.ui.mouse = (r.x + 30.0, r.bottom() + 6.0);
        }
        self.library_add_for_shot();
    }
}
