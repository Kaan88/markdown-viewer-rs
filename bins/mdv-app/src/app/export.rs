//! Export as HTML and Print: the active document written as one self-contained HTML file.
//!
//! Export asks where to save in the system's Save dialog, which runs on a helper thread like the
//! Open dialogs; the HTML is then made on that same thread (formulas and diagrams are drawn by
//! the engine process, which can take a moment) and the window only hears how it went. Print has
//! no dialog: it writes the page to a temporary file and opens it in the default browser, which
//! shows its own print dialog. Neither touches the page on screen.

use super::*;
use mdview_core::Document;
use mdview_render::{ExportOptions, Theme, export_html};

/// How an export or print went, sent from its helper thread to the window.
pub enum Progress {
    /// The destination is chosen (or Print started) and the page is being made.
    Working,
    /// The file was written.
    Saved(PathBuf),
    /// The browser was started on the printable page.
    Printing,
    Failed(&'static str),
    /// The Save dialog was closed without a file, or could not open.
    Cancelled,
}

const WORKING: &str = "Exporting…";
const PRINT_WORKING: &str = "Preparing to print…";
const PRINT_OPENED: &str = "Opened in the browser to print";
const FAILED_WRITE: &str = "Could not save the HTML file";
const FAILED_PRINT: &str = "Could not open the browser to print";
const BUSY: &str = "Another export is still running";

/// Printable pages older than this are removed from the temporary folder at the next print.
const PRINT_KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// What the helper thread needs, copied from the active tab so the window stays free.
pub(super) struct Job {
    doc: Document,
    /// The file name without its extension: the page's title when the text has no heading.
    stem: String,
    base_dir: Option<PathBuf>,
    theme: Theme,
    remote: bool,
}

impl Job {
    /// The page, or `None` if making it failed outright: a panic must not leave the window
    /// believing an export is still running.
    fn html(&self, print: bool) -> Option<String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.page(print))).ok()
    }

    pub(super) fn page(&self, print: bool) -> String {
        let title = if self.doc.headings.is_empty() {
            self.stem.as_str()
        } else {
            ""
        };
        export_html(
            &self.doc,
            &ExportOptions {
                title,
                theme: self.theme,
                base_dir: self.base_dir.as_deref(),
                remote_images: self.remote,
                print,
            },
        )
    }
}

/// The name the Save dialog suggests: the document's, with `.html`, in the document's folder.
fn suggested(path: &Path) -> PathBuf {
    let mut out = path.to_path_buf();
    out.set_extension("html");
    out
}

/// The file the Save dialog answered with, given the `.html` extension when it has none.
fn with_extension(mut path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension("html");
    }
    path
}

/// A file name made of the document's stem that no file system refuses.
fn safe_stem(stem: &str) -> String {
    let s: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .take(60)
        .collect();
    if s.trim_matches(['.', '_']).is_empty() {
        "document".to_string()
    } else {
        s
    }
}

/// Where a printable copy goes: `mdview-print` in the temporary folder, one file per print.
fn print_path(stem: &str, now_ms: u128) -> PathBuf {
    std::env::temp_dir()
        .join("mdview-print")
        .join(format!("{}-{now_ms}.html", safe_stem(stem)))
}

