# Handoff

`mdview` is a Rust remake of `Kaan88/markdown-viewer-cpp`: a Markdown viewer with its own renderer,
**WinUI 3 on Windows** and **Qt Widgets (CXX-Qt) on Linux**, no webview, and no authored C++.

> **2026-11-11: WinUI is retired.** Qt 6 is the only shell, on Windows and Linux. The WinUI shell
> (`bins/mdv-winui`), its installer script and build script live on the `winui-archive` branch;
> sections about WinUI below are history. The Windows installer is
> `packaging/windows/mdview-qt.iss`, built by `packaging/windows/build-installer-qt.ps1`.

This document is the current state: what works, how to build and check it, the traps already found,
and what to do next. `docs/PLAN.md` is the full plan (budgets, phases, gates). Read this file first,
then the plan.

**Branch:** `main`, tracking `origin/main`; `phase2-renderer` and `winui-tabs-wip` are merged into it
and are history only. Check `git log --oneline -5` for where it actually is - this file is updated at
each milestone, so the commit that last changed it is the state it describes. There is no CI: tests are run by hand before a
push. Commit and push at milestones.

Working agreements (from `CLAUDE.md`): read this file before touching anything; commit and push as
phases or milestones complete, not continuously, but do push, because the user checks GitHub; there is
no CI, so run the tests yourself; `spikes/` is throwaway; report results plainly, including what was
**not** run; nothing counts as working on Windows until it has run on Windows.

**Where the work happens.** Up to 2026-11-08 the user worked on a Windows PC and tested Qt on a Linux
VM (section 5). From 2026-11-10 the work moved to the user's Linux desktop (CachyOS, KDE Plasma), with
Windows checks in a WinApps Windows 11 VM that has no real GPU. On 2026-11-11 a session ran on the
**physical Windows PC** (section 26): both shells were redesigned (command bar, tabs in the title
bar) and brought to parity there. **Next step:** the open items in section 12, then the Qt shell on
the Linux desktop (it has not run on KDE since section 26).

Sections 3, 4 and 6-10 were written on that Windows PC (Windows 11, RTX 4070 Ti SUPER, 3440x1440 at
96 DPI), and their numbers come from it unless they say Linux. Sections 13-25 are dated notes from
the Linux desktop, section 26 from the Windows PC; section 12 is the current to-do list. Where
sections 7 and 8 describe menus, the find title or the outline menu, section 26 supersedes them.

---

## 1. Where the project is on the plan

`docs/PLAN.md` has six phases (its "Phases" section).

| Phase | State |
|---|---|
| 0 spikes | closed |
| 1 core | closed |
| 2 headless renderer | closed 2026-11-08 (goldens approved) |
| 3 Qt shell parity | in progress; the checklist features exist and were redesigned on 2026-11-11 (section 26), but `docs/PORT_CHECKLIST.md` has not been ticked row by row and the new layout has not run on KDE, so the exit gate is not met |
| 4 engine | complete |
| 5 WinUI parity (W1-W3) | in progress; built, tested and redesigned on the physical PC 2026-11-11 (section 26); W1 text quality, launch/memory numbers and the checklist are not done |
| 6 polish and packaging | started early: PKGBUILD, AppImage (with self-update, which the user chose to keep), Inno installer compiled in the VM, PGO trial (not adopted) |

Phase 2 detail, kept for reference:

| Phase 2 item | State |
|---|---|
| every block type laid out and painted | done |
| themes, zoom, DPI scaling | done |
| images (PNG/JPEG, limits, inline) | done; SVG, math and Mermaid draw through `mdview-engine` |
| virtualized viewport, scroll budget | done: 0.2-2.6 ms per scroll frame (budget 4 ms at 150%, 8 ms at 4K) |
| selection, link/anchor hit-testing | done in the renderer and both shells; Ctrl+C copies the selection |
| `<details>` toggling | done: a summary click flips it in both shells; view state lives in the renderer |
| exit gate: goldens approved by the user | **passed** 2026-11-08: the user approved the four `showcase-*` renders, which `crates/render/tests/golden.rs` now compares on Windows |

Phase 4 is complete. Three slices run through `mdview-engine`: SVG through resvg, TeX math through
RaTeX and Mermaid through merman (flowchart, sequence, class, state, ER, gantt, pie, mindmap). The
fourth, syntax highlighting, is in-process: syntect colours fenced code in the renderer. The
showcase's badge, formulas, diagrams and coloured Rust block all draw, as do `fixtures/math.md` and
`fixtures/diagrams.md`.

The shells were built ahead of the plan's order (the user asked for the UIs early), so Phase 3 and 5
are "make the existing shells match the checklist" rather than "build them".

## 2. Repository

- `Kaan88/markdown-viewer-rs`, private, branch `main`.
- Recent commits: `git log --oneline -5`.
- Workspace members: `crates/core`, `crates/render`, `crates/engine`, `crates/engine-proto`,
  `bins/mdv-bench`, `bins/mdv-winui`. `crates/engine` is both a library and the `mdview-engine`
  binary. `bins/mdv-qt` is **excluded** from the workspace because it needs Qt to build; it has its
  own `Cargo.lock` and its own `target/`.
- `spikes/` is throwaway reference code and is excluded from the workspace.
- `target/` is git-ignored; the helper scripts in it (`vm.py`, `seed-lock.py`, `cap.ps1`) are **not
  committed**. Section 5 and section 9 describe what they do in case they have to be rewritten.

## 3. Build and test on Windows

Toolchain here: rustc/cargo 1.99.1, MSVC 2022, Qt 6.9.3 MSVC at `C:\Qt\6.9.3\msvc2022_64`.

```powershell
cd C:\Users\kaank\Desktop\mv
cargo test --release --offline --workspace     # 7 core, 11 fuzz, 113 regression, 3 spec,
                                               # 12 engine, 7 engine-proto, 10 render unit
                                               # (10 + 2 ignored deadline waits), 19 render integration,
                                               # 1 golden compare (Windows), 1 engine watchdog (ignored),
                                               # 2 winui args
cargo fmt --all --check
cargo build --release --offline --workspace    # includes target/release/mdview-engine.exe
cargo build --release --offline -p mdview-winui
```

The shells look for `mdview-engine` beside themselves or in a `target/release` above them, so build
the workspace (or set `MDV_ENGINE`) before opening a document with SVG or math.

**crates.io.** It was unreachable from this machine for a while (`schannel:
AcquireCredentialsHandle failed: SEC_E_NO_CREDENTIALS`) and works again as of 2026-11-08. With the
crates cached, `--offline` still builds. A stale local index can fail to resolve locked versions
(it once reported only `bincode` 2.0); the `Cargo.lock` entries are what count, so a build without
`--offline` refreshes the index.

For MSVC builds inside the sandbox, point `TEMP`/`TMP` at a writable directory or `lib.exe` fails
with access denied:

```powershell
$env:TEMP = "$PWD\target\tmp"; $env:TMP = $env:TEMP
```

The Qt shell builds separately (section 8).

## 4. Running the shells on Windows

**WinUI** (`target/release/mdview-winui.exe`):

| flag | meaning |
|---|---|
| `<file>` | document to open; without one, a built-in welcome page |
| `--light` / `--dark` | override the system app theme, for screenshots |
| `--zoom <x>` | page scale, 0.5-3.0 |
| `--trace` | write `target/release/mdview.trace.log` (beside the exe), one line per frame |
| `--measure` | first frame only, then write `measure.log` and exit 0 |
| `--no-shift` | send every frame whole instead of using the shifted frame |
| `--scroll <device px>` | start partway down the page |
| `--scroll-steps <n>` | after the first frame, walk down `n` 40 px steps, one frame each |
| `--select-test` | after the first frame, select the word "Markdown" and log the range and first rect to the trace |
| `--find-test` | type "Markdown" through the find path, step to the second match and log the mark counts |
| `--outline-test` | scroll to the "Diagrams" heading through the outline and log the heading count |
| `--copy-test` | select "Markdown", copy it and read the clipboard back |
| `--details-test` | after the first frame, toggle the first `<details>` and log the page height before and after |
| `--size <w>x<h>` | logical window size (default 1100x900) |

`--no-shift`, `--scroll`, `--scroll-steps` and `--size` exist so a capture or a measurement needs no
input injection; that matters, see section 9.

The frame trace reads like:

```
frame 1084x821: paint 0.5 upload 0.1 (band at 773 of 48 rows) present 0.3 total 0.9 ms
```

`paint` is the renderer, `upload` is everything sent to the GPU plus the back-buffer copy, the band
is what was sent, and `present` is mostly waiting for vsync.

**Qt** (`bins/mdv-qt/target/release/mdview-qt.exe`): `bins/mdv-qt` is excluded from the workspace, so
build it from its own directory:

```powershell
cd bins\mdv-qt
$env:PATH = "C:\Qt\6.9.3\msvc2022_64\bin;$env:PATH"
$env:QMAKE = "C:\Qt\6.9.3\msvc2022_64\bin\qmake.exe"
$env:CMAKE_PREFIX_PATH = "C:\Qt\6.9.3\msvc2022_64"
$env:TEMP = "C:\Users\kaank\Desktop\mv\target\tmp"; $env:TMP = $env:TEMP
cargo build --release --offline
```

Run it with Qt's `bin` on `PATH` so the DLLs resolve. Flags: `<file>`, `--light`/`--dark`,
`--zoom <x>`, `--scroll <px>` (start partway down, for captures), `--drop-test <path>` (sends one
`QDragEnterEvent` and `QDropEvent` through the window
and exits 0 when the dropped path was loaded; it exists because Qt uses OLE drag and drop on Windows,
so a posted `WM_DROPFILES` from another process is ignored), `--select-test` (drags over "Markdown"
through the real mouse path, prints the selection and rect count, exits), `--select-hold` (the same
but leaves the window open for a screenshot), `--link-test` (clicks the `#diagrams` link and
compares the scroll row with what `y_of` says) and `--details-test` (clicks the first summary and
compares the page height before and after).

## 5. Linux: the test VM

The user gave this box for Qt and Linux testing: `user@vm-host`, password not recorded, Debian 13
(trixie), 20 cores, 7.8 GB RAM. The user's X server is MobaXterm on the Windows host, which listens on
`0.0.0.0:6000`, so a VM window can also be shown on the user's own screen:

