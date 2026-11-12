# Handoff

`mdview` is a Rust remake of `Kaan88/markdown-viewer-cpp`: a Markdown viewer with its own
renderer, no webview, and no authored C++. This file is the current state: what ships, what is
being built, how to build and check it, the traps already found, and what to do next.
`docs/PLAN.md` is the full plan (budgets, phases, the 2026-11-11 roadmap). Read this file first.

Rewritten on 2026-11-12 from the dated sections that had accumulated since 2026-11-08. The commit
that last changed this file is the state it describes; check `git log --oneline -5`.

---

## 1. Where the project is

**What is being built.** The shell is `bins/mdv-app` (package and binary `mdview`): winit 0.30 for
the window and input, softbuffer 0.4 for the pixels, a frameless window, and every control (title
row, tabs, command bar, panes, menus, dialogs, tooltips, text field) drawn by the crate through the
page's own text path. Windows first. The user approved its look by using the running app on
2026-11-11 (PLAN roadmap step A2). It has never been built or run on Linux.

**What ships.** The Qt 6 shell, `bins/mdv-qt` (CXX-Qt, Qt Widgets), is still the shipping shell
until the new one is approved by hand (roadmap A3) and the checklist and numbers are done (A4). Its
installer is `packaging/windows/mdview-qt.iss` (built by `build-installer-qt.ps1`) and is on the
v0.1.0 GitHub pre-release. On Linux it is packaged as a PKGBUILD and an AppImage (section 13).

**What is retired.** The WinUI 3 shell (`bins/mdv-winui`), its installer and build script are on
the `winui-archive` branch since 2026-11-11. Why it was dropped: about 383 ms to first frame and
about 180 MiB private memory for an empty window against PLAN's 50-90 MB, a system-wide Windows App
Runtime dependency, Windows only, and hand-trimmed generated bindings.

**Decided** (do not reopen without the user):

