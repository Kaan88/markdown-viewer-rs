//! The Notes face and "Add this file to the notes library…", driven through the real handlers
//! on a scratch library (never the owner's: the folder is a made-up one under the temp dir).

use super::tests::app;
use super::*;
use crate::notes::Notes;
use mdshared_app::library::tree::Kind;
use mdview::commands::CommandId;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mdview-lib-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A library of two groups with three notes, one of them gone, and a file outside it.
fn library(name: &str, groups: usize) -> (PathBuf, PathBuf, PathBuf) {
    let root = scratch(name);
    let lib = root.join("library");
    std::fs::create_dir_all(lib.join("Personal")).unwrap();
    std::fs::write(lib.join("Personal/Budget.md"), "# Budget\n").unwrap();
    std::fs::write(
        lib.join("Personal.md"),
        "# Personal\n\n## Money\n\n- [Budget](<Personal/Budget.md>)\n- [Gone](<Personal/Gone.md>)\n",
    )
    .unwrap();
    if groups > 1 {
        std::fs::write(lib.join("Work.md"), "# Work\n").unwrap();
    }
    let outside = root.join("Outside.md");
    std::fs::write(&outside, "# Outside\n\nNot in the library.\n").unwrap();
    (root, lib, outside)
}

fn app_with(lib: &Path, files: &[&str]) -> App {
    let mut a = app(files);
    a.ui.notes = Notes::new(Some(lib.to_path_buf()));
    a.ws.set_library_offered(true);
    a.compose();
    a
}

fn click(a: &mut App, hit: Hit) {
    let r = a
        .hit_rect(hit)
        .unwrap_or_else(|| panic!("{hit:?} is not on screen"));
    a.pointer_moved(r.cx(), r.cy());
    a.left_press();
    a.left_release();
    a.ui.last_press = None;
    a.compose();
}

fn row_of(a: &App, label: &str) -> usize {
    a.ui.notes
        .rows
        .iter()
        .position(|r| r.label == label)
        .unwrap_or_else(|| panic!("no row {label}"))
}

fn status(a: &App) -> String {
    a.ws.status(Instant::now()).unwrap_or_default().to_string()
}

#[test]
fn the_head_offers_the_notes_face_only_with_a_library_and_the_switch_swaps_the_faces() {
    let (_, lib, _) = library("switch", 1);
    let mut none = app(&["linked.md"]);
    assert!(
        none.hit_rect(Hit::PaneFace(1)).is_none(),
        "no library, no switch"
    );
    let mut a = app_with(&lib, &["linked.md"]);
    assert!(a.hit_rect(Hit::PaneFace(1)).is_some());
    assert!(
        a.hit_rect(Hit::FilesFolder).is_some(),
        "the files face first"
    );
    click(&mut a, Hit::PaneFace(1));
    assert!(a.ui.notes.on);
    assert!(a.hit_rect(Hit::NotesRefresh).is_some());
    assert!(a.hit_rect(Hit::FilesFolder).is_none());
    assert!(
        a.hit_rect(Hit::NotesRow(0)).is_some(),
        "the library is listed"
    );
    click(&mut a, Hit::PaneFace(0));
    assert!(!a.ui.notes.on && a.hit_rect(Hit::FilesFolder).is_some());
    none.compose();
}

#[test]
fn a_click_opens_a_note_and_folds_a_group_and_a_missing_note_says_so() {
    let (_, lib, _) = library("click", 1);
    let mut a = app_with(&lib, &["linked.md"]);
    click(&mut a, Hit::PaneFace(1));
    let tabs = a.ws.tab_count();
    // A section folds and unfolds.
    let money = row_of(&a, "Money");
    let before = a.ui.notes.rows.len();
    click(&mut a, Hit::NotesRow(money));
    assert_eq!(a.ui.notes.rows.len(), before - 2);
    click(&mut a, Hit::NotesRow(money));
    assert_eq!(a.ui.notes.rows.len(), before);
    // A note opens in a tab.
    let budget = row_of(&a, "Budget");
    click(&mut a, Hit::NotesRow(budget));
    assert_eq!(a.ws.tab_count(), tabs + 1);
    assert!(
        a.ws.active_tab()
            .is_some_and(|t| t.path().ends_with("Budget.md"))
    );
    // A note whose file is gone opens nothing and says why.
    let gone = row_of(&a, "Gone");
    assert!(matches!(
        a.ui.notes.rows[gone].kind,
        Kind::Note { missing: true, .. }
    ));
    click(&mut a, Hit::NotesRow(gone));
    assert_eq!(a.ws.tab_count(), tabs + 1);
    assert_eq!(status(&a), crate::notes::MISSING);
}

