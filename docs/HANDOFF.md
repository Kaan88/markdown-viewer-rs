# Handoff

`mdview` is a Rust remake of `Kaan88/markdown-viewer-cpp`: a Markdown viewer with its own
renderer, no webview, and no authored C++. This file is the current state: what ships, what is
being built, how to build and check it, the traps already found, and what to do next.
`docs/PLAN.md` is the full plan (budgets, phases, the 2026-11-11 roadmap). Read this file first.

Rewritten on 2026-11-12 from the dated sections that had accumulated since 2026-11-08; the Qt shell
was retired on 2026-11-13. The commit
that last changed this file is the state it describes; check `git log --oneline -5`.

---

## Shared with mdedit: brief tabs, the folder lock, the session read (2026-11-21, `f/brief-tabs`)

The owner: the apps are sisters and more may join; what they can share goes in markdown-shared-rs.
mdshared-app moved from `49de4ec` to `cb0ffd3` (also bringing the audit 2 synced writes and the
updater's `extras`, empty here).
- Don't reopen tabs barely looked at (`mdshared_app::brief`, as mdedit 1.2.1): `Tab::focus_ms`,
  `Workspace::focus_tick` in `about_to_wait` while the window is focused and not covered, the
  `focus`, `skip_brief`, `brief_secs` session lines, `barely_seen` at start (the tab that was
  showing and pinned ones always come back). Test
  `tabs_barely_looked_at_are_left_out_at_start_when_asked`.
- Settings in categories, as mdedit's (no longer `single_page`): "Content and tabs" is now
  "Content" and "Tabs", the card at most 640 px tall (was 760). Test
  `the_settings_dialog_has_categories_and_fits_the_window`.
- One window per settings folder (`mdshared_app::folder_lock`, was mdedit's): a launch next to a
  window still starting waits for it; one next to a hung window runs without the session.
- The session is read with `Session::read_patiently`: a file that cannot be read is an error, and
  the window runs without its session rather than writing defaults over it (audit 2's finding in
  mdedit). Test `a_session_that_cannot_be_read_is_an_error_not_the_defaults`.
- The two notes are a transient status line (and stderr), not a bar.
- Checks (Windows): fmt, clippy, all tests. Run by hand with a scratch `MDVIEW_SETTINGS`: the lock
  file is made, a second launch handed its file over in 129 ms, and the closed window wrote both
  tabs with their focused seconds. Not done: budgets not measured; the owner has not seen the
  settings; nothing on Mac or Linux.

## START HERE: feature complete, in maintenance (2026-11-17)

The owner agreed that mdview is feature complete: see the status at the top of `docs/PLAN.md`
(what maintenance means, the small known gaps, what was set aside). The latest release is
**1.0.0**: 1.0.1 was withdrawn (below, and `docs/UPDATE.md`). `main` holds the preview server's
selection requests used by mdedit 1.0.1 and 1.0.2 (which pin `8faf059`) and says version 1.0.1,
so the next mdview release is 1.0.2 or later.

## Tabs not opened for a week (2026-11-18, branch `f/stale-tabs`)

The owner asked for an offer to close tabs left unopened, on by default, in both applications.
The rules are `mdshared_ui::stale` (shared `f/stale-tabs` at `49de4ec`): time counts in days of
use (a start on a new date is one more; `Session::use_days`), each tab keeps the day it was last
shown (`Tab::seen`, `seen` lines; none in the session counts as today, so the first start after
an update offers nothing). At start, tabs not shown in the last 7 days of use are offered on a
bar under the other bars (`notice::draw_offer_bar`): Close them, Keep (their count starts
again), and the close button (asked again next start). Never offered: pinned tabs, the tab
showing, a document mdedit has not saved. Setting "Offer to close tabs not opened in a week"
(`offer_stale`, on), with "Start on the last pinned tab" in the group "Content and tabs".

The Settings dialog did not fit at 1280 x 800 on Windows (its backdrop row; the `ci` run of
1.0.2 failed on it; Windows has the backdrop and the corners rows, both with help lines). Now:
help lines only on those two rows, the tab toggles in the Content group (renamed "Content and
tabs") rather than a group of their own, and the shared single-page card may be as tall as the
window less its margin (`SINGLE_MAX_CARD_H` 760; only mdview uses that form). It fits with 1 px
to spare: one more row needs a cut elsewhere. Checked here by turning both rows on for Linux for
one run of `the_settings_dialog_is_one_page_at_the_default_and_the_minimum_window_size`; not yet
run on Windows. Test: `tabs_not_opened_for_a_week_of_use_are_offered_and_closed_or_kept`.

## Pinned tabs (2026-11-18, branch `f/pins`, for 1.0.2)

The owner asked for mdedit's pinned tabs here too, the rules shared. They are
`mdshared_ui::pins` (shared `main` at `6dcaf7f`): pinned tabs come first, a drag keeps them
there, closing several passes them by, the session brings them back first.
- `Tab::pinned`; `Workspace::set_pinned`, `is_pinned`. "Pin tab" / "Unpin tab" heads the tab's
  right-click menu (`CommandId::PinTab`, also in the palette). The tab draws a pin where its close
  button was; clicking it unpins (tooltip "Unpin tab").
- `close` refuses a pinned tab with `strings::STATUS_PINNED`; middle-click and Ctrl+W too.
  `close_others` and `close_all` keep the pinned tabs (`close_many`); the internal
  `remove_tab` closes a pinned one (an untitled document saved as a file already open: the pin
  moves to that tab).
- Session: `pin` lines (the pinned files in order), `active_tab` (the file showing at close; the
  old `active` index counts files only and stays as the fallback for older sessions),
  `last_pinned` (the pinned tab shown last) and `start_on_pinned`, the setting "Start on the
  last pinned tab" (Settings, Content). `pins::start_tab` picks the start tab. A document mdedit
  has not saved is never in the session, pinned or not.
- Tests: `pinned_tabs_come_first_and_closing_passes_them_by`,
  `a_restart_shows_the_tab_left_showing_or_with_the_setting_the_last_pin` (workspace), the
  session round trip, the tab menu count in `regressions.rs` (5).

## Preview protocol 6: the cursor over mdedit's pane, the page still after a tick (2026-11-17)

The owner tried mdedit 1.0.3: the cursor was the text cursor over the whole pane, and a tick
scrolled the page to the top. `MDVPREVIEW6`:
- `Request::Hover { x, y }` is answered in `Update::pointer` (`Pointer::Hand` over a task box,
  `Text` over text, `Arrow` elsewhere), as mdview picks its own cursor (links are not followed on
  the pane, so they get no hand).
- After a tick, the next text does not scroll the page to the caret (`Server::keep_top`): the
  scroll came from `go_to(follow)` in the `Text` request with the caret off screen, often at
  line 1. A moved caret still comes as its own `Follow`.
- Tests: `the_text_a_tick_brings_leaves_the_page_where_it_is`, and the hover checks in
  `a_click_on_a_task_box_gives_its_mark_and_selects_nothing`. Not an mdview release.

## Preview protocol 5: task boxes on mdedit's pane (2026-11-17, not an mdview release)

For mdedit 1.0.3 (the owner: tick boxes on the pane as in mdview, without saving). `MAGIC` is
`MDVPREVIEW5`: `Update::task` is `Some((at, checked))` after a single click on a task box, where
`at` is the byte offset of the mark in the last `Request::Text` and `checked` the state the click
gives it. `Server` keeps that text (`source`) and finds the byte with `tasks::toggle`, the check
mdview uses on files: nothing when the text and the page disagree about the boxes. The page does
not change; the editor edits its text. A double or triple click on a box selects as before. Test:
`a_click_on_a_task_box_gives_its_mark_and_selects_nothing`. mdview itself gains nothing from this
and is not released for it; mdedit ships the engine built from this commit.

## mdview 1.0.1, selecting text on mdedit's preview pane (2026-11-16, released, then withdrawn)

Released as 1.0.1 with mdedit 1.0.1 on the owner's word ("I trust you"), without a hands-on
test on Windows; the owner deleted the release the same day, since mdview itself had not
changed. Record in `docs/UPDATE.md`.


Protocol `MDVPREVIEW4`: requests `Press { x, y, clicks }`, `Drag { x, y }` (device pixels from the
pane's top left), `Release`, `SelectAll`, `Unselect`, `Copy`. Every answer carries
`Update::selection` when the selection's rectangles changed (page positions, `None` when not)
and `Update::copied` (text and HTML, as `Workspace::selected_text` and `selected_html` give them)
after `Copy` (`Server::selection_answer`, called by `serve` after each frame). The server keeps
the selection as mdview's workspace does (anchor first; a click that does not move clears it;
two clicks a word, three the block): `word_range` and `block_range` moved to
`mdview_core::select` and both use them. A new text clears it. Tests: 3 in `server.rs`.

## mdview 1.0.0 is released (2026-11-16)

**The released state is `main`, mdview 1.0.0, tag `v1.0.0` at `94280e0`, published with mdedit
1.0.0 on the owner's word. Shared crates: `main` of markdown-shared-rs at `51e7604` (the
crash-report branch merged). 1.0.0 holds what the two sections below describe (the preview
server, crash reports, input-method compositions, Mac self-update); notes in
`docs/release-notes/1.0.0.md`, the release record in `docs/UPDATE.md`. The `f/preview-pane`
branch is merged.**

## Preview server for mdedit (2026-11-16, merged, in 1.0.0)

For mdedit's preview pane (its handoff has the whole story and the timings):
- `crates/preview` (`mdview-preview`): the wire format (`wire.rs`: requests Text, View, Follow,
  Scroll, Quit; updates carry the rows that changed plus a shift) and, with feature `server`,
  the drawing loop (`server.rs`, `serve`, `serve_stdio`). mdedit's embedded build runs `serve`
  on a thread; its default build starts `mdview --preview-server`, which runs `serve_stdio`
  with no window (checked first in `main`). The server greets with `MDVPREVIEW3\n`.
- `mdview_render::set_engine_path`: where the math and diagram engine is, ahead of
  `MDV_ENGINE` and the exe's folder (for the embedded build, whose exe is mdedit).
- The page follows the caret's source line only when that line is out of view.
- Synced scrolling (protocol `MDVPREVIEW3`): `Request::Align { line, tail, end }` puts a
  source line (fractional) at the pane's top, and every `Update` carries `line`, the source
  line at the pane's top. Both go straight between knots at each top-level block's first line
  and page row (`server.rs`, `knots`, `Sync`), so they are each other's inverse. `tail..end` is
  the editor's last screen of scrolling: over it the page's top goes on to its bottom, so both
  reach their ends together (the line coming back is found by halving, the exact inverse).
  The editor decides who leads.
Tests: 9 in the crate (with `--features server`). Not run on Windows by hand.

Crash reports (same branch, shared crates `f/crash-report` at `21fd49b`, the owner's request):
- `main.rs` installs `mdshared_app::crash` after the `--preview-server` branch, into
  `data_dir("mdview")/crashes`, and calls `crash::closed_normally()` after `app::run()`: mdview
  builds with `panic = "unwind"` and catches panics in image decoding and export, so a run that
  ends normally deletes the reports it wrote (it lived through them).
- `app/crash.rs`: the context line (document size by 16 KB, tab count, a folder open,
  presenting; never text or names), set from `compose`; `look_for_crash` at launch (one read of
  a small folder); the bar's buttons. The bar sits under the update bar (`page.rs`), with Open
  report (the system's text editor: `platform::open_file(.., ["txt"])`), Copy, close; each marks
  the report seen (`.seen.txt`, kept). Strings `CRASH_REPORT_COPIED`, `CRASH_REPORT_NOT_OPENED`.
- Tests: `app/crash.rs` (context), `last_times_crash_report_gets_a_bar_…` in `app/tests.rs`.
  The shared module's panic and access-violation reports were checked on a Windows runner
  (mdedit's preview-test workflow); mdview's bar has not been seen on Windows.

## After 0.5.1 (2026-11-16, in 1.0.0)

Shared crates at `d775493`. Input-method compositions are drawn in the find box, the palette and
the settings (`ime_preedit` in `app/input.rs`; test in `app/tests.rs`), not yet tried with a real
input method. macOS builds update themselves in place (see `docs/MACOS.md`, decision 5), checked
in the Mac CI job by `tests/mac-update.sh`; `--install-update` does the same from a terminal.

## START HERE: mdview 0.5.1 is released (2026-11-15)

**The released state is `main`, mdview 0.5.1, tag `v0.5.1` at `a73da37`, published with mdedit
0.7.1 on the owner's word. Shared crates: `main` of markdown-shared-rs at `d955a48`. 0.5.1 holds
the fixes from a hands-on test of 0.5.0 (`docs/release-notes/0.5.1.md`); the release record is in
`docs/UPDATE.md`. A plan for a macOS version is in `docs/MACOS.md`; the owner has given that work
to someone else, nothing of it is built here.** The sections below describe 0.5.0 and 0.4.0 and
are kept for their detail.

Hands-on test of 0.5.0 on the Windows PC, 2026-11-15, with real mouse and keyboard (about 50
checks passed in the viewer, more in a pass with both applications). Faults found and fixed in
0.5.1, each seen working in a real window afterwards:
- The snap-layouts flyout showed on a maximized window only. Cause: winit moves the client area
  of an undecorated, not maximized window down by 1 px, and with that strip Windows never starts
  the flyout. `snap.rs` in the shared crates gives the strip back in `WM_NCCALCSIZE`. The shadow
  is still drawn (measured over a light background; not told apart from a dark one).
- The Outline pane's focus ring was not drawn again after Up and Down: `region_sig` for the pane
  did not include `ui.outline_focus` (`app.rs`).
- The resize band (8 px) lay over the page scrollbar and the Outline pane's scrollbar at the
  right edge of a window that is not maximized: `App::resize_band`, 3 px there (`SCROLLBAR_EDGE`).
- Text without a key name (KEYEVENTF_UNICODE: on-screen keyboards, emoji pickers) was dropped by
  `key()`: `event_key` in `app/input.rs`.
- A window that was maximized before "Preview side by side" came back maximized but with the half
  screen as its normal size: `place::Tiling` keeps the normal rectangle (shared crates).

Passed in real windows: Copy as Markdown, presentation mode (keys, links, selection, the slide
list), the outline by keyboard, wiki links and embeds, floats, `<abbr>`, ruby, Vue and Svelte,
find in folder, export, access keys in every menu, and with mdedit: preview, live text, the page
following the caret down and up, side by side and the return of both windows in six ways,
"Edit in mdedit". Not tested: picking a snap layout, an external link, Ctrl+plus, scales above
100 %, a real Escape key (it was posted). For the owner to judge, not changed: "Find in folder"
with no folder open does nothing and says nothing; Ctrl+Shift+P opens the same box as Ctrl+P; a
folder handed to a running instance is ignored; the `^id` marker of a block stays visible as
text; when following mdedit's caret the line sits at the top edge of the page.

How the hands-on tests were done, for the next one: Sonnet workers drove the release builds
under `Desktop\md-wt` with real mouse and keyboard input from a PowerShell helper (SetCursorPos,
mouse_event, keybd_event, captures of the application's own window only), one worker at a time
because pointer and keyboard are shared. Lessons: computer use must not be used (a real Escape
key aborts the whole Claude session, twice; its open_application started the owner's installed
mdview), so Escape is posted to the window; the PC has a Turkish Q layout, so characters are
mapped with VkKeyScanEx; a click 60 ms after a pointer jump is ignored by menus, hover first;
every key and click checks that the foreground window is the build under test. The logs, the
captures and the helpers are local files in `C:\Users\kaank\Desktop\md-wt\control-tests\`
(not in any repository).

## START HERE: mdview 0.5.0 is released (2026-11-15)

**The released state is `main`, mdview 0.5.0, tag `v0.5.0` at `6470ece`, published with mdedit
0.7.0 the same day (the owner's instruction: finish the remaining items, then release). Shared
crates: `main` of markdown-shared-rs at `065cbdc`. What 0.5.0 added is in "After 0.4.0: gaps
closed" below and in `docs/release-notes/0.5.0.md`; the release record is in `docs/UPDATE.md`.
The owner has not looked at any of the new interface work.** The text under the next heading
describes 0.4.0 and is kept for its detail.

Measured on the Windows PC on 2026-11-15 (plain `--release` build of the 0.5.0 source,
`--no-session`, median of 5 warm runs): window shown 51 ms with no file, 81 ms with
`showcase.md`, 104 ms with a 5 MB document (its page is fully refined after 1.27 s); private
bytes about 3 s after start 12.4, 25.2 and 38.6 MiB. A build with `opt-level = "s"` for the whole
workspace is 2.0 MB smaller (`mdview.exe` 7,087,104 against 9,066,496 bytes) and 5 to 17 % slower,
with the same memory: the profile stays as it is. Not measured: scroll and idle CPU, cold launch,
engine job time, the static dist build.

Known open points: `<bdo>` is not built (the text layout has no per-range direction); no key
opens a menu, so access keys need a menu opened by mouse and Alt; the Recent menu has no access
keys; on Linux one test of the page following the caret upward allows one row of slack and the
cause is not found; the KDE blur was never run; the snap-layouts flyout was checked by window
messages only. Not planned by decision of the session, the owner may overrule: speaker notes and
slide transitions, the system accent colour, Mica Alt.

## START HERE: mdview 0.4.0 is released (2026-11-15)

**Later on 2026-11-15, not released: branch `f/adopt-shared` holds the gaps closed after 0.4.0 (Copy as
Markdown, more render gaps, presentation additions, the shared Vue/Svelte, menu access keys,
snap-layouts and KDE blur additions). The owner has not looked at any of it; read "After 0.4.0:
gaps closed" below.**

**The released state is `main`, mdview 0.4.0, tag `v0.4.0` at `09e1dc5`, published with mdedit
0.6.0 on the owner's word; see "Release 0.4.0" below and `docs/UPDATE.md`.** The text that
follows was written before the merge and the release: read "0.3.1" as the release before, and
its list of what was not verified as still open. Nothing is in progress.

**Update, later on 2026-11-15: the owner asked for the merge.** `main` was fast-forwarded to
`merge/next-release` in the shared repository (`11563a3`), the viewer (`14aedec`) and the editor
(`d56c4ee`) and pushed. Where the text below says `main` is behind or that the merge is the
owner's, that step is done. Still open: the owner's look at the interface, a real update followed
by Restart, Linux by hand, the version numbers and the tags.

The released state is `main` (mdview 0.3.1). **Every feature of the next-release batch is finished
and merged on the branch `merge/next-release`**, in this repository, in the editor's
(`Kaan88/markdown-editor-rs`) and in the shared crates (`Kaan88/markdown-shared-rs`). Nothing is
merged into `main`, no version was bumped, nothing is tagged or published: those three steps are
the owner's, after looking at the interface.

### What is on `merge/next-release` (viewer)

On top of 0.3.1, each as one merge commit followed by its finishing commits (the sections under
"Next release work" at the end of this file say what was built, the decisions taken without the
owner and what was not verified, per feature):

Settings on one page; rounded window corners; Back and Forward as commands; watch the open
folder; the outline follows the page; copy as rich text; copy button on code blocks; footnote and
link previews on hover; find in folder; export as HTML and print; presentation mode; live preview
of a document mdedit has not saved; finer scroll sync with mdedit; window backdrop (Mica,
Acrylic); Restart after an update. And one fix: a close request that comes from the system alone
(the taskbar's "Close window") now ends the app at once (`App::finish`, called from the
`CloseRequested` handler; the loop reads an exit asked for in `about_to_wait` only after the next
event). Tried in a real window: gone 6 ms after a posted `WM_CLOSE`; before, never until the
pointer moved.

### Pins

All three `mdshared-*` pins (in `bins/mdv-app`, `crates/core`, `crates/render`) name the head of
the shared branch `merge/next-release` (`11563a3`), which is shared `merge/viewer-next` and
`merge/editor-next` merged, plus one change (a single-page settings form may be 720 px tall, so
the viewer's form with the corners and backdrop rows needs no scrolling in the default window).
mdedit's `merge/next-release` pins the same commit. The shared `main` is still `ce1345d`; a push
to it is the owner's. Third-party crates in `Cargo.lock` are as on `main` (a lock refresh moves
`windows-sys` under `arboard` and `os_pipe` and `base64` under `serde-saphyr`: put those lines
back, as was done here).

### What ran on the Windows PC at the head of the branch (2026-11-15)

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --release --workspace`: 546 passed, 0 failed, 6 ignored. (An existing test,
  `engine::tests::two_jobs_share_one_engine`, failed once in an earlier run and passed in every
  rerun; the cause was not found.)
- `tests/shots.sh`: 108 pictures rendered from the release build. Looked at by the session: the
  Settings form (both new rows, no scrolling at 1280x800), find in folder, a slide, a footnote
  card, the code copy button, the main menu. The workers looked at every new picture of their
  features.
- Real windows, by script only: the finer follow against mdedit (code block, long paragraph,
  one-line paragraph), the backdrop (Mica light and dark, Acrylic dark; captures in
  `docs/images/frosted-window/`), a start with `--restarted` waiting for the first window to go,
  the close request above.
- CI (`ci.yml`, Linux and Windows: fmt, clippy, tests, shots) passed on `merge/next-release` in
  all three repositories: viewer run 43689394395 at `e9fb5a1` (two earlier runs failed on Linux:
  a new test assumed Windows' row height, and the new export tests raced the engine's tests for
  the one engine process; both fixed), editor run 43686313531, shared run 43686315745.

### Not verified, and what the owner has to do

- **The owner has not looked at any of it.** The interface is approved by eye only. In a real
  window nobody has seen: presentation mode going full screen, the hover cards with a real
  pointer, find in folder on a large folder, the Save dialog of Export and the browser opening for
  Print, a paste of copied text into a word processor, menus and dialogs over a frosted chrome.
- A real update followed by a click on Restart (it needs a newer build on an https host; the
  editor's handoff says how to stage one).
- Linux: nothing of this batch was built or run there, except by CI.
- Known gap of the finer follow: when the editor's caret walks up line by line, the page follows in
  steps of 48 px, so the caret's line can be up to two rows above the top of the page before the
  page moves (CI on Linux, rows of 21 px, showed two; the test allows what the step allows).
  Making the upward follow keep the line in view is a change of `Workspace::jump`'s `min_move`
  rule that was not made.
- Startup time and memory of the whole branch were not measured again (finer sync and the
  backdrop were measured on their own; see their sections).
- Then: merge `merge/next-release` into `main` in the shared repository, the viewer and the editor,
  choose the version numbers, tag (`release.yml` builds the files from a tag `v<version>`).

### Decisions taken without the owner in this session (the features' own are in their sections)

- Find in folder has no key: Ctrl+Shift+F is Focus mode's. Present is Shift+F5: F5 stays Reload.
- Export and Print are in a tab's menu and the palette, not in the main menu (it would no longer
  fit an 800 px window).
- "Copy as Markdown" was left out of copy as rich text.
- In Settings the rows under Theme are Window corners, then Window backdrop.
- The Restart button was merged without a real restart ever having been performed.

### Branches

Every feature branch (`f/*`, `feat/*`, `next`) is merged and can be deleted once `main` has the
work. Local worktrees and build folders of the sessions are under `Desktop\md-wt` on the Windows
PC (about 30 GB of `target-*` folders); they can be removed.

## 1. Where the project is

**What is being built.** The shell is `bins/mdv-app` (package and binary `mdview`): winit 0.30 for
the window and input, softbuffer 0.4 for the pixels, a frameless window, and every control (title
row, tabs, command bar, panes, menus, dialogs, tooltips, text field) drawn by the crate through the
page's own text path. Windows first. The user approved its look by using the running app on
2026-11-11 (PLAN roadmap step A2). It builds and runs on Linux from 2026-11-14 (section 12; KDE Plasma 6 on Wayland, sway headless; the owner has not tested Linux).

**What ships.** `mdview` is the only shell. Its installer is `packaging/windows/mdview.iss` (built
by `build-installer.ps1`, section 8). **v0.1.0 and v0.2.0 (the seven features of section 13, item 4) were published on GitHub on 2026-11-13, and 0.2.2 is the latest (section 14)**; v0.1.0 from
`5cbd354`, as the latest release, with the installer, the two executables and `mdview-update.txt`
(the earlier v0.1.0 pre-release, which carried the Qt installer, was deleted with its tag on
2026-11-12). The installer from that release has not been downloaded and run (not re-checked). 0.3.0
(see "Release 0.3.0", before section 13) adds an AppImage for Linux.

**What is retired.** The Qt 6 shell (`bins/mdv-qt`, CXX-Qt, Qt Widgets), its installer
(`mdview-qt.iss`, `build-installer-qt.ps1`) and its Linux packaging (`packaging/arch`,
`packaging/appimage`) are on the `qt-archive` branch since 2026-11-13 (roadmap A6). The branch is
`main` as it was just before the removal; whether the Qt crate still built against the renderer
API of that commit was never checked. Its build output on the PC
(`bins/mdv-qt/target`, `target/qt-deploy`, the Qt installer in `target/installer`) was deleted the
same day. The WinUI 3 shell (`bins/mdv-winui`), its installer and build script are on
the `winui-archive` branch since 2026-11-11. Why it was dropped: about 383 ms to first frame and
about 180 MiB private memory for an empty window against PLAN's 50-90 MB, a system-wide Windows App
Runtime dependency, Windows only, and hand-trimmed generated bindings.

**Decided** (do not reopen without the user):

- The shell is winit + softbuffer, chosen over fltk-rs and Slint after mockups on 2026-11-11
  (Slint's text was rejected as blurry in software and fringed with Skia). PLAN, "Next: a lighter
  shell than Qt" has the numbers.
- The user approves the interface by looking at it. Screenshots and tests do not stand in for that.
- Remote images are off until the reader allows them, per document, with an "always" setting.
- Updates (2026-11-13, replacing "manual only" of 2026-11-10): mdview looks for a newer version
  at every launch (changed in 0.2.2; it was once a day), and asks before it downloads anything; it then replaces its own
  two executables in place. A setting turns the look off. `docs/UPDATE.md`.
- No bundled fonts (2026-11-08), so the golden compare runs on Windows only.
- An editor is a second project, not a mode of mdview (PLAN section E).
- The installer never takes the `.md` default; it registers mdview as a handler, so Windows' Default apps
  page lists it. Its last page offers only "Launch mdview" (the "Choose mdview as the default app"
  checkbox was removed on 2026-11-13 as a redundant question, as in the editor's installer).
- Linux was low priority until the Windows shell had replaced Qt; it was done for 0.3.0 (section 12).

**Plan phases.** 0 spikes, 1 core, 2 headless renderer, 4 engine: closed. 3 (Qt parity) and 5
(WinUI) are superseded by the roadmap in PLAN ("Roadmap from 2026-11-11"): A1-A2 done, A3 (hand
checks) and A4 (checklist and numbers) open, A5 (installer) built and run, A6 (retire Qt) done.
B1-B5 (GIF/WebP, remote images, Mermaid families, emoji list, ruby/dl/figure and copy-source) and
C1-C5 (image store, `layout.rs` split, estimated heights, cheap toggles, text and bitmap work) have
landed in `main`; see section 14 for the commits.

**Where the work happens.** On 2026-11-14 the Linux desktop (CachyOS, KDE Plasma, Wayland) again, for
the Linux port and 0.3.0. Before that the physical Windows PC (Windows 11, RTX 4070 Ti SUPER, 3440x1440 at
100%) since 2026-11-11. Before that, the user's Linux desktop (CachyOS, KDE Plasma) with a WinApps
Windows 11 VM, and before 2026-11-10 a Debian VM (section 12).

## 2. Repository

`Kaan88/markdown-viewer-rs`, private, branch `main` tracking `origin/main`. There is no CI: run the
tests yourself before a push. Commit and push as phases complete or when asked.

| Path | What it is |
|---|---|
| `crates/core` | Parser (pulldown-cmark), HTML allow-list mapping, autolink, slugs, document model, find, command table, session, recents, limits, tab labels; re-exports of the shared file watching, single instance and update logic |
| `crates/render` | Headless renderer: parley layout, swash raster, image store, patchable `PageLayout`, scroll frames, hit testing, selection, syntect highlighting, the engine client |
| `crates/engine` | Library and the `mdview-engine` binary: SVG (resvg 0.47), TeX math (RaTeX 0.1.15), Mermaid (merman 0.9.3) |
| `crates/engine-proto` | Job and reply wire format between the renderer and the engine process |
| `bins/mdv-app` | The new shell, binary `mdview` |
| `bins/mdv-bench` | Headless render to PNG, timings, golden generation |
| `packaging/windows` | `mdview.iss` + `build-installer.ps1` |
| `fixtures/` | Test documents; `everything.md` exercises every feature (section 5) |
| `tests/golden/` | Renderer goldens (`showcase-*`, compared by a test) and review screenshots |
| `spikes/` | Throwaway reference code, excluded from the workspace; `spikes/parser/data` holds the spec suites and `generated_1mib.md` |
| `docs/` | This file, `PLAN.md`, `PORT_CHECKLIST.md`, `COVERAGE.md`, `UPDATE.md`, `WINDOWS_HANDOFF.md` (2026-11-10 WinUI checklist, history), `AUDIT_PHASE1.md` |

**The editor.** `Kaan88/markdown-editor-rs` (`mdedit`) has its own `docs/HANDOFF.md`, written 2026-11-13, which
also covers the shared crates and what was left unchecked in this repository after the move.

**Shared crates (2026-11-12).** Code the editor also needs now lives in
`Kaan88/markdown-shared-rs`, pinned by `rev` in `crates/core`, `crates/render` and `bins/mdv-app`:
`mdshared-text` (`Bitmap`, `TextEngine`, `shared_fonts`), `mdshared-highlight` (syntect),
`mdshared-ui` (`gfx`, `theme`, `menu`, `textfield`, `tree`, the key parser, `platform`, and the
generic parts of `chrome`, `panes` and `overlays`) and `mdshared-app` (path helpers, file reading,
recents, the settings file layer, single instance, file watching, and since 2026-11-13 updating
in place: manifest, swap, `updater::Updates`) and `mdshared-fetch` (the former `crates/fetch`:
remote images and the update download, with the user agent passed in). `mdshared-ui` also draws
the update bar (`notice.rs`). mdview's part of the updater is `updater::APP` in `bins/mdv-app`.
The viewer re-exports them at
their old paths, so `crate::gfx::Painter`, `mdview_render::TextEngine` and the like still resolve.
To change shared code: edit it there, push, and move the `rev` here. The move was checked with 43
`--shot` renders compared byte for byte before and after; `mdview.exe` went from 7,439,872 to
7,457,280 bytes. First-frame time and memory were re-measured on 2026-11-13 (section 9) and are
unchanged. The user has not run the app since the move. Tests moved with the code: after the seven
features of 2026-11-13 this workspace runs 366 (4 ignored) and the shared repository 156
(3 ignored). The pinned shared commit is `1c3710a` (2026-11-13, shared branch `fix/update-look-every-launch`; the shared `main` was not moved, so the pin is a commit on a branch). It carries the launch look at every launch and the new code-colouring scanners (TypeScript, Dart, Zig, SCSS, Less, INI, CMake) on top of `870d620` (the shared icons). `mdview.exe` is 8,378,368 bytes with it (static build).

`target/` is git-ignored. Helper scripts that once lived there (`vm.py`, `seed-lock.py`,
`cap.ps1`) are not committed; sections 9 and 12 say what they did.

## 3. Build, test and run on Windows

Toolchain on the PC: rustc/cargo 1.99.1, MSVC 2022, Inno Setup 7. Qt 6.9.3 MSVC is still installed
at `C:\Qt\6.9.3\msvc2022_64`; only the `qt-archive` branch needs it.

Point `TEMP`/`TMP` at a writable folder when building, or `lib.exe` and `rc.exe` fail with access
denied:

```powershell
cd C:\Users\kaank\Desktop\mv
$env:TEMP = "$PWD\target\tmp"; $env:TMP = $env:TEMP
cargo build --release --workspace          # includes mdview.exe and mdview-engine.exe
cargo test --release --workspace           # 2026-11-13: 332 passed, 0 failed, 6 ignored
cargo fmt --all --check
```

The ignored tests wait out real deadlines (engine stall and idle, fetch) or dump files; run them
with `-- --include-ignored` when touching the engine client or the watchdog.

**Launch built `.exe` files from Git Bash, not PowerShell.** PowerShell reports an unsigned-binary
error for them on this PC.

The renderer and shells find `mdview-engine` through `MDV_ENGINE`, beside the current executable,
or in a `target/release` above it. Build the workspace (or set `MDV_ENGINE`) before opening a
document with SVG, math or Mermaid.

**The new shell** (`target/release/mdview.exe`):

| Flag | Meaning |
|---|---|
| `<files>` | Documents to open (a folder sets the Files pane) |
| `--light` / `--dark` | Override the theme |
| `--zoom <pct>`, `--page-width narrow\|wide\|full`, `--folder <dir>`, `--maximize` | Start-up overrides |
| `--no-session` | Neither reads nor writes the session and skips the single-instance hand-over. Use it for every test |
| `--load-remote` | Load the first document's remote images without the bar (for checks) |
| `--shot out.png` | Draw one frame of the state the other flags describe, write it, exit. No window |
| `--size WxH`, `--scale f` | Shot size and device scale |
| `--files`/`--no-files`, `--outline`/`--no-outline`, `--focus-mode` | Panes and focus mode |
| `--find q`, `--scroll px`, `--hover x,y` | Find query, scroll offset, pointer position. `--scroll` also applies to a real window's first document |
| `--menu main\|recent\|cascade\|tab\|page\|find`, `--dialog about\|shortcuts\|recent` | Open a popup or dialog in the shot |
| `--scroll-steps n` | Open a real window, wait until the page is refined, scroll n wheel steps, log timings, exit |
| `--update-bar available\|installing\|installed\|installed-plain` | Show the update bar at that stage with a made-up version, for a shot |
| `--check-update` | Look for a newer version as the app would, print the answer, exit. No window |

```sh
target/release/mdview.exe --no-session --shot out.png --dark --size 1280x800 --menu main fixtures/showcase.md
target/release/mdview.exe --no-session --shot out.png --light --load-remote fixtures/remote.md
```

The no-pixel-change check is `tests/shots.sh <folder>` (55 `--shot` states: both themes, every
`--menu` and `--dialog`, the empty state, panes, find, focus mode, page widths, scales, hovers).
Run it into one folder before a change and into another after, then `cmp` every PNG; also run
the workspace tests (they include the goldens) and `cargo fmt --check`.

`MDVIEW_STARTUP_TRACE=1` prints the first-frame and page-refined times with image decode and
engine job counts; `MDVIEW_TRACE=1` prints per-frame timings (layout, paint, compose, copy,
present, rows).

**The renderer bench** (`target/release/mdv-bench.exe`):

```sh
mdv-bench.exe --input fixtures/showcase.md --out x.png --theme dark --width 780 --scale 1.0
mdv-bench.exe --all                      # writes the showcase renders to target/goldens/
mdv-bench.exe --input spikes/parser/data/generated_1mib.md --out target/tmp/b.png \
    --width 780 --viewport 900 --scale 1.0 --scroll 40 --repeat 60
```

`--viewport <height>` reports what a window pays: first layout, first frame, refinement, scrolling.
`--resize <width>`, `--zoom <factor>`, `--details` and `--error` add phases.

**Goldens.** `crates/render/tests/golden.rs` compares the four `tests/golden/showcase-{light,dark}@{1.0,1.5}.png`
and, for `fixtures/tables.md`, `tables-light@1.0.png` and `tables-dark@1.5.png`,
and two renders of `fixtures/floats.md` (`floats-light@1.0.png`, `floats-dark@1.5.png`)
on Windows only (system fonts). After an intended visual change, re-bless them and look at the
difference before committing:

```powershell
$env:MDV_BLESS = 1; cargo test --release -p mdview-render --test golden; Remove-Item Env:MDV_BLESS
```

A failing compare writes `target/golden-actual-<fixture>-<name>.png`. The engine must be built, or SVG, math
and diagrams render as placeholders.

**crates.io** was unreachable from the PC for a while (`SEC_E_NO_CREDENTIALS`) and worked again from
2026-11-08. With crates cached, `--offline` builds; a stale local index can fail to resolve locked
versions, and a build without `--offline` refreshes it.

## 4. The new shell (`bins/mdv-app`)

Pure Rust, no DLLs beyond the system's. `mdview_render::shared_fonts()` gives the chrome a clone of
the renderer's font context, so system fonts are enumerated once.

| Module | What is in it |
|---|---|
| `workspace.rs` | All tab, document, session, find, selection, link, scroll, outline, remote-image and status logic, headless and unit-tested. Ported from the Qt shell's `shell.rs`. One shared `DocumentRenderer`; only the active tab is laid out |
| `app.rs`, `app/input.rs`, `app/commands.rs` | State, region compose and present, the winit handler, pointer and key handling, every `CommandId`, wakes and timers |
| `app/tests.rs` | A headless `App` at 1280x800 driven through the real handlers |
| `chrome.rs`, `panes.rs`, `page.rs`, `overlays.rs` | Title row and command bar; Files and Outline panes and the overlay scrollbar; page blit, find and selection overlays, missing-file banner, remote-image bar, empty state; menus, dialogs, toast, link hint, tooltips |
| `ui.rs`, `keys.rs`, `args.rs` | Regions, `Hit`, `Act` and window state; shortcuts parsed from core's command strings; command line |
| `updater.rs` | Looking for a newer version, downloading and checking its two executables, swapping them in (`docs/UPDATE.md`) |
| `gfx.rs`, `menu.rs`, `textfield.rs`, `tree.rs`, `theme.rs` | Painter (logical px, device snapping, clip, shadows), popup menu, single-line editor, lazy folder tree, chrome palettes |
| `platform.rs` | The only `unsafe`: clipboard, URLs, file manager, system theme and subpixel order, dialogs (rfd), DWM attributes |
| `build.rs` | Embeds `assets/mdview.ico` when `rc.exe` is found |

**Frames.** The window is five regions (`Region::Title`, `Bar`, `Files`, `Outline`, `Page`) plus
overlays (menus, dialogs, toast, link hint, tooltip). Each region has a signature, a hash of
everything it shows; a frame redraws only regions whose signature changed (the page also when the
workspace reports it dirty), redraws overlays with every region under them, and keeps the composed
`screen` bitmap between frames. `present` copies only the damaged rows into softbuffer's buffer and
calls `present_with_damage` with those rectangles; when the buffer's age is 0 it copies and
presents the whole frame.

**Hidden first frame.** The window is created with `with_visible(false)`, the first frame is
composed and presented, then the window is shown and the whole frame is presented again, because
a present to a hidden window shows nothing. A `RedrawRequested` the app did not ask for (the
system showed, restored or uncovered the window) sets `expose` and presents the whole frame; before
`846a9c2` the title row and command bar stayed white until something changed under the pointer.

**Hit and Act.** Drawing records `(Hit, Rect)` pairs per region and for overlays; `hit_at` asks
overlays first when a dialog is open, then the regions. `Hit::region()` says which region repaints
for hover and press; `is_button()` marks hits that act on release over the same hit. Popup menu
items carry an `Act` (a `CommandId`, a tab command, a recent path, open/copy link, copy formula or
diagram source, copy, select all, "always load remote images").

**Window.** Frameless (`with_decorations(false)`) with `with_undecorated_shadow(true)` on Windows,
default 1280x800, minimum 520x360, size and maximized state saved in the session
(`geometry=WxH [max]`). Resizing is the app's own: a press within 8 logical px of an edge resizes,
and within 20 px of a corner along an edge resizes diagonally (`EDGE`, `CORNER` in `app.rs`).
Maximized, the client rectangle equals the monitor work area, so the taskbar stays visible.

**Popup shadows** (`Painter::shadow`): ten expanding translucent boxes, 1.4 px apart and offset
3 px down, each at alpha 0.024 on the dark theme and 0.010 on the light one. Radii: menus 8, toast
8, tooltips 5, dialogs 10; dialogs draw the shadow twice on the dark theme.

**Idle.** The loop waits for events and sets a timer only for something pending: caret blink in
the find box, tooltip delay (500 ms), toast expiry, drag auto-scroll (16 ms), the idle-tab check
(every 30 s; tabs left 60 s ago drop their document), saving the size 800 ms after a resize,
page refinement (4 ms slices while there is more), and the next frame of an animated image that is
on screen. A refined, idle window with no animation in view costs no CPU.

**Differences from the Qt shell** (all noted in `workspace.rs`): restored tabs load when first
shown and inactive tabs drop their layout; the reading position survives reload, resize and zoom
as block plus offset (and closing the file, see "The place left in each file"); find re-runs
after a reload; double click selects a word and triple click a
block; status messages are toasts (no status bar); the outline highlights the current heading; the
empty state lists recent files; the page text and the chrome follow the display's subpixel order.

**Chrome text and icons.** UI text goes through `TextEngine` with the system's ClearType order
(`platform::subpixel_order`). Icons come from Segoe Fluent Icons or Segoe MDL2 Assets, which Linux
will not have.

**Back and forward (2026-11-12).** The workspace keeps one history for the window, across tabs:
following a link, a footnote mark or an outline entry records the file and scroll position left
behind. The mouse's Back and Forward buttons and Alt+Left / Alt+Right walk it, reopening a tab that
was closed since. The session also keeps the window's position (`geometry=1010x720 @333,177`,
restored only if a screen still shows that place) and the side panes' widths
(`layout=files=1;outline=0;files_w=300`). The text cursor shows only on a line of text; margins,
gaps and pictures keep the arrow, and a link is a link only where its text is.

**Page width (2026-11-12).** The reader chooses how wide the page's column may grow: narrow (the
theme's 700 px reading width, the default), wide (1000 px, `WIDE_PAGE` in `workspace.rs`) or full
(the window). It is one setting for every document, saved in the session as
`page_width=narrow|wide|full`. A button after the zoom buttons steps through the three (its icon
is drawn by hand and shows the current choice), and the main menu lists them. The user asked for
it after a report with wide tables read badly in the narrow column.

**Table columns (2026-11-12).** A table too wide for the page no longer shrinks every column by
one factor, which made short columns narrower than their longest word so text ran into the next
column. Each column keeps its longest word (parley's min-content width) and the room left goes to
the columns in proportion to what they would still wrap.

**Content-sized tables (2026-11-13).** Tables follow a browser's automatic layout
(`layout/table.rs`; the arithmetic is in `content_width` and `column_widths`, which have unit
tests and which `estimate.rs` calls too). Every column has a narrowest width (longest word plus
padding) and a widest (longest line plus padding).

- A table is as wide as its widest columns together, so a small table is small and stands at the
  left; before, every table was stretched to the column. `align` on the table, or on a `<div>`,
  `<center>` or `<figure>` around it, centres it or puts it at the right.
- Wider than the room it has, it takes that room and each column gets its narrowest width plus a
  share of what is left in proportion to (widest − narrowest), as before.
- When the narrowest widths do not fit, only the widest columns are cut, to one common width, and
  their words break (`break_words` in the pager); columns under that width keep their words
  whole. Before, all columns shrank by one factor and a short word next to a long path was
  broken too. There is still no sideways scrolling.
- `<table width>` is the table's width (not under its longest words, not over the column); spare
  room goes to the unstated columns in proportion to their widest. A cell's px `width` replaces
  its column's widest width; a % is a share of the table, and a table with no width grows until
  the share holds. Percent columns are served first, then px, then the rest.
- A `colspan` cell adds what it lacks to its columns in proportion to their widest widths,
  narrower spans first; before, it was split evenly.
- A `<caption>` is centred on the table, not on the column, and wraps only at the column's
  width (a browser wraps it at the table's).
- The height estimate gives a cell that fits its column one line. It used to count two for the
  longest cell of each column, so a table of short cells was estimated about 50% too tall.
- `hit_test` picks, among lines at the same height, the one the point is in or nearest to. It
  took the first, so a click in any cell but the first of its row answered with the end of the
  first cell's text, and links there were not found (`a_click_in_a_table_lands_on_the_text_of_its_cell`).
- `fixtures/tables.md` shows each case and has two goldens. The look has not been approved by
  the user yet.

**Find box menu and Show in Explorer (2026-11-12).** A right click in the find box opens Cut, Copy,
Paste and Select all (`overlays::find_menu`, `Act::Edit`); `--menu find` shows it in a shot. "Show in
Explorer" on a tab did nothing useful because the slash of Explorer's `/select` switch was turned
into a backslash with the path's; `platform::select_argument` now converts the path only. That fix
has a test of the argument, and has not been watched opening an Explorer window.

**Ticking task boxes (2026-11-12).** The first write the viewer makes to a document, asked for by
the user. A click on a task box flips `[ ]`/`[x]` in the file. The first click in a file asks
(`DialogKind::AllowEdit`: Cancel, Always allow, Change this file); "this file" lasts until the tab
closes, "always" is `edit_tasks=1` in the session and a checked item in the main menu.
`crates/core/src/tasks.rs` finds the box by its own scan of the file's bytes and writes only when
the scan finds the same boxes in the same states as the page, then writes one byte in place; a
file that differs is left alone and reloaded. UTF-16 and read-only files are refused. The page is
re-read after the write with the reading position and open `<details>` kept. Not handled: a box
the scan reads differently from the parser (the tick is refused with a message), raw HTML
`<input>` boxes. `--dialog edit` shows the question in a shot. Everything else stays read-only;
the editor is still a second project.

**Live Files tree (2026-11-13).** The watcher follows the tree's root and expanded folders while
the Files pane is open (`App::watch_tree`, `FileWatcher::set_folders` in `mdshared-app`): an entry
created, removed or renamed directly in one of them wakes the shell, which re-reads the tree.
Content changes wake only for open files, as before. `MDVIEW_TRACE=1` prints a line per wake. Run
in a real window with that trace; the user has not looked at it.

**The place left in each file (2026-11-13).** A file opened again starts where the reader left it,
by any route (`Workspace::open` and the lazy `load` of a restored tab), in this run or a later
one. The session holds one line per file for the 300 files used last (`MAX_PLACES`), most recent
first: `place=<position>|<path>`. The bounded store and its lines are `mdshared_app::places`
(shared with the editor, position opaque); the position is the viewer's
`mdview_core::place::ReadingPlace`, written as
`1 <block> <offset> <blocks> <hash> <below> <heading>`: the top-level block at the top of the
window, the offset into it in logical px, the document's block count, FNV-1a of the block's text,
and the slug of the last heading above with the number of top-level blocks between. No pixels, so
window size, zoom and theme do not matter. Found again in this order: same block with the same
text; a block with that text (nearest, within the same heading's section first); the same count
below the same heading; the same index if the block count is the same; the same share of the way
down; the top. Places are noted when a tab is left or closed and for the showing tab whenever the
session is saved (never on a scroll step), and written by the session's own atomic save, so a
crash loses what was scrolled since the last save. A file left at its top has no line. A link's
fragment, Back/Forward and `--scroll` are applied after the place and win. Removing a Recent
entry forgets its place; Clear recent forgets all. With `--no-session` nothing is read or
written; places still work inside the run. There is no setting to turn it off. Not seen in a
window by the user yet.

**Live reload keeps the place (2026-11-13).** `reload_tab` (F5 and the watcher) no longer uses
core's `remap_scroll` (still there, with its tests; it compares from both ends only, so a change
on each side of the reader moved the page). `mdview_core::place::Remap` compares the top-level
blocks of the two versions by kind and text: unchanged start and end first, then, in the changed
middle, blocks whose text occurs once in both versions are landmarks (longest run in the same
order) and the search starts over between the two landmarks around the block. The block at the
top of the window stays where it was with its offset; an edited block is the one between the same
neighbours (offset held a line short of its new height); a removed block gives way to the one
that followed; a wholly different text keeps the share of the way down. Open `<details>` stay
open by `details_keys`; the selection stays if both ends are in unchanged blocks and its text is
the same; the find runs again and the same match stays current if it survived. **At the end:** a
page that was scrolled and showed its end shows the end of the new text (a log being appended is
followed, as in a terminal); scrolled up by any amount, the content stays still and the new text
arrives below. A page shorter than the window stays at its top. One layout and one frame per
reload. Not seen in a window by the user yet.

**Wiki-links and backlinks (2026-11-13).** The neutral parts (`mdshared_app::notes` and
`wikilink`) are in the shared repository at the pinned `rev`. The user has not looked at any
of it.

- *Parsing.* The viewer's own pass over finished text runs (`find_wiki` and `rewrite_run` in
  `parse.rs`, called from `flush_run` beside the bare-URL pass), not pulldown-cmark's
  `ENABLE_WIKILINKS`: that option was tried first and took `[[1]](url)`, `[[2]][ref]` and
  `[[x]]` with a definition away from CommonMark, which ordinary READMEs use. The pass sees
  only what the Markdown parser left as plain text, so those keep their meaning and the spec
  suites have no wiki exceptions. `LinkInfo.wiki` marks the link, `dest` is `Note#Heading`;
  the shared `wikilink::parse_target` splits it. A label with markup in it is several runs,
  so that link stays text. Embeds (`![[x]]`) stay text.
- *Who decides.* `parse_wiki(src, &mut |note| WikiAs)`: the application answers per note name
  `Link`, `Missing` or `Text`; plain `parse` answers `Text` for all. The workspace
  (`notes::Decider`) answers `Link` for a note it finds, and for one it does not, `Missing`
  with a folder open and `Text` without: **a file read with no folder open shows an unfound
  `[[name]]` as written, not as a link.** `Document::wiki_text` keeps the names left as text.
- *Resolution* (`mdshared_app::notes`, `workspace/notes.rs`): see the COVERAGE row. Following
  a link goes through `Workspace::open_wiki` and then the same `open_markdown` as a Markdown
  link; `link_destination` gives a wiki-link as `wiki:Note#Heading`, which `open_link` takes.
  The hover hint (`link_hint`) is the resolved path, or "No note named X".
- *Missing notes.* `LinkInfo.missing` is drawn by the renderer: muted, dashed underline
  (`Style::broken`; the run is marked with an underline size, which nothing else sets).
  `check_wiki_links` (before each layout check, only when the notes generation changed) asks
  again for every link and every name left as text: a changed `missing` is a relayout; text
  that should be a link, or the reverse, makes `parse_again` read and parse the file again
  silently, keeping the reading position and open `<details>` (the selection is dropped).
- *The index.* Only with a folder open in the Files pane (shown or not). `notes::Indexer`
  builds it on its own thread, within `limits::NOTES_*` (20,000 files, 2 MiB read per file,
  256 MiB in total, 16 folder levels, 200,000 directory entries, 2,000 links per file),
  skipping what the Files tree skips, never following links to folders. A light scanner
  (`wikilink::scan`) reads the files, not the parser. Without a folder nothing is walked: the
  document's own folder is listed once per read. Measured in the shared crate's test on the
  PC, release: 2,000 notes (566 KB, 20,000 links) in 127 ms, an unchanged rebuild 3.8 ms, one
  note's backlinks 0.19 ms.
- *Keeping it current.* `Wake::Files` and F5 call `notes_changed`, which walks again off
  thread and reads only files whose size or time changed; the result arrives as `Wake::Notes`.
  The watcher is given the indexed notes too (up to `WATCHED_NOTES`, 5,000), so a save anywhere
  in the folder wakes. Nothing polls. `MDVIEW_TRACE=1` prints a line per new index.
- *Backlinks* are a list under the outline in the Outline pane (`panes::draw_outline`,
  `Hit::BacklinkRow`): a hairline, "Backlinks n", then the linking file's name over the link's
  line, one entry per link; absent when there are none. A click opens the file with the link
  selected. Wiki-links and relative Markdown links (and reference definitions) count.
- *Not done.* (Updated 2026-11-15; the outline keyboard, new folders and notes waking the window,
  `[[#Heading]]` with no folder and `[[Note#^block]]` were built later: see "After 0.4.0: gaps
  closed".) Idle CPU with
  the indexer thread was not measured, only reasoned (it blocks on a channel). Nothing was run
  in a real window: tests and `--shot` only. `fixtures/wiki/` is the test folder.

**Updates (2026-11-13).** `updater.rs` and the "updates" part of `workspace.rs`; the bar is drawn
by `page::draw_update_bar` above the remote-image bar, and the About dialog has the "Check for
updates" and "Check at launch" buttons. `docs/UPDATE.md` describes all of it.

**Logo (2026-11-12).** One drawing in two places. `assets/make_icon.py` writes `mdview.svg`,
`.ico` and `.png` from a list of shapes (and the editor's mark with `mdedit` as its argument);
`chrome::logo` draws the same shapes with `Painter::polygon`, in the icon's own blue in both
themes. Change the shapes in both together.

**Public, and history rewritten (2026-11-12).** The repository is public under the MIT licence,
with a README. Before that, every version of this file was rewritten to remove the old test VM's
address and login, and `main`, `winui-archive`, `phase2-renderer` and `winui-tabs-wip` were
force-pushed; all commit hashes changed, and the ones written in this file were updated. Any
other clone (the Linux desktop's) has the old history and must be cloned again, not pulled: do
that before any work there. On the PC the old history was deleted the same day (the backup bundle,
`refs/original/` and the nine merged agent worktrees), so the PC's checkout holds only what GitHub
has.

## 5. The renderer (`crates/render`)

Headless: no window system, BGRA bitmaps, parley 0.12 for layout, swash for raster. The layout
code is split into `crates/render/src/layout/` (`page`, `relayout`, `estimate`, `refine`, `frame`,
`hit`, `selection`, `store`, `pager`, `inline`, `paint`, `prose`, `containers`, `table`, `media`,
`floats`);
the module doc in `layout/mod.rs` names each.

**`DocumentRenderer`** holds the font and layout contexts, the glyph cache (4 MB), the per-block
paint cache, the image store, `<details>` overrides, the kept frame and the current `PageLayout`.
One per window (the new shell shares one across all tabs).

| Call | Meaning |
|---|---|
| `layout(doc, &Options) -> &PageLayout` | First layout: **estimates** every top-level block's height without shaping (kind, text length, an average glyph advance measured once per scale) |
| `refine(doc, &Options, budget) -> bool` | Measures remaining estimates, nearest the viewport first, within `budget`; true while there is more. The shell gives it 4 ms slices between events |
| `view_frame(doc, &Options, top, height) -> &Frame` | Draws the viewport into the kept frame |
| `top()` / `set_top()` | The viewport top in corrected coordinates; the shell adopts it as its scroll position |
| `jump_to(..., Jump)` | Scroll to a block (`Jump::Block`, which fragments and the outline use), a footnote (`Jump::Footnote`) or a text offset (`Jump::Text`, find matches), measuring what it lands on |
| `hit_test(doc, &Options, x, y) -> Option<Hit>` | Block, document offset, link, footnote, `<details>` summary and formula/diagram `Source` under a point |
| `selection_rects(doc, &Options, from, to)` | One page rectangle per selected line |
| `set_details(doc, block, open)` / `details_open` | `<details>` open state is view state the renderer holds |
| `set_subpixel(Subpixel::{Rgb, Bgr, None})` | Page text antialiasing for the display's stripe order; a change drops the layout |
| `take_remote_requests()`, `set_remote(url, RemoteState)`, `remote_arrived(url, bytes, scale)` | Remote images: the URLs passes asked for, pending/blocked marks, delivery |
| `page(doc, &Options)` | The whole page in one bitmap, for tests, goldens and the bench |
| `image_stats()` | Decodes and engine jobs so far, for tests and traces |

`layout_exact` (also behind the crate-level `layout()` and `render_slice`) still measures everything
up front; the bench, the goldens and tests use it (the Qt shell did too).

**Patchable layout.** Each top-level block's height is recorded; positions are prefix sums rebuilt
from the first changed block. Anything that lays a block out (a frame, a hit test, a selection, a
jump, a `<details>` toggle, an arrived image, a refinement slice) may correct its height in place.
Corrections keep the viewport's anchor (a block and an offset into it) fixed, so content on screen
does not jump; a correction above the viewport moves `top()` instead. A block with an image,
formula or diagram the store lacks is laid out with a typical box and stays provisional until
shown; refinement never loads anything. A `<details>` toggle or an arrived image re-measures only
the blocks that use it.

**Floats across blocks (`layout/floats.rs`).** `<img align="left|right">` floats, and the blocks
after its paragraph run beside it until its bottom is passed (`docs/COVERAGE.md` says which kinds
wrap, narrow or start below). This is the one place where a block's layout depends on the blocks
before it, so it is designed around the lazy layout:

- What a top-level block hands to the next is its *float state*: the floats still open at its
  bottom edge, relative to that edge, on whole rows. A block's layout is a function of the
  document, the options, the image store and the state of the block before it, and of nothing
  else. The one-pass page (`Pager::blocks`) and the block-by-block layout (`measure_with`) pass
  floats on through the same two functions (`floats_out`, `floats_in`).
- `FloatBook` (in the renderer, rebuilt per layout) lists the *starters*, the top-level blocks
  that hold a floated image, found from the model without layout, and keeps the state of every
  measured block that leaves one. With no starter before a block nothing is looked up: a
  document without floated images pays one comparison per measured block.
- `floats_before` finds the state for a block measured out of order: the nearest measured block
  before it, or, when nothing back to the last starter is measured, that starter and the blocks
  after it, measured in order until its floats close (unmeasured blocks with nothing open are
  skipped). Every order walks the same chain, so jumps, restores and refinement agree with a
  top-down layout byte for byte (`crates/render/tests/floats.rs` scrambles the order).
- A float's size is the image's: decoded, or its declared `width` and `height` while a remote
  one loads. Blocks of a floated run are always measured with loading allowed, **so refinement
  decodes the images of floated runs** (and only those); a stand-in there would make every later
  block a guess. A remote image with no declared size is alt text until it arrives.
- When a block's state changes (an image arrived, a `<details>` beside a float was toggled), the
  next block is invalidated, and so on until a state comes out unchanged; `correct_pending`
  loops until nothing is pending.
- A floated image is painted by the block that holds it and can hang below it: a frame, a band
  and a hit test also take the owners of the floats open at their first block
  (`FloatBook::owners`), and a change to such an owner makes the kept frame stale.
- Lines beside a float at the left start at parley's `inline_min_coord`, which cluster offsets
  leave out; `LineGeom::shift` carries it for selection and find rectangles.

Not done: `<br clear>`, narrowing tables. A document with very many floated images pays for
the runs before a jump target once (they are measured, with their images decoded).

**The frame delta is the contract.** `Frame { bitmap, top, stats, band, shift }`: `band` is the
rows painted this time (`None` means the whole frame is new: first frame, resize, zoom or theme
change, a jump of a viewport or more, or a correction that changed the kept rows; an empty range
means unchanged); row `r` of this frame was row `r + shift` of the previous one. Scrolling moves
the kept rows with one `copy_within` and paints only the band that came into view, on its own small
bitmap, then copies it in (a block can reach past the band and would otherwise be blended twice).
Images are snapped to a page row before the shift. Tests rebuild frames from `band` and `shift`
alone and compare bytes with a fresh `page()` render.

**Hit testing and selection** live beside the frame. Each laid-out line keeps its geometry and the
inline origin map; a hit asks parley's `Cluster::from_point` and maps back to `Document::text`;
selection asks `Cluster::from_byte_index` for the boundaries. Selection and find marks are drawn by
the shell over the frame, never into it.

**Image store** (`layout/store.rs`, since `8d2437f`). Every raster (file images, remote images,
inline and display math, Mermaid) is keyed by what its pixels depend on: a file's resolved path,
length and mtime; a formula's source, pixel size, colour and display flag; a diagram's source,
scale and theme. Relayout, a width change, a zoom back to a known scale, a `<details>` toggle, a
reload or a tab switch re-reads and re-renders nothing. Pixels are capped at 64 MiB
(`IMAGE_CACHE_BYTES`) with LRU eviction that never drops what the current frame uses. File SVGs are
rasterized by the engine at the device scale (`KIND_SVG_AT`).

**Images.** PNG, JPEG, GIF and WebP decode in-process; SVG goes to the engine.
Limits 12 MiB and 24 Mpx. `<picture>` light/dark sources follow the theme; `<img width height>`
are honoured and reserve the box while loading. Below half size an image is area-averaged
(`56c61a3`), at half size and up bilinear.

**Animated GIF and WebP (2026-11-13).** The image decoder still draws the first frame, on the
window's thread, as before. Everything after it is `crates/render/src/anim.rs`: a `Player` per
file walks it forward one frame at a time on one worker thread (`mdview-anim`, started by the
first image that may animate, asleep in `recv` otherwise), keeping only the composited canvas, so
memory does not grow with the frame count; a loop starts the decoder over. GIF disposal (keep,
background as transparent, previous), transparency, interlace, local palettes, frames clipped to
the canvas, the Netscape loop count (none: once; n: n repeats; 0: for ever) and WebP's loop count
are honoured; a delay of 10 ms or less plays as 100 ms. The store (`layout/store.rs`) holds the
frame on screen in the entry's slot and the one decoded ahead beside it;
`DocumentRenderer::animate(now)` (`layout/animate.rs`) swaps in what is due among the images the
kept frame shows, records their rows, and returns when to come back; the next `view_frame` paints
only those rows (`band`, `shift` 0). The renderer has no timer: the shell's `timers()` calls
`Workspace::animate`, sets `WaitUntil` for the time returned, and the worker's wake
(`Wake::Animation`) covers a frame still being decoded. The shell does not call it while the window
is hidden, minimised or reported occluded, and an image outside the viewport or in another tab is
never advanced, so none of those ask for a wake. A frame advance draws and presents only the
image's rows of the page region (`page_band` in `App::compose`), unless a menu, dialog or toast
lies over them. An animation shown below half its size gets its frames already scaled to its box by
the worker (`anim::fit`, opaque frames of endless loops only) and they are copied in; when a whole
loop of them fits `MAX_ANIMATION_KEPT_BYTES` (32 MiB) it is kept and plays with no decoder at all.
Limits (`crates/core/src/limits.rs`): 10,000 frames and a 4 M pixel canvas (over either: first
frame only), and 64 MiB (`ANIMATION_BYTES`) for all players and kept frames together; past it,
animations off screen give theirs up and start from their first frame when shown again (a local
file is read again, a remote image is asked of the shell again), and one that still does not fit
stays still. Animation is off in the renderer until `set_animation(true)`: tests, goldens, the
bench and `--shot` never animate. The shell turns it on when there is a window, the session's
`animations` is not 0 ("Play animated images" in the main menu) and Windows' "Animation effects"
(`SPI_GETCLIENTAREAANIMATION`, read at start and when the window gains focus) is on. Measured on
the PC, release build, 1280x800 at scale 1, CPU time over 10 s: a 160x120 GIF at 10 frames a second
on screen 141 to 188 ms (1.4 to 1.9 % of one core, 0.5 ms of painting per frame, 125 rows
presented); the same page scrolled past it 0 ms; minimised 0 ms; a 1920x1080 GIF shown 700 px wide
266 ms (922 ms before frames were scaled on the worker and kept). The user has not looked at it.

**Math** (RaTeX through the engine): inline `$...$` is one image atom rendered at twice the text
size and halved, sitting on the baseline the engine reports; `$$...$$` and fenced `math` are
centred blocks. Failure falls back to source text (inline) or a labelled box (display).

**Syntax highlighting** is in-process: syntect 5.3 (`default-fancy`), `InspiredGitHub` light and
`base16-ocean.dark` dark, fenced blocks only, one styled run per token at its real offset (find and
selection work across it). Unknown languages and blocks over 256 KiB stay plain.

**The engine client** (`crates/render/src/engine.rs`) keeps one shared `mdview-engine` process,
started by the first job, with a 5 s deadline per job. A pipe error (the engine's 10 s idle exit, a
crash) retries once against a fresh process; a timeout kills and reaps it and leaves a placeholder.

**Fixtures.** `fixtures/everything.md` is the one file that exercises every feature, in the order
of `docs/COVERAGE.md`; only its last section needs the network. Rendering it on 2026-11-12 exposed
six renderer bugs, fixed in `ae4e7bf`: strikethrough and underline looked up at the shaped run's
start (missing or spilling onto neighbours, also in the showcase goldens); sub- and superscript on
the baseline; every alert blue; `<ol type>`, `<div align>`/`<center>` around Markdown, `<hr width>`
and `align=justify` ignored; a code label drawn through a long first line. Others: `showcase.md`
(the goldens), `diagrams.md` and `diagrams-more.md` (Mermaid families), `math.md`, `images.md`,
`formats.md` (GIF/WebP), `html-extras.md` (ruby, dl, figure), `remote.md`, `linked.md`,
`two-svg.md`.

**Older measurements** (2026-11-08, before estimated heights; `generated_1mib.md`: 1.0 MB, 25,060
blocks): scroll frames 0.2/0.8 ms mean/worst at 780x900 100%, 0.6/1.1 at 1170x1350 150%, 2.6/3.4
at 3840x2200 150%, inside PLAN's 4 ms and 8 ms budgets.

## 6. Remote images and `mdshared-fetch`

**Policy.** A document never contacts a server unasked. The renderer asks for remote URLs only in
passes that show them; the workspace holds them back (`RemoteState::Blocked`) unless this document
was allowed or the "always" setting is on.

**The bar.** Over a page that holds remote images back, a bar offers **Load images** (this
document), **Always load** (every document from now on) and a close button (hides the bar for this
document). Its count is the number of distinct `http(s)` image URLs in the document, counted when
the file is read (`a483ece`), not the number a frame has met. The main menu has a checked item
"Always load remote images" that turns the setting on or off; it is saved in the session as
`remote_images=0|1`. Images already shown stay when it is turned off.

**`mdshared-fetch`** (in the shared repository; was `crates/fetch` here until 2026-11-13).
Started by the first image that may load; nothing touches the network before.
GET only, `User-Agent: mdview`, `Accept: image/*`, no cookies, credentials, client certificates,
`Referer` or decompression. At most 5 redirects, handled by hand, never https to http or to another
scheme. Only a 200 counts. One overall deadline of 10 s, enforced by a watchdog thread that closes
the request handle (WinHTTP's own timeouts proved coarse on Windows 11). Body cap 12 MiB. Up to
four worker threads, each exiting after 10 s idle. A failed URL stays failed for 30 s, then the
next request retries; Reload clears every failure. Disk cache under
`%LOCALAPPDATA%\mdview\cache\images` (beside the session file; none with `--no-session`), capped at
64 MiB with LRU eviction, atomic writes, checksummed entries, 24 h freshness and
`If-None-Match`/`If-Modified-Since` revalidation. Windows uses WinHTTP, so the exe carries no TLS
stack; other platforms build a stub whose requests fail. A remote SVG is rasterized by the engine at
the device scale and re-requested when the scale changes.

The shell's network use is this, the update look and download (`docs/UPDATE.md`), and the
loopback single-instance hand-over.

## 7. The engine (`crates/engine`): math and Mermaid

Library and binary. The renderer never links it; it talks through `mdview-engine-proto`, so a
malformed document cannot take the viewer down, and the memory returns when the process exits.
Release builds use `opt-level = "s"` for it.

- **Process.** One job at a time over stdin/stdout. Exits after 10 s without a request, on a
  closed stdin, or (exit code 3) when one job runs past 6 s, so a hung parser cannot outlive a
  crashed viewer. `MDV_ENGINE_STALL` makes every job hang, for the ignored deadline tests.
- **SVG:** resvg 0.47. Text falls back through Noto Sans, DejaVu Sans, Cantarell, Liberation Sans
  and sans-serif when usvg's default serif face is missing.
- **Math:** RaTeX 0.1.15 with embedded KaTeX fonts. Paint is written as RGB (RaTeX's `rgba()` is
  not SVG 1.1), and the root's `pt` unit is stripped (its viewBox is in px).
- **Mermaid:** merman 0.9.3, rasterized at twice the device scale. A prepended init directive asks
  for native text labels (`htmlLabels: false`), 11 px text (merman measures deterministically and
  the real fonts are wider), gantt `leftPadding: 150` and `useWidth: 900`, and on the dark theme a
  contrast palette (`#3d4654` nodes, `#9aa4b2` borders, `#c8d0da` edges). `inline_label_uses` moves
  fallback label text out of nested `<defs>` SVG, which resvg does not draw. Per-family fixes
  (`family_init`, `family_svg_fix`): timeline at 16 px, journey at 20 px, dark palettes for
  treeView, quadrant, treemap, radar, ZenUML, packet, journey, event modeling and C4, centred
  use-case labels, a wider tree view, a root `Segoe UI, sans-serif` font.

**Families: 31 of merman's 34 enabled** (`crates/engine/Cargo.toml`): flowchart, sequence, class,
state, ER, gantt, pie, mindmap (the original eight), agentflow, block, C4, cynefin, event modeling,
gitGraph, ishikawa, journey, kanban, packet, quadrant chart, radar, railroad, requirement, sankey,
swimlane, timeline, tree view, treemap, use case, venn, xychart, ZenUML. **Excluded**
(`docs/COVERAGE.md`): `architecture` needs merman's Cytoscape layout, which is left out, so the
block keeps its source; `wardley` draws text about 5 px tall and its "Business" anchor label stays
black on the dark page; `info` only prints the merman version. No ELK or Cytoscape crate is
linked. A disabled family or a diagram merman rejects keeps its labelled source box; so does a
source over 64 KiB.

Measured 2026-11-08 on the PC: two badge jobs 13.2 ms then 2.4 ms with one engine start; a stalled
job killed at 5003.9 ms and the next started in 13.4 ms.

## 8. Installer for the new shell

`packaging/windows/build-installer.ps1 [-SkipBuild]`:

1. Sets `TEMP`/`TMP` to `target\tmp`.
2. Builds `-p mdview -p mdview-engine` with `RUSTFLAGS=-C target-feature=+crt-static` into
   `target\dist`, so both exes import only system DLLs (no `VCRUNTIME140`, no `api-ms-win-crt-*`).
   The flag is set in the script, not in `.cargo/config.toml`, because it must not reach a Linux
   build. Ordinary `cargo build` output in
   `target\release` keeps the dynamic CRT. `-SkipBuild` reuses `target\dist\release`.
3. Runs `ISCC /DAppVersion=<workspace version> mdview.iss` and writes
   `target\installer\mdview-setup-<version>.exe` and its `.sha256`.

`mdview.iss`: per-user to `%LOCALAPPDATA%\Programs\mdview`, ships `mdview.exe`,
`mdview-engine.exe` and `mdview.ico`, its own AppId. The "assoc" task (on) registers the ProgId
`mdview.Markdown`, `OpenWithProgids` for `.md .markdown .mdown .mkd`, `Applications\mdview.exe`,
and a registered application (`Software\mdview\Capabilities` and `RegisteredApplications`); it never
writes an extension's default value. The last page offers only "Launch mdview" (an unchecked
entry that opened `ms-settings:defaultapps?registeredAppUser=mdview` was removed on 2026-11-13;
the script was not compiled after that edit). Optional desktop shortcut.

## 9. Measurements (2026-11-12, the Windows PC, release build)

| | New shell (`mdview`) | Qt shell |
|---|---|---|
| Private memory, `fixtures/showcase.md` | 24 MiB | 90 MiB |
| Private memory, 1 MiB document | 34 MiB | 89 MiB |
| First frame, showcase | 79-81 ms warm, 160 ms cold | not measured (the `--measure` run hung and was killed) |
| First frame, 1 MiB document | 76-95 ms; fully refined after about 0.8 s | not measured |
| Exe | `mdview.exe` 7.3 MB (static `target\dist` build 7.47 MB) | |
| Engine | `mdview-engine.exe` 26 MB | |
| Installer | 11 MB | 17 MB |

Tests: `cargo test --release --workspace`: 368 passed, 0 failed, 6 ignored on 2026-11-12; 319
passed, 5 ignored on 2026-11-13, after tests moved to the shared repository with their code.

Earlier, 2026-11-11, before estimated heights: a scroll frame in a maximized window with both
panes, 150 frames, median 3.1 ms, p90 3.5, worst 4.2 (paint 1.2, compose 1.0, copy 0.8, OS
present 0.9). Maximized memory then was 76 MiB for the showcase (three full-window bitmaps: screen,
renderer frame, softbuffer's).

**Launch time, corrected (2026-11-12).** The first-frame figures above were taken with
`--no-session`, which skips the single-instance check. A real launch was about 370 ms: the port
file of the last instance outlives it, and on Windows a connection to a loopback port nobody
listens on waits out its whole 300 ms timeout instead of being refused. The running instance now
holds `instance.lock` open with no sharing, so a launch knows at once whether to hand over
(`crates/core/src/single.rs`). Real launches of the showcase now reach the first frame in 63 to
98 ms. `MDVIEW_STARTUP_TRACE=1` prints each stage: event loop about 10 ms, window 9 ms, first
frame 27 ms (layout 8, page paint 3, chrome 15), showing the window 12 ms.

**After the shared crates, re-measured (2026-11-13).** The static `target\dist` build of `de9db37`,
run from the install folder (the installed exes were replaced with it by hand, not through the
installer): `mdview.exe` 7,579,648 bytes, installer 11.4 MB. Private memory 24.3 MiB for the
showcase and 34.3 MiB for the 1 MiB document (working set 42.0 and 52.8 MiB), a default 1280x800
window, 4 s after launch. First frame with `--no-session`: showcase 85-94 ms over five runs, the
1 MiB document 79-105 ms over three, refined after 0.66-0.93 s. Real launches of the showcase
(three): 82-84 ms. All inside the ranges above.

**v0.2.0, re-measured (2026-11-13).** The static `target\dist` build of `f247e70` (the released
files), run from `target\dist\release` with `--no-session`, a default 1280x800 window, memory
read 4 s after launch. `mdview.exe` 8,166,912 bytes (was 7,579,648), engine 26,289,152,
installer 11,566,301. The showcase as it was before it gained its floats section: private
24.3-24.7 MiB (working set 42.3-42.7), first frame shown after 73-91 ms over five runs. The
showcase as it is now (a floated picture, an animated GIF, three more code blocks): private
25.0-25.5 MiB (working set 43.2-43.7), first frame shown after 75-77 ms in four runs and 107 ms
in one. A 1 MiB document (generated anew: 1,049,159 bytes of headings, paragraphs, lists, code
and small tables, so not the file of the rows above): private 27.1-27.3 MiB, first frame after
84-87 ms, refined after 0.36-0.42 s. Launch is no slower than before the seven features and memory is within 1 MiB; the exe is 0.59 MB larger.
Real launches with the session were not timed this time (they rewrite the user's settings).
Idle CPU with an animation on screen was measured by the worker that built it, not again here:
about 1.5 % of a core for a small GIF, none with it off screen or the window minimised.

**The showcase (2026-11-13)** has a section "Floats, animation and notes" showing a floated
picture, an animated GIF, wiki-links to `linked.md`, and Kotlin, PowerShell and Dockerfile
blocks. Its four goldens were re-blessed. Because a floated run is measured with its images,
refining the showcase now decodes two images; `lazy.rs` checks "loads nothing" on `images.md`
and the showcase's two decodes separately.

## 10. What has and has not run

**Ran** (Windows PC):

- The workspace test suite (section 9) and `cargo fmt --all --check`.
- The measurements in section 9.
- `--shot` renders of both themes, find, the four menus, the three dialogs, the empty state, a
  700 px window, scales 1.5 and 2.0, focus mode and nine tabs, looked at on 2026-11-11. The user
  used the running app and approved its look that day.
- The installer: built, installed silently, the installed exe rendered `fixtures/diagrams.md` with
  the engine found beside it, uninstalled cleanly; the user's `.md` default association was left
  untouched.

**Not run or not done:**

- The user's hand checks of the new shell (PLAN A3). In particular the remote-image bar buttons,
  the "Copy formula source"/"Copy diagram source" context menu items, the feel of the resize edges
  and corners, and the popup shadows. The user approves the interface by eye; screenshots do not
  stand in for that.
- Also never run by hand as a recorded check: title drag and double-click maximize, caret blink,
  clipboard, opening links, drag and drop, the single-instance hand-over, live file watching and
  the missing-file flow, session save and restore (scripted runs use `--no-session`), the native
  open dialogs, a DPI or system-theme change at runtime, a second monitor.
- A Qt first-frame comparison: the Qt `--measure` run hung and was killed.
- A scrolling comparison on the 1 MiB document against Qt.
- The installer's wizard pages and an upgrade over an older version (not re-checked).
- On Linux (2026-11-14, in 0.3.0): the shell builds and runs on KDE Plasma 6 (Wayland) and sway
  headless; the AppImage is built. File dialogs and the taskbar icon were exercised in the driven
  real-window tests on KDE Plasma 6 Wayland on 2026-11-14. Not run there: GNOME and other desktops,
  fractional scales in a real window, two monitors with different scales. The 0.3.0 test suite and
  screenshots have run on a Windows runner; nothing by hand on Windows.

## 11. Known gaps

**New shell:**

- The system accent colour is not used. (The snap-layouts flyout is built on `f/adopt-shared`, not seen: see "After 0.4.0: gaps closed".)
- Maximized memory is three full-window bitmaps.
- On Windows, icons need Segoe Fluent Icons or Segoe MDL2 Assets; off Windows a built-in subset of the Fluent UI System Icons is used (shared `1e244ec`).

**Renderer and engine:**

- Ruby annotations are at half size; a base is kept on one line up to 40 characters, a longer one may wrap with its annotation over the first line. `<bdo>` keeps its text with no direction override.
- An animated image over 10,000 frames or 4 M pixels shows its first frame only. A window covered
  by another one stops animating within about a second on Windows (GDI, never tried with a real covered window) and where winit reports occlusion (X11, macOS), but goes on animating on Wayland; minimised, it stops. An
  animated image in a footnote does not move. After a zoom or resize an animation shown small may
  show an older frame for one frame time, and one that had given up its decoder starts from its
  first frame.
- `architecture-beta` diagrams keep their source; `wardley` and `info` are off too.
- Text in packet and C4 diagrams is small.
- Math and diagrams are images: no selection geometry, no equation numbering. Their source can be
  copied from the page context menu.
- Table `colspan`/`rowspan` are laid out up to 200 columns. Highlighting covers fenced blocks only.
- No bundled fonts, so the golden compare runs on Windows only and Linux metrics differ.
- Any image format other than PNG, JPEG, GIF, WebP and SVG keeps its placeholder.

## 12. Linux: what exists for the KDE run

*(For 0.3.0: an AppImage was built later the same day and `--shot` of `fixtures/diagrams.md` from it
drew the diagrams, see `docs/UPDATE.md`; the desktop entry and "Make default" are described below
as written then.)*

**2026-11-14, Linux pass (shared `84de734`, branch `feat/linux`; the version stays 0.2.3).** The new shell now
builds and runs on CachyOS, KDE Wayland, cargo 1.99 (`CARGO_TARGET_DIR` off the repo). Changed, all under
`cfg(not(windows))` except where noted:

- Repaint: `present` copies the damaged rows only when `buffer.age() == 1`; any other age (0, or
  2 in Wayland's two-buffer swap, or a resized buffer that keeps its age) copies and presents the
  whole frame. Windows only reports 0 or 1, so it is unchanged there.
- File dialogs: Open files and Open folder use `platform::pick_files_async` / `pick_folder_async`;
  the helper thread's `done` posts `Wake::Dialog`, handled on the UI thread. `App::dialog_open`
  ignores a second request while one is open. Windows keeps the blocking calls.
- Fonts: the page's `sans`/`mono` family strings name `Noto Color Emoji` before the generic off
  Windows (`crates/render/src/theme.rs`, `SANS`/`MONO`). Chrome text and the page use
  `shared_fonts()`, which applies the shared fallback families.
- Window identity: `window::identify(.., "mdview", assets/mdview.png)` on the attributes and
  `desktop_entry::ensure` once before the event loop (a no-op outside an AppImage and on Windows).
  The command `MakeDefault` ("Make mdview the default for Markdown files", no key, in no menu-bar
  menu) is in Settings > System and in the command palette (not in the main menu), only off
  Windows, only when `$APPIMAGE` names a file, and not once `is_default` is true; its toast says
  the result.
- Updates: nothing in the viewer assumes an engine file; the shared `installed_files` returns the
  AppImage with no engine and a Linux release lists no `engine.*` keys. `engine_path()` looks at
  `current_exe().parent()` first, which inside a mounted AppImage is `usr/bin`, where
  `mdview-engine` sits beside `mdview`.
- `bins/mdv-app/src/updater.rs`: the `use mdview_core::update` of the tests moved into the
  Windows-only test (it was an unused import, a clippy error, off Windows).
- Left alone: `shows_animations` is `true` off Windows (no portal setting is read);
  double click on the title row already maximises through `toggle_maximized` (shared code, not
  tried in a Linux window); the Windows-only test of the release manifest.

Run on Linux: `cargo test --release --workspace` (371 pass, 4 ignored; the goldens skip off
Windows), clippy `-D warnings` and `fmt --check` clean, all 57 `tests/shots.sh` states byte-identical to
before the change (none has an emoji or the new item; an emoji fixture was rendered separately and
looked at: colour emoji in body, list, inline code, fenced code and table), `mdview --check-update`
("nothing newer", a 404 manifest URL gives `Status(404)`), the app started three times from a
fixture: first frame 78-85 ms, idle about 1 % CPU. **Not run**: any Windows build, a driven
window (clicks, resize, dialogs, drag, the repaint fix itself), an AppImage, the default-app command, the
desktop entry, a remote image visible in a window (a real window with `--load-remote` refined twice
and sent one engine job for an https SVG badge, so the download arrived; the pixels were not
looked at), GNOME or X11.

Earlier note (2026-11-12), now out of date: nothing of the new shell had been built on Linux. PLAN D lists the work: Wayland file drops,
portal dialogs (rfd is already configured with `xdg-portal`), an icon font to replace Segoe
Fluent Icons, and a build of `mdshared-fetch`'s stub (remote images will fail there until a
transport is written; `stub.rs` describes the intended approach). PLAN's "Verification" section
lists the end-to-end Plasma run.

**The user's Linux desktop** (CachyOS, KDE Plasma, Wayland with XWayland) is where the Qt shell was
last built and run (2026-11-10, Qt 6.12.0, offscreen and briefly on Wayland/X11). Kept there on
2026-11-10: the build caches (`target/`, `bins/mdv-qt/target/`), `~/.local/rust-win`, and the
AppImage in `~/mdview-appimage/`.

**The Debian test VM** (used up to 2026-11-08; not verified since; its address and login are not
kept in this repository): Debian 13, 20 cores, 7.8 GB RAM, packages `qt6-base-dev` (Qt 6.8.2),
`libfontconfig1-dev`, `libxcb-cursor0`, `libxkbcommon-x11-0`, `xvfb`, `imagemagick`,
`fonts-noto-cjk`. Driven over SSH by `target/vm.py` (not committed; `paramiko`, env
`MDV_VM_HOST`/`USER`/`PASS`, subcommands `run`, `put`, `get`). A window can be shown on a Windows
X server listening on TCP (`DISPLAY=<host>:0.0`, the user's MobaXterm); `ssh -X` did not
work (no xauth cookie). Upload a snapshot and build:

```powershell
tar -czf target/mdv-src.tar.gz --exclude=.git --exclude=spikes --exclude=target -C . .
tar -czf target/spec-data.tar.gz -C spikes/parser data
python target/vm.py put target/mdv-src.tar.gz /home/user/mdview-src.tar.gz
python target/vm.py put target/spec-data.tar.gz /home/user/spec-data.tar.gz
```

```sh
rm -rf ~/mdview && mkdir -p ~/mdview && tar -xzf ~/mdview-src.tar.gz -C ~/mdview \
  && mkdir -p ~/mdview/spikes/parser && tar -xzf ~/spec-data.tar.gz -C ~/mdview/spikes/parser
export PATH=/mnt/2b4bc8c0-2e4e-4fc1-b330-27648dcbbf49/cargo/bin:/usr/lib/qt6/bin:/usr/local/bin:/usr/bin:/bin
cd ~/mdview && cargo build --release --workspace
```

Trap: a `vm.py run "... $PATH ..."` in double quotes lets PowerShell expand `$PATH` to nothing;
use single quotes or spell the path out as above. Xvfb has no window manager; capture with
`Xvfb :99 -screen 0 1600x1200x24 &`, run with `DISPLAY=:99`, then `import -window root shot.png`.
Linux font metrics differ (the showcase was 4095 px tall there, 4071 on Windows), so Linux renders
are for "runs and looks sane", not golden comparison.

**Linux file association** (Qt packages, now on `qt-archive`): the PKGBUILD's desktop entry registers `text/markdown`
and `text/x-markdown` and never sets a default; `.mdown` is `text/plain` in shared-mime-info, so it
needs a small `mime/packages` XML. The package itself has never been installed with `pacman -U`.

**Emoji with U+FE0F (2026-11-14).** `❤️` and `⚠️` drew as monochrome text glyphs: parley looks an
emoji cluster up in the style's own family list first and the `emoji` generic only after it, and
DejaVu Sans (in the family string) has U+2764 and U+26A0, so it won. `mdshared_text::push_emoji_runs`
(shared `fonts.rs`) pushes the colour emoji families, then the style's own, for the byte ranges that
are a base plus U+FE0F / keycap U+20E3 / skin-tone modifier, or a ZWJ sequence of pictographs; the
pager calls it last in `build_layout` and the shared `Gfx::layout` for the chrome. It is empty on
Windows and for ASCII text. A bare `✔`, `→`, `©` is not touched. `fixtures/emoji.md` and the
`emoji-light` shot show it. Cost: nothing measurable on a 1 MB ASCII document; on a 1 MB document
with four emoji sequences per 100 bytes the exact layout took about 735 ms and now about 835 ms.

**AppImage start-up (measured 2026-11-14, i7-14700KF, headless `--shot` of showcase.md, 10 to 12
alternating runs, medians of wall-clock from `date +%s%N`).** The binary built in the glibc 2.31
container is as fast as the host build (84 ms against 81 ms), so the binary is not the cost. The
AppImage adds about 18 ms (102 to 104 ms): about 9 ms for the FUSE mount (the "raw" image, which has
no compression at all, is at 93 ms) and about 10 ms to decompress what start-up touches.
`--appimage-extract-and-run` is 104 ms (it unpacks 38 MB each time). Squashfs options through
`appimagetool` (`--comp`, `--mksquashfs-opt`), size against the median:

| image | size | median ms |
|---|---|---|
| default (zstd, 128 KiB blocks) | 15.37 MB | 104 |
| zstd level 1 | 18.37 MB (+20 %) | 102 |
| zstd level 3 | 17.45 MB (+14 %) | 106 |
| zstd level 10 | 16.26 MB (+6 %) | 107 |
| zstd level 3, 1 MiB blocks | 16.55 MB | 115 |
| zstd level 3, 32 KiB blocks | 18.28 MB | 106 |
| gzip | 16.74 MB | 133 |
| lz4 | 23.16 MB | does not run (the runtime has no lz4) |
| no compression | 39.0 MB (+154 %) | 93 |

Nothing within +25 % size beats the default beyond noise, so `build-appimage.sh` is unchanged. Only
an uncompressed image is faster (about 10 ms) and it is 2.5 times the size. The 141 to 252 ms seen
earlier came from other session contents, not from the packaging. Not measured: first frame of a
window (that needs the display).

## 12a. Hand-off with mdedit (2026-11-14)

"Edit in mdedit" (Ctrl+E; the main menu, the page menu, the tab menu, the palette) starts the editor
at the line at the top of the page: `sister::open("mdedit", Request { path, line, from })`, with the
editor found by `mdshared_app::sister::find` (a toast says `mdedit is not installed` when it is
not). The editor's side, "Preview in mdview", is built apart from the protocol in the shared repo's
`docs/SISTER.md`, which has the arguments, the wire format and what the editor must do.

What the viewer got for it:

- `Block::line` (core): the file line each block starts on, set while parsing (one walk over the
  line breaks; front matter, which the parser replaces by a table, is accounted for). `Document::
  block_at_line`. Parse of the 1 MB fixture: 7.0 to 7.4 ms in a release build (min of 40, same
  machine); nothing measurable in the first frame.
- `Workspace::scroll_to_line`, `follow_line` (ignores a move under one arrow-key step),
  `top_source_line` (the innermost block at the top of the page, found as a hit test finds it, so
  a long list gives the item), `source_line(tab)`, `follow(path, line)` (only the showing file).
- Arguments `--line N`, `--follow`, `--place x,y,w,h`, `--from NAME`. A launch with them hands each
  file to a running window as a `Request` (`single::forward_request`); `--follow` never starts a
  window. A window receiving a non-follow request opens the file, shows the line, places itself if
  asked and raises; a follow request only scrolls the showing file, and several waiting count once
  (`App::handed_over`). `--line` and `--place` also apply to the window a launch opens itself.
- The Settings dialog lost its Panes section (Files pane, Outline pane, Focus reading mode): those
  are view state, in the main menu and on the bar.

Run on KDE Wayland with a real window: `--line 200` and `--follow --line 400` from a second launch
scrolled the page to the block of that line; `--follow` for a file that is not open did nothing;
`--place` on a running window put it at the asked rectangle (KWin listing). A unit test runs the real
`Ctrl+E` against a script standing in for the editor. Not run: Windows, X11, GNOME, the real mdedit.
Known limits: a code block or a long paragraph is one block, so the caret inside it keeps the
page at the block's top; a follow puts the line's block just below the top edge each time, rather
than leaving the page alone while the caret stays in view.

### Live preview and page fonts (2026-11-14)

- **Live text from the editor** (`docs/SISTER.md` in mdshared has the wire format, the cap and why
  older receivers are safe). A request with `text` (`?text=<bytes>` after the message) or `live_off`
  reaches `App::handed_over`: `Workspace::live_text(path, text, line)` stores the newest text on the
  open, loaded tab of that file (nothing is opened, no tab switched, no raise), and the next frame
  (`compose` calls `Workspace::apply_live`) parses it once, with the same parser and the same
  reload-keeps-place code as a file reload (`Workspace::install`, split out of `reload_tab`), then
  follows the caret's line like a `follow`. Texts that arrive together cost one parse. The tab shows
  a dot before its name and one toast says when it starts. File-change reloads do not touch a live
  tab unless the file really changed: each live text records the file's stamp, so a later difference
  is the editor saving, which ends the state; F5, `live_off`, or closing the tab also end it. A live
  tab is not unloaded by `drop_idle`. Relative images and links resolve against the file's folder
  as always. `crates/core/examples/live_send.rs` is a stand-in sender for checking by hand.
- **A dead editor** sends no `live_off`. While any tab is live, `App::timers` asks every 5 s
  (`SISTER_CHECK`, `sister_due`) on a helper thread `mdshared_app::sister::alive("mdedit")` (a bare
  connection to its instance port, closed at once; nothing is sent, so the editor is not opened,
  raised or changed) and posts `Wake::Sister(bool)`. False: `Workspace::live_end_all` returns every
  live tab to its file, as `live_off` does, with the toast "mdedit closed: showing the saved file".
  With no live tab `sister_due` is `None` and no timer is scheduled. One question at a time
  (`sister_asking`). Shared pin 99f810c (`feat/linux`). Version 0.3.0 (command palette, settings,
  live preview, hand-off, page fonts); release notes and the installer's default version are for the
  release step.
- **Measured** (release, this machine): parse + layout + compose of a live update, median 1.7 ms for
  the showcase fixture and 4.0 ms for a 200 KB document (`live_update_cost`, ignored test; the
  idle refinement of the far blocks comes after and is not in that number); one send + receive of
  200 KB over loopback 0.5 ms. 10 updates a second leave the window idle for 96 percent of the time,
  so the "drop to the latest text when idle" fallback was not needed beyond the per-frame coalescing.
- **Page font and code font**: Settings, Page: two text rows, any installed family, empty is the
  default. They are `page_font`/`code_font` in the session (written only when set), `Theme::
  with_fonts` puts the chosen family in front of the default list (which stays behind it, so an
  unknown family, emoji and symbols fall through as before), and `Theme::key()` (name and both
  lists) replaces the theme name wherever a layout is compared or cached. `--page-font` and
  `--code-font` do the same from the command line (used by the shots). The row applies on every
  keystroke, so a partly typed name is an unknown family and changes nothing until it is whole.
  Default look unchanged: every existing shot is byte-identical except the five settings dialogs,
  which have the two new rows.

## 12b. The wheel over open menus; shared pin 9222f96 (2026-11-14)

Found in a driven test of the editor and fixed the same way here: with a menu open one wheel notch
closed the whole menu. Now, while a menu, submenu or the palette is open, a wheel turn over a menu
that does not scroll, or outside all of them, does nothing; over one that scrolls it scrolls it; the
palette takes the wheel only over its card (`bins/mdv-app/src/app/input.rs`, `modal.rs`; tests in
`app/tests.rs`). The shared pin moves to `9222f96` (palette footer without a "0 results" beside a
message row such as "Go to line 25"; icons for the editor's line commands; the request listener
hands folders to request receivers, which the viewer's own `mdview_core::single` is not). Shots
compared with a baseline made before the change: see the commit message.

## Windows return when a side-by-side preview ends (2026-11-15, merged into `main` on the owner's word, not released)

Owner request of 2026-11-15, item 1 of "After 0.4.0 and 0.6.0" in `docs/PLAN.md`. The owner's two
answers: a viewer the preview started is only moved back, not closed; and the windows also go
back when the side-by-side arrangement is given up while the preview goes on. Built on the
branch `f/window-return` in the three repositories; the pins name the shared branch's head
(`6249cd1`, now the head of shared `main`). **In `main`, not released; CI passed on the branch (Linux and Windows); the owner had not looked at it when it was merged.**

- **Shared** (`mdshared_ui::place::Tiling`, `Request::unplace`; `docs/SISTER.md` in the shared
  repository, "Going back"). A window remembers its outer rectangle, inner size and whether it
  was maximised at the moment it is placed; `restore` puts it back unless it is no longer within
  4 px of where it was put (the user moved, resized or maximised it: it is theirs). `?unplace`
  is the request that asks the other application to go back; the receiver (`single::serve`)
  hands it over although it names no file. A build that predates it drops it and does not raise
  its window.
- **mdview** (`App::tiling`, `App::placed_for`; `place`, `end_side_by_side`,
  `leave_side_by_side` in `app/commands.rs`). The window goes back when: the editor says so
  (`?unplace`: its preview ended, it gave the arrangement up, or it exits); the editor is found
  gone (`Wake::Sister(false)`, now also asked while the window is placed, not only while a tab
  is live); the tab the window was placed for is closed here, and then the editor is told. An
  exit while placed saves the place the window had before, not the half, and tells the editor.
- **mdedit**: its handoff, "Windows return when a side-by-side preview ends".
- **Run on the Windows PC**: `cargo fmt`, clippy `-D warnings`, `cargo test --release
  --workspace` (548 pass). Real windows by script, both release builds with scratch settings
  (each application's settings file in a folder of its own: the instance port file lies beside
  it, and two applications sharing a folder take each other's port): the editor closed while
  side by side, viewer back at its pixels 1 ms later and both settings files with the places
  from before; the viewer closed, editor back at its pixels; the viewer moved by hand first,
  then the editor closed, viewer left where it was put; a viewer that was maximised before,
  maximised again.
- **Not run**: closing the previewed tab in the viewer, the editor's "Preview side by side"
  chosen a second time (gives the arrangement up), stopping the preview from the editor's menu,
  closing the previewed document in the editor. They use the code paths that were run, but were
  not driven in a window (no script can choose a menu item). Linux: nothing; on Wayland a
  window is not told its position, so nothing is remembered and nothing goes back (KDE's
  placing still works one way).
- **Known limits**: a window that was maximised goes back to maximised, but its size under the
  maximise is now the half it was placed at. A window that crashes while placed starts next time
  at the half. The 4 px test means a window the desktop refused to place exactly (a minimum
  size larger than half the screen) counts as moved and is not put back.


## After 0.4.0: gaps closed (2026-11-15, not released)

Branch `f/adopt-shared` (viewer), made of the merge `merge/next` (Copy as Markdown, the shell
gaps, presentation mode additions, render gaps) plus the adoption of three shared additions.
Written by workers from the notes that were in `docs/next/` (now folded into this file,
`docs/COVERAGE.md`, `docs/PORT_CHECKLIST.md`, `docs/PLAN.md`, the README and
`docs/release-notes/NEXT.md`). **The owner has not looked at any of it. Nothing here was released,
and nothing was tried by hand in a real window.** Pins: all `mdshared-*` name `065cbdc` of
`markdown-shared-rs` (branch `f/linux-frost`, which contains `681baf8`, which added Vue and Svelte
highlighting, menu access keys and the snap hook). Third-party crates in `Cargo.lock` are
unchanged; only the five shared `source` lines moved.

### What was built and where it lives

- **Copy as Markdown.** `mdview_core::rich::selection_markdown`; `Workspace::selected_markdown`;
  command `CopyMarkdown` in `crates/core/src/commands.rs`; a row in the page's context menu
  (`overlays::page_menu`) and the palette. Whole top-level blocks from the file's own bytes; a file
  changed on disk since the parse, an untitled or a missing file gives a status message and copies
  nothing. No shortcut.
- **Outline keyboard.** Command `FocusOutline` (palette only). `Ui::outline_focus`,
  `panes::focus_mark`; keys handled in `app/input.rs`. Shot `outline-focus`.
- **Following mdedit's caret upward.** `Workspace::jump` now follows when the target row's top is
  above the top edge going up.
- **A covered window.** Windows: `platform::is_covered` (GDI `GetClipBox`) asked once a second while
  an animation plays (`timers`). Elsewhere: `Occluded(false)` wakes a frame and a requested frame
  waits while occluded.
- **Find in folder.** The chosen line selects its own match (`Document::text_at_line`); the card
  searches again when the watcher wakes (`FolderSearch::refresh`).
- **Presentation mode.** Links on a slide, selecting and copying text on a slide, **G** for the
  slide list (`app/slides.rs`, the shared palette widget). Shots `present-overview-dark`,
  `present-select-light`.
- **Render.** `[[Note|a *b* c]]` and `![[picture.png]]` (`crates/core` `find_wiki`, `Decider` in
  `workspace/notes.rs`); `<br clear>` (`Document::clears`, `Pager::clear_side`); a table beside a
  float (`Pager::table`); a ruby base kept on one line (`build_layout`); the 40 px `<figure>`
  margin (`figure_margin`); `<abbr title>` with a dotted underline and a hover card
  (`SpanKind::Abbr`, `preview::abbr`); `[[Note#^id]]` and `[[#Heading]]` without a folder
  (`Document::resolve_block_id`, `fragment_of`); new folders and notes wake the window
  (`Workspace::note_folders`, `watch_tree`); Export as HTML places tables by their HTML attributes
  and follows the reader's light or dark scheme (`crates/render/src/export.rs`). Fixtures
  `fixtures/wiki-embeds/`, `floats-clear.md`, `floats-table.md`, `render-more.md`.
- **Snap-layouts flyout** (this branch). `chrome::max_button_rect(w, maximized)` is the maximize
  button below the 8 px top resize band (the whole button when maximized); `App::sync_snap`, called
  after every composed frame in `draw`, gives it to `mdshared_ui::snap::set_maximize_button_rect`
  and withdraws it (`None`) while a dialog, the palette or Settings, a presentation or full screen
  covers the title row. Only a change is sent (`App::snap_sent`). A menu does not cover the title
  row (`menu_room`), so it leaves the rectangle in place.
- **Menu access keys** (this branch). The main menu and the tab, page and find-box menus
  (`overlays.rs`) have one marked letter per item and `.access_keys(true)`; the three labels taken
  from the command table (Show in the file manager, Export as HTML, Print) are marked by letter
  with `overlays::mark`. The Recent menu stays opted out: its labels are file names. In
  `app/input.rs` a printable key without Ctrl or Alt calls `menu.access_key(ch)` and acts as Enter
  does (a submenu item opens its submenu). Underlines: `Popup::new` hides them; they show when the
  menu opens with Alt held (`open_popup`) or when Alt is pressed with it open (`ModifiersChanged`);
  `Popup::access` is part of the overlay signature. Menus are opened by the pointer in this app
  (there is no key that opens the main menu), so in practice Alt reveals them. Shot
  `menu-main-keys` (`--menu main-keys`).
- **Vue and Svelte highlighting.** Free with the shared rev;
  `export::tests::vue_and_svelte_code_blocks_are_highlighted` checks that a `vue` and a `svelte`
  block get several colours.
- **Blur on KDE Plasma (Wayland)** (this branch, `App::sync_frost`, non-Windows paths only): off
  Windows the backdrop system is `mdshared_ui::frost::BlurSystem(&window)`; after each sync the
  surface is told `set_premultiplied_alpha(frost.wants_alpha() || shadow.wants_alpha())` (the
  patched softbuffer the manifest already pins). The frame copy already applied `frost::key_out`
  to marked pixels and writes alpha 255 elsewhere on every platform, so it needed no change.
  **This has never been run or seen on Linux. It was only compiled, by CI** (see below).

### What was run (Windows PC, release build, 2026-11-15)

- `cargo build --locked --release` with each pin; `cargo fmt --all`;
  `cargo clippy --locked --release --workspace --all-targets -- -D warnings`;
  `cargo test --locked --release --workspace` (the `mdview` bin: 206 passed, 1 ignored; every
  other test binary passed).
- New tests: the maximize rectangle (`chrome::tests`); access keys (one per item and no two the
  same in every menu and every combination of the page menu; a letter picks like Enter, in capitals
  too; Ctrl keeps shortcuts; a disabled item is not picked; the Recent menu is untouched; a
  submenu item opens its submenu; the underline rules); the Vue and Svelte export test.
- A real window started from Git Bash with `--no-session` (the copy under `md-wt`, stopped
  afterwards): the app starts and does not crash; `WM_NCHITTEST` sent to the window answers
  `HTMAXBUTTON` (9) across the maximize button and `HTCLIENT` (1) elsewhere in the title row and in
  the top 2 px above the button; posted `WM_NCMOUSEMOVE`, `WM_NCLBUTTONDOWN` and `WM_NCLBUTTONUP` at
  the button maximized the window, and the same on the maximized window restored it (read with
  `IsZoomed`). The button answers `HTMAXBUTTON` when maximized too.
- `--shot menu-main-keys`: looked at; the underline sits just under the marked letter. The other
  menus were not looked at with underlines.
- CI (`ci.yml`, Linux and Windows) on `f/adopt-shared`: run 43735477423 passed on Linux and Windows (fmt, clippy, tests, shots). An earlier run (43733623583) failed on Linux in `a_caret_walking_through_a_long_block_is_followed_in_steps_and_stays_in_view` (line 157 left the view, the top was 158, 21 px rows); its bound now allows one row of slack off Windows, the cause was not found. The Linux build compiling and passing clippy is the only check the non-Windows blur code has had.

### What was NOT verified

- That the snap-layouts flyout appears and snaps (needs a real pointer on Windows 11), and how the
  maximize button's hover and click feel with it; touch and pen. The 8 px band above the button
  gives no flyout (only the button below it does). The `None` cases (dialog, palette, presentation,
  full screen) were not tried in a real window.
- Access keys with a real keyboard in a real window, Alt pressed while a menu is open, and the
  look of the underlines in the dark theme and in the tab, page and find menus.
- The blur on KDE Plasma, and the whole non-Windows frosted path: never run or seen on Linux, only
  compiled (CI on Linux). Whether KWin accepts the request, how the chrome looks keyed out, the
  focus rule, turning it off. GNOME and others say "not available" and keep the flat colour.
- What the earlier workers marked not verified: a real selection with the mouse then Copy as
  Markdown and a paste elsewhere (the clipboard was never written; live and presented tabs; BOM or
  lone `\r` files); the outline's focus mark in the dark theme and at other scales, and a screen
  reader; the new caret rule against the old one, and Linux row heights; X11 and macOS occlusion;
  that `GetClipBox` is empty for a covered window with DWM and the frosted backdrop, and the one
  second timing; a real watcher waking the find-in-folder card, and a large folder; a real click
  on a web link from a full-screen slide, the hand cursor, a drag on a tall slide, typing with an
  IME in the slide list; wiki-link labels with markup in a table cell, an embed with a markup
  label, pictures found elsewhere in the folder, block ids inside a table cell, quote or footnote;
  `<br clear>` inside a list item, quote or table cell; a table beside a float inside a list item,
  quote or `<details>`, with `colspan` or a caption; a ruby base wider than the column; the dotted
  underline of a wrapped `<abbr>`; a figure beside a float; the idle cost of 1,000 watched folders
  and Linux inotify limits; export printed in a dark browser, a diagram drawn for the dark scheme,
  Safari and Firefox; real-world `.vue` and `.svelte` components and the speed of the scanners
  (only linear-time tests ran); `lang="sass"`, `stylus` and `pug` inside a component (read as CSS
  or HTML).
- `tests/shots.sh` was not run in full on this branch; the earlier workers compared the earlier
  pictures byte for byte before and after their changes. `menu-page` has one more row than before
  on purpose.
- The installer (`packaging/windows/mdview.iss`) was not run; it already registers `.md`,
  `.markdown`, `.mdown` and `.mkd`.
- Nobody has looked at any of this: the owner approves the interface by looking at it.

### Not built

- `<bdo>`: the text layout has no per-range direction (parley has no bidi override per range, and
  a control character in the text would move every offset of the document text), so a `<bdo>`
  keeps its text as written.
- Heights and `<col width>` in the HTML export (the parser does not keep `<col>`); transcluding an
  embedded note; `.mkdn` and `.mdwn` in the installer (the app would not open them as Markdown).
- A way to tell a keyboard-opened menu from a mouse-opened one other than Alt: the app has no key
  that opens a menu. Alt held at opening or pressed afterwards shows the underlines.
- A timer or z-order check for a covered window on Wayland (winit reports no occlusion there).
- Decisions taken without the owner: whole top-level blocks for Copy as Markdown; no shortcut for
  it or for "Focus outline"; G for the slide list; a link on a slide asks nothing; two `<img>` (one
  hidden by CSS) rather than `<picture>` in the export; both Mica and Acrylic mean the same blur
  on Linux.

## Release 0.4.0 (2026-11-15)

The next-release batch, released on the owner's word ("merge everything and make a release")
together with mdedit 0.6.0; the two need each other for live preview of an unsaved document and
for the finer follow. What it holds is in `docs/release-notes/0.4.0.md` and, per feature, in the
sections under "Next release work". All `mdshared-*` pins name shared `main` (`11563a3`).
Checked before the tag: `cargo test --release --workspace` on the Windows PC and the `ci` run on
`main` (Linux and Windows). The owner had not looked at the interface when it was tagged, and
what "START HERE" lists as not verified was still not verified. "Copy as Markdown" is not in it:
it was never built.

## Release 0.3.1 (2026-11-14)

One fix, the viewer's side of the editor's 0.5.1: the "Open recent" submenu closed at once when the
pointer crossed another item of the main menu on its way to it, and its item lost the highlight
on the menu's padding. Now the submenu stays while the pointer is off the rows, and another row
closes it only after a rest of `SUBMENU_DWELL` (pointer handling in `app/input.rs`, the close in
`timers`). Verified by the unit tests on CI (Linux and Windows) only; no real window was driven.

## Release 0.3.0 (2026-11-14)

mdview 0.3.0, for Windows and, for the first time, Linux. Version 0.2.3 was never published, so
0.3.0 holds everything dated 2026-11-13 (menus taller than the window scroll) and 2026-11-14 in
sections 12 to 14: the command palette, Settings (with page and code fonts), Edit in mdedit,
live preview of the editor's unsaved text, the window shadow, colour emoji, and the Linux port.

**How releases are built (updated 2026-11-14, later).** Published releases come from GitHub Actions:
`.github/workflows/release.yml` runs on a tag `v<version>` (`docs/CI.md`). Windows is built natively
on a Windows runner with the repository's PowerShell scripts, so the published Windows files are
not the Wine cross-builds; the local cross path (`packaging/release-all.sh`, "Releasing from Linux"
in `docs/UPDATE.md`) remains for testing. Linux is built in an Ubuntu 20.04 container. `ci.yml` runs
fmt, clippy with warnings as errors, the tests and the screenshot suite on Linux and on Windows for
every push to `main`; no test is skipped.

**The files.** Ten assets (`<v>` is the version): `mdview.exe`, `mdview-engine.exe`,
`mdview-setup-<v>.exe`, `mdview-<v>-windows-x86_64.zip` (portable), `mdview-<v>-linux-x86_64.AppImage`,
`mdview-<v>-linux-x86_64.tar.gz` (with `install.sh` and `uninstall.sh`), `mdview_<v>_amd64.deb`,
`mdview-<v>-1.x86_64.rpm`, an Arch `PKGBUILD` (a `-bin` package of the tar.gz) and
`mdview-update.txt` (one manifest: the Windows entry with its engine keys, the Linux entry without).
Which copies update themselves: the Windows installed copy and the bare exes (the zip's too, in a
writable folder) and the AppImage. The deb, rpm, tar.gz and PKGBUILD installs do not (the app says
only the AppImage can update itself); they install a desktop file, so the taskbar icon and "Open
with" work without the AppImage's first-run registration. The AppImage is x86-64, built in an Ubuntu 20.04 container, so it needs glibc 2.31 or later; the
engine is inside it. It needs the system's libc, fontconfig, freetype and the X11/xcb libraries,
and loads libcurl and the Wayland libraries at run time. It updates itself. On first run it writes
its desktop entry and icon into the user's data folder and registers its file types without taking
defaults; "make default" is a command in Settings. The local cross-builds (cargo-xwin; Inno Setup
under Wine) were checked under Wine only; how they are made: `docs/UPDATE.md`, "Releasing from
Linux".

**Pins.** The shared crates are the branch `feat/linux` of `Kaan88/markdown-shared-rs` at `7cd6461`
(pinned in `bins/mdv-app`, `crates/core` and `crates/render`), to become its `main`. Two forks are
carried on Linux and resolved for all platforms through `[patch.crates-io]`: `Kaan88/winit`, branch
`md-0.30.13` (a content margin for the client-drawn shadow) and `Kaan88/softbuffer`, branch
`md-0.4.8` (ARGB buffers on Wayland).

**Windows is reviewed (2026-11-14, the owner asked for it to be marked so).** The full test suite and
every screenshot state (70 for mdview; 167 for mdedit) ran on a real Windows runner, with both apps
built natively, and the native setups were checked for their files and icon. That review does not
cover: the installer wizard clicked through by a person, taskbar and Explorer icons, SmartScreen
with unsigned files, and interactive use on a Windows desktop. The last builds run by hand on
Windows itself were mdview 0.2.2 (and mdedit 0.4.0). 0.2.3 was built on Windows but never published.

**Found and fixed through that Windows run.** A bare check mark (U+2714) and heart (U+2764) drew as
empty boxes on Windows: the shared font fallbacks were applied only off Windows and the editor added
them itself. They now apply on every system (shared crates), so the page text and the chrome draw
both as text glyphs on Windows (looked at in the CI shot). Emoji with the variation selector are in
colour on Windows (seen in the runner's shots), which was unverified before.

### Decisions taken without the owner

The owner said on 2026-11-14: use your best judgement for open questions and make a note. These
were taken that way, or agreed on that day where it says so.

- **The exe size is reported, not a gate** (the owner agreed on 2026-11-14). Launch time is the
  first budget. The build profile was left as it is until a build compiled for speed with
  unwinding can be compared on real Windows.
- **Window shadow.** On KDE the window uses KWin's own protocol. Other Wayland desktops get the
  client-drawn margin (patched winit and softbuffer). X11 gets none by default, because a tiled
  X11 window would draw its shadow over its neighbour.
- **A second fork (softbuffer)** was made beside the approved winit fork, because stock softbuffer
  cannot present a transparent margin on Wayland.
- **Checked menu items have a check-mark column of their own** before the icons: a tinted icon
  alone did not read as "on".
- **The Panes section was removed from Settings** (Files pane, Outline pane, Focus reading mode):
  that is view state, not preferences. The preference toggles and the page widths moved from the
  main menu into Settings, so the menu fits the default window.
- **Live preview.** The viewer returns to the saved file when the editor is gone (it checks every
  5 s while a tab is live). Text is sent only while the document is modified. The cap is 2 MiB.
- **Versions.** mdview 0.3.0 and mdedit 0.5.0, as feature releases; 0.2.3 and 0.4.1 were never
  published.
- **The AppImage keeps the default squashfs compression.** It adds about 18 ms to start-up; only
  an uncompressed image (2.5 times the size) was measurably faster (table in section 12).
- **The Windows files were first cross-built and Wine-checked** (the owner's choice, "find a way to
  cross compile windows"). The published ones are now built natively by the release workflow.

### Not verified

- By hand on Windows: the installer wizard clicked through by a person, taskbar and Explorer icons,
  SmartScreen with unsigned files, interactive use on a Windows desktop. (The test suite and the
  screenshots ran on a Windows runner; see above.)
- GNOME and other desktops than KDE Plasma 6 and sway (sway ran headless), and X11.
- Fractional display scales in a real window on Linux.
- Two monitors with different scales.
- The toast when the editor closes during a live preview ("mdedit closed: showing the saved
  file"): the return to the file was seen, the toast was not.

Verified in the driven real-window tests on KDE Plasma 6 Wayland, 2026-11-14: file dialogs (the
window stayed responsive while one was open) and the taskbar icon (both apps showed their own icon
in the panel). An AppImage updated itself end to end: the offer at launch, Update clicked, the file
replaced with a matching hash, the newer version reported at the next start, the old copy cleaned
up.

### Next release (agreed with the owner 2026-11-14)

- Search across the open folder.
- Export or print (HTML; PDF through printing). First check what exists.
- Copy as rich text. First check what a copy puts on the clipboard today.
- A copy button on code blocks.
- Carried over: the speed-versus-size build comparison on Windows, and a hand run of this
  release on a Windows desktop.
- Rounded window corners (agreed 2026-11-14): built on branch `next`, see "Next release work".

The same list is in `docs/PLAN.md`.

## 13. What to do next

The user set a list on 2026-11-12; everything on it is done: Qt retired (2026-11-13), the port
checklist gone through, automatic updates built and working (0.2.2 is the latest release), and
the seven features built, released as 0.2.0 and approved ("changes approved", 2026-11-13).
What is open:

1. **Hand checks** of what only a person can see (the list in section 10, "Not run or not
   done", and the open rows of `docs/PORT_CHECKLIST.md`). The user approved the look of the
   shell on 2026-11-11/12 and the seven features on 2026-11-13; whether they have since checked
   the 0.2.1 and 0.2.2 changes (update look at every launch, cascade submenus) by hand was not
   re-checked.
2. **Move the shared `main`.** mdview pins commit `7cd6461` of `markdown-shared-rs`, on the branch
   `feat/linux`, to become its `main`. A push to the shared `main` is refused from here; the owner
   moves it, then the `rev`s here (`bins/mdv-app`, `crates/core`, `crates/render`) can name
   commits on `main`.
3. **Linux** was built for 0.3.0 (see "Release 0.3.0"). The owner has not tested it.
4. Left open by the workers of the seven features: the main menu, with "Play animated
   images" added, just fits a window 800 px high (not re-checked). (A table beside a float,
   markup in a wiki-link label, `![[embed]]` and the covered window were closed later: see "After
   0.4.0: gaps closed".)
5. The no-pixel-change check is now `tests/shots.sh` (55 states, deterministic, pinned to
   scale 1.0 except the shots that ask for another). Before a change to the renderer, the
   shell or the shared pin run it into one folder, after it into another, and `cmp` every
   file.
6. The editor and the shared code have their own repositories (`Kaan88/markdown-editor-rs`,
   `Kaan88/markdown-shared-rs`); the editor's plan is built to its end.

Set aside by the user for now: crash reporting, accessibility, non-Latin input methods, testing on
other machines (there is one PC). The list for the next release is in "Release 0.3.0" and in
`docs/PLAN.md`. The editor is a second project (PLAN section E).

## 14. History

- **2026-11-14, command palette, settings dialog, window shadow (0.2.3 still, not published; in 0.3.0; shared
  `d6e6cf1`, branch `feat/linux`, pinned in `bins/mdv-app`, `crates/core`, `crates/render`).**
  Built by an agent without the owner; the owner has not looked at any of it. Run headless on
  Linux (CachyOS, KDE Wayland): the tests (385 pass, 4 ignored; 371 before), clippy, fmt, 67 `--shot`
  renders and one real window for a few seconds (the shadow, one screenshot). **Not run: anything
  interactive** (typing into the palette, clicking in the settings dialog, the wheel, the keys
  Ctrl+P / Ctrl+Shift+P / Ctrl+, in a real window), nothing on Windows, no installed copy (so the
  Updates actions were not pressed). Nothing counts as working on Windows until it has run there.
  - **Window shadow.** `Shadow::attach(&window)` after `create_window`, kept in `App::shadow`;
    `update(scale, active, maximized)` on `ScaleFactorChanged`, `Focused` and `Resized` (a
    maximise arrives as a resize). A no-op off Linux and without KWin's protocol. Seen: the
    pixels around the window darken by 5 to 7 grey levels over about 30 px on the dark desktop.
  - **Command palette** (`palette.rs` sources, `app/modal.rs` hosting, `ui.rs` state). Command
    `CommandPalette`, **Ctrl+P and Ctrl+Shift+P** (both were free; same behaviour, it does not
    open on ">" for the second), in View of the table and in the main menu. Sources by prefix: no
    prefix, the open tabs ("Open tabs"), the recent files ("Recent", those already open left out)
    and, once the box has text, the Markdown files of the open folder ("Files", relative paths, at
    most 5,000, those already listed left out); `>` the commands (core's table, labels as the main
    menu words them, shortcuts as hints, icons from `overlays::command_icon`, grouped by the
    table's menu, one that cannot run now greyed as the main menu does; not listed: the theme
    cycle, Open recent, Escape, the palette itself); `#` the headings (the Outline's data);
    `?` the three prefixes (choosing one puts it in the box). **Where the folder's files come
    from:** the note index (`Workspace::folder_files`), which the wiki-link feature already builds
    on its own thread within the `NOTES_*` limits; the palette reads memory only and never walks
    a folder. No index yet (folder just opened) or no folder: none are listed. Nothing is
    allocated until the palette opens (`Ui::palette` is `None`); idle CPU was 0 ticks over 5 s
    with it closed. While it is open the caret blink timer runs, as for the find box.
    Hosting: drawn last; every key, pointer and wheel event goes to it; **keys it does not use
    are dropped, except Ctrl+P, which closes it** (a file shortcut acting on the page behind an
    open list would surprise); Chosen closes it first, then runs; Submitted does nothing.
    `--palette <query>` (always takes a value; `""` for empty) for shots.
  - **Settings dialog** (`settings.rs` rows, `app/modal.rs` hosting). Command `Settings`,
    **Ctrl+,**, in File of the table and in the main menu (glyph SLIDERS). Rows as asked; every
    change goes through the setter the menu used, so it applies and saves at once. Panes and
    Focus mode follow each other (a pane on leaves focus mode). **Decisions:** the Updates
    section is always shown, because the About dialog shows its two buttons always (a copy that
    cannot update itself says so when asked); its action reads "Check now", or the About
    label ("Update to x") when a version is waiting; **Check now and Make default close the
    dialog**, as About's buttons do, so the answer (bar or toast) is not under the dimmed
    backdrop; the System section shows only where the main menu showed the default-app item (an
    AppImage that is not the default yet), so it does not show in a cargo build. **Not added:**
    a "Page font" option does not exist; adding it would need a session key, a font list (system
    fonts through `shared_fonts`) and a picker (the form has choice and text rows only, so a
    dropdown of installed families or a text box), and the page's `SANS` family string in
    `crates/render/src/theme.rs` made a parameter of the renderer, with every layout cache keyed
    on it. Not done here (no new rendering options). (Built later the same day: page and code font
    settings, section 12a.)
  - **Main menu** now: Open files, Open folder, Open recent, Reload, Close tab, Close all tabs,
    Zoom in, Zoom out, Reset zoom, Files pane, Outline pane, Focus reading mode, Command palette,
    Settings, Keyboard shortcuts, About, Exit: 606 px tall (from 713), so it no longer needs to
    scroll at 1280x800 (`menu-main-*` shots). Gone from it (in Settings now): Always load remote
    images, Tick task boxes without asking, Play animated images, the three page widths, the
    default-app item, and the top-level Manage recent files (it stays in Open recent). The bar's
    page-width button and every shortcut work as before; `Act::{RemoteAlways, PageWidth,
    EditAlways, Animations}` are gone. The core tests that count commands per menu: File 9 to 10
    (Settings…), View 7 to 8 (Command palette…).
  - **Shots:** `tests/shots.sh` now renders 68. Compared with a build of the commit before:
    nothing without a menu changed except `dialog-shortcuts` (three more rows: Settings and the
    two palette keys) and `dialog-edit` (its text says "Settings turns it off again"); the menu
    shots changed because the menu did (`menu-main-*`, `menu-cascade-*`, `hover-menu`,
    `scale-150-menu`, `menu-tall-main`, `menu-tall-main-end`). New: `dialog-settings-{light,dark,
    scale-150,760,640}` and `palette-{empty,commands,headings}-{light,dark}`. At 760 px wide the
    form still has its category list (the card is 728, the list shows from 700); it goes to one
    column below that (the 640 shot). The form's card is at most 640 px tall, so at 800 the
    Updates rows are below the fold and scroll. **Trap:** a baseline made with only `mdview`
    copied out of `target/` differs in six shots (no `mdview-engine` beside it).
  - **Size question** (nothing changed): the root `Cargo.toml` has one per-package entry,
    `[profile.release.package.mdview-engine] opt-level = "s"`, added in `df984c5` (2026-11-07,
    "engine: render SVG out of process") with the comment "the engine is a separate process that
    starts on demand; code size matters more than its speed" (PLAN says the same). It covers only
    that package's own code, not resvg, merman or RaTeX, which stay at 3. On Linux: `mdview-engine`
    24,828,192 bytes with it, 25,575,528 without (+3%); `mdview` 13,027,968 either way (it does
    not link the engine). First frame of `mdview` (5 runs, `MDVIEW_STARTUP_TRACE`, showcase):
    71.8 to 91.5 ms with, 71.8 to 79.6 ms without, typical 73 to 74 either way; the engine is
    not started for the first frame, so none is expected. Not measured: engine job time (a
    diagram's render) and anything on Windows. The decision is the owner's and needs a Windows
    measurement.
  - First frame before and after this work (showcase, 6 interleaved runs): 70.5 to 74.9 ms
    before, 71.7 to 77.4 after: unchanged.

- **2026-11-13, 0.2.3 (not published; in 0.3.0).** Menus taller than the window scroll and stay below the
  title row. Found in a real window 665 to 760 logical px tall: the main menu rose to the top edge
  over the caption buttons and its last rows ("Keyboard shortcuts", "About", "Exit") were cut off
  (the scrollbar seen there was the page's); nothing in the menu scrolled and the wheel closed it.
  Shared (branch `fix/tall-menus` of `markdown-shared-rs`, head `f49edd3`, pinned in
  `bins/mdv-app`, `crates/core`, `crates/render`; the shared `main` was not moved):
  `Menu::place_in` / `draw_in` take the area a menu may use; a menu pushed up to fit stops at the
  area's top, one taller than the area is as tall as fits and scrolls (`scroll_by`, `step` keeps the
  lit row in view and wraps as before, a thin scrollbar, hits on the placed rows), `row_rect`,
  `scroll`, `scroll_to_end`. The viewer: `app.rs` `menu_room(title, w, h)` for both the
  pre-placement and the draw (they must use the same room) and the scroll in the overlay signature;
  `app/input.rs` `wheel` scrolls the menu under the pointer when it is taller than its room (else
  closes the menus as before); `open_submenu` subtracts the parent's scroll; `--menu main-end` and
  two shots, `menu-tall-main` and `menu-tall-main-end` (1000x520), in `tests/shots.sh` (then 57, now 68;
  `MDVIEW_EXE` overrides the exe, for a build outside `target
elease`). **The installer's last page
  no longer asks about default apps** (changed in `fc6e825`, after 0.2.2). **Shots:** 55 before and
  after (both with `mdview-engine.exe` beside the exe: without it the math, diagrams and images shots differ for that reason alone): 49 byte-identical; six differ (`hover-menu`, `menu-cascade-dark`, `menu-cascade-light`,
  `menu-main-dark`, `menu-main-light`, `scale-150-menu`), and only in the menu's area: at the
  default 1280x800 the main menu (792 px with Exit) used to fill the window from y 4 to 796, over
  the caption buttons, and now starts under the title row and scrolls by about 12 px. That is the
  fix itself, not a regression; the baseline could not stay identical and keep the title row clear.
  **Ran:** build, `cargo test --release --workspace` (371 passed, as before: the new logic is tested in the shared crate), clippy `-D warnings`, fmt; a real window at the
  display's scale 1.0, 1000x560: the main menu below the title row, the wheel scrolled it to Exit,
  "About mdview" opened its dialog, Up from nothing lit Exit with the menu scrolled, Down wrapped to
  the first row, Right on "Open recent" opened its submenu inside the window. First frame (`MDVIEW_STARTUP_TRACE=1`, `--no-session`, showcase, dev-style non-static build): drawn
  and presented hidden at about 60 ms, window shown at about 70 ms (three runs). **Not run:** another
  display scale, a menu taller than the window opened by a right click, a scrollbar drag, the
  installer on another PC, an update by the app itself.

- **2026-11-13, clean-up (no release).** Headless: the shared pin moved to `24865c5` (clippy
  clean, and `menu::row_span`, which replaced the three mirrored menu constants in
  `overlays::menu_row_offset`). `cargo clippy --release --workspace --all-targets -- -D warnings`
  now passes: `chunks_exact(N)` became `as_chunks::<N>()`, two collapsible or loop-shaped
  statements were rewritten, struct literals replace field assignments in two tests, one
  `#[allow(clippy::too_many_arguments)]` on `begin_text`, and `can_go_back` / `can_go_forward`
  (Back and Forward exist; only the tests ask for the queries, added with them in `ac5563c`)
  carry `#[allow(dead_code)]`. The installer lost its "Choose mdview as the default app" entry.
  `tests/shots.sh` was added. Checked: 55 shots rendered before and after, byte-identical;
  371 tests pass with `--release`; fmt clean. Not run: the installer script was not compiled (no
  way found to check it without writing an output file), no window was opened.

- **2026-11-13, 0.2.2.** Two changes. The look for a newer version now runs at every launch (the
  shared `check_due` no longer compares with a day; a look earlier the same day used to suppress
  it, which is why the owner's manual check found 0.2.1 and the launch did not). Submenus open when
  the pointer rests on their item for 300 ms, as a cascade: the parent menu stays and the submenu
  stands beside it (`Popup::sub`, `Popup::dwell`, `App::open_submenu`). A click opens it at once,
  another parent item closes it, Escape or Left closes only it, Right or Enter on the item opens it,
  a click outside closes both; `--menu cascade` shows it in a shot. "Open recent" is the only item
  with a submenu. Ran: build, the whole test suite, fmt, clippy (one warning that was already
  there), shots of the menus in both themes, a real launch with the settings file's
  `update_checked` set a minute back (it moved to now), and the hover in a real window (cursor
  moved with SetCursorPos: nothing at 100 ms, the submenu at 700 ms, gone on another item).
  Published as v0.2.2 from `ccf48f8`: https://github.com/Kaan88/markdown-viewer-rs/releases/tag/v0.2.2. Copies of 0.2.1 and older still look once a day. Not run: the installer, an update from 0.2.1
  by the app itself, the hover on a second monitor or at another scale.
- **2026-11-13.** Qt shell retired to the `qt-archive` branch with its installer and Linux
  packaging; `mdview` is the only shell. Numbers re-measured after the shared crates. Port
  checklist gone through; the Files tree follows its folder live. In-place updates built
  (`docs/UPDATE.md`); v0.1.0 published on GitHub.
- **2026-11-08.** Phases 1 and 2 closed: the user approved the four `showcase-*` goldens. Phase 4
  slices landed: SVG, math and Mermaid (eight families) through `mdview-engine`, syntect in-process.
  Both shells (WinUI and Qt) built ahead of the plan; the Qt shell ran on the Debian VM under Xvfb.
  WinUI used a pair of D3D11 textures with band uploads and its own scrollbar, because the trimmed
  bindings had no `ScrollBar`, text box or `ItemInvoked`.
- **2026-11-10, Linux desktop.** Linux budget numbers for Qt (offscreen): warm first paint 119 ms
  for 20 KB, 37.0 MB private; the 1 MB first height pass was 615 ms cold on the UI thread (later
  solved by estimated heights). Font enumeration shared per thread (`shared_fonts`), which saved
  about 35 ms per extra renderer. PGO tried and not adopted (no measurable difference). OS file
  watching (`crates/core/src/watch.rs`, notify 8.2.1, parent folders, 2 s poll fallback) replaced
  the 800 ms poll. Qt page context menu.
- **2026-11-10, packaging.** PKGBUILD (`packaging/arch`, `makepkg -f`, about 3.5 min). AppImage
  built in `ubuntu:24.04` with `build-in-container.sh` (glibc 2.39; needs `lld` in the container,
  because `gold` drops cxx-qt's init symbols), 36.7 MB, kept in `~/mdview-appimage/`; Qt AppImage
  self-update wired (the user kept it, PLAN does not list it). `crates/core/src/update.rs` and
  `docs/UPDATE.md`: manifest and SHA-256 logic, no network code. Inno Setup 7.1.1 compiled the
  first installer in the WinApps VM.
- **2026-11-10, WinUI in the WinApps VM.** Files pane (`TreeView` by slot index), menus driven by UI
  Automation and real clicks; a MenuBar bug (later items stop responding after one is used) was
  bisected but not fixed and may have been a VM artifact. `docs/WINDOWS_HANDOFF.md` written.
  Clean-up of VM and temp leftovers at the user's request.
- **2026-11-11, Windows PC, layout A** (`cb82a36`, `16c5cd9`). No menu bar; tabs in the title row;
  one command bar (Open, Reload, Recent | Files, Outline | zoom; find box with count,
  previous/next, Theme, main menu); Files pane left, Outline right. Both shells brought to parity
  side by side. Two Qt bugs fixed: both right-click menus used `ActionsContextMenu` and never opened,
  and every mouse button acted as the left one.
- **2026-11-11, WinUI final work** (`46b2e97`). Single instance before the Windows App SDK bootstrap
  (17-24 ms hand-over), OS file watching through `DispatcherQueue`, Manage recent dialog, 0.1.0
  installer that registers the extensions and a registered application. Measured 383 ms first frame
  and 180-210 MiB private memory; `ExtendsContentIntoTitleBar` alone cost 70-95 ms. Retired the
  same day (`b8e57ba`); the code is on `winui-archive`.
- **2026-11-11, Qt as the only shell.** Panes and window size saved in the session
  (`layout=files=1;outline=0`, `geometry=WxH [max]`); Qt Files tree loads folders on expand; no
  console window; Qt installer (`mdview-qt.iss`, windeployqt output plus the MSVC runtime
  app-local, ProgId `mdview-qt.Markdown`) on the v0.1.0 pre-release.
- **2026-11-11, shell mockups** (`ca331de`). `spikes/shell-fltk`, `shell-slint`, `shell-soft`, each
  its own workspace; numbers in PLAN. winit + softbuffer chosen.
- **2026-11-11, the new shell** (`e84e117`). `bins/mdv-app` with every control hand-drawn; the user
  approved the look. Same day: full GitHub emoji list (`85ba744`), `layout.rs` split into modules
  (`f0d3adc`), clipped text draw (`f34848d`), GIF and WebP (`feca9fc`), cheaper glyph cache
  bookkeeping (`dfbbd68`), subpixel order setting and ClearType-following chrome (`a9c6f1b`),
  installer with static CRT (`7945d88`, moved into `target\dist` by `b0dcd29`, `de981b9`),
  area-averaged downscaling (`56c61a3`), `mdview-fetch` (`7a6ef4a`), 23 more Mermaid families
  (`b140780`, `8e005c2`), the image store (`8d2437f`), startup trace counts (`dd11284`), the
  patchable layout (`cff387c`), estimated first layout and refinement (`ffd38e0`, `b115835`), the
  editor noted as a second project (`d21dbc7`).
- **2026-11-12.** Whole-frame present on a system repaint (`846a9c2`); remote images off until
  asked, with the bar and the "always" setting (`099ded4`, `a483ece`); softer popup shadows and
  wider resize bands with corner zones (`c5377a2`); ruby, `<dl>` and `<figure>` styling (`33bf887`);
  copy formula and diagram source (`103654b`); page text follows the display's subpixel order
  (`429adc9`); the six `everything.md` fixes (`ae4e7bf`); `<mark>`, `dir`, table `align`/`width`/
  `height`/`valign`/`<caption>`, image `align`, TypeScript and TOML highlighting, a reason on a
  block that falls back, and find agreeing with the page for unsupported diagrams and unlisted
  footnotes (2026-11-12).

**Traps worth remembering from that history:**

- A GUI app started with `&` in a PowerShell command that then finishes is killed with the job;
  use `Start-Job { & $exe @args }` and stop it afterwards. `PrintWindow` returns black for a
  minimized window. A window can be behind another app even after `SetForegroundWindow`; make it
  topmost and check with `WindowFromPoint` before a synthetic click.
- Synthetic drag and drop and `SetCursorPos` were refused by Windows in one session on the PC, so
  drag and drop has never been run by hand in any Windows shell.
- `rc.exe` (for the exe icon) is at
  `C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64`; from Git Bash the build script
  could not run it once.

## Next release work (branch next)

### Rounded window corners (base `774cea3`)

Built in the shared crates (`mdshared_ui::corners`, `shadow`; branch `next` of the shared
repository, docs/LINUX.md "Rounded window corners") and wired here the same way as in mdedit.

- Built: the "Window corners" setting (Square, Small = 4, Round = 8 logical px; default Round),
  in the Page group of Settings under Theme, kept as `corners=` in the session file (the radius;
  written only when not 8; `Session.corners`, `Workspace::corners` / `set_corners`). Windows:
  the DWM preference, applied at window creation, on a theme change and on a change of the
  setting. Wayland client margin: the anti-aliased corner cut in `present` (only the corner
  squares a copied rectangle touches, all four after a whole copy or a repainted margin) with the
  shadow under it. `--frame square|small|round` for `--shot`, six `frame-*` shots (scale 1 and 2,
  both themes). Tests: the session value, the settings row and its change, `--frame`, the frame
  picture; the corner mask and painter are tested in the shared crate.
- Decisions: radii 0, 4 and 8 only (exact on Windows too: do not round, small, round). On KDE the
  default (Round) uses the client margin instead of KWin's protocol, because KWin's route is
  opaque; Square keeps KWin's route, and a change from Square to Round applies at the next start
  (the help line says so). The row is hidden where nothing can round (X11 without the margin). The
  Windows 11 preference was set to Round unconditionally in the shared `style_window`; that call is
  now the setting's.
- NOT verified (no local builds by the owner's rule; CI only): any real window on any desktop.
  Not seen: the corner in a real Wayland window, KDE with the route change, the shadow under the
  corner, a tiled or maximized window, switching the setting at run time, Windows 11 DWM rounding,
  Windows 10 staying square, fractional scales other than in the shared unit tests. The startup
  and scroll budgets were not measured: the change adds a `Shadow::rounds()` check to `present` and
  nothing else on the scroll path; with Square or without a margin nothing runs. The KDE route
  change moves the default KDE window from KWin's shadow to the client-drawn one (a larger buffer
  by the 36 px margin on each side), which was not timed.

### Merge branch `merge/next-release` (2026-11-15, the Windows PC)

The features below were merged one after another into `merge/next-release` (viewer) on top of
main 0.3.1 with settings-one-page, rounded corners, history and watch-folder; the shared crates
they need are on the shared branch `merge/viewer-next`. Each feature is one merge commit followed
by its finishing commits, so `git diff <commit before the merge>..<last commit of the feature>`
shows one feature. Everything here was built and tested on Windows in debug builds (fmt, clippy
`-D warnings`, the workspace tests, `tests/shots.sh` with `MDVIEW_EXE` set to the debug binary,
every new picture looked at). Nothing was driven in a real window: another worker was using the
desktop, so no feature below has been seen live, and no budget was measured.

### Back and forward history (`f/history`)

- Already there since 0.2.x: one history for the window, up to 100 places (file and scroll
  position), Alt+Left / Alt+Right and mouse buttons 4/5. It is window-wide and not per tab on
  purpose: a link to another file opens it in a new tab (or switches to it), so a per-tab history
  would never hold the jump. Back switches to that file's tab, opening it again if it was closed,
  and restores the scroll. Recorded by links (same-file anchor, other Markdown file, wiki links),
  footnote jumps, outline and palette `#` headings, backlinks. A jump that did not move the reader
  is not recorded; a new jump drops what Forward held; vanished files are skipped; not persisted.
- Built: commands `Back` and `Forward` (menu None, so the Keyboard shortcuts dialog lists them), in
  the command palette (greyed when there is nowhere to go), icons BACK / FORWARD, shot
  `palette-history-light`.
- Decisions: no toolbar buttons (no natural place) and no main-menu rows (the menu would no longer
  fit the default 800 px window without scrolling).
- Left out: history of find-in-document matches, persistence.

### Watch the open folder (`f/watch-folder`)

- Already there: OS notifications (`mdshared_app::watch::FileWatcher`, the `notify` crate) for the
  parent folders of open files and notes and for the Files pane's root and expanded folders while
  the pane is shown. Built: a wake no longer checks at once, it sets `watch_due` 150 ms ahead
  (`WATCH_DEBOUNCE`) and the timer runs one check (`watch_check`: tabs reload, note index,
  `tree.refresh()`), so a git checkout causes one check. Nothing polls, no thread was added.
- NOT verified: a real window, real watcher timing on Windows or Linux. Left out: a poll fallback
  when the OS watcher cannot start.

### The outline follows the page (`f/toc-follow`)

- Already there: the highlight of the current heading (`Workspace::current_heading`, a binary
  search over the known heading positions) and `reveal_outline_row`. Built: `panes::follow_outline_row`
  and `last_heading_tab` in `App`. The outline scrolls by the least amount that shows the current
  row; it does not move while the pointer is on one of its rows, a backlink or its scrollbar, or
  while that scrollbar is dragged, and catches up on the first frame after the pointer leaves; the
  same when it was not drawn yet, after a tab switch (the heading index is tracked per tab), and
  when it was hidden while the page scrolled (fixed after the merge: the branch marked a hidden
  outline as done, so showing it again did not bring the row into view). Above the first heading
  nothing is current and the outline returns to its top.
- Decisions: "the pointer is on the outline" means on a row, a backlink or the scrollbar, not the
  pane's empty space or its header.
- No new shot: the visible states are the existing outline shots. Cost: one comparison per frame.
- NOT verified: a real window.

### Copy as rich text (`f/copy-rich-text`; shared `f/copy-rich-text`)

- Before: a copy put only the selected slice of `Document::text` on the clipboard. That text is
  unchanged. Now `Workspace::selected_html` builds an HTML fragment for the same range with
  `mdview_core::rich::selection_html`, from the parsed model (never from the source), and
  `platform::set_clipboard_html(html, text)` (mdshared-ui) writes both; if that fails the copy
  falls back to the text alone. Windows: `CF_UNICODETEXT` plus the registered "HTML Format"
  (`mdshared_ui::cf_html`: UTF-8, ten-digit byte offsets, StartFragment/EndFragment comments).
  Linux: `text/html` with `text/plain` as its alternative through arboard.
- The HTML is clipped to the selection: a partly selected paragraph gives that part, a partly
  selected span is cut at the range, a container (list, quote, table, row) is written only when
  something in it is selected; in a table row with any selected cell all cells are written (the
  others empty) so columns keep their place. Mapping: `p`, `h1..h6`, `blockquote` (an alert with
  its label in bold), `ul`/`ol` (`start` kept), task boxes as ballot-box characters, code, diagram
  and formula blocks as `pre code` with their source, `hr`, tables with borders, `colspan`,
  `rowspan` and `text-align`, details as a div with the summary in bold, footnote marks as `sup`,
  definition lists, figures; inline `em`, `strong`, `del`, `u`, `code`, `kbd`, `sub`, `sup`,
  `mark`, `small`, `q`, `ruby`/`rt`, `br`. No external resources: an image is its alt text. A link
  is kept only when it is http, https or mailto. Everything is escaped; raw HTML of the source
  never reaches the output.
- The shared CI failure of `f/copy-rich-text`: `mdshared-ui --lib` failed on Windows because two
  tests (`clipboard_round_trip`, `html_copy_leaves_the_text_readable`) wrote and read the one
  system clipboard at the same time. Fixed on shared `merge/viewer-next`: both take one lock, and
  the HTML test now also reads the "HTML Format" back and checks the fragment by the header's
  offsets (so the CF_HTML write has really run on Windows, on this PC).
- Decisions: no new menu entry or setting (the existing Copy does it); the body of a closed
  `<details>` is included when the selection spans it, as in the plain text. "Copy as Markdown"
  is left out: the model keeps no source positions and the parsed text is not the file's bytes
  (front matter and math are rewritten first), so a selection cannot be mapped back reliably.
- No new shot (no new visible state; the status line is the existing "Copied").
- NOT verified: a real paste into Word, LibreOffice, a mail program or a browser on any system;
  `text/html` on X11 and Wayland (never run). The app-level `copy()` is not covered by a test,
  because it would write the owner's clipboard; `selected_html` and the writer are.

### Copy button on code blocks (`f/code-copy-button`)

- Built: the pager records each code box (`mdview_render::Code`: rect, text range, label width)
  beside the lines, and a hit test reports it in `Hit::code`. `Workspace::code_spot`, `code_text`,
  `code_in_reach`. `bins/mdv-app/src/codebtn.rs` holds the geometry, the drawing and the spot under
  the pointer; `Hit::CopyCode`; `Ui::copied` (which block shows the check, until when); the page
  region's signature includes the spot and the check. Command `CopyCodeBlock` ("Copy code block",
  `Menu::None`, no key): the block the selection starts in, else the first code block in view;
  status "Code copied" or "No code block in view".
- Look: 24 x 24 logical px, 6 px in from the block's top and right edges, the code background with
  a hairline edge, the shared COPY glyph (muted; text colour when hovered), CHECK in the accent
  colour for 1.5 s after a copy (`codebtn::COPIED_FOR`), tooltip "Copy code". It is drawn over the
  page, never part of the layout. It shows for the box under the pointer (also beside the text in
  the box) and while the pointer is on the button; not while a menu, dialog, the palette or a drag
  is open or going, and not for boxes shorter than 36 px or narrower than 72 px. The block's
  language label is in the same corner and gives way while the button shows. When the top of a tall
  block has scrolled out, the button stays at the top of the viewport, inside the block.
- The copied text is the block's source from the document: exact, tabs kept, the newline that ends
  the last line dropped.
- Added after the merge (the branch was an unbuilt WIP; it compiled and its tests passed as it
  was): `codebtn::to_clipboard`, which the tests replace with a thread-local, so the click path is
  now tested without writing the real clipboard (the exact text, no selection started by the
  press, the check mark and the timer that removes it), and the command's two statuses.
- Decisions: no setting; no button on Mermaid or math blocks (their pictures have "Copy source" in
  the context menu); `--hover` over the page in a shot now composes once and moves again so a
  control that shows only under the pointer is hovered (existing shots are byte-identical).
- Shots: `code-copy-light`, `code-copy-tip-light`, `code-copy-done-dark`, `code-copy-dark`.
- Cost: nothing unless the pointer is over the page; with it there, two hit tests per page frame
  (signature and draw), also while the wheel scrolls under a resting pointer. Not measured.
- NOT verified: a real window (the real clipboard write of the click, the 1.5 s timer on screen).

### Footnote and link previews on hover (`f/hover-previews`)

- Trigger: the pointer rests (the tooltips' 500 ms; moving on the mark before the card shows
  starts the wait again) on a footnote reference, on a link to `#heading` of the open page (a
  heading only), or on a link to a local Markdown file (with or without `#heading`). Never for
  http(s)/mailto, wiki-links, missing targets or other file types.
- Card: a title in semibold ("Footnote N", the heading, or the file's first heading) and up to 6
  wrapped lines, at most 380 px wide, placed 12/20 px right and below the pointer, above it when
  there is no room, kept inside the window; the tooltips' frame, shadow and palette. The body is
  at most 360 characters (`mdview_core::preview`), whitespace made single spaces; rules, formulas,
  diagrams, code blocks and tables are skipped; a heading ends it. A card goes away when the
  pointer leaves the mark or link, on any key, press or wheel turn, on history travel, and when
  the setting is turned off.
- Files: read when the card is due, on one short-lived `preview` thread per file, at most 128 KiB,
  parsed with the page's parser; the answer wakes the loop (`Wake::Preview`). 12 answers are kept
  and used only while the file's modification time is unchanged. Nothing starts at launch.
- Setting `previews` in the session file (default on); the row "Preview footnotes and links on
  hover" under Content in Settings (`settings::PREVIEWS`, id 30).
- Code: `crates/core/src/preview.rs` (card text), `bins/mdv-app/src/preview.rs` (target, cache,
  thread, layout, drawing), `bins/mdv-app/src/app/cards.rs` (wanting, timing, resolving),
  `Hover::preview` in the workspace, fixtures `previews.md` and `previews-target.md`.
- Changed after the merge (the branch was an unbuilt WIP; it compiled and its tests passed):
  - A linked file with front matter showed the front-matter table as the card's text
    ("Property Value title ..."). `preview::start` now takes the first heading as the title and
    what follows it when nothing readable comes before it; tables and code blocks are no card text.
  - The 360-character cut and the card's last line now end after a whole word.
  - The settings row lost its help line and got "on hover" in its label: with the help line the
    one-page form was 2 px taller than its card and showed a scrollbar in the default window.
  - The app tests found their marks and links at fixed pixel positions of the Windows fonts; they
    now scan the page for them, so they do not depend on the system's fonts.
- Decisions: the card is drawn with the UI font, not the page's text styles (the Painter has only
  the UI faces and the page renderer cannot draw into an overlay without a layout pass); plain
  text only. It is drawn in mdview, not in the shared crate, because it is multi-line and the
  editor has no use for it yet.
- Shots: `preview-footnote-light`, `preview-footnote-dark`, `preview-footnote-long`,
  `preview-heading-light`, `preview-heading-dark`, `preview-file-light`, `preview-file-dark`,
  `preview-remote`. Existing shots that changed: the five `dialog-settings-*` (the new row) and
  `files-pane`, `both-panes` (the two new fixtures are listed in the pane).
- One run of `tests/shots.sh` stopped after `preview-footnote-light` with no message: another
  worker's cleanup on the same PC had killed that mdview process. Not a fault of the app (6 more
  full runs and 125 single preview shots were clean and identical).
- NOT verified: a real window (the 500 ms rest, the thread's wake, the card following real pointer
  moves), Linux. Cost: `Hover::preview` is one link lookup when the pointer is on a link or mark.

### Find in folder (`f/folder-search`; shared `f/folder-search`)

- The card is the command palette's card as a literal list (`mdshared_ui::palette`:
  `set_literal`, heading and note rows, `Entry::marks`, `append_entries`, `set_footer`,
  `label_of`): the query box, rows grouped under the file's path (a heading row with its match
  count), each row one matching line with the match in the accent colour and `Line N` at the
  right, a footer ("Searching… 12 files", then "31 matches in 6 files", ", N files skipped",
  "Searched N files" when nothing matched). Up and Down skip the headings; Enter or a click opens
  the row; Escape or a click outside closes.
- The query is what the find in the page takes (case-insensitive, the same folding, no options),
  searched in the Markdown source line by line (`mdview_core::find::line_matcher`). The box starts
  with the find box's text. Typing starts a new search at once and cancels the old one.
- What is searched (`mdshared_app::foldersearch`, written for both apps): the files the Files pane
  lists, with its rules (dot names, the hidden attribute on Windows, `target`, `node_modules`,
  links to folders not followed, depth 32), folders first, sorted by name. A file over 16 MiB,
  unreadable or holding a NUL is skipped and counted. Caps: 200 lines per file, 2000 in all, then
  the last row says "More results are not shown. Narrow the search."
- Choosing a row opens the file (or switches to its tab), puts the query in the find box and makes
  current the first match in the block that source line is in (`Workspace::find_set_at_line`);
  when that block has no match on the page (the query was in markup) the page scrolls to the line.
  It is a jump: Back returns. Without a folder the command says "Open a folder to search it".
- The thread exists only during a search; nothing is built until the command is used.
- State at the merge: an unbuilt WIP that compiled and passed its tests as it was; nothing had to
  be changed beyond the merge conflicts (icon table, module list, tests).
- Decisions: **no default key.** Ctrl+Shift+F was reserved for this feature but is Focus reading
  mode's since 0.2 and a regression test pins it; moving Focus mode would change existing
  behaviour, so the owner decides. The command is `Menu::None` in the table, in the "⋯" menu
  (greyed without a folder) and in the palette. No search options (the viewer's find has none).
  The palette card is reused instead of a new panel.
- Shots: `folder-search-light`, `folder-search-dark`, `folder-search-none-light`. Existing shots
  that changed: the eight with the main menu open (`menu-main-*`, `menu-cascade-*`,
  `menu-tall-main*`, `hover-menu`, `scale-150-menu`): one more row.
- NOT verified: a real window (streaming on screen, typing while a search runs), a huge folder,
  Linux. Left out: results updating while files change; the exact match inside a long paragraph
  (the first of the paragraph is taken); the editor adopting `foldersearch`.

### Export as HTML and Print (`f/export-print`; shared `f/export-print`)

- Commands `ExportHtml` ("Export as HTML…") and `Print` ("Print (opens in browser)"), enabled with
  a document open; icons DOWNLOAD and OPEN_WITH.
- Export: the Save dialog (`pick_save_async`, helper thread) suggests `<name>.html` beside the
  document; `.html` is added when missing. The page is made on that thread; the status says
  "Exporting…", then "Exported to <file>" or "Could not save the HTML file". One at a time.
- Print: writes `<temp>/mdview-print/<name>-<ms>.html` (files older than a day are removed at the
  next print) and opens it with `platform::open_file` (shared; html/htm only, ShellExecute or
  xdg-open). The page opens every `<details>` and calls `window.print()` after load.
- The page (`crates/render/src/export.rs`, `export_html`, from the document model): always the
  light theme with the reader's page font, code font and page width (Narrow 700, Wide 1000, Full
  no limit). Local images are embedded as data URIs (png, jpeg, gif, webp, svg) up to 4 MiB each
  and 24 MiB in all, larger ones link by `file://`; remote images are linked only when the reader
  loaded them, else their alt text; nothing is downloaded. Formulas and diagrams are drawn by the
  engine at 2x and embedded as PNG with the source as alt text; without the engine the source
  stays in a labelled code box. Links keep only http, https, mailto and relative addresses;
  wiki-links are their words. Headings and anchors keep ids; footnotes are listed at the end. A
  print style sheet: white page, no break inside code, tables or images, addresses after links.
- Changed after the merge (the branch was an unbuilt WIP that compiled):
  - Two regression tests failed: the commands were in `Menu::File`, whose items are pinned. They
    are `Menu::None` now, at the end of the table.
  - With two more rows the "⋯" menu no longer fitted the default 800 px window (it covered the
    toolbar and scrolled). **Export and Print are therefore in the tab's context menu** (after
    "Show in file manager"; they act on that tab, which comes forward) **and the command palette,
    not in the main menu.**
  - A link around a picture alone (a badge: `<a><img></a>`, `[![alt](p.png)](url)`) was dropped
    from the page; it is written now. A `<summary>` stood on the line under its marker; fixed in
    the style sheet. Both seen by rendering the exported fixtures in headless Edge.
  - The escaping is shared with the copy-as-rich-text writer (`mdview_core::rich::escape_html`).
    The two writers themselves stay apart: one writes a fragment clipped to a selection with no
    resources for the clipboard, the other a whole page with ids, pictures, code colours and a
    style sheet. Unifying them is possible later and was judged not worth the risk now.
- Decisions: light theme only; no setting; no key (Ctrl+P is the palette); print through the
  browser, no native printing or PDF.
- Shots: `palette-export-light`, `palette-print-dark`, `menu-tab-light`; `menu-tab` changed (two
  more rows). No shot shows the status messages.
- Seen: the exported `showcase`, `math` and `tables` fixtures rendered in headless Edge on this PC
  (formulas as pictures, code colours, tables, the float, the badge link, details).
- NOT verified: the Save dialog and its thread, ShellExecute opening a real browser, a real print
  dialog, Linux. Left out: table widths and centring from HTML attributes, dark export, PDF.

### Presentation mode (`f/presentation`)

- Command `Present` ("Present", icon PLAY), enabled with a document open: in the "⋯" menu, the
  command palette, and on **Shift+F5**. It toggles: the same key or command leaves, as Escape does.
- **Key decision: Reload keeps F5.** The spec says "F5 unless taken"; F5 is Reload since the first
  release and a regression test pins it, so Present is Shift+F5 (PowerPoint's "from this slide").
  A test pins both.
- Slides (`crates/core/src/slides.rs`, `split`): cut at top-level horizontal rules when the
  document has any, otherwise before every top-level H1 and H2 (a rule or heading inside a quote
  or list does not cut). Front matter is skipped; the link reference definitions of the whole
  document are appended to every slide so `[text][ref]` resolves.
- The show (`bins/mdv-app/src/present.rs`, `Show`; `app/slides.rs`): its own `DocumentRenderer`,
  made when Present starts and dropped when it ends, so nothing exists before. One slide is
  parsed and laid out at a time: first at zoom 1 in an 860 px column to learn its height, then at
  `fit_zoom` (as wide as the window allows, up to 3; less when the slide would not fit in 90 % of
  the height, but not under 0.6 of that and never under 1), centred vertically; a slide still
  taller than the window starts at its top and scrolls. The page's theme and fonts; formulas,
  diagrams and local pictures draw as on the page.
- Keys: Right, Enter, Page Down, Space next; Left, Backspace, Page Up, Shift+Space previous (Space
  and the Page keys scroll a tall slide by a screen first); Home, End; Up, Down and the wheel
  scroll a tall slide; a left click goes on. Every other key, the right button, menus, the
  palette, find and history travel are ignored while presenting; Exit still works.
- The counter ("2 / 4", bottom right) shows for 1.8 s after a slide change or a pointer movement
  and fades over 0.4 s; the pointer is hidden when the counter has gone and returns when it
  moves. The timer runs only while a presentation shows.
- The window goes borderless fullscreen (winit) and returns to what it was; a window that
  already was fullscreen stays so. The tab's scroll position is never touched, so Escape returns
  to the same place.
- The tab keeps its source text only while it is presented (`Workspace::present_source`,
  `present_update`, `present_end`); a reload or live text from mdedit cuts the slides again and
  keeps the slide number (the last slide when there are fewer; the show ends when none is left
  or the tab closes).
- `--present N` starts at slide N (for shots and checks).
- Added after the merge: the branch had unit tests for the cutting and the show but no app-level
  tests and no docs. Four app tests now cover starting, every navigation key, the click, Escape
  returning to the same scroll position, input being swallowed, the toggle, the key table, "Nothing
  to present", and live text keeping the slide number. No behaviour of the branch was changed.
- Shots: `present-title-light`, `present-bullets-light`, `present-bullets-dark`,
  `present-table-dark`, `present-small-light` (700x500), `present-large-dark` (1920x1080),
  `present-scale-150`. Also rendered once and looked at, not kept: slides with a formula, a
  Mermaid diagram and a picture (all drawn). Existing shots that changed: the main-menu shots
  (one more row; the menu still fits the 800 px window), `dialog-shortcuts` (the Present row),
  and the shots that list or search the fixtures folder (`fixtures/slides.md` is new).
- NOT verified (needs a real window): the switch to fullscreen and back, on one and on two
  monitors; the pointer hiding and returning; the counter's fade as an animation; the wheel on a
  tall slide; remote pictures on a slide; Linux.
- Left out: speaker notes, transitions, a slide overview, clicking links on a slide (a click
  always goes on), text selection on a slide.

### A document mdedit has not saved (`f/live-preview-unsaved`; shared `f/live-preview-unsaved`)

Merged into `merge/next-release` on 2026-11-15 from the branch `f/frosted-window`, which held all four.

**DESIGN.**

**A document that has no file.** mdedit can hand over a document it has not saved (`?doc`,
`?title`, `?open` and the text; the shared `docs/SISTER.md` has the wire format).

- It is shown in a tab named as the editor's tab ("Untitled 1"), with the dot of a live tab. The
  tab's tooltip and the palette say "not saved yet: shown as mdedit has it" in place of a path.
- Inside, the tab is keyed by a path that cannot exist on disk (under a folder whose name starts
  with a NUL character, then the document's name and title). Nothing is read from or written to
  that path, and it is never watched.
- Relative images and links have nothing to resolve against: an image is drawn as a missing
  image, and following a relative link says "not a file: <destination>". Absolute paths, web
  links and anchors inside the document work as usual.
- Later texts for the same name replace the text in place and keep the place read; the line the
  editor sends is followed as for a file. A text without `?open` for a name that has no tab does
  nothing.
- When the editor says the document was saved (`?doc` with a path), the tab becomes that file's
  tab: same position in the row, the file is read from disk, the live state carries on if the
  editor keeps sending. If the file is already open in another tab, the untitled tab is closed
  and that tab is shown: never two tabs for one file.
- When the preview ends (`live_off`), or the editor is found gone, the tab stays and keeps the
  last text, without the dot; if it is the tab in front when the editor goes, the toast is
  "mdedit closed: this document was not saved". There is no file to fall back to.
- Not for such a tab: Reload (says "This document has no file yet: it is shown as mdedit has
  it."), Copy file path and Show in file manager (disabled in the tab menu), the session and the
  recent list (the tab is left out, so it is not restored), the file watcher, a new window's
  start folder.
- Edit in mdedit asks the running editor for the document by its name, at the line being read;
  with no editor running it says "mdedit is not running" (nothing can be started for a document
  that has no file).

**Handoff.**

Built:

- `bins/mdv-app/src/workspace.rs`: `Tab.untitled`, `untitled_path`, `is_untitled_path`,
  `Workspace::document_text`, `document_off`, `document_saved`; the exclusions listed above.
- `bins/mdv-app/src/app/commands.rs`: `handed_over` routes requests that carry a document; the
  tab commands for such a tab.
- `crates/core/src/commands.rs`: the strings `LIVE_EDITOR_GONE_UNSAVED`, `UNTITLED`,
  `UNTITLED_HINT`, `UNTITLED_NO_FILE`, `EDITOR_NOT_RUNNING`.
- Argument for shots: `--demo-untitled <file>` shows the text of a file as such a tab.
- Shots: `untitled-live`, `untitled-live-dark`, `untitled-tab-menu`, `untitled-tip`.

Decisions taken, for the owner to confirm:

- The tab stays with its last text when the preview ends or the editor is gone, instead of
  closing.
- The tab is not kept in the session or the recent list.
- Relative links say "not a file: ..." instead of trying a folder (the editor's working folder
  would be a guess).
- `--demo-untitled` was added as a launch argument.

Tried in real windows on Windows 11 together with the editor: see the editor's file of the same
name. Also seen there: with the editor gone the tab kept its text and the toast appeared.

Budgets, measured on this PC on release builds of the base (`e909adc`) and of this branch, ten
launches each in turn (`MDVIEW_TRACE=1`, `--scroll-steps`): first frame of `fixtures/showcase.md`
75 to 82 ms before and 76 to 82 ms after; scroll frame median 0.56 to 0.57 ms before and 0.57
to 0.61 ms after. Nothing runs for the feature until the editor hands such a document over.
Not measured: memory.

Not verified: Linux in real windows; "Edit in mdedit" from such a tab in real windows (app-level
tests only); an older mdedit needs nothing new (it never sends a document).

### Finer scroll sync (`f/finer-sync`; shared `f/finer-sync`)

Merged into `merge/next-release` on 2026-11-15 from the branch `f/frosted-window`, which held all four.

**DESIGN.**

**A line of the file is a place inside its block.** Until now a source line was shown by the
block it is in: the block's first row went below the top edge, so a caret walking through a
300-line code block or a long paragraph did not move the page. Now:

- A line of a code block (fenced or indented) is shown exactly: that row of the code.
- A line of a paragraph or list item written over several lines is shown where its text begins
  on the page. The page joins and wraps those lines anew, so that is somewhere in a row.
- A table is shown by its row, a list by its item, as before.
- With a column (the byte in the line, from 1, which mdedit sends since this release) the place
  is that far along the line's text: a paragraph written as one long line, which wraps to many
  rows on the page, is shown at the row the caret is in. Markup that shows nothing (`**`, a
  link's address) is counted with the text before the next mark, so inside such a line the place
  can be some words off (not measured how far at worst; marks are 64 bytes apart).
- A place before the line's first text (an indent, a list marker, a fence) is that first text; a
  line with no text of its own (a blank line inside a block quote, an image alone on its line)
  is the nearest line before it; a block with no text at all (a rule) is the block.

**How.** While parsing, the builder notes where lines of the file are in the document's text
(`Document::marks`, `LineMark { line, col, text }`): one mark for each line a run of text or code
begins on, and one more every 64 bytes along a line. `Document::text_at_line(line, col)` finds the
place by a binary search in the marks of the block `block_at_line` gives; the workspace hands it
to the renderer as `Jump::Text(offset)`, which already existed (find-in-page uses it) and reads
the row from the lines the layout has. No layout pass is added: the jump measures the same one
block a jump to its start measured.

**The other way.** `Document::line_at_text(offset)` gives the line and byte of a place in the
text. "Edit in mdedit" sends the line of the file that is in the first row of the page, and the
column when that row begins past the line's first text (a row inside a line that wraps), so the
editor's caret lands in the row that was at the top, not at the paragraph's start.

**A follow moves by a step.** As before, a live update that would move the page by less than one
arrow-key step (48 logical px) leaves it where it is, in both directions. With places inside
blocks that is what keeps the page still while the caret moves within the top rows: walking down
a code block the page moves every second or third line, by that many rows. Typing at the end of
a document adds rows below; the page moves down when a step's worth has been added and never
back (tested: 400 letters typed at the end, with line breaks, never move the page up).

**Older builds.** No new key and no new answer. `?col=` was always carried and skipped by a
viewer that had no use for it, so an mdedit that sends it to an older mdview gets the old
behaviour (the block's start), and an older mdedit that sends no column to this mdview gets the
line's beginning.

**Handoff.**

Built:

- `crates/core/src/model.rs`: `Document::marks`, `LineMark`, `text_at_line`, `line_at_text`.
- `crates/core/src/parse.rs`: `Builder::mark`, `Builder::code_text`, `MARK_GAP`.
- `bins/mdv-app/src/workspace.rs`: `SourcePlace` (line and column), `scroll_to_line` /
  `follow_line` / `follow` / `live_text` / `document_text` take it; `top_source_place`,
  `source_place`.
- `bins/mdv-app/src/app/commands.rs`: requests give their column, "Edit in mdedit" sends one.
- Tests: five in `crates/core/tests/core_tests.rs` (the mapping: paragraph, list, code, table,
  one long line, every fixture), four in `workspace.rs` (a line inside a long block, a place
  along a wrapped line, a caret walking down and up, typing at the end), one in `app/tests.rs`
  (a hand-over with a column). `parse_cost` (ignored) prints what a parse costs.

Started by an earlier session (the marks, the two `Document` functions, the plumbing); this
session renamed the type (it clashed with the history's `Place`), rewrote `text_at_line` as a
binary search, and took out one thing the earlier session had begun: a smaller step upwards
(24 px) than downwards. It made an existing test fail
(`live_text_replaces_the_page_keeps_the_place_and_ends_on_live_off`: a live text whose caret is
on the line at the top edge moved the page), so the step is the old one in both directions.

Decisions taken, for the owner to confirm:

- Marks are made at every parse, for every document, used or not: 12 bytes for each line of the
  file that has text, and a parse about 3% longer (below). Making them only when the editor asks
  would need a second parse.
- A column is a byte in the line, as the editor's `--goto line:column` already was.
- The follow step stays 48 px both ways. One consequence: while the caret walks up, its row can
  be the one just above the top edge for one line before the page follows.
- No shot: nothing new is visible in a still picture.

Budgets, measured on this PC (Windows 11, release builds):

- Parse, `parse_cost` on 8 MB of `fixtures/showcase.md` repeated: 73.0 / 74.4 ms (min / median of
  9) before, 75.5 / 78.4 ms after; a second pair of runs 73.4 / 75.8 and 76.1 / 77.1. 8 MB of
  one-line paragraphs: 36.8 / 37.3 ms before, 37.6 / 38.2 ms after.
- First frame and scrolling, the release before (`f/live-preview-unsaved`) and after, three runs
  each of `MDVIEW_TRACE=1 mdview --no-session --scroll-steps 120`: `fixtures/showcase.md` first
  frame after 73.4 / 74.3 / 75.3 ms before, 73.7 / 73.8 / 80.1 ms after; a scroll frame's own
  work, median 0.57 ms before and 0.56 to 0.57 ms after (p90 0.83 to 0.88 and 0.84 to 0.89).
  The 610-line test document: first frame 53.2 to 55.3 ms before, 53.6 to 55.6 ms after; scroll
  median 0.56 to 0.57 ms before, 0.56 to 0.61 ms after. No difference beyond the runs' own spread.

Tried in real windows on Windows 11: a script started mdedit on a 610-line test
document with `--run PreviewSideBySide`, which started this mdview beside it (both with scratch
settings), posted arrow, Home and End keys to the editor's window and saved each window's own
picture after each. Seen: the caret on line 150, 190 and 230 of a 300-line code block has that
line of code in the second row of the page each time (before, the page stayed at the block's
start); the caret at byte 9000 of a 27000-byte one-line paragraph has the row with that word
second from the top, End shows the paragraph's end, Home its first row; 100 lines up into a
300-line paragraph the page shows the row that line's text begins in. Not tried in a real window:
"Edit in mdedit" from a row inside a wrapped line (tested headless), typing with live text on a
long line, and anything by hand.

Not verified: Linux in real windows (the tests run there on CI; the unix-only test
`edit_in_mdedit_starts_the_editor_at_the_top_line` was changed to expect a column when the top
row is inside a wrapped line, and was not run here); a person's judgement of how the following
feels; right-to-left text; a document over the live-text cap (the line still maps through the
file on disk).

### Window backdrop (`f/frosted-window`; shared `f/frosted-window`)

Merged into `merge/next-release` on 2026-11-15 from the branch `f/frosted-window`, which held all four.

**DESIGN.**

**The setting.** `backdrop=mica|acrylic` in the session file (`Session::backdrop`, default off;
the line is written only when it is not off, so a session that never chose it is the file it
was). The row "Window backdrop" is under Theme in Settings, with the same title, choices and help
line as mdedit's (`mdshared_app::backdrop::Backdrop::{TITLE, LABELS, HELP}`), and exists only in
a Windows build. `--backdrop off|mica|acrylic` sets it for a run (for a capture; with
`--no-session` nothing is saved).

**What is frosted.** The regions Title (title row and tab strip), Bar, Files and Outline. The
page with its banners and the update bar, menus, the palette, tooltips and dialog cards stay
opaque; a dialog's dimming and a menu's shadow darken the frosted chrome as they darken the flat
one.

**When, and how.** As in mdedit, by the same code: `mdshared_ui::frost::Frost` decides (the
setting, Windows 11 22H2 or later, transparency effects on, no high contrast, no battery saver,
the calls accepted, the window has the keyboard) and asks the system through
`frost::WindowSystem`; a frosted region's pixels are marked after it is drawn (`frost::mark`) and
keyed out in the copy that goes to the window (`frost::key_out` in `App::present`, shifted by the
client margin). With the setting off the cost is one comparison a frame and no system call.

**Where the code is.** `bins/mdv-app/src/frost.rs` (which regions), `App::sync_frost`, the mark
in the region loop of `compose`, the key-out in `present`, `App::focused` (set by
`WindowEvent::Focused`), `Workspace::backdrop` / `set_backdrop`, the row in `settings.rs`.

**Handoff.**

**Built.** As above, plus tests: `session::the_window_backdrop_is_off_unless_chosen_and_then_kept`,
`frost::tests::the_page_stays_opaque`, and five in `app/frost_tests.rs` on composed frames (the
chrome marked and the page not, at scale 1 and 1.5, and still so after a scroll; a menu over the
chrome opaque and its shadow not; the Settings dialog dims without making opaque, and leaves
nothing behind when closed; unfocused or off: every pixel opaque; the row sits under Theme on
Windows and is absent elsewhere). Shot `dialog-settings-backdrop` (the row set to Mica; a shot
has no window, so its chrome is flat).

**Setting off is unchanged.** `tests/shots.sh` against the set of `f/finer-sync`: 67 of 74
byte-identical. The seven that differ: the five Settings shots (`dialog-settings-light`, `-dark`,
`-scale-150`, `-760`, `-640`: one more row), `update-installed` (the Restart button's branch:
the button and the new sentence) and `dialog-recent` (it prints the folder the checkout is in).
New: `dialog-settings-backdrop`, `update-installed-plain`.

**Seen in a real window** (Windows 11 build 26300, release build, `--no-session` and a scratch
settings file, started and closed by a script; the window was put above the others for a moment
and the screen where it is was saved): Mica in the dark and the light theme and Acrylic in the
dark theme, each with both panes open. The title row, the bar and both panes show the material,
the page is solid, the text of the chrome is sharp. With the window not in front the chrome was
the flat colours, the same pixels as with the setting off. `docs/images/frosted-window/` has
`dark-mica-active.png`, `light-mica-active.png` and `dark-off-active.png` (the Acrylic capture
shows what was behind the window on this desktop and was not kept).

**Numbers** (release builds, this PC, three runs each of `--scroll-steps 120` on
`fixtures/showcase.md` with both panes open; "before" is `f/finer-sync`):

| | before | off | Mica | Acrylic |
|---|---|---|---|---|
| first frame, ms | 83.2 to 92.6 | 80.8 to 89.8 | 81.9 to 95.6 | 84.2 to 89.8 |
| scroll frame, own work, ms (median / p90) | 0.53-0.54 / 0.77-0.81 | 0.51-0.54 / 0.77-0.87 | 0.67-0.69 / 0.89-1.05 | 0.66-0.67 / 0.92-0.96 |

Off is the same as before within the runs' spread. On: about 0.14 ms more on a scroll frame, and
only with the Outline pane open (without it, or with only the Files pane, the figure is the one
with the setting off: 0.56 to 0.58 ms against 0.57 to 0.59). The Outline pane is repainted while
the page scrolls (the heading in view is marked in it), and a repainted frosted region is marked
and keyed out again. Memory was not measured.

**Decisions taken, for the owner to confirm.**

1. Both side panes are frosted, as mdedit's one is.
2. The row is in the Page group under Theme (mdview's Settings has no View group).
3. `--backdrop` was added as a launch argument, for captures.
4. The order on this branch is Restart first, then the backdrop: the shared head this branch
   pins has the Restart button's bar, so the viewer needs that branch's code to build.

**Not run.** Everything in mdedit's list (transparency effects off, battery saver, high
contrast, Windows 10, Windows 11 before 22H2, maximized, snapped, a second monitor, scales other
than 1 in a real window, remote desktop); changing the setting in the dialog by hand; a menu, a
dialog, the palette or a tooltip over the frosted chrome in a real window (a menu and a dialog
are tested headless, the palette and tooltips are not); Linux
(the row is absent and the calls return false; not built there in this session). Nobody has
looked at it by eye.

### Restart after an update (`feat/restart-after-update`; shared `feat/restart-after-update`)

Merged into `merge/next-release` on 2026-11-15 from the branch `f/frosted-window`, which held all four.

**DESIGN.**

Beside the Restart button the bar now reads "mdview X is installed. Restart to use it." (shared
`Updates::bar_label_with`). Where there is no file to start (a copy that cannot update itself) it
keeps "It starts the next time you open mdview." and has no button. Shots `update-installed` and
`update-installed-plain` show the two.

**Handoff.**

Tried by script on Windows 11 with this branch's release build and a scratch settings file: a
window was open; a second process without the mark handed its file to that window and returned
in 0.1 s; a second process started with `--restarted` (what Restart starts) had no window for
the 3 s the first one stayed open, and opened its own window, with the session's tabs and its
file, once the first had gone.

Not tried: a real update followed by a click on Restart (it needs a second build with a higher
version on an https host); the click itself in a real window (`--update-bar installed` has no
installed file to start); an AppImage.

Found on the way, **not changed** (it is in the released 0.3.1 too and belongs to no feature
here): a close request that comes from the system (`WM_CLOSE`, as the taskbar's "Close window"
sends it) sets `App::exit`, but the window goes only at the next event (a pointer move over it
is enough). Measured on the `main` build: still open 8 s after the request, gone right after a
posted pointer move. mdedit does not have this: its `CloseRequested` also asks for a frame
(`self.redraw()`), which is probably all mdview needs in `window_event`.
