//! Crash reports in mdview: where they go, the context a report carries, and the bar that offers
//! last time's report on the next start. The reports themselves are `mdshared_app::crash`.

use super::*;
use mdshared_app::crash;
use mdview::commands::strings;

/// The folder of mdview's crash reports, under its data folder.
pub fn crash_dir() -> Option<PathBuf> {
    mdshared_app::settings::data_dir("mdview").map(|d| d.join("crashes"))
}

/// What a report says mdview was doing, as a key that changes only when the text would: the
/// active document's size (by 16 KB), the tab count, a folder open, a presentation. Never a
/// document's text or a file's name.
#[derive(Clone, PartialEq, Default)]
pub(super) struct CrashKey {
    size_16k: Option<u64>,
    tabs: usize,
    folder: bool,
    presenting: bool,
}

impl App {
    /// Updates the context of a report when it changed; a few comparisons a frame.
    pub(super) fn crash_context(&mut self) {
        let key = CrashKey {
            size_16k: self.ws.active_size().map(|n| n as u64 / 16_384),
            tabs: self.ws.tab_count(),
            folder: self.ws.folder().is_some(),
            presenting: self.ui.present.is_some(),
        };
        if key != self.crash_key {
            crash::set_context(&context_text(&key));
            self.crash_key = key;
        }
    }

    /// At launch: the report of a crash last time, if there is one, gets its bar.
    pub(super) fn look_for_crash(&mut self) {
        self.ws
            .set_crash_report(crash_dir().and_then(|d| crash::pending(&d)));
    }

    /// The bar's buttons: open the report in the system's text editor (mdview shows Markdown),
    /// copy it, or close the bar. Each marks the report seen, so it is not offered again; it
    /// stays in the folder.
    pub(super) fn crash_bar(&mut self, hit: Hit) {
        let Some(report) = self.ws.crash_report().map(Path::to_path_buf) else {
            return;
        };
        self.ws.set_crash_report(None);
        let seen = crash::mark_seen(&report).unwrap_or(report);
        match hit {
            Hit::CrashOpen => {
                if !mdshared_ui::platform::open_file(&seen, &["txt"]) {
                    self.ws.set_status(strings::CRASH_REPORT_NOT_OPENED);
                }
            }
            Hit::CrashCopy => {
                let text = std::fs::read_to_string(&seen).unwrap_or_default();
                if !text.is_empty() && platform::set_clipboard_text(&text) {
                    self.ws.confirm(strings::CRASH_REPORT_COPIED);
                }
            }
            _ => {}
        }
        self.full = true;
        self.redraw();
    }
}

fn context_text(k: &CrashKey) -> String {
    let doc = match k.size_16k {
        Some(n) => format!("a document of {} KB", n * 16),
        None => "no document".to_string(),
    };
    let mut s = format!(
        "{doc}; {} tab{}",
        k.tabs,
        if k.tabs == 1 { "" } else { "s" }
    );
    if k.folder {
        s.push_str("; a folder open");
    }
    if k.presenting {
        s.push_str("; presenting");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_context_names_sizes_only() {
        let k = CrashKey {
            size_16k: Some(3),
            tabs: 2,
            folder: true,
            presenting: false,
        };
        assert_eq!(
            context_text(&k),
            "a document of 48 KB; 2 tabs; a folder open"
        );
        assert_eq!(
            context_text(&CrashKey {
                tabs: 0,
                ..Default::default()
            }),
            "no document; 0 tabs"
        );
    }
}