- The shell is winit + softbuffer, chosen over fltk-rs and Slint after mockups on 2026-11-11
  (Slint's text was rejected as blurry in software and fringed with Skia). PLAN, "Next: a lighter
  shell than Qt" has the numbers.
- The user approves the interface by looking at it. Screenshots and tests do not stand in for that.
- Remote images are off until the reader allows them, per document, with an "always" setting.
- Update checks are manual only (2026-11-10). The new shell has no update check at all.
- No bundled fonts (2026-11-08), so the golden compare runs on Windows only.
- An editor is a second project, not a mode of mdview (PLAN section E).
- The installer never takes the `.md` default; it registers and offers Windows' Default apps page.
- Linux is low priority until the Windows shell has replaced Qt.

**Plan phases.** 0 spikes, 1 core, 2 headless renderer, 4 engine: closed. 3 (Qt parity) and 5
(WinUI) are superseded by the roadmap in PLAN ("Roadmap from 2026-11-11"): A1-A2 done, A3 (hand
checks) and A4 (checklist and numbers) open, A5 (installer) built and run, A6 (retire Qt) waiting.
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
| `crates/core` | Parser (pulldown-cmark), HTML allow-list mapping, autolink, slugs, document model, find, command table, session, recents, limits, file watching (notify), single instance, tab labels, update-manifest logic |
| `crates/render` | Headless renderer: parley layout, swash raster, image store, patchable `PageLayout`, scroll frames, hit testing, selection, syntect highlighting, the engine client |
| `crates/engine` | Library and the `mdview-engine` binary: SVG (resvg 0.47), TeX math (RaTeX 0.1.15), Mermaid (merman 0.9.3) |
| `crates/engine-proto` | Job and reply wire format between the renderer and the engine process |
| `crates/fetch` | `mdview-fetch`: remote image downloads through WinHTTP, disk cache; a failing stub elsewhere |
| `bins/mdv-app` | The new shell, binary `mdview` |
| `bins/mdv-bench` | Headless render to PNG, timings, golden generation |
| `bins/mdv-qt` | Qt shell, **excluded** from the workspace (needs Qt to build); own `Cargo.lock` and `target/` |
| `packaging/windows` | `mdview.iss` + `build-installer.ps1` (new shell), `mdview-qt.iss` + `build-installer-qt.ps1` (Qt) |
| `packaging/arch`, `packaging/appimage` | Linux packaging for the Qt shell |
| `fixtures/` | Test documents; `everything.md` exercises every feature (section 5) |
| `tests/golden/` | Renderer goldens (`showcase-*`, compared by a test) and review screenshots |
| `spikes/` | Throwaway reference code, excluded from the workspace; `spikes/parser/data` holds the spec suites and `generated_1mib.md` |
| `docs/` | This file, `PLAN.md`, `PORT_CHECKLIST.md`, `COVERAGE.md`, `UPDATE.md`, `WINDOWS_HANDOFF.md` (2026-11-10 WinUI checklist, history), `AUDIT_PHASE1.md` |

`target/` is git-ignored. Helper scripts that once lived there (`vm.py`, `seed-lock.py`,
`cap.ps1`) are not committed; sections 9 and 12 say what they did.

## 3. Build, test and run on Windows

Toolchain on the PC: rustc/cargo 1.99.1, MSVC 2022, Qt 6.9.3 MSVC at `C:\Qt\6.9.3\msvc2022_64`,
Inno Setup 7.

Point `TEMP`/`TMP` at a writable folder when building, or `lib.exe` and `rc.exe` fail with access
denied:

```powershell
cd C:\Users\kaank\Desktop\mv
$env:TEMP = "$PWD\target\tmp"; $env:TMP = $env:TEMP
cargo build --release --workspace          # includes mdview.exe and mdview-engine.exe
cargo test --release --workspace           # 2026-11-12: 356 passed, 0 failed, 6 ignored
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
| `--zoom <pct>`, `--folder <dir>`, `--maximize` | Start-up overrides |
| `--no-session` | Neither reads nor writes the session and skips the single-instance hand-over. Use it for every test |
| `--load-remote` | Load the first document's remote images without the bar (for checks) |
| `--shot out.png` | Draw one frame of the state the other flags describe, write it, exit. No window |
| `--size WxH`, `--scale f` | Shot size and device scale |
| `--files`/`--no-files`, `--outline`/`--no-outline`, `--focus-mode` | Panes and focus mode |
| `--find q`, `--scroll px`, `--hover x,y` | Find query, scroll offset, pointer position |
| `--menu main\|recent\|tab\|page`, `--dialog about\|shortcuts\|recent` | Open a popup or dialog in the shot |
| `--scroll-steps n` | Open a real window, wait until the page is refined, scroll n wheel steps, log timings, exit |

```sh
target/release/mdview.exe --no-session --shot out.png --dark --size 1280x800 --menu main fixtures/showcase.md
target/release/mdview.exe --no-session --shot out.png --light --load-remote fixtures/remote.md
```

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
on Windows only (system fonts). After an intended visual change, re-bless them and look at the
difference before committing:

```powershell
$env:MDV_BLESS = 1; cargo test --release -p mdview-render --test golden; Remove-Item Env:MDV_BLESS
```

A failing compare writes `target/golden-actual-<name>.png`. The engine must be built, or SVG, math
and diagrams render as placeholders.

**The Qt shell** builds from its own directory:

```powershell
cd bins\mdv-qt
$env:PATH = "C:\Qt\6.9.3\msvc2022_64\bin;$env:PATH"
$env:QMAKE = "C:\Qt\6.9.3\msvc2022_64\bin\qmake.exe"
$env:CMAKE_PREFIX_PATH = "C:\Qt\6.9.3\msvc2022_64"
$env:TEMP = "C:\Users\kaank\Desktop\mv\target\tmp"; $env:TMP = $env:TEMP
cargo build --release
```

Run `bins/mdv-qt/target/release/mdview-qt.exe` with Qt's `bin` on `PATH`. Its scripted checks print
one line and exit: `--drop-test <path>`, `--select-test`, `--select-hold`, `--link-test`,
`--details-test`, `--find-test`, `--outline-test`, `--copy-test`, `--tabs-test`. Not verified in
this rewrite: that the Qt crate still builds against the renderer API changed on 2026-11-11 and
2026-11-12 (`7ef3831` adapted it; nothing since was built here).

**crates.io** was unreachable from the PC for a while (`SEC_E_NO_CREDENTIALS`) and worked again from
2026-11-08. With crates cached, `--offline` builds; a stale local index can fail to resolve locked
versions, and a build without `--offline` refreshes it.

## 4. The new shell (`bins/mdv-app`)

Pure Rust, no DLLs beyond the system's. `mdview_render::shared_fonts()` gives the chrome a clone of
the renderer's font context, so system fonts are enumerated once.

| Module | What is in it |
|---|---|
| `workspace.rs` | All tab, document, session, find, selection, link, scroll, outline, remote-image and status logic, headless and unit-tested. Ported from `bins/mdv-qt/src/shell.rs`. One shared `DocumentRenderer`; only the active tab is laid out |
| `app.rs`, `app/input.rs`, `app/commands.rs` | State, region compose and present, the winit handler, pointer and key handling, every `CommandId`, wakes and timers |
| `app/tests.rs` | A headless `App` at 1280x800 driven through the real handlers |
| `chrome.rs`, `panes.rs`, `page.rs`, `overlays.rs` | Title row and command bar; Files and Outline panes and the overlay scrollbar; page blit, find and selection overlays, missing-file banner, remote-image bar, empty state; menus, dialogs, toast, link hint, tooltips |
| `ui.rs`, `keys.rs`, `args.rs` | Regions, `Hit`, `Act` and window state; shortcuts parsed from core's command strings; command line |
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
`147b805` the title row and command bar stayed white until something changed under the pointer.

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
(every 30 s; tabs left 60 s ago drop their document), saving the size 800 ms after a resize, and
page refinement (4 ms slices while there is more). A refined, idle window costs no CPU.

**Differences from the Qt shell** (all noted in `workspace.rs`): restored tabs load when first
shown and inactive tabs drop their layout; the reading position survives reload, resize and zoom
as block plus offset; find re-runs after a reload; double click selects a word and triple click a
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

## 5. The renderer (`crates/render`)

Headless: no window system, BGRA bitmaps, parley 0.12 for layout, swash for raster. The layout
code is split into `crates/render/src/layout/` (`page`, `relayout`, `estimate`, `refine`, `frame`,
`hit`, `selection`, `store`, `pager`, `inline`, `paint`, `prose`, `containers`, `table`, `media`);
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
up front; the Qt shell uses it.

**Patchable layout.** Each top-level block's height is recorded; positions are prefix sums rebuilt
from the first changed block. Anything that lays a block out (a frame, a hit test, a selection, a
jump, a `<details>` toggle, an arrived image, a refinement slice) may correct its height in place.
Corrections keep the viewport's anchor (a block and an offset into it) fixed, so content on screen
does not jump; a correction above the viewport moves `top()` instead. A block with an image,
formula or diagram the store lacks is laid out with a typical box and stays provisional until
shown; refinement never loads anything. A `<details>` toggle or an arrived image re-measures only
the blocks that use it.

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

**Image store** (`layout/store.rs`, since `1ad57b4`). Every raster (file images, remote images,
inline and display math, Mermaid) is keyed by what its pixels depend on: a file's resolved path,
length and mtime; a formula's source, pixel size, colour and display flag; a diagram's source,
scale and theme. Relayout, a width change, a zoom back to a known scale, a `<details>` toggle, a
reload or a tab switch re-reads and re-renders nothing. Pixels are capped at 64 MiB
(`IMAGE_CACHE_BYTES`) with LRU eviction that never drops what the current frame uses. File SVGs are
rasterized by the engine at the device scale (`KIND_SVG_AT`).

**Images.** PNG, JPEG, GIF (first frame only) and WebP decode in-process; SVG goes to the engine.
Limits 12 MiB and 24 Mpx. `<picture>` light/dark sources follow the theme; `<img width height>`
are honoured and reserve the box while loading. Below half size an image is area-averaged
(`abf7b54`), at half size and up bilinear.

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
six renderer bugs, fixed in `f5a8bed`: strikethrough and underline looked up at the shaped run's
start (missing or spilling onto neighbours, also in the showcase goldens); sub- and superscript on
the baseline; every alert blue; `<ol type>`, `<div align>`/`<center>` around Markdown, `<hr width>`
and `align=justify` ignored; a code label drawn through a long first line. Others: `showcase.md`
(the goldens), `diagrams.md` and `diagrams-more.md` (Mermaid families), `math.md`, `images.md`,
`formats.md` (GIF/WebP), `html-extras.md` (ruby, dl, figure), `remote.md`, `linked.md`,
`two-svg.md`.

**Older measurements** (2026-11-08, before estimated heights; `generated_1mib.md`: 1.0 MB, 25,060
blocks): scroll frames 0.2/0.8 ms mean/worst at 780x900 100%, 0.6/1.1 at 1170x1350 150%, 2.6/3.4
at 3840x2200 150%, inside PLAN's 4 ms and 8 ms budgets.

## 6. Remote images and `crates/fetch`

**Policy.** A document never contacts a server unasked. The renderer asks for remote URLs only in
passes that show them; the workspace holds them back (`RemoteState::Blocked`) unless this document
was allowed or the "always" setting is on.

**The bar.** Over a page that holds remote images back, a bar offers **Load images** (this
document), **Always load** (every document from now on) and a close button (hides the bar for this
document). Its count is the number of distinct `http(s)` image URLs in the document, counted when
the file is read (`a4065b9`), not the number a frame has met. The main menu has a checked item
"Always load remote images" that turns the setting on or off; it is saved in the session as
`remote_images=0|1`. Images already shown stay when it is turned off.

**`mdview-fetch`.** Started by the first image that may load; nothing touches the network before.
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

This is the shell's only network use besides the loopback single-instance hand-over
(`docs/UPDATE.md`, top note).

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
   The flag is set in the script, not in `.cargo/config.toml`, because it must not reach the Qt
   crate (Qt needs the dynamic CRT) or a Linux build. Ordinary `cargo build` output in
   `target\release` keeps the dynamic CRT. `-SkipBuild` reuses `target\dist\release`.
3. Runs `ISCC /DAppVersion=<workspace version> mdview.iss` and writes
   `target\installer\mdview-setup-<version>.exe` and its `.sha256`.

`mdview.iss`: per-user to `%LOCALAPPDATA%\Programs\mdview`, ships `mdview.exe`,
`mdview-engine.exe` and `mdview.ico`, its own AppId. The "assoc" task (on) registers the ProgId
`mdview.Markdown`, `OpenWithProgids` for `.md .markdown .mdown .mkd`, `Applications\mdview.exe`,
and a registered application (`Software\mdview\Capabilities` and `RegisteredApplications`); it never
writes an extension's default value. The last page offers (unchecked) to open
`ms-settings:defaultapps?registeredAppUser=mdview`. Optional desktop shortcut.

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

Tests: `cargo test --release --workspace`: 356 passed, 0 failed, 6 ignored.

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
- The installer's wizard pages, an upgrade over an older version, the Default apps page link.
- Anything on Linux or KDE for the new shell; the Linux dependency features in `Cargo.toml` are
  untested.
- Retiring Qt (A6).

## 11. Known gaps

**New shell:**

- No snap-layouts flyout on the maximize button. The system accent colour is not used.
- Pane widths are not saved (which panes are open is).
- The Files tree refreshes when the window gains focus, on F5 and on a file-watcher wake (the
  watcher watches the open files' folders), not on every change under its root.
- Maximized memory is three full-window bitmaps.
- Icons need Segoe Fluent Icons or Segoe MDL2 Assets (Windows only).
- No update check (by decision; a manual menu item is future work, `docs/UPDATE.md`).
- `cargo clippy` style warnings remain.

**Renderer and engine:**

- Ruby annotations sit over the first line of their base only, at half size.
- `<figure>` has no side margin.
- GIF shows its first frame only (no animation, so idle CPU stays zero).
- `architecture-beta` diagrams keep their source; `wardley` and `info` are off too.
- Text in packet and C4 diagrams is small.
- Math and diagrams are images: no selection geometry, no equation numbering. Their source can be
  copied from the page context menu.
- Table `colspan`/`rowspan` are laid out up to 200 columns. Highlighting covers fenced blocks only.
- No bundled fonts, so the golden compare runs on Windows only and Linux metrics differ.
- Any image format other than PNG, JPEG, GIF, WebP and SVG keeps its placeholder.

**Qt shell** (until retired): Manage recent is a dock, not a dialog; no empty-state Open button;
frameless window that resizes only from the status-bar grip; the AppImage build still checks for
updates once a day (`spawn_check` in `shell.rs`), against the manual-only decision; not run on KDE
since the 2026-11-11 redesign.

## 12. Linux: what exists for the KDE run

Nothing of the new shell has been built on Linux. PLAN D lists the work: Wayland file drops,
portal dialogs (rfd is already configured with `xdg-portal`), an icon font to replace Segoe
Fluent Icons, and a build of `crates/fetch`'s stub (remote images will fail there until a
transport is written; `stub.rs` describes the intended approach). PLAN's "Verification" section
lists the end-to-end Plasma run.

**The user's Linux desktop** (CachyOS, KDE Plasma, Wayland with XWayland) is where the Qt shell was
last built and run (2026-11-10, Qt 6.12.0, offscreen and briefly on Wayland/X11). Kept there on
2026-11-10: the build caches (`target/`, `bins/mdv-qt/target/`), `~/.local/rust-win`, and the
AppImage in `~/mdview-appimage/`.

**The Debian test VM** (used up to 2026-11-08; not verified since): `user@vm-host`,
password not recorded, Debian 13, 20 cores, 7.8 GB RAM, packages `qt6-base-dev` (Qt 6.8.2),
`libfontconfig1-dev`, `libxcb-cursor0`, `libxkbcommon-x11-0`, `xvfb`, `imagemagick`,
`fonts-noto-cjk`. Driven over SSH by `target/vm.py` (not committed; `paramiko`, env
`MDV_VM_HOST`/`USER`/`PASS`, subcommands `run`, `put`, `get`). A window can be shown on a Windows
X server listening on TCP (`DISPLAY=windows-host:0.0` was the user's MobaXterm); `ssh -X` did not
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

**Linux file association** (Qt packages): the PKGBUILD's desktop entry registers `text/markdown`
and `text/x-markdown` and never sets a default; `.mdown` is `text/plain` in shared-mime-info, so it
needs a small `mime/packages` XML. The package itself has never been installed with `pacman -U`.

## 13. What to do next

The user used the new shell with `fixtures/everything.md` on 2026-11-12 and approved it
("it looks beautiful and everything seems to work"); the three things they raised (cursor, back and
forward, window position and pane widths) are fixed. They then set this list, in this order.
Nothing on it is started.

1. **Retire Qt:** archive branch, as with WinUI; drop `bins/mdv-qt`, `mdview-qt.iss` and its build
   script from `main`; the new installer becomes the release.
2. **Go through `docs/PORT_CHECKLIST.md`:** the new-shell table's "differs" and "not done" rows,
   one by one. Fix or record each.
3. **A simple automatic update:** the app replaces its own exe. The manifest and SHA-256 logic is
   in `crates/core/src/update.rs` and `docs/UPDATE.md`; the download can go through `mdview-fetch`.
   The binary will never be code-signed (the user's decision), so the hash check is the only
   integrity check there is.
4. Features, in the user's order: remember the scroll position per file; live reload that keeps
   the reader's place; animated GIFs; content-sized tables; the missing highlight languages
   (Kotlin, Swift, PowerShell, Dockerfile); a true float across blocks; wiki-links and backlinks.

Set aside by the user for now: crash reporting, accessibility, non-Latin input methods, Linux,
testing on other machines (there is one PC). The editor is a second project (PLAN section E).

## 14. History

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
- **2026-11-11, Windows PC, layout A** (`0e8ee6d`, `09c4371`). No menu bar; tabs in the title row;
  one command bar (Open, Reload, Recent | Files, Outline | zoom; find box with count,
  previous/next, Theme, main menu); Files pane left, Outline right. Both shells brought to parity
  side by side. Two Qt bugs fixed: both right-click menus used `ActionsContextMenu` and never opened,
  and every mouse button acted as the left one.
- **2026-11-11, WinUI final work** (`6014a7e`). Single instance before the Windows App SDK bootstrap
  (17-24 ms hand-over), OS file watching through `DispatcherQueue`, Manage recent dialog, 0.1.0
  installer that registers the extensions and a registered application. Measured 383 ms first frame
  and 180-210 MiB private memory; `ExtendsContentIntoTitleBar` alone cost 70-95 ms. Retired the
  same day (`94cf36f`); the code is on `winui-archive`.
- **2026-11-11, Qt as the only shell.** Panes and window size saved in the session
  (`layout=files=1;outline=0`, `geometry=WxH [max]`); Qt Files tree loads folders on expand; no
  console window; Qt installer (`mdview-qt.iss`, windeployqt output plus the MSVC runtime
  app-local, ProgId `mdview-qt.Markdown`) on the v0.1.0 pre-release.
- **2026-11-11, shell mockups** (`ed9fb41`). `spikes/shell-fltk`, `shell-slint`, `shell-soft`, each
  its own workspace; numbers in PLAN. winit + softbuffer chosen.
- **2026-11-11, the new shell** (`a82c8c0`). `bins/mdv-app` with every control hand-drawn; the user
  approved the look. Same day: full GitHub emoji list (`035b78c`), `layout.rs` split into modules
  (`66c792e`), clipped text draw (`7bee6a3`), GIF and WebP (`bf3ab80`), cheaper glyph cache
  bookkeeping (`3ffbe64`), subpixel order setting and ClearType-following chrome (`72b8285`),
  installer with static CRT (`6c66a5c`, moved into `target\dist` by `a2faecb`, `8f441cb`),
  area-averaged downscaling (`abf7b54`), `mdview-fetch` (`8ba8fa4`), 23 more Mermaid families
  (`2915f08`, `50a0b85`), the image store (`1ad57b4`), startup trace counts (`3fafe69`), the
  patchable layout (`7ef3831`), estimated first layout and refinement (`db29742`, `feee929`), the
  editor noted as a second project (`49d5fbc`).
- **2026-11-12.** Whole-frame present on a system repaint (`147b805`); remote images off until
  asked, with the bar and the "always" setting (`6ac87a8`, `a4065b9`); softer popup shadows and
  wider resize bands with corner zones (`a7d2417`); ruby, `<dl>` and `<figure>` styling (`b0fde23`);
  copy formula and diagram source (`87d63f1`); page text follows the display's subpixel order
  (`52814ce`); the six `everything.md` fixes (`f5a8bed`); `<mark>`, `dir`, table `align`/`width`/
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
