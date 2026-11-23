## Audit 4 fixed on `fix/audit4` (2026-11-24, not merged)

Audit 4's findings against mdview (mdedit's `docs/review-prompts/audit4/e-mdview.md`; its FIXES names them E1 to E12) are fixed on the
branch `fix/audit4`, which is **not merged**. The shared crates are pinned there at `f0f8159`,
shared's own `fix/audit4`, for development only: the pin moves to shared's `main` once that
branch is merged there. Nothing was driven by hand in a real window and nothing ran off Windows.

- **Fixed, each with a test that fails with its fix reverted** (checked by reverting each one):
  - **E1.** The loop looked for the editor's tabs and the placed-for tab with a `canonicalize` per
    tab on every turn (104 to 128 µs a turn with three tabs). The paths are resolved once, when
    they arrive (`EditorTab::file`, `placed_for`), and compared with `Workspace::shows_resolved`:
    0.7 µs. `looking_for_the_editors_tabs_on_each_turn_asks_the_disk_nothing`.
  - **E2.** The zoom view counted its size in logical px per raster px, so it opened at 3x the
    page's size at 150 % and 4x at 200 %. It is now device px per raster px, the same against
    the page at every scale, and it follows a change of scale. `zoom::tests` (four) and
    `the_zoom_view_opens_at_twice_the_pages_size_at_every_device_scale`; the shots
    `zoom-diagram-2x-light` and `-dark`.
  - **E3.** The zoom test's drag asserted a pan the wheel had already made; it now measures the
    drag from where the wheel left it.
  - **E4.** A palette group is a byte: the library's ids ran into the headings' `G_PLAIN` (100)
    from the 85th file and saturated past the 240th. Files 1 to 238 have their own header
    (`G_LIBRARY..=G_LIBRARY_LAST`), the rest go under "More library files" with the file's name
    in each note's hint, and `G_PLAIN` is 255.
    `every_note_is_under_its_own_files_header_or_one_that_names_its_file` (512 files).
  - **E5.** The palette resolved every note's path at each change of its list: 17 to 21 ms a
    keystroke with 400 notes. They are resolved once per read of the library
    (`Notes::resolve_notes`, when the palette opens): 1.6 ms.
    `the_palette_asks_the_disk_nothing_about_the_notes_as_the_reader_types`.
  - **E6.** The shared writer's new refusals are said in the viewer's words, naming the file:
    `notes::not_added` for `PutError::Changed` and `PutError::NotUtf8`.
    `a_refused_write_says_why_and_names_the_library_file`,
    `a_library_file_that_is_not_utf8_is_left_as_it_is_and_the_reader_is_told_why`.
  - **E7 (within a batch).** A tick not yet answered when a new text arrives is dropped, and the
    page follows the caret. `a_tick_followed_by_a_new_text_before_its_answer_is_dropped`.
  - **E8.** The zoom view remembers its tab, the tab's revision and the theme (`zoom::Origin`)
    and closes when any of them changes. `the_zoom_view_closes_when_the_page_under_it_changes`.
  - **E9.** A fit goes below `MIN_ZOOM` for a diagram too large for it, and a pinch over the view
    zooms the diagram, not the page. `a_diagram_wider_than_ten_windows_still_fits_whole`,
    `a_pinch_over_the_zoom_view_zooms_the_diagram_not_the_page`.
  - **E10.** A `View` is drawn at most `wire::MAX_SIDE` (8192) a side, and a count of records is
    held to the bytes the message has left (`Fields::count`).
    `a_view_larger_than_a_frame_can_carry_is_drawn_at_the_largest_one`,
    `a_count_larger_than_what_follows_is_refused_before_anything_is_set_aside`.
  - **E11.** A file the editor asked for that could not be opened is remembered all the same, so
    the editor is told `closed` at once. `a_file_the_editor_asked_for_that_cannot_be_opened_is_told_closed`.
  - **E12.** The library is looked for on a helper thread once the window runs (`Wake::Library`),
    not in `App::new`, and the empty face says what the folder was at the last read instead of
    looking at every repaint. `a_window_looks_for_the_library_after_it_is_made_not_before`,
    `the_empty_face_says_what_the_folder_was_at_the_last_read`.
- **Not a bug.** E9's title bar: the zoom view's close button sits over the window's own, as
  every dialog's does (`hit_at` gives only the overlay's hits while one is open).
- **Left, and why.**
  - E7 across batches: a tick answered by a text sent after a second one needs a count of texts
    in `Update::task`, a protocol change in both apps. mdedit's own `[ ]` check catches most of it.
  - E11b: a `closed` that arrives late, after the editor asked again at once, needs a sequence
    number in the protocol, in both apps. The window is milliseconds.
  - E6's other half: asking mdedit to add the note over the sister channel, so that only mdedit
    writes the library, is a new feature in both apps.
- **Decisions taken without the owner.** The zoom view closes on a theme change rather than
  draw the old theme's raster; a scale change keeps it open at the same density. Library files
  past the 238th share one header rather than be left out. A shot looks for the library at once,
  as it has no loop to wait in.
- **Budgets** (release build, this PC, `--no-session`, scratch settings, against the build of the
  pinned tree before the fixes): RESULTS
