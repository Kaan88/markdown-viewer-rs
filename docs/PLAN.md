# Plan: mdview, a performance-first Rust Markdown viewer

> Copy of the working plan, kept in the repo for the move to Windows. Phase 0 and most of Phase 1 are done, so "Nothing has been built yet" below is out of date. Current state is in [HANDOFF.md](HANDOFF.md).

## Context

`Kaan88/markdown-viewer-cpp` (C++20, Qt Widgets, `QTextBrowser`) works, but its rendering and design are imperfect. Most of that comes from `QTextBrowser`'s limited HTML subset. It also starts a helper process for every render and polls each tab every 800 ms.

The goal is a Rust remake with these priorities:

1. **Instant launch and low memory, and Markdown coverage, equally.** Coverage work goes in lazily loaded code, so it stays off the launch path. See "Coverage".
2. The best native look on Windows and Linux (KDE Plasma on Wayland is the daily desktop), with all authored code in Rust.
3. Better rendering and design than the old app.
4. Feature parity with the old reader, without its known bugs.

Decisions made with the user:

- Custom Rust renderer, no webview.
- **Windows: WinUI 3**, committed regardless of the measured numbers.
- **Linux: real Qt Widgets through CXX-Qt**, with no authored C++ or QML. Confirmed after a side-by-side with Slint, which was rejected for its text rendering despite better launch and memory numbers. A small C++ helper for system icons may still be needed; ask before adding one.
- New private GitHub repo, no CI.

Project folder: `/home/kaan/markdown_viewer` (empty, not yet a git repo). Repo: `Kaan88/markdown-viewer-rs` (private).

**Nothing has been built yet.** Every budget below is an estimate until measured.

## Accepted trade-offs

