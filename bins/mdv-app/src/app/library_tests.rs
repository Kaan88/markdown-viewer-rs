//! The Notes face and "Add this file to the notes library…", driven through the real handlers
//! on a scratch library (never the owner's: the folder is a made-up one under the temp dir).

use super::tests::app;
use super::*;
use crate::notes::Notes;
use mdshared_app::library::tree::Kind;
use mdview_core::commands::CommandId;

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
    let r = a.hit_rect(hit).unwrap_or_else(|| panic!("{hit:?} is not on screen"));
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
    assert!(none.hit_rect(Hit::PaneFace(1)).is_none(), "no library, no switch");
    let mut a = app_with(&lib, &["linked.md"]);
    assert!(a.hit_rect(Hit::PaneFace(1)).is_some());
    assert!(a.hit_rect(Hit::FilesFolder).is_some(), "the files face first");
    click(&mut a, Hit::PaneFace(1));
    assert!(a.ui.notes.on);
    assert!(a.hit_rect(Hit::NotesRefresh).is_some());
    assert!(a.hit_rect(Hit::FilesFolder).is_none());
    assert!(a.hit_rect(Hit::NotesRow(0)).is_some(), "the library is listed");
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
    assert!(matches!(a.ui.notes.rows[gone].kind, Kind::Note { missing: true, .. }));
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
    assert!(a.ui.notes.rows.iter().all(|r| r.label != "Later"), "nothing watches");
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
    assert_eq!(std::fs::read_to_string(lib.join("Personal.md")).unwrap(), before);
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
    assert!(status(&a).contains("already in the notes library"), "{}", status(&a));
    assert_eq!(std::fs::read_to_string(lib.join("Personal.md")).unwrap(), after);
    // The next file goes in with no question.
    a.ws.open(&second).unwrap();
    a.compose();
    a.command(CommandId::LibraryAdd);
    a.compose();
    assert!(a.ui.dialog.is_none(), "asked only the first time");
    assert!(std::fs::read_to_string(lib.join("Personal.md")).unwrap().contains("Second"));
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
    let items: Vec<_> = a.ui.popup.as_ref().unwrap().menu.items.iter().map(|i| i.id.clone()).collect();
    assert_eq!(items, vec![Act::LibraryPut(0), Act::LibraryPut(1)]);
    // The second group is Work.md.
    a.act(Act::LibraryPut(1));
    a.ui.popup = None;
    a.compose();
    click(&mut a, Hit::DlgButton(1));
    assert!(std::fs::read_to_string(lib.join("Work.md")).unwrap().contains("Outside"));
    assert!(!std::fs::read_to_string(lib.join("Personal.md")).unwrap().contains("Outside"));
}

#[test]
fn an_untitled_document_and_a_run_with_no_library_are_told_so_and_write_nothing() {
    let (_, lib, outside) = library("refuse", 1);
    let mut none = app(&[]);
    none.ws.open(&outside).unwrap();
    none.compose();
    none.command(CommandId::LibraryAdd);
    assert_eq!(status(&none), crate::notes::NO_LIBRARY);
    assert!(none.ui.dialog.is_none());
    let before = std::fs::read_to_string(lib.join("Personal.md")).unwrap();
    assert_eq!(std::fs::read_to_string(lib.join("Personal.md")).unwrap(), before);
}

#[test]
fn the_tab_menu_and_the_palette_list_the_command_only_with_a_library() {
    let (_, lib, _) = library("menus", 1);
    let none = app(&["linked.md"]);
    let items = crate::overlays::tab_menu(&none.ws, 0, none.ui.notes.dir.is_some(), 0.0, 0.0).items;
    assert!(!items.iter().any(|i| i.id == Act::TabCmd(CommandId::LibraryAdd, 0)));
    let a = app_with(&lib, &["linked.md"]);
    let items = crate::overlays::tab_menu(&a.ws, 0, a.ui.notes.dir.is_some(), 0.0, 0.0).items;
    assert!(items.iter().any(|i| i.id == Act::TabCmd(CommandId::LibraryAdd, 0)));
    let listed = |a: &App| {
        crate::palette::PaletteState::open(&a.ws, ">notes library")
            .palette
            .listed_ids()
            .len()
    };
    assert_eq!(listed(&none), 0);
    assert_eq!(listed(&a), 1);
}
