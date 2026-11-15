# Shell gaps (`f/shell-gaps`)

Four small gaps in the shell, one commit each. Everything below was run on the Windows PC with
`cargo test --locked --release --workspace`, `cargo clippy --locked --workspace --all-targets -- -D
warnings` and `cargo fmt --all`; nothing was run in a real window except the one `--shot` of gap 1.

## 1. Outline pane keyboard navigation

**Built.** A new command `FocusOutline` ("Focus outline", in the palette and nowhere else: no key,
no menu) opens the Outline pane if it is closed (and leaves Focus mode), and puts the keyboard on
the heading the page is at (the first row if none). A click on an outline or backlink row puts it
there too, and still does what it did. While focused, Up, Down, Home, End, PageUp and PageDown
move a focused row (no wrapping; the pane scrolls to keep it in view), Enter goes to the heading
(the keyboard stays in the pane so the walk can go on) or opens the backlink (the keyboard then
goes back to the page), Escape gives the keyboard back to the page. The focused row has an
accent outline and a bar at its left edge (`panes::focus_mark`). `--outline-focus N` (shots only)
and the shot `outline-focus` in `tests/shots.sh` show it.

**Decisions.** Headings and backlinks are one list (`Ui::outline_focus` counts the headings, then
the backlinks). Keys with Ctrl or Alt are not taken, so shortcuts still work; every other key
falls through to the page as before. The focus is dropped by a click anywhere but on the pane's
rows or scrollbar, by the find box taking the keyboard, by closing the pane, and by opening a
backlink. The Files pane has no keyboard focus to copy, so the palette's list was the model for
the keys. No default key for the command.

**Run.** Three new tests in `app/tests.rs` (walking; focus following clicks and panes; headings
into backlinks and Enter on a backlink); the shot was rendered and looked at.

**Not verified.** A real window with a real keyboard. The focus mark in the dark theme and at
other scales. A screen reader (nothing is exposed).

**Release note.** The Outline pane can be used from the keyboard: choose "Focus outline" in the
command palette or click a row, then move with the arrow keys, Enter goes to the heading and
Escape returns to the page.

## 2. Following mdedit's caret upward

**Built.** `Workspace::jump` kept the page still for any move under the step (48 px), in either
direction. Going up, a move of 24 px or more already means the target row's top is above the top
edge (the jump leaves the target one margin below the edge), so the caret's row could be up to
two rows out of view. Now the page stays only if the target is not above the edge going up (a
move up of under one margin) and not below the bottom edge going down; otherwise it follows,
putting the row a margin below the top as before. Walking down is unchanged (steps of 48 px or
more, the caret in the top rows). Walking up now moves about every second row.

**Decisions.** The rule is about the target's row top, so a row cut by the edge (a few px
visible) counts as above. One consequence: an echo of the page's own top line (a line that wraps,
whose first row is partly above the edge) moves the page up by under a step to show it.
`live_text_replaces_the_page_keeps_the_place_and_ends_on_live_off` allowed no move; it now allows
a move up of less than a step and the same line (within 3 lines) at the top.

**Run.** `a_caret_walking_through_a_long_block_is_followed_in_steps_and_stays_in_view`: the upward
bound is now `top <= line` (the caret's line is never above the top of the page), plus a bound on
the number of moves from the measured row height (`row` is measured from two lines, not assumed
to be 20 or 21). The whole suite passes.

**Not verified.** The new test was not run against the old rule to see it fail. Linux rows
(21 px): CI only. A real mdedit.

**Release note.** When mdedit's caret moves up through a long paragraph, the page now follows
before the caret's line leaves the top of the window.

## 3. A covered window and animations

**Found.** Most was already built: `shows_page` already refused a minimised, hidden or
`Occluded` window, and the timer for GIF and WebP is armed only through it. What winit 0.30
reports decides the rest: `WindowEvent::Occluded` is sent on X11 and macOS only; **on Windows and
Wayland it is never sent**, so a window fully covered by another keeps animating there. Nothing
was added for those (a check of the z-order would need a timer or work per frame, and is not
what winit reports).

**Built.** (a) `Occluded(false)` asks for a frame (what changed while covered is drawn, and
`about_to_wait`, which follows the event, arms the animation timer again). (b) While occluded
(and after the first frame) a requested frame waits instead of being drawn into a window nobody
sees. (c) `shows_page` with no window (the tests') says visible, so it can be tested.

**Decisions.** The first frame is always drawn: it is what shows the window, and a hidden window
may be reported as covered.

**Run.** `a_covered_window_arms_no_animation_timer_and_wakes_when_seen_again` (a page with an
animated GIF: a timer is armed; occluded, `timers` returns nothing; seen again, it is armed).

**Not verified.** X11 and macOS in a real window (no such machine here). Windows cannot be fixed
this way. Idle CPU was not measured, only reasoned from the timers.

**Release note.** On systems that tell mdview a window is fully covered (X11, macOS), animated
images stop and nothing is drawn until it is visible again.

## 4. Find in folder

**(a) The match of the chosen line.** `find_set_at_line` took the first match of the block. It now
takes the first match written between where the chosen line's text begins in the block and where
the next line's does (`Document::text_at_line`); a line whose match is in markup falls back to the
block's first, as before. The column is not needed: a row stands for a line, and its first match
is the one chosen.

**(b) Results while files change.** `FolderSearch::refresh` searches again for the same query (on
a thread of its own, as the search always is) when the file watcher's debounced check
(`watch_check`) runs while the card is open. The list on screen stays untouched while the new
search reads, and is replaced when it is done, the selection on the same file and line or, if
that is gone, the nearest line of that file (else the first row, as a new list). A new query drops
a pending refresh. While the card is open the watcher also follows the Files tree's folders, as
it does with the Files pane open, so a file made or removed there is seen.

**Decisions.** The selection is moved with the palette's own Up and Down keys (the shared crate
has no way to set it, and may not be edited here). A wake with nothing changed still searches
again (the watcher does not say which file; it costs one search). No timer: a refresh happens only
when the watcher wakes.

**Run.** `a_refresh_shows_new_results_and_keeps_the_selected_row_where_it_can` and
`the_list_stays_on_screen_while_a_refresh_reads` in `folder_search.rs`; the app test
`find_in_folder_searches_again_when_files_change_and_keeps_the_selected_row`;
`find_at_a_line_of_a_long_block_goes_to_the_match_written_on_that_line` in `workspace.rs`.

**Not verified.** A real watcher waking a real card; a large folder (a refresh costs a full
search); Linux.

**Release note.** Find in folder follows the folder: results update when files change, and
opening a result goes to its own match inside a long paragraph.
