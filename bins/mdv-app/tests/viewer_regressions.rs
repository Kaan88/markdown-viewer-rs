//! Regressions for viewer policy and the shared parser used through mdview's file paths.

use mdview::commands::{self, CommandId, CommandStyle, Menu, Shortcut, ToolbarItem, strings};
use mdview::files::{self, FileStamp, LoadError};
use mdview::session::*;
use mdview::status::*;
use mdview_core::find::find_in_text;
use mdview_core::limits::{FIND_CAP, MAX_DECODED_BYTES, MAX_FILE_BYTES};
use mdview_core::model::{Document, EngineJob, Warning};
use mdview_core::parse;
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let in_root = |root: PathBuf| {
        let dir = root.join(format!("mdview-regression-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).ok().map(|()| dir)
    };
    in_root(std::env::temp_dir())
        .or_else(|| in_root(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp")))
        .expect("a writable directory for test files")
}

#[test]
fn refreshing_status_never_sticks() {
    for outcome in [
        ReloadOutcome::Ok { warnings: vec![] },
        ReloadOutcome::Ok {
            warnings: vec!["w".into()],
        },
        ReloadOutcome::Failed("bad".into()),
        ReloadOutcome::Missing { had_content: true },
        ReloadOutcome::Missing { had_content: false },
    ] {
        let mut t = ReloadTracker::default();
        let (generation, shown) = t.begin(true);
        assert_eq!(shown.unwrap().text, strings::REFRESHING);
        let done = t
            .finished(generation, outcome)
            .expect("the final message replaces Refreshing");
        assert_ne!(done.text, strings::REFRESHING);
        assert!(!t.is_running());
    }
}

#[test]
fn refresh_success_is_transient_and_silent_when_automatic() {
    let mut t = ReloadTracker::default();
    let (generation, _) = t.begin(false);
    t.finished(generation, ReloadOutcome::Ok { warnings: vec![] });
    let (generation, shown) = t.begin(false);
    assert!(shown.is_none());
    let done = t
        .finished(generation, ReloadOutcome::Ok { warnings: vec![] })
        .unwrap();
    assert_eq!(done.text, strings::REFRESHED);
    assert_eq!(done.timeout_ms, Some(2500));
}

#[test]
fn timeout_message_is_not_overwritten() {
    let mut t = ReloadTracker::default();
    let (generation, _) = t.begin(false);
    assert_eq!(
        t.timed_out(generation).unwrap().text,
        strings::RENDER_TIMEOUT
    );
    // Killing the worker makes it report a failure. The old app showed that instead.
    assert_eq!(
        t.finished(
            generation,
            ReloadOutcome::Failed("Could not render document: ".into())
        ),
        None
    );
}

#[test]
fn stale_generation_is_ignored() {
    let mut t = ReloadTracker::default();
    let (old, _) = t.begin(false);
    let (new, _) = t.begin(false);
    assert_eq!(t.finished(old, ReloadOutcome::Failed("late".into())), None);
    assert!(
        t.finished(new, ReloadOutcome::Failed("current".into()))
            .is_some()
    );
}

#[test]
fn find_cap_is_ten_thousand() {
    let text = "ab ".repeat(FIND_CAP + 5);
    let r = find_in_text(&text, "AB", FIND_CAP);
    assert_eq!(r.hits.len(), 10_000);
    assert!(r.capped);
    let exact = "ab ".repeat(FIND_CAP);
    assert!(!find_in_text(&exact, "ab", FIND_CAP).capped);
    assert_eq!(
        strings::find_label(false, 0, 10_000, true),
        "10000+ matches"
    );
}

#[test]
fn find_labels_match_the_old_app() {
    assert_eq!(strings::find_label(true, 0, 0, false), "");
    assert_eq!(strings::find_label(false, 0, 0, false), "No matches");
    assert_eq!(strings::find_label(false, 0, 2, false), "2 matches");
    assert_eq!(strings::find_label(false, 1, 2, false), "1 of 2");
    assert_eq!(
        strings::find_tooltip(2, false),
        "2 matches in this document"
    );
    assert_eq!(
        strings::find_tooltip(0, false),
        "No matches in this document"
    );
}

#[test]
fn invalid_utf8_file_loads_with_a_warning() {
    let dir = temp_dir("invalid");
    let path = dir.join("bad.md");
    std::fs::write(&path, b"# ok\n\nbad \xFF byte\n").unwrap();
    let (d, stamp) = files::load_document(&path).unwrap();
    assert!(d.text.contains('\u{FFFD}'));
    assert_eq!(d.warnings, [Warning::InvalidUtf8]);
    assert!(matches!(stamp, FileStamp::Present { .. }));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn file_limit_is_enforced() {
    let dir = temp_dir("limit");
    let path = dir.join("big.md");
    std::fs::write(&path, vec![b'a'; MAX_FILE_BYTES + 1]).unwrap();
    assert_eq!(files::load_document(&path).err(), Some(LoadError::TooLarge));
    assert_eq!(
        LoadError::TooLarge.to_string(),
        "File exceeds the 16 MiB viewer limit"
    );
    let d = mdview_core::parse_bytes(&vec![b'a'; MAX_FILE_BYTES + 1]);
    assert_eq!(d.warnings, [Warning::FileTooLarge]);
    let d = parse(&"a".repeat(MAX_DECODED_BYTES + 1));
    assert_eq!(d.warnings, [Warning::FileTooLarge]);
    assert!(d.blocks.is_empty());
    std::fs::write(&path, vec![b'a'; MAX_FILE_BYTES]).unwrap();
    assert!(files::load_document(&path).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn block_tree_invariants_on_the_showcase() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/showcase.md"),
    )
    .unwrap();
    let d = parse(&src);
    common_invariants(&d);
    assert_eq!(d.str(d.headings[0].title), "Native Markdown Reader");
    let dup: Vec<&str> = d
        .headings
        .iter()
        .map(|h| d.str(h.slug))
        .filter(|s| s.starts_with("repeated-heading"))
        .collect();
    assert_eq!(dup, ["repeated-heading", "repeated-heading-1"]);
    assert!(
        d.engine_jobs()
            .iter()
            .filter(|j| matches!(j, EngineJob::Diagram { .. }))
            .count()
            >= 8
    );
}

fn common_invariants(d: &Document) {
    for (i, b) in d.blocks.iter().enumerate() {
        assert!(b.end as usize > i && b.end as usize <= d.blocks.len());
        assert!(
            d.text.is_char_boundary(b.text.start as usize)
                && d.text.is_char_boundary(b.text.end as usize)
        );
    }
}

#[test]
fn session_round_trips_and_tolerates_damage() {
    let dir = temp_dir("session");
    let path = dir.join("nested/settings.ini");
    let mut s = Session {
        tabs: vec!["/a b/Türkçe test.md".into(), "/x%y\n.md".into()],
        active: 1,
        theme: Theme::Dark,
        zoom: 150,
        ..Session::default()
    };
    s.recent.add("/r2.md");
    s.recent.add("/r1.md");
    s.folder = Some("/folder".into());
    s.geometry = "AdnQywAD=\n%x".into();
    s.save(&path).unwrap();
    assert_eq!(Session::load(&path), s);
    assert_eq!(s.recent.paths()[0], Path::new("/r1.md"));

    assert_eq!(Session::load(&dir.join("absent.ini")), Session::default());
    let damaged = Session::parse("garbage\nzoom=9999\ntheme=7\nactive=44\ntab=/a.md\n=\nzoom=\n");
    assert_eq!(damaged.zoom, ZOOM_DEFAULT);
    assert_eq!(damaged.theme, Theme::System);
    assert_eq!(damaged.active, 0);
    assert_eq!(Session::parse("zoom=9999\n").zoom, ZOOM_MAX);
    assert_eq!(Session::parse("zoom=1\n").zoom, ZOOM_MIN);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn session_keeps_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt;
    let p = PathBuf::from(std::ffi::OsString::from_vec(b"/a\xFFb.md".to_vec()));
    let s = Session {
        tabs: vec![p.clone()],
        ..Session::default()
    };
    assert_eq!(Session::parse(&s.serialize()).tabs, [p]);
}

#[test]
fn zoom_range_and_step() {
    assert_eq!(zoom_in(100), 110);
    assert_eq!(zoom_out(100), 90);
    assert_eq!(zoom_in(250), 250);
    assert_eq!(zoom_out(60), 60);
}

#[test]
fn stamps_and_paths() {
    let dir = temp_dir("stamp");
    let p = dir.join("a.md");
    assert_eq!(files::stamp(&p), FileStamp::Missing);
    std::fs::write(&p, "x").unwrap();
    assert!(matches!(
        files::stamp(&p),
        FileStamp::Present { size: 1, .. }
    ));
    assert!(files::canonical_or_absolute(&p).is_absolute());
    assert!(files::canonical_or_absolute(&dir.join("gone.md")).is_absolute());
    assert!(matches!(
        files::load_document(&dir.join("gone.md")),
        Err(LoadError::Io(_))
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn command_table_matches_the_inventory() {
    let keys = |id| commands::command(id).shortcuts.to_vec();
    assert_eq!(keys(CommandId::OpenFiles), [Shortcut::Keys("Ctrl+O")]);
    assert_eq!(
        keys(CommandId::OpenFolder),
        [Shortcut::Keys("Ctrl+Shift+O")]
    );
    assert_eq!(keys(CommandId::Refresh), [Shortcut::Keys("F5")]);
    assert_eq!(
        keys(CommandId::CloseTab),
        [Shortcut::Keys("Ctrl+W"), Shortcut::Keys("Ctrl+F4")]
    );
    assert_eq!(
        keys(CommandId::CloseAllTabs),
        [Shortcut::Keys("Ctrl+Shift+W")]
    );
    assert_eq!(keys(CommandId::Find), [Shortcut::Keys("Ctrl+F")]);
    assert_eq!(keys(CommandId::FindNext), [Shortcut::Keys("F3")]);
    assert_eq!(keys(CommandId::FindPrevious), [Shortcut::Keys("Shift+F3")]);
    assert_eq!(keys(CommandId::ZoomReset), [Shortcut::Keys("Ctrl+0")]);
    assert_eq!(
        keys(CommandId::ToggleFiles),
        [Shortcut::Keys("Ctrl+Shift+1")]
    );
    assert_eq!(
        keys(CommandId::ToggleOutline),
        [Shortcut::Keys("Ctrl+Shift+2")]
    );
    assert_eq!(keys(CommandId::FocusMode), [Shortcut::Keys("Ctrl+Shift+F")]);
    assert_eq!(commands::menu_items(Menu::File).count(), 11);
    assert_eq!(commands::menu_items(Menu::Edit).count(), 4);
    assert_eq!(commands::menu_items(Menu::View).count(), 8);
    assert_eq!(commands::menu_items(Menu::Theme).count(), 3);
    assert_eq!(commands::menu_items(Menu::Help).count(), 2);
    assert_eq!(commands::menu_items(Menu::TabContext).count(), 5);
    assert!(commands::menu_items(Menu::File).any(|c| c.label == "Clear recent files"));
    let toolbar: Vec<_> = commands::TOOLBAR
        .iter()
        .filter_map(|item| match item {
            ToolbarItem::Button { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(
        toolbar,
        [
            CommandId::OpenFiles,
            CommandId::OpenFolder,
            CommandId::Refresh,
            CommandId::FindPrevious,
            CommandId::FindNext,
        ]
    );
}

#[test]
fn shortcuts_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for c in commands::COMMANDS {
        for s in c.shortcuts {
            if let Shortcut::Keys(k) = s {
                assert!(seen.insert(*k), "{k} bound twice");
            }
        }
    }
    let help = commands::shortcut_help();
    assert!(
        help.iter()
            .any(|(k, l)| k == "Ctrl+W, Ctrl+F4" && *l == "Close tab")
    );
    assert_eq!(
        strings::window_title(Some("a.md")),
        "a.md — Markdown Viewer"
    );
    assert_eq!(strings::window_title(None), "Markdown Viewer");
}

#[test]
fn first_load_is_not_announced_as_a_refresh() {
    let ok = || ReloadOutcome::Ok { warnings: vec![] };
    let mut t = ReloadTracker::default();
    let (g, _) = t.begin(false);
    assert_eq!(t.finished(g, ok()), None);
    assert!(!t.is_running());
    let (g, _) = t.begin(false);
    assert_eq!(t.finished(g, ok()).unwrap().text, strings::REFRESHED);

    // Nothing can stay stuck: a first load clears what was showing, without saying "refreshed".
    let mut t = ReloadTracker::default();
    let (g, _) = t.begin(true);
    let done = t.finished(g, ok()).unwrap();
    assert_eq!(done.text, "");
    let (g, _) = t.begin(false);
    assert_eq!(t.finished(g, ok()).unwrap().text, strings::REFRESHED);

    let mut t = ReloadTracker::default();
    let (g, _) = t.begin(false);
    let missing = ReloadOutcome::Missing { had_content: false };
    assert_eq!(
        t.finished(g, missing).unwrap().text,
        strings::FILE_NOT_FOUND
    );
    let (g, _) = t.begin(false);
    assert_eq!(t.finished(g, ok()).unwrap().text, "");

    // Warnings are shown on a first load too, and a later failure doesn't make it first again.
    let mut t = ReloadTracker::default();
    let (g, _) = t.begin(false);
    let warned = ReloadOutcome::Ok {
        warnings: vec!["w".into()],
    };
    assert_eq!(t.finished(g, warned).unwrap().text, "w");
    let (g, _) = t.begin(false);
    t.finished(g, ReloadOutcome::Failed("bad".into()));
    let (g, _) = t.begin(false);
    assert_eq!(t.finished(g, ok()).unwrap().text, strings::REFRESHED);

    // A first load that times out, then succeeds on the retry, clears the stuck timeout message.
    let mut t = ReloadTracker::default();
    let (g, _) = t.begin(false);
    assert_eq!(t.timed_out(g).unwrap().text, strings::RENDER_TIMEOUT);
    assert!(!t.is_running());
    let (g, _) = t.begin(false);
    assert_eq!(t.finished(g, ok()).unwrap().text, "");
}

#[test]
fn command_table_matches_the_old_toolbar_and_view_menu() {
    assert_eq!(
        commands::TOOLBAR,
        [
            ToolbarItem::Button {
                id: CommandId::OpenFiles,
                label: "Open files…",
                tooltip: None
            },
            ToolbarItem::Button {
                id: CommandId::OpenFolder,
                label: "Open folder…",
                tooltip: None
            },
            ToolbarItem::Button {
                id: CommandId::Refresh,
                label: "Refresh",
                tooltip: None
            },
            ToolbarItem::Separator,
            ToolbarItem::FindBox,
            ToolbarItem::MatchCount,
            ToolbarItem::Button {
                id: CommandId::FindPrevious,
                label: "Previous",
                tooltip: Some("Previous match (Shift+F3)")
            },
            ToolbarItem::Button {
                id: CommandId::FindNext,
                label: "Next",
                tooltip: Some("Next match (F3)")
            },
        ]
    );
    let theme = commands::command(CommandId::Theme);
    assert_eq!(
        (theme.menu, theme.label, theme.style),
        (Menu::View, "Theme", CommandStyle::Submenu)
    );
    let radios: Vec<_> = commands::menu_items(Menu::Theme).map(|c| c.label).collect();
    assert_eq!(radios, ["System", "Light", "Dark"]);
    assert!(commands::menu_items(Menu::Theme).all(|c| c.style == CommandStyle::Radio));
    let view: Vec<_> = commands::menu_items(Menu::View)
        .map(|c| (c.label, c.separator_before))
        .collect();
    assert_eq!(
        view,
        [
            ("Theme", false),
            ("Zoom in", true),
            ("Zoom out", false),
            ("Reset zoom to 100%", false),
            ("FILES", false),
            ("ON THIS PAGE", false),
            ("Focus reading mode", true),
            ("Command palette…", true),
        ]
    );
    let titles = [Menu::File, Menu::Edit, Menu::View, Menu::Help].map(|m| m.title().unwrap());
    assert_eq!(titles, ["&File", "&Edit", "&View", "&Help"]);
    let file: Vec<_> = commands::menu_items(Menu::File)
        .map(|c| (c.label, c.separator_before))
        .collect();
    assert_eq!(
        file,
        [
            ("Open files…", false),
            ("Open folder…", false),
            ("Refresh", false),
            ("Edit in mdedit", false),
            ("Open recent", true),
            ("Manage recent files…", false),
            ("Clear recent files", false),
            ("Close tab", true),
            ("Close all tabs", false),
            ("Settings…", true),
            ("Exit", true),
        ]
    );
    let edit: Vec<_> = commands::menu_items(Menu::Edit)
        .map(|c| (c.label, c.separator_before))
        .collect();
    assert_eq!(
        edit,
        [
            ("Find", false),
            ("Find next", false),
            ("Find previous", false),
            ("Clear search", false),
        ]
    );
    let help: Vec<_> = commands::menu_items(Menu::Help)
        .map(|c| (c.label, c.separator_before))
        .collect();
    assert_eq!(
        help,
        [
            ("Keyboard shortcuts", false),
            ("About Markdown Viewer", false),
        ]
    );
    assert_eq!(strings::ABOUT_TITLE, "About Markdown Viewer");
    assert_eq!(strings::SHORTCUTS_TITLE, "Keyboard shortcuts");
    assert_eq!(strings::MATCH_COUNT_NAME, "Search result count");
    assert_eq!(Menu::TabContext.title(), None);
    assert_eq!(strings::TOOLBAR_TITLE, "Reader");
    assert_eq!(strings::MANAGE_RECENT_TITLE, "Manage recent files");
    let show = commands::command(CommandId::ShowInFileManager).label;
    assert_eq!(
        show,
        if cfg!(windows) {
            "Show in File Explorer"
        } else if cfg!(target_os = "macos") {
            "Show in Finder"
        } else {
            "Show in file manager"
        }
    );
}