```sh
DISPLAY=windows-host:0.0 /mnt/.../target/release/mdview-qt fixtures/showcase.md
```

`ssh -X` from Windows does not work (no xauth cookie); use that direct TCP display.

`target/vm.py` (not committed, needs `paramiko`) runs commands and copies files over SSH; it reads
`MDV_VM_HOST`, `MDV_VM_USER`, `MDV_VM_PASS` (defaults `vm-host`, `user`, none):

```powershell
python target/vm.py run "uname -a"
python target/vm.py put C:\...\file.tar.gz /home/user/file.tar.gz
python target/vm.py get /tmp/shot.png target/shot.png
```

Installed packages: `qt6-base-dev` (Qt 6.8.2), `libfontconfig1-dev`, `libxcb-cursor0`,
`libxkbcommon-x11-0`, `xvfb`, `imagemagick`, `fonts-noto-cjk`.

**Upload and build** (the tree is a snapshot; re-run after changes):

```powershell
cd C:\Users\kaank\Desktop\mv
tar -czf target/mdv-src.tar.gz --exclude=.git --exclude=spikes --exclude=target --exclude=.dsh -C . .
tar -czf target/spec-data.tar.gz -C spikes/parser data        # the spec suites read this
python target/vm.py put target/mdv-src.tar.gz /home/user/mdview-src.tar.gz
python target/vm.py put target/spec-data.tar.gz /home/user/spec-data.tar.gz
```

```sh
python target/vm.py run "rm -rf ~/mdview && mkdir -p ~/mdview && tar -xzf ~/mdview-src.tar.gz -C ~/mdview && mkdir -p ~/mdview/spikes/parser && tar -xzf ~/spec-data.tar.gz -C ~/mdview/spikes/parser"
```

The VM's default `PATH` is already usable
(`/mnt/2b4bc8c0-2e4e-4fc1-b330-27648dcbbf49/cargo/bin:/usr/local/bin:/usr/bin:/bin`), so cargo and the
usual tools are found without doing anything. The trap is adding Qt to it from PowerShell:
`python target/vm.py run "... export PATH=/usr/lib/qt6/bin:$PATH"` in double quotes lets **PowerShell**
expand `$PATH` (to nothing), which leaves the remote shell with `/usr/lib/qt6/bin:` and no `cargo` or
`/usr/bin` at all. Either use single quotes in PowerShell so `$PATH` reaches bash, or spell the whole
path out:

```sh
export PATH=/mnt/2b4bc8c0-2e4e-4fc1-b330-27648dcbbf49/cargo/bin:/usr/lib/qt6/bin:/usr/local/bin:/usr/bin:/bin
```

Then:

```sh
cd ~/mdview/bins/mdv-qt && export QMAKE=/usr/lib/qt6/bin/qmake6 && cargo build --release
cd ~/mdview && cargo build --release -p mdview-engine      # for SVG, math and Mermaid
cd ~/mdview && cargo test --release -p mdview-core -p mdview-render -p mdview-engine -p mdview-engine-proto
                                                                      # 7 core, 11 fuzz, 113 regression,
                                                                      # 3 spec, 12 engine, 7 proto, 10+18 render
cd ~/mdview && xvfb-run -a /mnt/2b4bc8c0-2e4e-4fc1-b330-27648dcbbf49/target/release/mdview-qt \
    --drop-test fixtures/linked.md                                   # prints "drop test ok: ..."
cd ~/mdview && BIN=/mnt/2b4bc8c0-2e4e-4fc1-b330-27648dcbbf49/target/release/mdview-qt
xvfb-run -a $BIN fixtures/showcase.md --light --select-test          # "select test ok: ..."
xvfb-run -a $BIN fixtures/showcase.md --light --link-test            # "link test ok: ..."
xvfb-run -a $BIN fixtures/showcase.md --light --details-test         # "details test ok: ..."
xvfb-run -a $BIN fixtures/showcase.md --light --find-test            # "find test ok: ..."
xvfb-run -a $BIN fixtures/showcase.md --light --outline-test         # "outline test ok: ..."
xvfb-run -a $BIN fixtures/showcase.md --light --copy-test            # "copy test ok: ..."
```

The scripted checks print one line and exit; on this VM they report selection `7..15` ("Markdown"),
a link jump to 1266 px (Linux font metrics) and a details height of 3064 -> 3118 px.

Capturing a Linux window (`Xvfb` has no window manager, so the window sits at 0,0):

```sh
Xvfb :99 -screen 0 1600x1200x24 &
DISPLAY=:99 /mnt/.../target/release/mdview-qt fixtures/showcase.md & sleep 6
DISPLAY=:99 import -window root /tmp/shot.png
```

Linux font metrics differ from Windows (the showcase page is 4095 px tall there, 4071 here), so Linux
renders are for "does it run and look sane", not for golden comparison. CJK needed
`fonts-noto-cjk`; the renderer's stack falls back through fontconfig.

## 6. The renderer (`crates/render`)

Headless: no window system, BGRA bitmaps, parley for layout, swash for raster.

| Item | Meaning |
|---|---|
| `DocumentRenderer::new()` | font/layout contexts, glyph cache, per-block paint cache, decoded images, the frame; one per window, kept warm |
| `.layout(doc, &Options) -> PageLayout` | one height pass over every top-level block; also stores each block's `y`/`height` and **clears the frame** |
| `.view(doc, &Options, &PageLayout, top, height) -> Slice` | fresh viewport bitmap for one view; used by tests |
| `.view_frame(...)-> &Frame` | draws into the frame kept between calls (this is what the shells use) |
| `.frame() -> Option<&Frame>` | the frame `view_frame` drew last |
| `.page(doc, &Options) -> Rendered` | the whole page in one bitmap; for tests and small documents |
| `.hit_test(doc, &Options, &PageLayout, x, y) -> Option<Hit>` | the document offset under a point, plus the link, footnote or `<details>` summary there; lays out one block on demand |
| `.selection_rects(doc, &Options, &PageLayout, from, to) -> Vec<Rect>` | one page rectangle per selected line, for the shell to draw |
| `.set_details(block, open)` / `.details_open(doc, block)` | view state for `<details>`: a shell override, falling back to how the source was written |
| `render`, `render_slice`, `render_view`, `layout` | one-shot forms |

`Options { width, scale, theme, max_height, base_dir }`; `base_dir` is where relative image paths
resolve. `Slice { bitmap, top, total_height, stats }`.

**The frame delta is the contract the shells depend on.** `Frame` is `{ bitmap, top, stats, band,
shift }`:

- `band: Option<(u32, u32)>` - the rows of `bitmap` painted this time. `None` means the whole frame is
  new (first frame, resize, zoom/theme change, or a jump of a viewport or more) and nothing from the
  previous frame can be reused; `Some((0, 0))` means nothing changed.
- `shift: i32` - row `r` of this frame was row `r + shift` of the previous one.

Scrolling keeps the bitmap, moves the surviving rows with one `copy_within`, and paints only the band
that came into view. The band is painted on its own small bitmap and copied in, because a block can
reach past the band and would otherwise be blended twice on the overlap.

**Hit testing and selection live beside the frame, not in it.** Every laid-out text layout keeps one
`LineGeom` per line (top/bottom, its x range, the layout, the page origin and the `Inline` origin
map), cached per top-level block with the paints. `PageLayout` exposes `blocks()`, `block_at(y)`,
`index_at(y)`, `y_of(doc, block)` (walks up to the top-level block, so an anchor inside a quote
works) and `footnote_y(number)`. A hit finds the line under `y`, asks parley's
`Cluster::from_point` for the cluster and side, and maps the inline offset back to `Document::text`
through the origins; atoms (a footnote mark, an image placeholder) map every offset in their
generated text to the atom's single position. A selection asks `Cluster::from_byte_index` for the x
of the range boundaries, one rectangle per line. The frame bitmap is never modified by selection, so
`band`/`shift` still describe the document alone; a test checks that a frame after hit testing and
selection is byte-identical and still reports `Some((0, 0))`.

**`<details>` open state is view state.** `DocumentRenderer` keeps a per-block override map. A hit
that walks up from the text to a `Summary` whose parent is `Details` reports it in
`Hit::details`. `set_details(block, open)` clears the paint cache and the frame, because the
body's height changes; the shell then measures the page again (3-9 ms for the showcase, one height
pass). `details_open(doc, block)` answers with the override or `Document::details_open_as_written`.
The paint cache is per top-level block, but a closed details hides its body entirely, so the
current toggle is a full height pass rather than a one-block relayout.

Tests that pin this down (`crates/render/tests/render.rs`, 17 tests):

- `a_shifted_frame_matches_a_fresh_view` - five scroll offsets, images included, byte-identical.
- `a_scrolling_frame_reports_the_band_and_shift` - rebuilds each frame from the previous one using only
  `band` and `shift` and compares bytes; also checks that a viewport jump reports `None`.
- `a_cached_frame_still_draws_its_images` - fails if decoded images are not shared by the renderer.
- `a_click_maps_back_to_the_word_under_it` - a click at the word's rectangle returns the word's
  document offset; a click past the line lands at its last caret position.
- `a_click_finds_a_link_and_a_footnote_reference` - link index and destination, and a footnote mark
  swept across the line.
- `blocks_are_found_by_page_row` - `block_at` clamps at both ends, and a block nested in a list
  reports its top-level row.
- `hit_testing_and_selection_do_not_change_the_frame` - hit tests (including one on a block that was
  never painted) and selection leave the frame delta and bytes untouched.
- `a_selection_that_wraps_gives_one_rect_per_line` - a narrow page gives several rectangles, all
  inside the page.
- `a_details_summary_toggles_and_the_page_remeasures` - a click on the summary names its details,
  opening grows the page, and closing restores the exact height.

**Measured** (`spikes/parser/data/generated_1mib.md`: 1.0 MB, 25,060 blocks, 583,619 px page):

```powershell
cargo run --release --offline -p mdv-bench -- --input spikes/parser/data/generated_1mib.md `
    --out target/tmp/bench.png --width 780 --viewport 900 --scale 1.0 --scroll 40 --repeat 60