- **WinUI cost:** an estimated 200-350 ms warm launch and 50-90 MB, versus 25-60 ms and 8-12 MB for Win32. No published Rust figures exist. I will measure and report the numbers, and reduce them where possible, but they won't reverse this choice.
- **Wine probably won't run the UI:** G8 tries it anyway at the user's request, but WinUI 3 is expected not to run under Wine ([bug 56916](https://bugs.winehq.org/show_bug.cgi?id=56916) is open with no working reports). **I will write the WinUI shell without being able to run it here**, so it needs the user's Windows PC at each checkpoint. Expect more back-and-forth for that shell.
- **Windows runtime:** the app needs the Windows App Runtime (installer chains it) or a roughly 57 MiB self-contained folder. A single-file exe isn't possible.
- **Windows 11 only:** the user's decision. The framework-dependent bootstrap needs build 22000 or later, so no Windows 10 support.
- **Qt bridge risk:** CXX-Qt's maintainer advises against bridging Widgets because of raw pointers and heavy `unsafe`. Overriding `paintEvent` and input events on `QWidget` is unproven, so G1 tests it first. C++ is generated and compiled at build time, but none is authored.
- **Fidelity:** roughly 80-90% of GitHub's look overall (estimate). Prose, tables and code can look excellent. Embedded HTML is an allow-listed subset, and Mermaid will visibly differ in places.
- **No screen-reader accessibility:** dropped by the user unless useful in normal use. Keyboard navigation, selection and high-contrast colours stay; UI Automation and QAccessible bridges don't get built.
- **Double UI work:** every shell feature is built twice. A shared command table limits the drift.

## Architecture

```
crates/
  core/          files, session, recents, watcher, doc model, HTML mapping, autolink, find, Command table
  render/        layout cache, viewport, selection, glyph cache, blitters, GlyphRaster trait
  engine-proto/  job and reply wire format
  capi/          C ABI between the main exe and the Qt shell library (Rust on both sides)
bins/
  mdview/        entry: single-instance check, early thread, then dlopen Qt shell or start WinUI
  mdview-engine/ merman, RaTeX, resvg, syntect, two-face
  shell-winui/   windows-bindgen generated bindings (checked in), code-only UI, no XAML files
  shell-qt/      cdylib, CXX-Qt bridges for Widgets plus a safe wrapper module containing the unsafe
  mdv-bench/     headless render, PNG output, timing
xtask/           verify, perf, cross-check
fixtures/ spikes/ tests/golden/
```

### Linux shell (Qt Widgets through CXX-Qt)

- `extern "C++Qt"` declares existing classes; `#[base]` and `#[cxx_override]` subclass them. Estimated 1,000-2,500 lines of bridges plus a safe wrapper. `unsafe` stays inside the wrapper module.
- Cargo-only build with `cxx-qt-build`, no cmake. Uses `moc` and `g++`/`clang++` already installed. lld satisfies the linker requirement.
- Real Breeze `QStyle`, real `QMainWindow`, `QMenuBar`, `QTabWidget`, `QTreeView`, `QDockWidget`. QtWayland includes the global menu and fractional scale protocols.
- CPU backing store with no GPU startup. Paint in device pixels with a snapped origin, proved exact at 1.5, 1.7 and 2.2 by G2.
- `mdview` has no Qt in `DT_NEEDED`: it tries the single-instance socket first, so a second launch never loads Qt, then starts the read/parse/font thread and `dlopen`s the shell. The main exe owns core and render, and the shell reaches them through `capi`. If G1 shows this saves too little, drop it and link directly.
- After first paint: folder tree (small custom item model, not `QFileSystemModel`), theme icons, watcher. File dialogs through ashpd portals, loaded on first use.
- Keep the KDE platform theme; the agent's `qtdiag6` check suggests only about 5-15 ms and 17 MiB. Fall back to `generic` + Breeze via `QStyleFactory` if G1 shows over 30 ms.

**Memory limit (G1):** private memory under 45 MB applies at 1220x820 up to scale 1.5. Larger scales and windows are reported, not failed; measured so far, 47.6 MiB at 2.2 and 49.1 MiB in a nearly maximized window at 1.0.

### Windows shell (WinUI 3)

- **Route:** raw `windows-bindgen` on the `Microsoft.UI.Xaml` winmds, unpackaged, imperative code-only UI, Windows App SDK 2.6.x. `windows-reactor` is a reference only: copy its bootstrap and `IXamlMetadataProvider` patterns. It lacks wheel events, `ContextFlyout`, lazy tree events and automation peers.
- **Controls:** `TabView`, `MenuBar`, `TreeView`, `TextBox` find bar, `ContextFlyout`, Mica on chrome. Dark and light follow the system, and the renderer repaints on theme change.
- **Pixels:** `SwapChainPanel` with a D3D11 flip-model swap chain (`CreateSwapChainForComposition`, 2 buffers, `ALPHA_MODE_IGNORE`). Dirty rectangles via `UpdateSubresource`, with inverse scale from `CompositionScaleChanged`. Input stays on the UI thread first; independent input only if needed. A 1800×1200 window costs about 17 MB of swap chain plus the CPU buffer.
- **Reducing launch cost:**
  - Single-instance forward (named mutex + `WM_COPYDATA`) before any WinUI load.
  - D3D11 device created on a background thread.
  - First frame builds only tabs and panel; tree, outline, find bar and menu contents come after.
  - Both Windows App SDK 2.3.1 style optimizations on; Mica after first frame.
  - Optional later experiment: a GDI first frame in our own HWND before attaching an island. Kept only if there's no flash.
- **Target:** `x86_64-pc-windows-msvc`.

### Shared rules that deliver speed and low memory

- **No extra pixel copies.** CPU raster goes into the Qt backing store directly; painting only exposed strips when scrolling. On Windows, the CPU buffer holds what's uploaded.
- **One lazy engine process, not one per tab.** `mdview-engine` handles math, Mermaid, highlighting and SVG.
  - It starts only when such a block nears the viewport, and exits after 10 s idle, which returns its memory.
  - Each job has a 5 s deadline, with memory limits via `RLIMIT_AS` or a Job Object.
  - Code shows as plain text first, then gains colour. A hang or crash can't take down the viewer.
- **Virtualized layout.** Blocks live in flat `Vec`s with one `DocText` per document. Blocks have estimated heights, and a Fenwick tree gives Y positions. Only visible blocks are laid out, and scroll position is stored as `(block, offset)`.
- **Lazy tabs.** Restored tabs hold only a path. Background tabs drop layout and bitmaps, then the model after 60 s idle.
- **Hard caps.** Glyph cache 4 MB (LRU), raster cache 32 MB (LRU). System allocators, `M_ARENA_MAX=2` and `malloc_trim` on Linux, SegmentHeap on Windows. mimalloc only if measurements justify it.
- **Zero idle CPU.** OS file watching (notify 8.2.1) on parent directories so atomic saves work, started after first paint. Network shares that don't support watching poll at 2 s.
- **Fonts.** fontique walks all fonts at construction on Linux (3,039 here), so it runs on the early thread.
- **Safe by construction.** There is no HTML engine: embedded HTML passes through an allow-list tokenizer into the doc model, so nothing can execute.

**Boundary:** the shell owns the event loop. `Viewer` is UI-thread-only. Workers push to a queue and call one thread-safe `wake()` (`postEvent` / `DispatcherQueue`), and the UI thread drains results. Menus, shortcuts and status strings come from core's `Command` table. All FFI entries use `catch_unwind`.

**Dependencies:**

| Where | Crates |
|---|---|
| Main process | pulldown-cmark 0.13.6, parley/fontique 0.12, swash 0.2.10, memchr, unicase, regex-syntax (`unicode-perl` only, for the `\p{Word}` table in slugs), notify 8.2.1, png, zune-jpeg |
| Linux shell | cxx-qt 0.10.0, cxx-qt-lib, cxx-qt-lib-extras, cxx-qt-build, ashpd 0.11.1 (lazy) |
| Windows shell | windows-core, windows-link, windows-bindgen 0.100.0, Windows App SDK runtime 2.6.x |
| Engine only | merman 0.9.3 (no ELK, 8 families), ratex-* 0.1.15, resvg/usvg 0.49, syntect 5.3, two-face 0.5.2 |

Avoid the `windows` 0.62.2 crate, because its `windows-core` line doesn't match 0.100.

Qt is LGPLv3, dynamically linked, Linux only. `xtask verify` fails if ELK (EPL-2.0) appears in the dependency tree. Notices are generated from `cargo metadata`, plus OFL for KaTeX fonts.

**Release profile:** `lto="fat"`, `codegen-units=1`, `strip`, `panic=unwind`, engine crates at `opt-level="s"`, lld on Linux, no `target-cpu=native`. PGO in Phase 6.

## Budgets

**"Instant" definition:** process start to first presented frame of a 20 KB document at 1220×820, median of 10 runs after 2 warm-ups, on AC power.

| Metric | Linux: likely / aspiration | Windows (WinUI): likely / aspiration |
|---|---|---|
| Warm launch | 100-160 / 80 ms | 200-350 / 150 ms |
| Cold launch | 400-900 ms | 500-1500 ms (Defender excluded) |
| Empty window, private memory | 25-40 / 20 MB | 50-90 / 45 MB |
| 1 MB document | +15-25 MB, +30 ms | same |
| 10 tabs, one selected | +2 MB | same |
| Engine while alive | Math and highlighting 30-60 MB; Mermaid about 25 MiB on small diagrams, about 150 MiB on large ones, capped by the job memory limit; 0 after idle exit | same |
| Scroll CPU per frame, p99 | 4 ms at 150%, 8 ms at 4K | same |
| Idle over 60 s | about 0 CPU and wakeups | same |
| Second-launch handoff | under 20 ms | under 20 ms |

On Wayland at 4K, QtWayland's own shm buffers add 66 MB or more, which we can't avoid.

**Measurement:** `mdv-bench` drives `--measure --exit-after-present`. Linux memory comes from `/proc/PID/smaps_rollup`. `scripts/measure.ps1` does the same on Windows.

## Windows build and testing

**Cross-building here:** Arch's rustc has no Windows standard library, so:

- I install a private official toolchain under `~/.local/rust-win` (rustup with `--no-modify-path`, 1.99.1, target `x86_64-pc-windows-msvc`). It doesn't touch the system Rust.
- `cargo-xwin` 0.23.1 goes in that same private location. It downloads Microsoft's CRT and Windows SDK libraries, which means **accepting Microsoft's licence terms**. I'll ask for that specific confirmation when I reach this step.
- Self-contained staging needs a custom script, because Microsoft's helper expects Windows.

**What runs where:**

| On this machine | Only on the user's Windows PC |
|---|---|
| Cross-build, Rust core and render tests | All WinUI: tabs, menus, flyouts, theme |
| Core and render test exes under Wine (attempted; plain Rust, no XAML) | Swap chain, DPI |
| Headless render to PNG, goldens at each scale | Every launch and memory figure, clean-PC install |

**Windows VM (added at the user's suggestion):** the user's WinApps container is real Windows 11 build 26200 with the home folder shared. `winapps manual` launches programs with its own stored login, and results come back through `\\tsclient\home`. W0-W3 functional checks and screenshots run there. It has no real GPU and only runtimes 1.5-1.8 installed, so its launch, memory and rendering numbers are proxies, and the final figures and look (W4) still need the user's PC.

**User checkpoints (each is a build I hand over, with steps and `measure.ps1`):**

- **W0:** blank window, tabs, menu, tree, pixel panel. Reports launch, memory, a 1 px checkerboard at 1.5/1.75/2.25, wheel, `ContextFlyout`, lazy tree. Numbers are reported, not a gate. Failures here are bugs to fix.
- **W1:** text quality at 100/125/150/200%. If swash looks worse than native, add a DirectWrite `GlyphRaster` backend.
- **W2:** first real document with renderer and tabs.
- **W3:** full parity.
- **W4:** clean-PC install and final numbers.

## Phases

### Phase 0: Spike (throwaway code in `spikes/`)

Each gate records real numbers in `spikes/RESULTS.md`.

| Gate | Test | Pass | If it fails |
|---|---|---|---|
| G1 | CXX-Qt `#[base = QWidget]`: override `paintEvent`, mouse, wheel, drag with no authored C++; skeleton with menu, toolbar, tabs, 2 docks, canvas | All overrides work; under 160 ms; private memory under 45 MB at 1220x820 up to scale 1.5; `unsafe` contained | Stop and decide with the user |
| G2 | 1 px checkerboard at 1.5/1.7/2.2 at several offsets | Pixel exact | Own buffer at exact mapping, then a native child window |
| G3 | Rust text vs QPainter at 100/150/170/220%, with Turkish, CJK, emoji, Arabic. **User judges** | Equal or better at reading distance | Add FreeType `GlyphRaster` backend |
| G4 | Parse 1 MB, pulldown-cmark vs comrak; spec suites | Numbers recorded; pulldown misses at most 2% of GFM after autolink pass | Comrak behind `parse()` |
| G5 | merman and mermaid-rs-renderer on all 8 showcase families, through resvg | All 8 render with labels, each under 300 ms, no ELK | Per family: other crate, then port old layouts, then show source |
| G6 | Engine round trip, hostile TeX, memory bomb | First result under 150 ms after paint; hang killed at 5 s; viewer unaffected | Keep engine for Mermaid/SVG only; highlight in-process |
| G7 | parley on 1 MB, per block | Height pass under 500 ms off-thread; visible layouts under 8 MB | Estimated heights for longer, or an ASCII fast path |
| G8 | Cross-build WinUI skeleton with generated bindings; try launching it under Wine and record the result; deliver W0 | Exe links, and runs on the user's PC | Build natively on the user's PC |

**Go** means G1-G3 and G8's cross-build pass, plus one Mermaid route per family. Otherwise I stop and decide with the user. The repo is created and pushed here.

### Phases 1-6

1. **Core:** parser, HTML mapping, autolink, slugs, doc model, find. Exit: spec targets met; core tests ported; BOM and Unicode pass.
2. **Headless renderer:** all block types, selection, anchors, details, images, themes, zoom, and a designed look (reading width, type scale, code blocks, tables, alert callouts, dark-friendly diagrams). Exit: goldens approved by the user; 1 MB scroll budget met in `mdv-bench`.
3. **Qt shell parity:** Linux first, as the daily driver. Exit: every inventory row checked; scenarios pass offscreen and on Plasma.
4. **Engine integration:** math, 8 Mermaid families, highlighting, SVG, lazy start, timeouts. Exit: `fixtures/showcase.md` fully renders; idle exit frees memory; malformed input shows source.
5. **WinUI shell:** W1, W2, then W3. Exit: the user's sign-off at W3, with `measure.ps1` numbers recorded.
6. **Polish and packaging:** PGO, allocator check, PKGBUILD, Inno Setup installer chaining the runtime (compiled on the user's PC), optional self-contained ZIP, all four extensions, never set as default. Exit: budget table filled with real numbers; W4 clean.

Commits go to the private repo as phases complete. No CI is set up.

### Next: a lighter shell than Qt (decided direction, 2026-11-11)

**Why.** The user wants the lightest possible native shell. Both shells built so far are heavy
for a read-only viewer:

- **WinUI 3** (retired, on `winui-archive`): about 383 ms to first frame and about 180 MiB private
  memory for an empty window (HANDOFF section 27), against PLAN's 50-90 MB. It needs the Windows App
  Runtime installed system-wide, Windows only, and hand-generated and hand-trimmed bindings.
- **Qt 6** (current): about 37 MB of Qt and MSVC runtime DLLs next to the exe on Windows, a CXX-Qt
  build that needs `QMAKE`, `CMAKE_PREFIX_PATH` and a short `TEMP`, no SVG module in this Qt install
  (icons are rendered to PNG first), and a large style sheet to make it look modern. Qt release
  memory and first frame have not been measured side by side with the candidates yet.

**Candidates, compiled and run on the Windows PC** (`spikes/shell-fltk`, `shell-slint`,
`shell-soft`; screenshots in `spikes/shell-mockups/`). Each is a working layout A mockup: title row
with tabs, command bar, Files pane, page, Outline pane, dark theme, Segoe UI and Segoe Fluent
Icons, hover and press fills, pane toggles, tab and row selection, wheel scrolling, an overlay
scrollbar that widens on hover, a typing find box. The page is the stand-in image from
`spikes/look-shared`, not the current renderer. Single release runs at 100% scaling:

| | exe | private memory | first frame | UI text |
|---|---|---|---|---|
| **fltk-rs** 1.5.23 | 1.16 MB, one file | 9.5 MB | 43 ms | GDI + ClearType (native Windows look) |
| **winit 0.30 + softbuffer 0.4** | 2.65 MB | 14.7 MB | 51 ms | `mdview_render::TextEngine` through parley: the page's own text path |
| Slint 1.18, software renderer | 10.5 MB | 13.6 MB | 20 ms | blurry: grey-only, little hinting. Rejected by the user |
| Slint 1.18, Skia renderer | 18.8 MB | 321 MB (GPU), 80.5 MB (`SLINT_BACKEND=winit-skia-software`) | 17 ms | colour fringes. Rejected by the user ("horrible, fringed") |

The first softbuffer build used fontdue (grey, unhinted, no gamma): 0.71 MB but 62.9 MB private
and soft text, which the user also disliked. Switching to the project's text engine fixed both.

**Decision so far:** Slint is out (text quality cannot be configured: blurry in software, fringed
in Skia, and heavy with Skia). Two candidates remain:

- **fltk-rs (preferred if widgets matter):** real text box, menus, tree, dialogs and drag and drop
  for free; one small exe with no DLLs. FLTK's stock widgets look dated, so the mockup draws its own
  buttons, tabs and rows in `draw` callbacks and keeps FLTK's `Input` for the find box. Needs the
  MSVC environment (`vcvars64.bat`) and CMake to build; it is C++ but none is authored here.
- **winit + softbuffer:** pure Rust and the only option where UI text matches the page exactly.
  Every widget is hand-written (text editing, menus, tree, focus, keyboard, accessibility). The
  mockup handles only 100% scaling.

**Not solved in any mockup:** the native caption stayed dark grey; DWM attributes 35/36 (fltk) and
winit's `with_title_background_color` (softbuffer, Slint) did not colour it on Windows 11 build
26300. Light theme, DPI above 100%, menus, dialogs, the real renderer and Linux were not tried.

**Next steps:** have the user pick between fltk-rs and winit + softbuffer by running both
(`spikes/shell-mockups/README.md` has the commands); then put the real `mdview_render` page into
the chosen mockup, measure it against Qt release (exe, private memory, first frame, 1 MB scroll),
and only then port the shell. Qt stays the shipping shell until the replacement reaches parity.

Not considered, because they are heavier: egui/eframe (redraws every frame, usually a GPU context),
iced (wgpu), GTK4 (a large runtime on Windows). Tauri is ruled out because it is a webview.

### Roadmap from 2026-11-11: winit shell, then README fidelity, then renderer work

**Decided 2026-11-11:** the shell is winit + softbuffer (`bins/mdv-app`, binary `mdview`), every
control drawn through the page's own text path. Windows first; Linux is low priority. The user
approves the interface by looking at it; no stage of it counts as accepted otherwise.

Ordered by what the reader notices first, with each stage's gate. Stages B and C touch different
crates and can overlap once A is approved.

**A. Finish the shell port** (`bins/mdv-app`)

| Step | Work | Gate |
|---|---|---|
| A1 | Layout A chrome over `workspace.rs`: tabs, command bar, panes, find, menus, dialogs, toasts, empty state | Screenshots of every state; tests pass |
| A2 | User review of the look and feel | **The user's sign-off.** If rejected, the interface is redone |
| A3 | Checks by hand: real mouse and keys, drag and drop, single instance, file watching, a DPI change at runtime, maximize and snap | Each one run on the Windows PC and recorded |
| A4 | Tick `docs/PORT_CHECKLIST.md` for the new shell; measure against Qt release (exe, private memory, first frame, 1 MB scroll) | Budget table rows filled with real numbers |
| A5 | Installer builds `mdview.exe` and the engine with no Qt; update checks become manual only | Install, launch, uninstall on the PC |
| A6 | Retire the Qt shell on Windows (archive branch, as with WinUI) | Only after A2-A5. Done 2026-11-13: `qt-archive` branch, with the Qt Linux packaging |

**B. Make real READMEs look right** (mostly `crates/render` image loading and `crates/engine`)

| Step | Work | Gate |
|---|---|---|
| B1 | GIF (first frame, no animation, so idle CPU stays zero) and WebP decoding | Fixture and golden per format; size and pixel limits enforced |
| B2 | Remote images: fetched off the UI thread and off the launch path, 10 s timeout, 12 MiB and 24 Mpx limits, 64 MiB disk cache, retry rules from `core::status::ImageRequests`. Windows uses WinHTTP so the exe gains no TLS stack. **Decided by the user 2026-11-11:** off until asked. A per-document "Load remote images" bar with an "always" setting, because opening an untrusted file should not contact a server unasked | A README with badges renders; nothing is fetched when off; a slow or hostile server cannot stall the viewer |
| B3 | Mermaid: test each of the roughly 22 remaining families in merman 0.9.3 for labels, time and memory; enable the ones that pass; per-family size limits. ELK stays excluded | Coverage table lists every family as enabled or excluded with the reason; engine size recorded |
| B4 | Full GitHub emoji shortcode list (186 names today) | Table generated from gemoji; lookup test |
| B5 | Small fidelity items: `<ruby>` annotations above the base, `<dl>` and `<figure>` styling, copy a formula or diagram as its source | Fixture and golden each |

**C. Renderer performance and structure** (`crates/render`)

| Step | Work | Gate |
|---|---|---|
| C1 | Check whether a resize or zoom re-decodes images and re-requests math and diagrams (`layout()` clears the image map). If so, keep decoded images across layouts, keyed by source and size | Trace shows no decode and no engine job on a pure resize |
| C2 | Split `layout.rs` (2,839 lines) into measure, paint, hit-test and selection modules. Behaviour-preserving, done only because C3 needs it | Goldens and the byte-identical frame tests unchanged |
| C3 | First layout of a large document: estimated heights for off-screen blocks, refined as they are laid out (PLAN gate G7's fallback) | 1 MB first paint within +30 ms of a small document; scroll position stable while heights refine |
| C4 | A `<details>` toggle re-lays out one block instead of the page (falls out of C3's per-block heights) | Toggle under 1 ms on the 1 MB fixture |
| C5 | Text and bitmap: a clip rectangle in `TextEngine` and `Bitmap`, cheaper LRU bookkeeping, a greyscale mode for BGR and rotated panels, area-averaged downscaling for large images | Measured before and after; goldens reviewed where pixels change |

**D. Housekeeping**

- `docs/COVERAGE.md` still calls the renderer and engine "planned": re-verify each row on screen with a fixture, and rewrite the status column.
- Rewrite `docs/HANDOFF.md` around the new shell; move the WinUI and Qt sections to history.
- Linux: build and run the winit shell on KDE (Wayland file drops, portal dialogs, an icon font to replace Segoe Fluent Icons).

**Order:** A1-A2, then C1 (small, and it may be free), B1, B2, then C2-C4, B3, B4-B5, C5, with A3-A6
running alongside as soon as A2 passes. Self-contained pieces (B1, B3's test harness, B4) go to
cheaper models; B2, C2 and the interface review stay with the main session.

**E. After the roadmap: editing (noted 2026-11-11, not scheduled)**

The user wants, for personal use, an editor as fast and pleasant as the viewer: a Notepad
alternative with syntax highlighting. Not started until the reader has shipped (A3-A6 done).

1. Source view first: a toggle that shows the raw Markdown read-only, highlighted, with selection,
   find and copy. Reuses the page's text path and scrolling.
2. Then the editing core: text buffer, undo/redo, caret and selection editing, IME, encodings and
   line endings, safe save, large files. Every control is hand-drawn, so there is no toolkit text
   widget to lean on; budget weeks, not days.
3. Decided 2026-11-11: the editor is a second project with the same goals (light, fast, pleasant),
   not a mode of mdview. It is a separate binary that opens any text file and shares the shell,
   text and highlighting code, so the viewer's exe and memory budgets stay untouched. Before it
   starts, the shared parts of `bins/mdv-app` (window chrome, `gfx`, `textfield`, menus, theme,
   `platform`) move into a library crate both binaries use.
4. 2026-11-12: two private repositories hold the plans, and nothing else yet.
   `Kaan88/markdown-shared-rs` is where the shared crates will live (what moves, in what order,
   and the checks for each step); `Kaan88/markdown-editor-rs` is the editor, `mdedit` (goals,
   budgets, phases, open decisions). Both are checked out beside this repository on the PC. No
   code has moved and the user has not said to start.

### Scope to port

The full inventory is in the research output and gets copied to `docs/PORT_CHECKLIST.md` in Phase 1 as the parity checklist:

- Tabs and their context menu, folder tree, outline with sync, find (cap 10,000, Turkish İ/ı tested).
- Zoom 60-250, System/Light/Dark, focus mode, recents (15) and manage dialog.
- Auto-refresh and deleted-file handling, link rules, details, images (limits and 64 MiB cache), session restore, single instance, drag and drop.
- Limits: 16 MiB file, 64 KiB diagram, 4 KiB equation. Diagram line limits are set per family from measurements (see "Coverage").

Not ported (each gets a regression test): duplicate-parent tab labels, stuck "Refreshing…", timeout message overwrite, failed images never retrying, snap-to-heading on reload.

Reused from the old repo: `fixtures/showcase.md`, `linked.md`, `assets/badge.svg`, icons, and the single-instance test scenario.

## Coverage

Coverage means how much real-world Markdown renders correctly. The aim is that anything that renders on GitHub also renders here, with every remaining gap known and listed. Anything we can't render shows its source and a reason, never a blank or a silent drop.

- **Spec suites:** the target is no visible difference from CommonMark and GFM, with exceptions listed by example number. Known today:
  - Collapse nested `strong` in the doc model, since it looks identical (398, 426, 434-436, 473-477).
  - The HTML allow-list has to satisfy the tag filter (652).
  - The autolink pass must read the source so an escaped character isn't linked (614).
  - Bare URLs stay linked as on GitHub, so 610, 616, 619 and 620 are listed exceptions.
- **GitHub features:** in Phase 1, list what GitHub renders from its documentation, and mark each item supported, planned or excluded with a reason. The user reviews the excluded items.
- **Diagrams:** G5 covered only 8 families, but merman 0.9.3's source has about 30, including timeline, journey, gitgraph, C4, sankey, xychart and architecture. All of them are unverified. Phase 4 tests each one for labels, time and memory, and enables those that pass. Families that need ELK are excluded by licence unless the user decides otherwise.
- **Large diagrams:** merman stops at about 320-400 lines in 5 of 8 tested families on synthetic input. Look for a way to raise that, and set limits per family from measurements.
- **Embedded HTML:** the allow-list is driven by what real READMEs use, measured on a corpus of real documents in Phase 2.
- **Non-Latin text:** pure CJK and Arabic documents become performance and golden fixtures, with a named font per script in one shared policy across parley, merman/resvg and RaTeX.
- **Engine size:** every extra family grows the engine binary (unmeasured), but only the engine, which loads on demand, so launch and idle memory don't change.
- **Report:** `xtask verify` prints a coverage table of spec results and exceptions, diagram families, GitHub features and corpus results. A drop fails verification.

**Exit:** added to Phases 1, 2 and 4. Done requires the table to be complete, with every exclusion approved by the user.

## Verification

`cargo xtask verify` is the gate before every push, since there's no CI:

- **Unit and scenario tests:** old `core_tests` move to `core` and engine integration tests. Old `ui_tests` run against `Viewer` with a fake shell, plus Qt shell `--self-test` with `QT_QPA_PLATFORM=offscreen`.
- **Spec suites:** CommonMark and GFM examples with an explicit known-failures list.
- **Goldens:** rendered with bundled fonts at 100/150/200%, using small tolerance; native-font screenshots for human review.
- **Performance:** seeded documents (20 KB, 1 MB, 10 MB, wide tables, many images, deep lists, huge code). `xtask perf` fails on a regression over 15%.
- **Fuzzing:** HTML tokenizer and autolinker, with a time budget.
- **End to end on Plasma:** open `fixtures/showcase.md`, then exercise tabs, find, zoom, themes, focus mode, external file edits and deletion, second-instance handoff, and all three fractional scales.
- **Windows:** cross-build, then a WinUI `--self-test` plus a checklist run by the user at W2-W4. Anything not yet run on real Windows is reported as unverified.

**Definition of done:** the budget table holds real numbers on both OSes, every checklist row is checked on both shells, and any known limit is listed plainly.
