# Handoff

`mdview` is a Rust remake of `Kaan88/markdown-viewer-cpp`: a Markdown viewer with its own
renderer, no webview, and no authored C++. This file is the current state: what ships, what is
being built, how to build and check it, the traps already found, and what to do next.
`docs/PLAN.md` is the full plan (budgets, phases, the 2026-11-11 roadmap). Read this file first.

Rewritten on 2026-11-12 from the dated sections that had accumulated since 2026-11-08; the Qt shell
was retired on 2026-11-13. The commit
that last changed this file is the state it describes; check `git log --oneline -5`.

---

## 1. Where the project is

**What is being built.** The shell is `bins/mdv-app` (package and binary `mdview`): winit 0.30 for
the window and input, softbuffer 0.4 for the pixels, a frameless window, and every control (title
row, tabs, command bar, panes, menus, dialogs, tooltips, text field) drawn by the crate through the
page's own text path. Windows first. The user approved its look by using the running app on
2026-11-11 (PLAN roadmap step A2). It has never been built or run on Linux.

**What ships.** `mdview` is the only shell. Its installer is `packaging/windows/mdview.iss` (built
by `build-installer.ps1`, section 8). **v0.1.0 and v0.2.0 (the seven features of section 13, item 4) were published on GitHub on 2026-11-13, and 0.2.2 is the latest (section 14)**; v0.1.0 from
`5cbd354`, as the latest release, with the installer, the two executables and `mdview-update.txt`
(the earlier v0.1.0 pre-release, which carried the Qt installer, was deleted with its tag on
2026-11-12). The installer from that release has not been downloaded and run. Nothing is packaged
for Linux.

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
- Linux is low priority until the Windows shell has replaced Qt.

**Plan phases.** 0 spikes, 1 core, 2 headless renderer, 4 engine: closed. 3 (Qt parity) and 5
(WinUI) are superseded by the roadmap in PLAN ("Roadmap from 2026-11-11"): A1-A2 done, A3 (hand
checks) and A4 (checklist and numbers) open, A5 (installer) built and run, A6 (retire Qt) done.
B1-B5 (GIF/WebP, remote images, Mermaid families, emoji list, ruby/dl/figure and copy-source) and
C1-C5 (image store, `layout.rs` split, estimated heights, cheap toggles, text and bitmap work) have
landed in `main`; see section 14 for the commits.

**Where the work happens.** The physical Windows PC (Windows 11, RTX 4070 Ti SUPER, 3440x1440 at
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
| `--update-bar available\|installing\|installed` | Show the update bar at that stage with a made-up version, for a shot |
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
- *Not done.* Outline rows have no keyboard navigation, so backlink rows have none either. A
  note created beside a document while no folder is open (nobody watches that folder) leaves
  its `[[name]]` as text until the document is read again or another wake comes. With no
  folder open `[[#Heading]]` is text too, since the parser decides before headings are known. A new folder below the open one is seen
  at the next wake, not by itself. `[[Note#^block]]` opens the note at its top. Idle CPU with
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
- Anything on Linux or KDE for the new shell; the Linux dependency features in `Cargo.toml` are
  untested.

## 11. Known gaps

**New shell:**

- No snap-layouts flyout on the maximize button. The system accent colour is not used.
- Maximized memory is three full-window bitmaps.
- Icons need Segoe Fluent Icons or Segoe MDL2 Assets (Windows only).

**Renderer and engine:**

- Ruby annotations sit over the first line of their base only, at half size.
- `<figure>` has no side margin.
- An animated image over 10,000 frames or 4 M pixels shows its first frame only. A window covered
  by another one goes on animating (winit reports no occlusion there); minimised, it stops. An
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
2. **Move the shared `main`.** mdview pins commit `24865c5` of `markdown-shared-rs`, on the branch
   `chore/finalize` (on top of `1c3710a`, branch `fix/update-look-every-launch`). A push to the
   shared `main` is refused from the PC; the owner moves it, then the `rev`s here
   (`bins/mdv-app`, `crates/core`, `crates/render`) can name commits on `main`.
3. **Linux** is set aside by the user (see the end of this section) and has never been built
   for the new shell.
4. Left open by the workers of the seven features: a table never stands beside a float even
   when it would fit; `[[Note|a *b* c]]` (markup in a label) and `![[embed]]` stay text; a
   window fully covered by another keeps animating; the main menu, with "Play animated
   images" added, just fits a window 800 px high (not re-checked).
5. The no-pixel-change check is now `tests/shots.sh` (55 states, deterministic, pinned to
   scale 1.0 except the shots that ask for another). Before a change to the renderer, the
   shell or the shared pin run it into one folder, after it into another, and `cmp` every
   file.
6. The editor and the shared code have their own repositories (`Kaan88/markdown-editor-rs`,
   `Kaan88/markdown-shared-rs`); the editor's plan is built to its end.

Set aside by the user for now: crash reporting, accessibility, non-Latin input methods, Linux,
testing on other machines (there is one PC). The editor is a second project (PLAN section E).

## 14. History

- **2026-11-14, command palette, settings dialog, window shadow (0.2.3 still, not published; shared
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
    on it. Not done here (no new rendering options).
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

- **2026-11-13, 0.2.3 (not published).** Menus taller than the window scroll and stay below the
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