```

`--viewport <height>` switches the bench to the scroll-frame path (the width comes from `--width`);
`--scroll` is the starting offset and `--repeat` the number of 40 px steps. `--all` writes the four
`showcase-*` goldens.

| viewport | scroll frame, mean / worst |
|---|---|
| 780x900, 100% | 0.2 / 0.8 ms |
| 1170x1350, 150% | 0.6 / 1.1 ms |
| 3840x2200, 150% | 2.6 / 3.4 ms |

Height pass 3.7-9.3 ms, glyph cache 92% hit, bitmap is the viewport rather than the page. Both plan
budgets (4 ms p99 at 150%, 8 ms at 4K) are met with room.

### Phase 4 slices: SVG, math and Mermaid through `mdview-engine`

`crates/engine` is both a library (`mdview_engine`) and the `mdview-engine` binary. The library
renders SVG with resvg 0.47 (0.49 is not in the offline cache; the API used here is unchanged), loads
system fonts once per process, and clamps the raster to the renderer's 24 M pixel cap. Text falls
back through the renderer's sans stack when usvg's default serif fallback is unavailable, which is
what makes the badge's label appear on a minimal Debian install as well as on Windows.

Math is a second job kind. `KIND_MATH` carries a small header (font size in device pixels per em,
a display flag, the RGB text colour) and then the TeX source; the reply adds a **baseline** to the
usual `status/width/height/length` header. RaTeX 0.1.15 parses and lays out the formula, writes an
SVG display list with **embedded KaTeX fonts** (the `embed-fonts` feature), and resvg rasterizes it
through the same `render_svg` path. Two details matter: the paint syntax is RGB rather than RaTeX's
default `rgba()`, which is not valid SVG 1.1, and RaTeX sizes the root in `pt` while its viewBox is
in pixels, so the engine strips the unit or every formula comes out a third too large. The engine's
reply baseline is `padding + layout height * font size`, which is what puts inline math on the text
line instead of hanging below it.

The renderer turns an inline `$...$` span into one image atom, rendered at **twice the surrounding
text size** so it stays crisp when a heading, table cell, footnote or zoom enlarges it; the box and
baseline are halved back in `text_layout`. A `$$...$$` (or fenced ```math) block is a centred image
with normal paragraph spacing. If the engine is missing, RaTeX rejects the source, or the model marked
the equation oversize, inline math falls back to its styled source text and a display block falls
back to the old labelled code box.
Building up a table cell used to drop its inline boxes entirely; `cell_inline` now shifts and keeps
them, which is also what makes math and images inside tables work.

Mermaid is the third job kind. `KIND_MERMAID` carries a device scale, a dark-theme flag and the
diagram source. merman 0.9.3 is built with **only the eight families the showcase uses** plus SVG
(`diagram-flowchart`, sequence, class, state, er, gantt, pie, mindmap), not ELK, Cytoscape or math
labels; it emits SVG and `render_svg_at` rasterizes it at twice the device scale. Three output fixes
matter. Mermaid defaults to HTML labels inside `foreignObject`, which resvg cannot draw, so the
engine prepends an init directive asking for native text labels. Families that still emit HTML labels
(mindmap) get the `readable` pipeline's `<text>` fallback; that fallback sits in a nested `<defs>`
SVG behind a `<use>`, which resvg also skips, so `inline_label_uses` inlines the text at the use
site. Finally, `themeVariables.fontSize` is 11 px: merman measures labels deterministically, and the
engine's real font (DejaVu on Linux, Arial/Segoe on Windows) is wider, so full size overflowed node
widths. The fallback text from the readable pipeline still carried Mermaid's default
`font-size: 16px`, so `inline_label_uses` rewrites that to 11 px; at 16 the mindmap labels spilled out
of their circles and boxes. Gantt gets `leftPadding: 150`, because at the default 75 a section name
as long as "Documentation" ran under the first task bar, and `useWidth: 900`, because DejaVu's
`2026-11-02` ticks collided at the default 800. The inline copy is compressed to 85% around
its own anchor (`transform` on the `<text>`): DejaVu is wider than merman's deterministic metrics,
and the nested label SVG's `overflow: hidden` clipped the first and last letters. Label ids differ by
family (`mindmap-foreignobject-label-N`), so the matcher keys on that common suffix.

The dark job asks for Mermaid's `dark` theme with contrast overrides: `#3d4654` nodes, `#9aa4b2`
borders, `#c8d0da` edges, light text, and brighter gantt/pie variables. Stock dark drew `#333`
shapes on the `#202020` page, so outlines and bars nearly disappeared. The renderer turns a Mermaid
block into a **centred image**, shrunk to the content width; an oversize or unparseable diagram falls
back to the labelled source box. The image keeps its transparent background and is drawn straight on
the page in both themes.

### Syntax highlighting (in-process)

Fenced code is highlighted **in the renderer**, not the engine, because it is text: syntect 5.3 with
its bundled syntax and theme dumps (`default-fancy`, so no Oniguruma C build) turns the block into
byte ranges and foreground colours. The sets load once per process through `OnceLock`; the light
theme is `InspiredGitHub` and the dark theme `base16-ocean.dark`. `highlight::tokens` covers every
source byte, including newlines, so the renderer can push one styled run per token at its real
document offset: selection, find and hit-testing keep working across the coloured block. Unknown or
missing languages stay plain, and a block over 256 KiB is not parsed. Inline code keeps the theme
text colour; only fenced blocks are tokenized.

`Style` grew an optional token colour, applied as a parley brush after the theme's own colours. The
code box, line height and page geometry are unchanged; only the glyph colours differ.

The renderer never links it: `crates/render/src/engine.rs` keeps **one shared process** in a global
slot. The first SVG, formula or diagram starts it and a worker thread owns its pipes; later jobs
reuse it, so the font database is loaded once. Each job has a 5 s deadline. A pipe error (the engine's 10 s idle
exit, or a crash) invalidates the slot and the same job retries once against a fresh process; a
timeout also kills and reaps the process and returns a placeholder, so the next job starts clean. A
missing engine, a bad raster or a malformed job leaves the existing placeholder, so a hostile file
can slow one image, not the viewer.

`image::load` sends anything whose first tag is `<svg` (after an optional XML declaration) to the
engine; PNG and JPEG stay in-process. The binary is found through `MDV_ENGINE`, beside the current
executable, or in `target/release` beside an ancestor, which covers the Qt shell building into its
own `bins/mdv-qt/target/release`.