/// Removes printable pages of earlier days from `dir`, so the folder does not grow.
fn clean_print_folder(dir: &Path, keep: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > keep);
        if old && path.extension().is_some_and(|e| e == "html") {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn send(proxy: &LoopProxy, progress: Progress) {
    let _ = proxy.send_event(Wake::Export(progress));
}

impl App {
    /// The active document, copied for a helper thread; `None` with no document.
    pub(super) fn export_job(&self) -> Option<Job> {
        let doc = self.ws.active_document()?.clone();
        let path = self.ws.active_tab()?.path();
        Some(Job {
            doc,
            stem: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            base_dir: path.parent().map(Path::to_path_buf),
            theme: self.ws.export_theme(),
            remote: self.ws.remote_images_allowed(),
        })
    }

    /// Export as HTML: the Save dialog, then the file.
    pub(super) fn export_html(&mut self) {
        if self.exporting {
            self.ws.set_status(BUSY);
            return;
        }
        let (Some(job), Some(proxy)) = (self.export_job(), self.proxy.clone()) else {
            return;
        };
        let Some(path) = self.ws.active_tab().map(|t| suggested(t.path())) else {
            return;
        };
        // Set before the dialog starts: when it cannot open, its answer (a cancel) comes back as
        // an event and clears this again.
        self.exporting = true;
        platform::pick_save_async(&path, move |chosen| {
            let Some(chosen) = chosen else {
                send(&proxy, Progress::Cancelled);
                return;
            };
            let chosen = with_extension(chosen);
            send(&proxy, Progress::Working);
            let result = match job.html(false) {
                Some(html) => std::fs::write(&chosen, html.as_bytes()),
                None => Err(std::io::ErrorKind::Other.into()),
            };
            send(
                &proxy,
                match result {
                    Ok(()) => Progress::Saved(chosen),
                    Err(_) => Progress::Failed(FAILED_WRITE),
                },
            );
        });
    }

    /// Print: a printable copy in the temporary folder, opened in the default browser.
    pub(super) fn print_page(&mut self) {
        if self.exporting {
            self.ws.set_status(BUSY);
            return;
        }
        let (Some(job), Some(proxy)) = (self.export_job(), self.proxy.clone()) else {
            return;
        };
        self.exporting = true;
        self.ws.set_status(PRINT_WORKING);
        let spawned = std::thread::Builder::new()
            .name("mdview-print".into())
            .spawn({
                let proxy = proxy.clone();
                move || {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis());
                    let path = print_path(&job.stem, now);
                    let made = path.parent().is_some_and(|dir| {
                        clean_print_folder(dir, PRINT_KEEP);
                        std::fs::create_dir_all(dir).is_ok()
                    }) && job
                        .html(true)
                        .is_some_and(|html| std::fs::write(&path, html.as_bytes()).is_ok());
                    let opened = made && platform::open_file(&path, &["html", "htm"]);
                    send(
                        &proxy,
                        if opened {
                            Progress::Printing
                        } else {
                            Progress::Failed(FAILED_PRINT)
                        },
                    );
                }
            });
        if spawned.is_err() {
            self.exporting = false;
            self.ws.set_status(FAILED_PRINT);
        }
    }

    /// A helper thread's report, on the window's thread.
    pub(super) fn export_progress(&mut self, progress: Progress) {
        match progress {
            Progress::Working => self.ws.set_status(WORKING),
            Progress::Saved(path) => {
                self.exporting = false;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.ws.set_status(&format!("Exported to {name}"));
            }
            Progress::Printing => {
                self.exporting = false;
                self.ws.set_status(PRINT_OPENED);
            }
            Progress::Failed(message) => {
                self.exporting = false;
                self.ws.set_status(message);
            }
            Progress::Cancelled => self.exporting = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_save_dialog_suggests_the_documents_name_with_html() {
        assert_eq!(
            suggested(Path::new("/docs/read me.md")),
            PathBuf::from("/docs/read me.html")
        );
        assert_eq!(
            suggested(Path::new("/docs/notes")),
            PathBuf::from("/docs/notes.html")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/out")),
            PathBuf::from("/x/out.html")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/out.htm")),
            PathBuf::from("/x/out.htm")
        );
    }

    #[test]
    fn a_printable_copy_gets_a_safe_name_in_its_own_folder() {
        let p = print_path("my: file/name?", 42);
        assert_eq!(p.file_name().unwrap(), "my__file_name_-42.html");
        assert_eq!(p.parent().unwrap().file_name().unwrap(), "mdview-print");
        assert_eq!(safe_stem(""), "document");
        assert_eq!(safe_stem("..."), "document");
        assert_eq!(safe_stem("Übersicht"), "Übersicht");
        assert_eq!(safe_stem(&"a".repeat(200)).len(), 60);
    }

    #[test]
    fn only_old_html_files_leave_the_print_folder() {
        let dir = std::env::temp_dir().join(format!("mdview-print-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.html"), "x").unwrap();
        std::fs::write(dir.join("keep.txt"), "x").unwrap();
        // Nothing is older than a day, so nothing goes.
        clean_print_folder(&dir, PRINT_KEEP);
        assert!(dir.join("a.html").exists());
        // Everything is older than nothing, but only the page is removed.
        std::thread::sleep(Duration::from_millis(20));
        clean_print_folder(&dir, Duration::from_millis(1));
        assert!(!dir.join("a.html").exists());
        assert!(dir.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