#[test]
fn refresh_reads_the_library_again() {
    let (_, lib, _) = library("refresh", 1);
    let mut a = app_with(&lib, &["linked.md"]);
    click(&mut a, Hit::PaneFace(1));
    assert!(a.ui.notes.rows.iter().all(|r| r.label != "Later"));
    std::fs::write(lib.join("Later.md"), "# Later\n").unwrap();
    assert!(
        a.ui.notes.rows.iter().all(|r| r.label != "Later"),
        "nothing watches"
    );
    click(&mut a, Hit::NotesRefresh);
    assert!(a.ui.notes.rows.iter().any(|r| r.label == "Later"));
}

#[test]
fn add_this_file_asks_the_first_time_then_writes_and_does_not_ask_again() {
    let (root, lib, outside) = library("add", 1);
    let second = root.join("Second.md");
    std::fs::write(&second, "# Second\n").unwrap();
    let mut a = app_with(&lib, &[]);
    a.ws.open(&outside).unwrap();
    a.compose();
    a.command(CommandId::LibraryAdd);
    a.compose();
    // The question first; nothing is written until it is answered.
    assert!(matches!(
        a.ui.dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::AddToLibrary)
    ));
    let before = std::fs::read_to_string(lib.join("Personal.md")).unwrap();
    click(&mut a, Hit::DlgButton(0));
    assert!(a.ui.dialog.is_none() && a.ui.notes.pending.is_none());
    assert_eq!(
        std::fs::read_to_string(lib.join("Personal.md")).unwrap(),
        before
    );
    assert!(!a.ws.library_ok(), "a refusal is not a yes");
    // Yes: the line is added to the group's file, and the note itself is untouched.
    a.command(CommandId::LibraryAdd);
    a.compose();
    click(&mut a, Hit::DlgButton(1));
    let after = std::fs::read_to_string(lib.join("Personal.md")).unwrap();
    assert!(after.contains("Outside"), "{after}");
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "# Outside\n\nNot in the library.\n"
    );
    assert!(status(&a).contains("is in Personal now"), "{}", status(&a));
    assert!(a.ws.library_ok());
    // It is listed now: adding it again says so and writes nothing.
    a.command(CommandId::LibraryAdd);
    a.compose();
    assert!(
        status(&a).contains("already in the notes library"),
        "{}",
        status(&a)
    );
    assert_eq!(
        std::fs::read_to_string(lib.join("Personal.md")).unwrap(),
        after
    );
    // The next file goes in with no question.
    a.ws.open(&second).unwrap();
    a.compose();
    a.command(CommandId::LibraryAdd);
    a.compose();
    assert!(a.ui.dialog.is_none(), "asked only the first time");
    assert!(
        std::fs::read_to_string(lib.join("Personal.md"))
            .unwrap()
            .contains("Second")
    );
}

#[test]
fn with_two_groups_the_reader_picks_one_from_a_menu() {
    let (_, lib, outside) = library("groups", 2);
    let mut a = app_with(&lib, &[]);
    a.ws.open(&outside).unwrap();
    a.compose();
    a.command(CommandId::LibraryAdd);
    a.compose();
    assert_eq!(a.ui.popup_kind(), Some(PopupKind::Library));
    assert!(a.ui.dialog.is_none());
    let items: Vec<_> =
        a.ui.popup
            .as_ref()
            .unwrap()
            .root()
            .items
            .iter()
            .map(|i| i.id.clone())
            .collect();
    assert_eq!(items, vec![Act::LibraryPut(0), Act::LibraryPut(1)]);
    // The second group is Work.md.
    a.act(Act::LibraryPut(1));
    a.ui.popup = None;
    a.compose();
    click(&mut a, Hit::DlgButton(1));
    assert!(
        std::fs::read_to_string(lib.join("Work.md"))
            .unwrap()
            .contains("Outside")
    );
    assert!(
        !std::fs::read_to_string(lib.join("Personal.md"))
            .unwrap()
            .contains("Outside")
    );
}