**Dependencies.** `bit-set`/`bit-vec` 0.8 (for `ratex-parser`'s `fancy-regex`), `quick-xml` 0.42
(merman) and `bincode` 1.3.3 (syntect) come from crates.io, pinned in both lock files with checksums
checked against the index. An earlier `vendor/` copy and its `[patch.crates-io]` pins, which existed
only to get past a stale offline index, were removed; the vendored sources were identical to the
published crates.

**Watchdog.** Besides the client's 5 s kill, the engine ends itself (exit code 3) when one job runs
past 6 s, so a parser that hangs after the viewer itself crashed cannot leave an orphan process.
Between jobs a closed stdin ends it, and 10 s idle does too. `MDV_ENGINE_STALL` makes every job hang,
which the client deadline tests and `crates/engine/tests/watchdog.rs` use (both `#[ignore]`d for
their waits; run them with `-- --include-ignored`).

Measured on this PC: two consecutive 180x28 badge jobs took **13.2 ms then 2.4 ms with one engine
start**; a deliberately stalled engine was killed at **5003.9 ms** and the next job started in
13.4 ms; after **11 s idle** the next job took 15.7 ms and started exactly one fresh process.
`fixtures/two-svg.md` (two badges) draws through `mdv-bench` in 24.8 ms. `fixtures/math.md` (seven
formulas including a matrix, a table and a heading) draws in **18.7 ms** on Windows and **14.7 ms**
on the VM, with one engine process for all of them; the shells show the same page. `fixtures/diagrams.md`
(flowchart, class, ER, mindmap and state) draws in **80.0 ms** on Windows and **59.9 ms** on the VM;
the full showcase with all eight diagram families takes about **161 ms** here (the first page pays syntect's one-time dump load). On the VM the
sharing checks report **5.2 ms then 0.8 ms**, a stalled job killed at **5003.0 ms** with a **3.6 ms**
respawn, and **3.7 ms** after 11 s idle. The idle and stalled cases are `#[ignore]`d tests, because
they wait out the real deadlines.

## 7. The WinUI shell (`bins/mdv-winui`)

WinUI 3 with hand-generated Windows App SDK bindings, imperative code only, no XAML files, no C# or
C++. Framework-dependent: it needs the Windows App Runtime 2.6.4 or newer (this PC has 2.6.5).
Window 1100x900, a `MenuBar` (File > Open..., Reload, Close; View > Light theme, Dark theme) over one
`SwapChainPanel`.

| File | What is in it |
|---|---|
| `main.rs` | COM apartment, `Application::Start`, argument parsing, `App` overrides |
| `ui.rs` | window, grid, menu, welcome page, input, the `Shell` state |
| `render.rs` | the GPU page textures, band upload, scrollbar gutter, `Frame` consumption |
| `bindings.rs` | generated bindings **plus** hand-written helpers (below); do not regenerate blindly |
| `bootstrap.rs`, `diag.rs` | Windows App SDK bootstrap, trace and `--measure` output |

**Pixels.** The page lives in a **pair of D3D11 textures** (`B8G8R8A8_UNORM`, `DEFAULT` usage, created
through a hand-written `create_texture_2d`). A scroll frame:

1. copies the rows that stay on screen from the texture holding the previous frame into the other one
   with `CopySubresourceRegion`, offset by `shift`;
2. uploads only the new band with `UpdateSubresource` and a `D3D11_BOX`;
3. copies the page texture into the back buffer, one full-screen GPU copy, because a flip-model swap
   chain does not keep its contents between frames - partial writes leave the other buffer showing;
4. uploads the scrollbar strip.

The scrollbar is **the shell's own**, 14 logical px in a gutter, because the checked-in bindings have
no `ScrollBar` control and cannot be regenerated without the NuGet winmds. It is a 15 px strip (a hair
line separator plus the bar) that is sent **whole** every frame, for the flip-model reason above. The
page can be a few px wider than the width it was laid out for, and the strip covers that overhang: the
bar has to sit at the panel's right edge, so placing the strip at the page's right edge instead clipped
the bar to 6 px.

**Input** arrives two ways, because WinUI moves it around:

- **Keys** come from XAML's routed events, bound by hand in `src/input.rs` (slot indices and IIDs
  read from `Microsoft.UI.Xaml.winmd` 2.6.4 with `ildasm`; see the module comment). The page panel
  is a tab stop and takes focus on `Loaded` and on every press. Its `KeyDown` handles scrolling and
  the other page keys; the root's `KeyDown` handles only Ctrl shortcuts, F3, and Escape while find is
  open, and Ctrl+F moves focus back to the page. `CharacterReceived` on the root feeds the find
  query. A control that uses a key marks it handled, and an open menu's keys never reach the page,
  so keyboard navigation of the menu bar works. This replaced a `WH_GETMESSAGE` thread hook that
  swallowed arrows, Space and Escape before XAML saw them. Checked with real keystrokes (SendKeys):
  Page Down scrolls; Tab, Right and Down open the Edit menu without scrolling; Ctrl+F, typing,
  Enter and Escape drive find from the menu bar.
- **The scrollbar** is driven by XAML pointer events (`PointerPressed`/`PointerMoved`/
  `PointerReleased`). The generator left those, and `IPointerPoint::Position`, as unbound vtable slots,
  so `bindings.rs` has hand-written `pointer_pressed/moved/released` and `IPointerPoint::position`.
  XAML reports the position in the panel's own logical px, so no window-coordinate maths is involved.

Keys: Home/End, arrows (48 px), Page Up/Down and Space (90% of the viewport). Mouse: a press in the
gutter jumps there and starts a drag; the drag follows the pointer.

**Selection and links.** A press on the page starts a selection through the same XAML pointer events;
moving extends it and release keeps it. If the pointer never moved, release follows the link or
footnote under the press, or toggles the `<details>` whose summary it is: a `#fragment` scrolls with
`PageLayout::y_of`, another local Markdown file is loaded and then scrolled to its fragment, and
`http(s):`/`mailto:` goes to `ShellExecuteW`. Escape clears the selection. The selection is
`Option<(u32, u32)>` over `Document::text`, and its rectangles are CPU-blended into the **back
buffer** just before the gutter strip, so the page texture - and with it `band`/`shift` - never
contains selection pixels. A details toggle drops the page layout, so the next frame measures it
again.

**Find.** Ctrl+F (or Edit > Find) starts find mode; `CharacterReceived` appends the
typed characters, Backspace deletes, Enter and F3 step forward and Shift+F3 steps back, and Escape
closes. The renderer runs core `find_visible` with its own `<details>` state, scrolls the active match
into view, and paints all marks (yellow) plus the current one (orange) through the same CPU overlay
as the selection. This shell has no text-field binding in its trimmed `bindings.rs`, so the query and
its `2 of 4` counter are shown in the **window title** (`mdview - file — Find: query [2 of 4]`)
rather than in a find box. `--find-test` feeds characters and F3 through the same key handlers and logs
`find test ok: 4 matches, 4 mark rects, 1 current rects, index 1`.

**Outline.** The trimmed WinUI bindings have no usable `ItemInvoked`/`TextBlock` support for a
sidebar, so the outline is a top-level **Outline menu**. `refresh_outline` clears and rebuilds it
from `Document::headings` on every load, indents children with spaces, and each item scrolls to its
block with `PageLayout::y_of`. `--outline-test` reports `15 headings, scrolled to 1242, expected
Some(1242.0)`.

**Clipboard.** Ctrl+C and Edit > Copy write the selected bytes through the Win32 clipboard
(`OpenClipboard`/`GlobalAlloc`/`SetClipboardData`), since the trimmed bindings have no WinRT
`DataPackage`. `--copy-test` selects "Markdown", copies and reads it back:
`copy test ok: Some((7, 15)), clipboard Some("Markdown")`.

**Hand-written additions in `bindings.rs`** (regenerating would drop them):

- `FrameworkElement::set_requested_theme` - the root grid is set Light/Dark with the page, otherwise
  the menu labels stay white on a light page.
- `ID3D11Device::create_texture_2d`, `ID3D11DeviceContext::copy_subresource_region`,
  `D3D11_TEXTURE2D_DESC`, `D3D11_USAGE_*`, `D3D11_BIND_SHADER_RESOURCE`.
- `IUIElement::pointer_pressed/moved/released`, `PointerPosition`, `IPointerPoint::position`.

**Measured** on this PC: first frame 308 ms from process start (`--measure`), height pass 8.4-9.6 ms
for a 1084 dip panel, parse 0.1 ms. In a 3384x1301 panel: a whole-frame upload is 2.6 ms, a 40-row
scroll band 0.0-0.2 ms, a scroll frame 1.7-4.1 ms end to end (the 3-6 ms "present" in the trace is
waiting for vsync).

**Verified by driving the running window** (real keys and clicks, plus posted messages): three Page
Downs reach the footnotes and the thumb reaches the bottom; a press in the gutter jumps and a drag
lands mid-document; File > Open... opens `GetOpenFileNameW` (class `#32770`) and typing a path loads
the file; Reload re-parses; View > Dark theme repaints page and menu bar; Close closes. The scrolled
view reached by six band frames is byte-identical, below the title bar, to the same offset drawn by a
fresh layout and sent whole (`--no-shift`), and the un-scrolled window is byte-identical to
`tests/golden/shell-winui-light.png`.

**Caveat:** after a *programmatic* resize from another process, the app needs one click before keys
reach it again - WinUI loses the XAML keyboard focus and `SetForegroundWindow` does not restore it. A
mouse resize keeps focus, and a click fixes it, so it is a focus matter, not a hook one.

## 8. The Qt shell (`bins/mdv-qt`)

Qt Widgets through CXX-Qt 0.10, ported from `spikes/qt-shell`: `src/qt/mod.rs` is the only module with
`unsafe`, `src/qt/ffi.rs` holds the C++ declarations, `src/shell.rs` is
`#![forbid(unsafe_code)]`. There is no authored C++. The clipboard is bound in `ffi.rs`:
`QApplication::clipboard()` through cxx's `#[Self = ...]` static members, and `text`/`setText` with an
explicit `QClipboard::Mode`, which picks the right overload because cxx names a member through a
pointer of exactly the declared signature.

**Handles.** `H<T>` is a copyable pointer, so it is only handed out for widgets that live until the
process exits (the window and its children, which this shell never deletes). Outline items are
deleted by every rebuild, so `qt::Tree` keeps their pointers to itself: `set_items` takes (level,
title) pairs, and a click arrives as an index into that list, which `shell.rs` maps to a block.
Checked by clicking: Mathematics scrolls there, then after hiding and re-showing the outline (a
clear and rebuild), Pie scrolls to Pie.

Menus File > Open... / Quit, View > Light/Dark theme, Help > About, a status bar with the file name and
block count. A file comes from the command line, File > Open... (`QFileDialog` built through `new_ptr`,
because `QFileDialog::getOpenFileName` cannot be bound - cxx takes its address and the C++ signature
has six parameters) or a drop on the window (`%20` decoded).

Scrolling: the wheel, a vertical `QScrollBar` in a `QHBoxLayout` (14 px), and the keyboard. The shell
owns the offset, sets the bar's range/page step/value while painting with a `syncing` flag so
`valueChanged` does not echo back, and a drag or a groove click arrives through `valueChanged`. The
canvas takes `Qt::StrongFocus`, which is what makes Home/End/arrows/Page Up/Down/Space work.

Painting: `paint` calls `view_frame`, then blits the frame bitmap through `QPainter` with an identity
transform (`blit_painter`); `blit_direct` can write into the backing store instead. Each paint prints
its cost:

```
mdview-qt: painted 1086x846 at scroll 96 in 0.7 ms, band 798+48, shifted 48
```

**Selection and links.** `mousePressEvent`/`mouseMoveEvent`/`mouseReleaseEvent` go to
`DocumentRenderer::hit_test`, and the selection is kept as two document offsets in `shell.rs`. It is
painted with a translucent `QPainter::fillRect` after the page blit, so the frame is still the
document only. A release without movement follows the link or footnote under the press, or toggles
the `<details>` whose summary it is: a `#fragment` scrolls with `PageLayout::y_of`, another local
Markdown file is loaded and scrolled to its fragment after layout, and `http(s)`/`mailto` goes
through `QDesktopServices::openUrl`. The cursor becomes a pointing hand over a link or footnote
mark. `--select-test`, `--select-hold`, `--link-test` and `--details-test` drive these through the
canvas's own mouse handlers for scripted checks. Those checks send events straight to the canvas:
a synthetic press through the `QWindow` does not establish Qt's implicit grab, so the release never
arrives that way, while `QApplication::sendEvent` on the canvas does reach `mouseReleaseEvent`.

**Find.** Ctrl+F opens an Edit menu item and a `QToolBar` with a `QLineEdit`, Previous, Next and
Close. Typing emits `textChanged`; the shell runs core `find_visible` with the renderer's `<details>`
state, scrolls the first match into view and paints every match yellow and the active one orange
after the page blit. Enter and Next step forward, Previous and Shift+F3 back, and the status bar
shows `Find: 2 of 4` (`+` when `FIND_CAP` stopped the count). `--find-test` sets the line edit's text
so the `textChanged` path runs, checks four marks and one current rectangle, and steps to index 1.

**Outline.** View > Outline toggles a left `QDockWidget` with a `QTreeWidget`. `Document::headings`
supplies level, title and block; levels nest into parent/child items and the tree is expanded, so the
showcase shows all 15 headings. `itemClicked` maps the clicked `QTreeWidgetItem` back to its block and
scrolls with `PageLayout::y_of`. The dock starts hidden, rebuilds on every load, and
`--outline-test` opens it, checks 15 headings and that scrolling to "Diagrams" lands on 1242 px
(1266 on Linux).

**Clipboard.** Ctrl+C and Edit > Copy pass the selection's byte range to `QClipboard`, and the status
bar says `Copied 8 characters`. `--copy-test` selects "Markdown", copies it and reads the clipboard
back: `copy test ok: "Markdown" at 7..15, clipboard 8 chars`.

**No band path here, on purpose.** The raster backend has no upload to save and `drawImage` only
touches the pixels the paint event exposes. Measured on Windows: 0.5-0.7 ms per 48 px scroll paint at
1086x846, and 2.0 ms at 3386x1327 (4.5 Mpx) - inside the 4 ms and 8 ms budgets. Cutting it further
would mean moving the backing store with `QWidget::scroll` before blitting the band, which is another
viewport-sized memmove on top of the renderer's own shift, so it keeps one blit.

A resize reflows: dragging the window to 3400x1380 relaid the page out in 0.9 ms and painted at
3386x1327 (a whole frame, 4.9 ms), then 48 px scroll steps stayed at 2.0 ms.

The crate emits seven `unnecessary unsafe block` warnings from the `pin!` macro; they predate this work
and are not errors.

## 9. Screenshots, goldens, and how to take them

`tests/golden/`:

| File | What it is |
|---|---|
| `showcase-{light,dark}@{1.0,1.5}.png` | the renderer on `fixtures/showcase.md`, from `mdv-bench --all` |
| `images-light.png` | `fixtures/images.md`: a real PNG drawn as a block and inline, plus placeholders |
| `shell-winui-{light,dark,images}.png` | the WinUI window at 1100x900, 100% DPI |
| `shell-winui-scrolled.png` | the same window after six 40 px scroll frames (`--scroll-steps 6`) |
| `shell-qt-{light,scrolled,open,images}.png` | the Qt window at the top, after a bar click, the Open dialog, images |
| `shell-qt-linux.png` | the Qt shell built against Qt 6.8.2 on Debian 13 under Xvfb |
| `shell-qt-1mb.png` | the Qt shell on the 1 MB generated document |
| `shell-qt-selection.png` | the Qt shell with "Markdown" selected (`--select-hold`) |
| `shell-winui-selection.png` | the WinUI window after a real press-drag-release over the heading |
| `shell-winui-anchor.png` | the WinUI window after clicking the `#diagrams` link |
| `shell-winui-details.png` | the WinUI window after `--details-test` opened the showcase `<details>` |
| `shell-qt-svg.png` | the Qt shell at scroll 760: the showcase badge is an SVG, drawn by the engine |
| `shell-winui-svg.png` | the same view in WinUI, with the engine running beside the shell |
| `shell-winui-two-svg.png` | `fixtures/two-svg.md`: both badges from the one shared engine process |
| `shell-qt-math.png` | the Qt shell on `fixtures/math.md`: inline, display, matrix, heading and table formulas |
| `shell-winui-math.png` | the same page in WinUI, one engine process for all seven formulas |
| `shell-qt-mermaid.png` | the Qt shell at scroll 1100: flowchart, sequence and class diagrams |
| `shell-winui-mermaid.png` | the same view in WinUI, one engine process for every Mermaid block |
| `shell-qt-highlight.png` | the Qt shell at scroll 500: the Rust block with syntect token colours |
| `shell-winui-highlight.png` | the same view in WinUI |
| `shell-qt-find.png` | the Qt find bar with "Markdown", four marks and `Find: 2 of 4` in the status bar |
| `shell-winui-find.png` | the same matches in WinUI, with query and counter in the title |
| `shell-qt-outline.png` | the Qt outline dock with all 15 headings, scrolled to Diagrams |
| `shell-winui-outline.png` | WinUI after the same outline jump, with the Outline menu in the menu bar |

**These are for review, not a pixel baseline yet.** They use this host's system fonts (Segoe UI,
Consolas), so another machine or Linux will differ; the plan calls for bundled fonts and a tolerance
before they become a suite. The four review images still have **not been approved by the user**.
Math and Mermaid changed the showcase page from 3020 to **4071 px** here (taller inline formulas and
diagram images instead of code boxes), so the four `showcase-*` files were regenerated with the
engine built; they are the current review copies. Syntax highlighting changed only the glyph colours
in the Rust block, so the page height stayed the same and the four images were regenerated again.

Renderer goldens regenerate byte-identically when `mdview-engine` is built beside the bench (SVG and
math need it):

```powershell
cargo run --release --offline -p mdv-bench -- --all
```

Windows window captures go through `target/cap.ps1` (not committed; `Add-Type` + `PrintWindow` with
flag 2 + `System.Drawing`; it restores and raises the window itself). The app has to be running, in a
job that outlives the command:

```powershell
$job = Start-Job { & "C:\Users\kaank\Desktop\mv\target\release\mdview-winui.exe" `
                    "C:\Users\kaank\Desktop\mv\fixtures\showcase.md" --light --trace }
Start-Sleep -Seconds 5
$p = Get-Process mdview-winui | Select-Object -First 1
powershell -NoProfile -File target/cap.ps1 -Target $p.Id -Out target/shot.png -WaitSeconds 1
Stop-Job $job; Remove-Job $job -Force; Get-Process mdview-winui -ErrorAction SilentlyContinue | Stop-Process -Force
```

For a window that has to be on top of other apps (for synthetic clicks), add
`SetWindowPos($h, [IntPtr](-1), 40, 40, 1100, 900, 0x0040)` first and check the point with
`WindowFromPoint`.

Traps that cost time here:

- **A GUI app started with `&` in a command that then finishes gets killed with the job.** Start it
  with `Start-Job { & $exe @args }`, keep the job alive while capturing, then `Stop-Job` and
  `Stop-Process`.
- `PrintWindow` returns a black or stale frame if the window is minimised: `ShowWindow(h, 9)` first.
- The window can be *behind another app* even when `SetForegroundWindow` succeeds, so a synthetic click
  lands on whatever is on top. `SetWindowPos(h, -1, ...)` (topmost) before clicking, and check with
  `WindowFromPoint` that the point is really ours.
- The capture includes the 31 px title bar, which follows Windows and differs between runs. Compare the
  client area, not the whole image: the page pixels are what the renderer owns.
- `Get-Process mdview-winui | Select-Object -First 1` can pick a stale instance; stop them first.

## 10. What has and has not run

**Windows:** the whole workspace test suite; the WinUI shell on `fixtures/showcase.md` and
`fixtures/images.md` with the wheel, real keys, scrollbar clicks and drags, both themes, `--zoom`,
`--trace`, `--measure`, a live resize (2000x1250, reflowed to 1984x1171 with a whole-frame upload),
and the shifted-frame path against a whole-frame reference; selection in both shells: Qt through
`--select-test` (selection 7..15 = "Markdown", one rect) and `--link-test` (scrolled to 1242, the row
`y_of` reports for the Diagrams heading), plus a captured `--select-hold`; WinUI through a real
press-drag-release on the running window (a drag from the heading into the next paragraph, two
selection rows) and a click on the `#diagrams` link, both captured; `<details>` toggling through
`--details-test` in both shells (4071 -> 4125 px, the body appearing) and a WinUI capture of the
open body. Find in both shells: Qt `--find-test` sets the line edit's text and reports
`4 matches, 4 mark rects, 1 current rects, bar true, index 1`; WinUI `--find-test` feeds WM_CHAR and
F3 through the key handlers and logs `4 matches, 4 mark rects, 1 current rects, index 1`; both
captured with the marks visible. Outline in both shells: Qt `--outline-test` reports
`15 headings, dock true, scrolled to 1242`, WinUI `15 headings, scrolled to 1242`, and both
captured. Clipboard: Qt copies 7..15 ("Markdown") and reads back 8 characters; WinUI logs
`copy: 8 characters` and `clipboard Some("Markdown")`. The Qt shell on the same files with layout,
first paint, wheel, scrollbar, keys, File > Open..., themes, `--zoom`, `--scroll`, a live resize to
3400x1380 and `--drop-test`. The SVG badge renders through `mdview-engine` in `mdv-bench` and both
shells; with `MDV_ENGINE` pointing at a missing file the same view falls back to
`[image: Native desktop badge]`. The shared client reuses one process across jobs (13.2 ms then
2.4 ms), kills a stalled one at 5003.9 ms and respawns in 13.4 ms, and replaces the idle-exited
process after 11 s in 15.7 ms; `fixtures/two-svg.md` draws both badges in 24.8 ms. In the running
WinUI shell with `two-svg.md`, `Get-Process mdview-engine` counted 1 process three seconds in and 0
after 15 s, while the window stayed open and both badges stayed drawn. Math renders in `mdv-bench` at
light, dark and 150% and in both shells (`fixtures/math.md`: inline, display, matrix, heading and
table formulas; one engine process for all seven), and falls back to source text or the labelled math
box with `MDV_ENGINE` missing. The table-cell image path that `cell_inline` used to drop is covered by
`a_table_cell_draws_its_inline_image`. Mermaid renders all eight showcase families through the same
engine process; `fixtures/diagrams.md` takes 80.0 ms and the full showcase 161 ms, both shells show
the flowchart/sequence/class captures above, and with `MDV_ENGINE` missing every diagram keeps its
source box. The fenced Rust block is tokenized in both themes in `mdv-bench` and both shells
(`shell-{qt,winui}-highlight.png`); unknown languages and blocks over 256 KiB stay plain.

**Linux:** the Qt shell built with Qt 6.8.2 on the VM, run and captured under Xvfb, `--drop-test`,
`--select-test` (7..15 = "Markdown"), `--link-test` (scrolled to 1266 px) and `--details-test`
(4095 -> 4149 px) passing, `--find-test` reporting four matches and four mark rects,
`--outline-test` reporting 15 headings scrolled to 1266, `--copy-test` reading "Markdown" back from
the clipboard, and `cargo test --release -p mdview-core -p mdview-render` passing
(18 render integration tests and the 10 render unit tests included). `mdv-bench fixtures/images.md`
renders the SVG badge at 780x413 in
11.4 ms with its text: the engine's font query now falls through Noto Sans, DejaVu Sans and the
other names in the renderer's stack because fontdb reports no generic `sans-serif` face there.
`mdv-bench fixtures/math.md` draws all seven formulas at 780x631 in 14.7 ms, `fixtures/diagrams.md`
draws the five families at 780x1722 in 59.9 ms, the full showcase with highlighted code and all
eight diagrams renders at 780x4095 in 114.8 ms, and the engine and protocol test binaries pass on
the VM (12 and 7).