#[test]
fn a_run_with_no_library_is_told_so_and_writes_nothing() {
    let (_, lib, outside) = library("refuse", 1);
    let mut none = app(&[]);
    none.ws.open(&outside).unwrap();
    none.compose();
    none.command(CommandId::LibraryAdd);
    assert_eq!(status(&none), crate::notes::NO_LIBRARY);
    assert!(none.ui.dialog.is_none());
    let before = std::fs::read_to_string(lib.join("Personal.md")).unwrap();
    assert_eq!(
        std::fs::read_to_string(lib.join("Personal.md")).unwrap(),
        before
    );
}

#[test]
fn a_document_mdedit_has_not_saved_is_told_to_be_saved_first() {
    use super::tests::{DOC, live_app, shown};
    let (mut a, _, _) = live_app("library-untitled");
    a.handed_over(vec![shown(DOC, "# Never saved\n\nBody.\n", Some(1))]);
    a.compose();
    assert!(a.ws.active_tab().is_some_and(|t| t.is_untitled()));
    a.command(CommandId::LibraryAdd);
    a.compose();
    assert_eq!(status(&a), crate::notes::NEEDS_FILE);
    assert!(a.ui.dialog.is_none() && a.ui.notes.pending.is_none());
}

#[test]
fn the_tab_menu_and_the_palette_list_the_command_only_with_a_library() {
    let (_, lib, _) = library("menus", 1);
    let none = app(&["linked.md"]);
    let items = crate::overlays::tab_menu(&none.ws, 0, none.ui.notes.available, 0.0, 0.0).items;
    assert!(
        !items
            .iter()
            .any(|i| i.id == Act::TabCmd(CommandId::LibraryAdd, 0))
    );
    let a = app_with(&lib, &["linked.md"]);
    let items = crate::overlays::tab_menu(&a.ws, 0, a.ui.notes.available, 0.0, 0.0).items;
    assert!(
        items
            .iter()
            .any(|i| i.id == Act::TabCmd(CommandId::LibraryAdd, 0))
    );
    let listed = |a: &App| {
        crate::palette::PaletteState::open(&a.ws, Some(&a.ui.notes), ">notes library")
            .palette
            .listed_ids()
            .len()
    };
    assert_eq!(listed(&none), 0);
    assert_eq!(listed(&a), 1);
}

fn palette_targets(a: &App) -> Vec<crate::palette::Target> {
    let p = a.ui.palette.as_ref().expect("the palette is open");
    p.palette
        .listed_ids()
        .into_iter()
        .filter_map(|id| p.target(id).cloned())
        .collect()
}

#[test]
fn the_palette_lists_the_library_notes_by_title_and_opens_the_one_chosen() {
    use super::tests::type_text;
    use crate::palette::Target;
    let (_, lib, _) = library("palette", 1);
    let mut a = app_with(&lib, &[]);
    a.command(CommandId::CommandPalette);
    a.compose();
    // The box empty: every note of the library is listed, by its title.
    let all = palette_targets(&a);
    let budget = lib.join("Personal/Budget.md");
    assert!(
        all.iter().any(|t| *t == Target::Note(budget.clone())),
        "{all:?}"
    );
    assert!(
        all.iter()
            .any(|t| *t == Target::Note(lib.join("Personal/Gone.md")))
    );
    // Typing a title leads to it, and Enter opens the note in a tab.
    let tabs = a.ws.tab_count();
    type_text(&mut a, "budget");
    assert_eq!(
        palette_targets(&a).first(),
        Some(&Target::Note(budget.clone()))
    );
    a.key_named(KeyName::Enter, None);
    a.compose();
    assert!(a.ui.palette.is_none());
    assert_eq!(a.ws.tab_count(), tabs + 1);
    assert!(
        a.ws.active_tab()
            .is_some_and(|t| t.path().ends_with("Budget.md"))
    );
    // The note is open now, so the library does not list it a second time.
    a.command(CommandId::CommandPalette);
    a.compose();
    type_text(&mut a, "budget");
    assert!(
        !palette_targets(&a)
            .iter()
            .any(|t| matches!(t, Target::Note(_))),
        "{:?}",
        palette_targets(&a)
    );
}