**Not run:** the Qt View/Help menu items; the Qt scrollbar and keys on Linux (Windows only); DPI scales
other than 100% (zoom is tested, DPI scaling is not); a real OLE drag from Explorer (the drop test drives
Qt's event path instead).

## 11. Known gaps

- **Images:** PNG and JPEG decode in-process, SVG and math through `mdview-engine`; `<picture>`
  light/dark sources pick the source for the current theme; `width`/`height` on `<img>` are honoured
  (pixels or percent of the content width, aspect kept when only one is given, clamped to the content
  width). Any other format keeps its placeholder. A blocked image is retried on every render.
- No bundled fonts (the user's decision, 2026-11-08), so the golden compare runs on Windows only.
- Table `colspan`/`rowspan` are laid out (at most 200 columns). Highlighting covers fenced blocks
  only; unknown languages and blocks over 256 KiB stay plain.
- Math and diagram blocks are images: no selection geometry, no copy-as-TeX, no equation numbering.
- 1 MB documents: the first height pass runs on the UI thread (615 ms cold on Linux, section 15).
  PLAN gate G7's fallback (estimated heights off screen) is not done.
- Restored tabs are read and parsed at startup; PLAN wants restored tabs to hold only a path, and
  background tabs to drop layout after 60 s idle. Neither is done in either shell.
- Menus and shortcuts are written out in each shell; only the strings come from core's `commands`
  table, so the two shells can drift.
- **Qt:** Manage recent is a dock with a toolbar, not a dialog (no `QDialog` binding); no empty-state
  widget with its Open button; the Files dock is a one-time scan (at most 4000 rows, 8 deep) that does
  not follow folder changes. The window is frameless (section 26): it resizes only from the status
  bar's corner grip, not its edges, and has not run on KDE.
- **WinUI:** no Manage recent; no status bar; side panes have no title. The scrollbar is still drawn
  by the shell (now a rounded thumb, section 26). The MenuBar bug (section 24) no longer applies: the
  menu bar is gone.
- Scroll frames meet the budgets, but a frame still waits for vsync (3-6 ms in the trace on Windows).

## 12. What to do next

**State of the parity work** (Phase 3 Qt, Phase 5 WinUI), after 2026-11-11 (section 26). Both
columns ran on the physical Windows PC; the Qt column has not run on KDE since.

| Feature | Qt | WinUI |
|---|---|---|
| layout: command bar, tabs in the title bar | done (frameless window) | done (`ExtendsContentIntoTitleBar`) |
| tabs: close, close others, cycle, move, labels from core | done, `--tabs-test` | done, `--tabs-test` |
| tab context menu | done, clicked | done, clicked |
| page context menu (open/copy link, copy, find, reload) | done, clicked (was never connected before) | done, clicked |
| Recent, clear, missing entries | done (bar button) | done (bar button) |
| Manage recent | dock + toolbar | dialog (section 27) |
| session: tabs, active tab, theme, zoom, folder | done | done |
| theme System/Light/Dark (cycling button) | done; System reads Windows' setting on Windows | done |
| zoom 60-250 step 10, Ctrl+0 | done | done, `--zoom-test` |
| refresh F5 | done | done |
| auto-refresh, deleted-file handling | OS file watching (section 25) | OS file watching through the `DispatcherQueue` (section 27) |
| single instance | loopback port, wakes the GUI thread | loopback port before the bootstrap, about 20 ms (section 27) |
| focus reading mode | done | done |
| Files pane, Open folder | one-time scan, left | lazy `TreeView`, left |
| outline pane | right | right |
| find box with count, selection, copy, links, `<details>` | done | done |
| About and Keyboard shortcuts | dialogs with the logo | WinUI dialogs with the logo |
| drag and drop | done | built, not run by hand |
| installer / package | PKGBUILD, AppImage | Inno Setup 0.1.0, installed and uninstalled on the PC (section 27) |

**Done on the Windows PC (2026-11-11, section 27):** single instance before the bootstrap, the OS
file watcher, Manage recent, drag and drop, Ctrl+Shift+1/2, launch and memory numbers, the WinUI
column of `docs/PORT_CHECKLIST.md`, and a 0.1.0 installer that registers the Markdown extensions.

**Next for WinUI** (the checklist's open items, then budgets):

1. Text quality (W1) at 125/150/200%: needs the display scaling changed by hand.
2. First frame is about 380 ms and private memory about 180 MiB (section 27), both over PLAN's
   likely range; the memory was already there before layout A. Profile both.
3. Status bar texts, window size/minimum and geometry in the session, live folder tree, outline
   scroll sync, Ctrl+Q.
4. Try drag and drop by hand (built, not run).

**Update checks must be manual (the user's decision, 2026-11-10).** No check runs on its own: drop
the once-a-day check at launch in `bins/mdv-qt/src/appimage_update.rs` (`spawn_check` from `run` in
`shell.rs`) and keep only a Help menu item that checks and then offers to apply. The Windows shell's
manifest check (`crates/core/src/update.rs`, `docs/UPDATE.md`), when it is wired, follows the same
rule. Not done yet; the Qt shell still checks once a day inside an AppImage.

**Linux file association.** The PKGBUILD's desktop entry registers `text/markdown` and
`text/x-markdown` and never sets a default. Checked 2026-11-10 with the entry and binaries in a temp
`XDG_DATA_DIRS` folder: `gio mime text/markdown` lists `mdview.desktop`, and `gio launch` with two files
opened both in one window. The package itself has never been installed with `pacman -U`, and Dolphin's
"Open with" was not looked at. `.mdown` files are `text/plain` in shared-mime-info, so they are not
offered (`.md`, `.markdown` and `.mkd` are); a small `mime/packages` XML would fix it. The AppImage
carries the same entry but registers nothing by itself: that needs Gear Lever, AppImageLauncher or a
hand-written `~/.local/share/applications` entry pointing at the image.

**Afterwards, on Linux:** tick the Qt column of `docs/PORT_CHECKLIST.md` (the Phase 3 exit gate), the
Qt empty state, a Files tree that follows folder changes, and the end-to-end Plasma run that PLAN's
"Verification" section lists. Bundled fonts stay deferred until the user decides.

## 13. Packaging (Linux)

`packaging/arch/PKGBUILD` builds from the working tree (no download): `cd packaging/arch &&
makepkg -f` (about 3.5 minutes) gives `mdview-0.1.0-1-x86_64.pkg.tar.zst` holding `/usr/bin/mdview`
(the Qt shell), `/usr/bin/mdview-engine` (found beside the viewer) and a desktop entry that lists
Markdown as an openable type and never sets a default. Built here on 2026-11-10 with its
`single` unit test as `check()`; makepkg warns that the binary mentions `$srcdir` (build paths in
panic messages). Not installed or launched from the package yet. Still open for Phase 6: PGO, the
allocator check, the Inno Setup installer and the all-four-extensions Windows registration.

## 14. Updates and installers (foundation only)

Written 2026-11-10 by two delegated agents and reviewed here; **none of it has been run end to end**.

- `crates/core/src/update.rs`: `Version`, strict line-based `UpdateManifest`, `newer`, SHA-256 verify
  (own implementation, 10 unit tests pass). No network code; the shells will download.
- `docs/UPDATE.md`: the design for both platforms and the manifest format.
- `packaging/windows/mdview.iss` and `build-installer.ps1`: Inno Setup script with a per-user
  install, an optional file-association task (ProgID `mdview.Markdown` under each extension's
  `OpenWithProgids`; it never writes an extension's default value), and a Windows App Runtime 2.6.4
  check that downloads Microsoft's installer and checks its signature. Never compiled: no
  `ISCC.exe` here, and the Inno 7 syntax is unchecked.
- `packaging/appimage/`: `build-appimage.sh` (`--appdir-only` works and its `usr/bin/mdview
  --tabs-test` passes offscreen), `update-check.sh`, desktop file, placeholder icon. linuxdeploy and
  the Qt plugin are not installed, so no AppImage has been built; `tools.sha256` is all `TODO`.
- Qt AppImage self-update is wired (`bins/mdv-qt/src/appimage_update.rs`). **To change:** the user
  wants update checks manual only (section 12); the automatic daily check below is to be removed.
  Today: when `APPIMAGE` is set
  and `appimageupdatetool` is on PATH, a launch runs `--check-for-update` on a worker thread at most
  once per 24 h (stamp in `$XDG_STATE_HOME/mdview`), exit code 1 puts "A new mdview is available" in
  the status bar, and Help > Update mdview now applies the delta. Opt out with `MDVIEW_NO_UPDATE_CHECK`
  or `~/.config/mdview/no-update-check` (no settings toggle yet). Checked with a fake tool: it is run
  once per day, again once the stamp is stale, and never with the opt-out file. Not checked against a
  real AppImage, release or `appimageupdatetool`.
- Still to do: the manifest-based update check for the Windows shell (`update.rs` is ready, the
  shell has no HTTP client and no settings), the installer run on Windows, and an actual AppImage
  build.

## 15. Linux budget numbers (measured 2026-11-10)

CachyOS, Qt 6.12.0 shell built `--release`, **offscreen** platform (no compositor, so real Wayland
memory will be higher), AC power, desktop not otherwise idle. The 20 KB document is the first 20,000
bytes of `generated_1mib.md` (plain prose); the PLAN's "instant" definition is process start to the
first `painted` line.

| Metric | Measured | Plan (likely / aspiration) |
|---|---|---|
| Warm first paint, plain 20 KB | **119 ms** median of 10 after 2 warm-ups (112-179); was 171 ms before the font-sharing fix below | 100-160 / 80 ms: inside the likely band |
| Same, showcase x5 (diagrams, math, syntect) | 418 ms (was 462) | not a plan row; shows the engine and highlighter cost |
| Private memory, plain 20 KB, 3 s in | 37.0 MB | 25-40 empty window: inside, with a document |
| 1 MB document, first paint / private | 661 ms / 40.3 MB (was 747) | +15-25 MB, +30 ms: **not met** on time |
| Scroll frame, 780x900 @100% | 0.2 mean / 1.6 worst ms | 4 ms p99 |
| Scroll frame, 1170x1350 @150% | 0.3 / 1.8 ms | 4 ms p99 |
| Scroll frame, 3840x2200 @150% | 2.0 / 3.7 ms | 8 ms p99 |
| Idle CPU over 10 s | 0.00 ticks | about 0 (the 800 ms poll was replaced by OS file watching on 2026-11-10, section 25; 0 ticks over 10 s again) |

**What was wrong with launch.** `DocumentRenderer::new()` built a parley `FontContext`, which
enumerates system fonts (about 35 ms), and the Qt shell made three of them before the first paint:
one in `Shell`, one per `Page`, and one more through `mem::take`'s `Default`. They now clone one
per-thread context (`shared_fonts` in `crates/render/src/layout.rs`), so only the first pays. The
outline tree is also filled only while it is showing. `MDVIEW_STARTUP_TRACE=1` prints the phase
timings (QApplication, `build`, `open`). What remains of the ~119 ms: Qt start, about 25 ms of
`build` (docks, toolbars, menus), the one font enumeration, and the first paint.

**The 1 MB number is the first height pass**, 615 ms cold on the UI thread (the bench's 4-9 ms is a
warm repeat). PLAN gate G7 wanted under 500 ms off-thread; the fallback it names is estimated
heights for blocks off screen, refined as they are laid out. Not done. Not measured: cold launch,
10 tabs, second-launch handoff time, a real Wayland or X11 window.

## 16. PGO trial (Qt, Linux, 2026-11-10)

Instrumented build, trained on the 1 MiB doc, showcase, math/diagram/image fixtures and the scripted
checks, then rebuilt with `-Cprofile-use`. Painted-frame time on the 1 MiB doc: 5.2-8.0 ms normal vs
5.5-5.9 ms PGO, i.e. no measurable difference in what the offscreen run can see. Cold layout time was
not compared. Not adopted; the PKGBUILD and normal builds stay as they are.

## 17. WinUI driven on the VM desktop (2026-11-10)

`~/mdview-winchk/ui.ps1` (run through `ui.cmd`) launches the app in the VM's interactive session, opens
each menu through UI Automation `ExpandCollapse`, sends real keystrokes with `SendKeys`, and saves
screenshots. Results: every menu (File, Recent, Edit, Outline, View) opens and lists its items, so
the earlier "menus broken" reports were not reproduced with scripted opens. Ctrl+plus changed the page, Ctrl+0
restored it exactly, F5 kept the app alive, Ctrl+F put Find in the title. Found and fixed: the Recent
menu doubled its entries (`Clear` on the collection does not empty it; now removed one by one).
Mouse clicks on menus were not tried; the windows show on the user's desktop while a script runs.

## 18. Inno Setup 7.1.1 compile and install test (2026-11-10)

The user installed Inno Setup 7.1.1 in the VM (`C:\Program Files\Inno Setup 7`). `ISCC /DAppVersion=0.1.0
packaging\windows\mdview.iss` compiled cleanly on the first try (no warnings, 9.4 MB
`target\installer\mdview-setup-0.1.0.exe`). A silent install with `/TASKS=assoc` in the VM
(`~/mdview-winchk/inst2.cmd`, needs `start /wait` because Setup is a GUI program) put both exes under
`%LOCALAPPDATA%\Programs\mdview`, created `HKCU\Software\Classes\mdview.Markdown` with the open command,
added the ProgID under `.md\OpenWithProgids` and left the `.md` default untouched; the silent uninstall removed
the keys (`unins000.exe` removes itself a moment later). Not run: the interactive wizard, the runtime
download branch (the runtime is already installed there), upgrade over an older version,
`build-installer.ps1` end to end, and what Windows' "Open with" list then looks like.

## 19. Qt page context menu (2026-11-10)

Right click on the page (`canvas_menu` in `shell.rs`, `H<QWidget>::on_context_menu_at`): over a link it offers Open
link and Copy link address; with a selection it adds Copy; elsewhere nothing. The existing scripted checks still
pass (tabs, select, link, details, find, outline, copy, drop). The menu itself was never opened: the offscreen
platform cannot show a popup and no check synthesises a right click, so only the build is verified. Not done:
WinUI has no link context menu.

## 20. AppImage built and run (2026-11-10)

With the user's go-ahead the tools (linuxdeploy, its Qt and AppImage plugins, appimageupdatetool) were
downloaded into the job temp folder and `packaging/appimage/build-appimage.sh` produced a 58 MB
`mdview-0.1.0-x86_64.AppImage` plus `.zsync` and `SHA256SUMS`. Fixes the build needed: a qmake wrapper
that hides KDE's `kimg_*` image plugins (one needs a missing `libjpegxr`), `EXTRA_PLATFORM_PLUGINS`
(Wayland and offscreen), and a real `AppRun` script (the default symlink made Qt look for plugins in
the wrong directory when mounted). `tools.sha256` now holds the hashes of those downloads. Run mounted
under FUSE with the offscreen platform: select, tabs, link, copy and find checks pass, and
`appimageupdatetool` reads the embedded update information (GitHub releases, `Kaan88/markdown-viewer-rs`).
Not done: running on a clean machine without the Arch Qt, a real Wayland/X11 window from the image,
and the update itself (it needs a published GitHub release; today's 404 is expected). The bundle also
carries Qt Quick/Pdf/VirtualKeyboard libraries it does not need, which is size to trim.

## 21. WinUI Files pane (2026-11-10)

A `TreeView` in a new left column (0 px wide until a folder is open, then 280 px). File > Open folder
(Ctrl+Shift+O, `SHBrowseForFolderW`), View > Files panel (Ctrl+B), `--folder DIR` for scripts. Folders first, then
`.md/.markdown/.mdown/.mkd`, hidden names skipped; a folder's children are read when it first expands
(`Expanding`), a click on a file opens it in a tab (`ItemInvoked`). The folder is saved in the session and
restored (an explicit `--folder` wins and is not saved). `ItemInvoked` is not in the trimmed bindings, so
`src/tree.rs` calls it by slot index (ITreeView 7/8, args IID `1a05853c-b101-542c-9a1e-775a044c4652`), read from
`Microsoft.UI.Xaml.winmd` 2.6.4 downloaded from NuGet; `Grid.SetRowSpan` was un-trimmed in `bindings.rs`.
Checked on the VM through UI Automation: a folder with a subfolder, two Markdown files, a `.txt` and a hidden
file showed `sub, a.md, b.markdown`; expanding `sub` added `deep.md`; invoking `a.md` opened it in a tab; a
screenshot shows the pane beside the tabs and page. The earlier tabs/zoom/session/single-instance checks
still pass. Not done: keyboard navigation in the tree, dragging the pane wider, a click on a folder row
(the chevron expands it), real mouse clicks (UIA invoked the row).

### 20a. AppImage follow-up (2026-11-10)

The PDF image plugin and the on-screen keyboard plugin are left out of the bundled plugin tree, which removed Qt
Pdf/Quick/Qml: 58 MB -> 45 MB; select, tabs, link, copy, find, outline and details checks pass on the slimmer
image. The image was also launched for 8 seconds each with `QT_QPA_PLATFORM=wayland` and `=xcb` on this
Wayland (XWayland :0) desktop and painted its first frame both ways (trace lines only; no screenshot was
taken). Still not done: a clean machine without this Arch Qt, and an update from a published release.

## 22. Real mouse clicks on WinUI (2026-11-10)

`~/mdview-winchk/mouse.ps1` and `flaky.ps1` move the VM's cursor and send real button events (`SetCursorPos` +
`mouse_event`) at the UI Automation rectangles. Results: View > Dark theme saved `theme=2`; a click on a
Files-pane row opened the file in a tab; Edit > Find put Find in the title. Popup reliability over five
clicks each: File, Edit, View, Outline and Recent showed their popup items 5/5 (25/25). One earlier single
run did not find the View popup right after launch, so the first click after a cold start is the one case
that has failed once; the user's intermittent "menus don't show" report is therefore not reproduced here
but not ruled out. These runs are in the Windows 11 VM over RDP; nothing was run on a physical PC.

## 23. AppImage built in Ubuntu 24.04 (2026-11-10)

`packaging/appimage/build-in-container.sh TOOLS OUT` builds the AppImage in `ubuntu:24.04` (Qt 6.4, glibc 2.39)
so it is not tied to Arch's glibc 2.43. Needed: `QStyleHints::colorScheme` (Qt 6.5+) is no longer bound
through cxx-qt; `system_is_dark()` finds it with `dlsym` and reports light on older Qt. `build-appimage.sh` falls back to
`libqwayland-generic.so;libqwayland-egl.so` when `libqwayland.so` is missing. `lld` must be installed in the container:
`qt-build-utils` picks the linker by looking for `lld`, falls back to `gold`, and `gold` drops cxx-qt's
`cxx_qt_init_crate_*` symbols (link error). `DOCKER_LIMITS` and `JOBS` env vars limit CPU on a busy machine.
Result: 35 MB image; the highest glibc symbol is 2.39 (`mdview`), 2.35 for the engine, 2.38 in the bundled libs,
GLIBCXX 3.4.30. Launched 8 s each with offscreen, wayland and xcb on this Arch desktop: each painted its first frame
(trace lines only; no screenshot). Wayland logs `Failed to load client buffer integration: wayland-egl` and
`Creating a fake screen` (software painting works; the EGL integration plugin is not bundled). Not done: a real
Ubuntu 24.04 machine or container run of the image, dark-scheme behaviour on Qt 6.4 (only the fallback is exercised), an
update from a published release.

## 24. Windows handoff and the menu bug (2026-11-10)

`docs/WINDOWS_HANDOFF.md` is the checklist for a physical Windows PC. Added View > System theme (WinUI; saved as
`theme=0`). Full workspace test on the VM passes, including the golden compare; the Qt shell builds and
`--tabs-test` passes there. Found and bisected (not fixed) a MenuBar problem: after one menu item is used,
later item clicks and keys do nothing; theme work, session save, redraw and deferral were each ruled out. It
may be a VM artifact; see section 3 of the Windows handoff. `system_is_dark()` in the Qt shell is Unix-only (dlsym);
Windows returns light.

## 25. Review of the 2026-11-10 work, and file watching in the Qt shell (2026-11-10)

A review against PLAN found the 38 commits from `0d4d160` to `d25e217` sound but off the plan in a few
places; sections 1, 2, 11 and 12 were rewritten from it. The user kept the AppImage self-updater,
which PLAN does not list.

**File watching.** `crates/core/src/watch.rs` (notify 8.2.1, as PLAN pins) watches the parent
folders of the open files, non-recursively, so an editor's save-by-rename is seen; events for other
files are ignored and a burst costs one wake. The Qt shell's 800 ms timer is gone: the watcher, the
single-instance listener (`single::listen_waking`) and the AppImage update thread call a waker that
queues `CanvasHandler::woken` onto the GUI thread through CXX-Qt `Threading` on `Canvas`. If the OS
watcher cannot start, a 2 s poll runs instead. The file set is re-aimed in `save_session`, which runs
after every open, close and move.

Checked on this Linux desktop (offscreen): the `watch` unit test (rename wakes, unrelated files do
not, delete wakes, nothing after unwatching); an end-to-end run with a real window process: a
save-by-rename of the showing tab reloaded it, a background tab's change reloaded it, a delete kept
the text, a re-created file reloaded, an unrelated file in the folder did nothing, a second launch
handed its file over (that process took 41 ms end to end, over the 20 ms budget, because it loads
Qt's libraries before checking), and CPU over 10 idle seconds was 0 ticks. The workspace tests and
the Qt `--tabs/find/outline/select/details/copy-test` checks pass. In the WinApps VM,
`cargo test --release --workspace` passes as well (the `watch` test included, so notify's Windows
backend sees the rename, the delete and nothing else; golden compare and WinUI build included). Not done: network shares (PLAN's
2 s poll for them is not separate from the fallback), the WinUI shell (still polling, section 12),
a real Plasma window.

**Clean-up.** At the user's request the work leftovers were deleted on 2026-11-10: the WinApps VM's
`C:\mv` build copy and its test files, `~/mdview-winchk` (the VM scripts sections 17, 18 and 22 name:
`ui.ps1`, `mouse.ps1`, `flaky.ps1`, `inst2.cmd`; those sections describe what they did if they are
needed again), the makepkg and AppImage outputs, the Ubuntu images used to build the AppImage, and
ignored build output under `spikes/`; in the VM also the downloaded installers in `%USERPROFILE%\tools`
and the loose scripts in the profile folder, and on Linux the test session in `~/.local/share/mdview`.
Kept on purpose: the build caches (`target/`, `bins/mdv-qt/target/`, about 2.3 GB), `~/.local/rust-win`,
and the VM's toolchains (Rust, VS Build Tools and Windows SDK, Qt 6.9.3 in `C:\Qt`, Inno Setup 7,
Windows App Runtime 2.6.5).

**AppImage rebuilt (2026-11-10, after the clean-up).** The clean-up had also deleted the only built
AppImage, which lived in the job's temp folder. Rebuilt with `build-in-container.sh` (tools
re-downloaded, all three matched `tools.sha256`) into **`~/mdview-appimage/`**:
`mdview-0.1.0-x86_64.AppImage` (36.7 MB, includes the file watching above), `.zsync` and
`SHA256SUMS`. That folder is outside the repo and any temp folder; keep built images there. Checked
offscreen: the tabs, find, select, details, outline and copy checks pass, a save-by-rename reloads,
and a second launch hands its file over. The tools and the `ubuntu:24.04` image were removed again
afterwards; a rebuild downloads them (the tools are checked against `tools.sha256`).

## 26. Layout A and side-by-side parity, on the physical Windows PC (2026-11-11)

Commits `0e8ee6d` and `09c4371`. The first session on the physical PC (Windows 11, RTX 4070 Ti
SUPER, 3440x1440 at 100%). Before any change, the workspace tests passed on it.

**Design.** Mockups were drawn and the user picked **layout A**: no menu bar; the logo and tabs in
the title bar; one command bar under it (Open, Reload, Recent | Files, Outline | zoom on the left;
find box with match count, previous/next, Theme, a main menu on the right). The Qt shell uses the
same layout with the tabs in its own title row. An Outline dropdown was tried and dropped at the
user's request; the outline is a pane on the right in both shells, Files on the left (as in the
original app).

**WinUI.**
- Controls the trimmed `bindings.rs` lacks come from a second generated module,
  `src/bar_bindings.rs`, written by `bins/mdv-winui/gen` (windows-bindgen 0.100 over the Windows App
  SDK 2.6.4 NuGet winmds in `%USERPROFILE%\.nuget\packages`; filter in `gen/filter.txt`).
  `bindings.rs` and its hand-written additions are untouched; objects cross with `cast()`. The
  generator is excluded from the workspace. Run: `cargo run --manifest-path bins/mdv-winui/gen/Cargo.toml`.
- `src/bar.rs` builds the bar, the title row (TabView header = logo, footer = drag area via
  `SetTitleBar`), the outline pane and the dialogs. Theme cycles System, Light, Dark with its icon
  and label; the title row, command bar and panes are shaded 10% and 6% towards the text colour.
- Right-click menus: the page (`ContextFlyout`, filled on the right press) and each tab.
- Find has a real `TextBox`; Ctrl+F focuses it, Enter/F3 step, Escape clears.
- The scrollbar is a rounded thumb, thin at rest, wider while the pointer is over it.
- About and Keyboard shortcuts are `ContentDialog`s (the shortcut list comes from core's
  `shortcut_help`); the logo is drawn to PNG with resvg and shown as a `BitmapIcon`.
- Focus reading mode (Ctrl+Shift+F, Escape) hides the command bar and both panes.
- New dependencies: `resvg` 0.47 (logo), `windows-future` 0.100 (`ContentDialog::ShowAsync`).

**Qt.**
- Frameless window (`Qt::FramelessWindowHint`): a title row toolbar holds the logo, the tabs (auto-hide
  off: a toolbar does not re-show a widget it saw hidden), a drag area (an event filter on the canvas
  calls `QWindow::startSystemMove`, a double click maximizes) and its own window buttons.
- **Two old bugs fixed:** both right-click menus set `ContextMenuPolicy(2)`, which is
  `ActionsContextMenu`, so neither ever opened (now 3, `CustomContextMenu`); and the canvas treated
  every mouse button as the left one (a right press started a selection, its release could follow a
  link). The page menu was also connected before the canvas was stored.
- The chrome is styled from the page theme with one style sheet (`chrome_style`); menus are parented
  to the window so they take it. Bar icons are SVG strings rendered to PNG with resvg into
  `%TEMP%/mdview-icons/<theme>/` and assigned by tooltip selectors, because this Qt install has no
  SVG module (Linux installs may not either).
- `system_is_dark` on Windows reads `AppsUseLightTheme`; with no flag and no session the theme is System.
- The main menu (☰) is a flat list; find has a count beside it and a clear button; About and Keyboard
  shortcuts are `QMessageBox::about` dialogs with the logo.

**Checked here:** 212 workspace tests, the 7 WinUI and 8 Qt scripted checks, `cargo fmt` in both
crates, and every bar button, menu, pane, theme mode, right-click menu, find and the dialogs clicked
in both apps placed side by side (real mouse input, screenshots compared).
**Not checked:** Qt on KDE (frameless window, icons, style sheet), WinUI at DPI other than 100%, a
second monitor, the installer.

## 27. WinUI section-12 work and the 0.1.0 installer (2026-11-11)

On the physical Windows PC, after section 26.

**Single instance.** `main.rs` now parses the arguments and calls `single::forward` before the
Windows App SDK bootstrap, the GPU thread and COM. A second launch that hands its file over took
**17-24 ms** process start to exit (52 ms the first time), against PLAN's 20 ms budget, with one
window left. The listener uses `listen_waking`, so forwarded files open at once.

**File watching.** The 800 ms `SetTimer` poll is gone. `mdview_core::watch::FileWatcher` and the
single-instance listener call `ui::wake`, which posts to the UI thread's `DispatcherQueue`
(`TryEnqueueWithPriority`, already in `bindings.rs`). The watched set follows `save_session`. If the
watcher cannot start, a 2 s poll runs instead. Checked live: a save-by-rename of the showing file
reloaded it, an unrelated file did nothing, a second launch opened its file as a tab.

**Manage recent** is a `ContentDialog` (Open, Remove, Clear list, Done). The list is rebuilt after
the click finishes (`later`), because rebuilding inside the click let the row that moved under the
pointer take the same click. Checked: one click removes one entry.

**Drag and drop:** `AllowDrop`, `DragOver` and `Drop` on the root; `GetStorageItemsAsync` completes
off the UI thread and posts the paths back. Built only: Windows refused the synthetic drag (and then
all `SetCursorPos` calls) in this session, so it has not been run.

**Also:** Ctrl+Shift+1 / 2 toggle the Files and Outline panes; the exe icon is embedded again when
`rc.exe` is on PATH (`C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64`; from Git Bash
the build script could not run it).

**Measured** (`spikes/winui/measure.ps1` and `Get-Process`, 100%, dark):

| | before layout A (`9dad837`) | now |
|---|---|---|
| first frame, median of 8 interleaved runs | 316-343 ms | 383 ms |
| private memory, empty window | 170 MiB | 180 MiB |
| private memory, showcase.md | 197 MiB | 210 MiB |

`ExtendsContentIntoTitleBar` costs 70-95 ms wherever it is called (moved after `Activate`, which
saved some); doing it after the first frame would make the page jump. The memory was over PLAN's
50-90 MB before this work too. Text quality at other DPIs was not checked: it needs the display
scaling changed by hand.

**Installer** (`packaging/windows/mdview.iss`, Inno Setup 7.1.1): the association task is now on by
default. It registers the `mdview.Markdown` ProgID, `OpenWithProgids` for `.md .markdown .mdown .mkd`,
`Applications\mdview-winui.exe`, and a registered application (`Software\mdview\Capabilities` plus
`RegisteredApplications`) so mdview appears in Settings > Default apps. Windows does not let an
installer take the default, so the last page offers "Choose mdview as the default app", which opens
`ms-settings:defaultapps?registeredAppUser=mdview`. Optional desktop shortcut. Checked on this PC: a
silent per-user install put both exes in `%LOCALAPPDATA%\Programs\mdview`, wrote all keys, left the
user's `.md` default and other handlers alone; the installed app opened a file through the
registered command; the silent uninstall removed every key, the files and the shortcut. Not run:
the wizard pages, the runtime download branch, an upgrade over an older version.