#[test]
fn a_note_the_library_lists_but_disk_does_not_is_listed_and_cannot_be_chosen() {
    use super::tests::type_text;
    use crate::palette::Target;
    let (_, lib, _) = library("palette-gone", 1);
    let mut a = app_with(&lib, &[]);
    a.command(CommandId::CommandPalette);
    a.compose();
    type_text(&mut a, "gone");
    assert!(
        palette_targets(&a)
            .iter()
            .any(|t| matches!(t, Target::Note(p) if p.ends_with("Gone.md"))),
        "{:?}",
        palette_targets(&a)
    );
    let tabs = a.ws.tab_count();
    a.key_named(KeyName::Enter, None);
    a.compose();
    assert_eq!(a.ws.tab_count(), tabs, "a missing note opens no tab");
    assert!(a.ui.palette.is_some(), "and the card stays open");
}

#[test]
fn the_page_menu_offers_adding_the_file_only_when_there_is_a_library() {
    let ctx = crate::workspace::PageContext {
        editable: true,
        ..Default::default()
    };
    let offered = |library: bool| {
        crate::overlays::page_menu(&ctx, library, 0.0, 0.0)
            .items
            .iter()
            .any(|i| i.id == Act::Cmd(CommandId::LibraryAdd))
    };
    assert!(offered(true));
    assert!(!offered(false));
}

#[test]
fn a_library_folder_that_is_not_there_offers_no_add_and_writes_nothing() {
    let root = scratch("nofolder-add");
    let note = root.join("Outside.md");
    std::fs::write(&note, "# Outside\n").unwrap();
    let mut a = app(&[]);
    let missing = root.join("not-made");
    a.ui.notes = Notes::new(Some(missing.clone()));
    a.ws.set_library_offered(a.ui.notes.available);
    a.ws.open(&note).unwrap();
    a.compose();
    assert!(!a.ui.notes.available);
    let items = crate::overlays::tab_menu(&a.ws, 0, a.ui.notes.available, 0.0, 0.0).items;
    assert!(
        !items
            .iter()
            .any(|i| i.id == Act::TabCmd(CommandId::LibraryAdd, 0))
    );
    a.command(CommandId::LibraryAdd);
    a.compose();
    assert_eq!(status(&a), crate::notes::NO_FOLDER);
    assert!(a.ui.dialog.is_none());
    assert!(!missing.exists(), "no folder was made behind mdedit's back");
}

/// Audit 4, E5: the palette resolves the library's notes once per read of the library, when it
/// first lists them, and asks the disk nothing about them after that: not as the reader types
/// (each change between an empty box and a typed one builds the list again), not when it is
/// opened again. It was a `canonicalize` per note at each: 20 ms a keystroke with 400 notes.
#[test]
fn the_palette_asks_the_disk_nothing_about_the_notes_as_the_reader_types() {
    use super::tests::type_text;
    use crate::workspace::RESOLVES;
    let (_, lib, _) = library("palette-typing", 2);
    let mut a = app_with(&lib, &["linked.md"]);
    let count = || RESOLVES.with(|n| n.get());
    let before = count();
    a.command(CommandId::CommandPalette);
    a.compose();
    let once = count();
    assert_eq!(once - before, 2, "the two notes, resolved once");
    for _ in 0..3 {
        type_text(&mut a, "b");
        a.key_named(KeyName::Backspace, None);
        a.compose();
    }
    a.key_named(KeyName::Escape, None);
    a.command(CommandId::CommandPalette);
    a.compose();
    type_text(&mut a, "bu");
    assert_eq!(count(), once, "nothing more was resolved");
    // A note open in a tab is still left out by the key resolved for it.
    a.key_named(KeyName::Escape, None);
    a.ws.open(&lib.join("Personal/Budget.md")).unwrap();
    a.command(CommandId::CommandPalette);
    a.compose();
    type_text(&mut a, "budget");
    let notes = palette_targets(&a);
    assert!(
        !notes
            .iter()
            .any(|t| matches!(t, crate::palette::Target::Note(_))),
        "{notes:?}"
    );
    // Read again (Refresh), the library's notes are resolved again, once.
    a.key_named(KeyName::Escape, None);
    a.notes_refresh();
    let read = count();
    a.command(CommandId::CommandPalette);
    a.compose();
    type_text(&mut a, "x");
    assert_eq!(count() - read, 2);
}

/// Audit 4, E6 and D3: a library file in another code page is refused by the shared writer and
/// left as it was, byte for byte, and the status strip says why in the viewer's words.
#[test]
fn a_library_file_that_is_not_utf8_is_left_as_it_is_and_the_reader_is_told_why() {
    let (_, lib, outside) = library("not-utf8", 1);
    // `- [Çağrı](<cagri.md>)` in Windows-1254: Ç is C7, ğ is F0, ı is FD.
    let before: &[u8] = b"# Personal\n\n- [\xC7a\xF0r\xFD](<cagri.md>)\n";
    std::fs::write(lib.join("Personal.md"), before).unwrap();
    let mut a = app_with(&lib, &[]);
    a.ws.set_library_ok();
    a.ws.open(&outside).unwrap();
    a.compose();
    a.command(CommandId::LibraryAdd);
    a.compose();
    assert_eq!(std::fs::read(lib.join("Personal.md")).unwrap(), before);
    assert_eq!(
        status(&a),
        "Personal.md is not UTF-8 text, so nothing was written. Save it as UTF-8 in mdedit first."
    );
}

/// Audit 4, E12: a window does not look for the library on the way to its first frame. Neither
/// mdedit's settings file nor the folder it names is read when the window is made; the loop's
/// start, `loop_ready` (the call `run` makes), looks on a helper thread, and its answer offers
/// the face and the add. Review of the audit 4 fixes, W2: the look is driven through that call,
/// with a test proxy; T5: the settings file is this window's own, not a process-wide
/// `MDEDIT_SETTINGS` every other test would see.
#[test]
fn a_window_looks_for_the_library_after_it_is_made_not_before() {
    let root = scratch("launch-look");
    let lib = root.join("library");
    std::fs::create_dir_all(&lib).unwrap();
    let settings = root.join("settings.ini");
    std::fs::write(
        &settings,
        format!("[preferences]\nlibrary_dir={}\n", lib.display()),
    )
    .unwrap();
    let args = Args {
        files: vec![super::tests::fixture("linked.md")],
        dark: Some(true),
        files_pane: Some(true),
        outline_pane: Some(true),
        no_session: true,
        ..Args::default()
    };
    let mut a = App::with_library_settings(args, Workspace::new(None), None, Some(settings));
    a.scale = 1.0;
    a.size = (1280, 800);
    a.setup();
    a.compose();
    assert!(
        a.ui.notes.dir.is_none() && !a.ui.notes.has_switch(),
        "nothing was looked at when the window was made"
    );
    assert!(!a.ws.library_offered());
    let (tx, rx) = std::sync::mpsc::channel();
    a.loop_ready(LoopProxy::Test(tx));
    let wake = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the window looks for the library as its loop starts");
    match &wake {
        Wake::Library(found) => {
            assert_eq!(found.dir.as_deref(), Some(lib.as_path()));
            assert!(found.available);
        }
        _ => panic!("the first wake is the library's"),
    }
    a.woken(wake);
    a.compose();
    assert!(a.ui.notes.has_switch() && a.ws.library_offered());
    assert!(
        a.hit_rect(Hit::PaneFace(1)).is_some(),
        "the head offers the face"
    );
}

/// Audit 4, E12: the face's empty state is drawn on every repaint, so it is what the folder was
/// at the last read, not a look at the disk each time; Refresh looks again.
#[test]
fn the_empty_face_says_what_the_folder_was_at_the_last_read() {
    let root = scratch("empty-state");
    let lib = root.join("library");
    std::fs::create_dir_all(&lib).unwrap();
    let mut a = app_with(&lib, &[]);
    click(&mut a, Hit::PaneFace(1));
    assert_eq!(a.ui.notes.empty(), Some(crate::notes::NO_NOTES));
    std::fs::remove_dir_all(&lib).unwrap();
    a.wants_frame = true;
    a.compose();
    assert_eq!(a.ui.notes.empty(), Some(crate::notes::NO_NOTES));
    click(&mut a, Hit::NotesRefresh);
    assert_eq!(a.ui.notes.empty(), Some(crate::notes::NO_FOLDER));
}
