# Handoff

`mdview` is a Rust remake of `Kaan88/markdown-viewer-cpp`: a Markdown viewer with its own renderer,
**WinUI 3 on Windows** and **Qt Widgets (CXX-Qt) on Linux**, no webview, and no authored C++.

This document is the current state: what works, how to build and check it, the traps already found,
and what to do next. `docs/PLAN.md` is the full plan (budgets, phases, gates). Read this file first,
then the plan.

**Branch:** `phase2-renderer`, tracking `origin/phase2-renderer`; the branch tip is pushed. Check
`git log --oneline -5` for where it actually is - this file is updated at each milestone, so the
commit that last changed it is the state it describes. There is no CI: tests are run by hand before a
push. Commit and push at milestones.

Working agreements (from `CLAUDE.md`): read this file before touching anything; commit and push as
phases or milestones complete, not continuously, but do push, because the user checks GitHub; there is
no CI, so run the tests yourself; `spikes/` is throwaway; report results plainly, including what was
**not** run; nothing counts as working on Windows until it has run on Windows. The user is on Windows
and the Linux Qt work happens on their VM (section 5).

Everything below was measured on this PC (Windows 11, RTX 4070 Ti SUPER, 3440x1440 at 96 DPI) unless
it says Linux; Linux numbers come from the test VM in section 5.

---

## 1. Where the project is on the plan

`docs/PLAN.md` has six phases (its "Phases" section). Phase 0 (spikes) and Phase 1 (core: parser,
HTML mapping, autolink, slugs, doc model, find) are closed on earlier branches. **This branch is
Phase 2, the headless renderer, and it is nearly done.**

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

Phase 2 is closed (goldens approved 2026-11-08). Next come Phase 3 (Qt shell
parity), Phase 5 (WinUI parity, W1-W3), Phase 6 (polish, installer). The shells already exist and run,
which is ahead of the plan's order - the user asked for the UIs early - so Phase 3 and 5 become "make
the existing shells match the checklist" rather than "build them".

## 2. Repository

- `Kaan88/markdown-viewer-rs`, private, branch `phase2-renderer`.
- Recent commits: `git log --oneline -5`; the tip is the shared lazy engine client and its docs.
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

- Links have no context menu (copy works in both shells).
- **Images:** PNG and JPEG decode in-process, SVG and math through `mdview-engine`; `<picture>`
  light/dark sources pick the source for the current theme; `width`/`height` on `<img>` are honoured (pixels or percent of the
  content width, aspect kept when only one is given, clamped to the content width; Markdown images
  carry none). Any other format keeps its
  placeholder. A blocked image is retried on every render.
- WinUI has no tabs, folder tree, recent files or session yet (Qt has them all); no bundled fonts.
- Table `colspan`/`rowspan` are laid out (grid placement, spanned widths and heights, separators that stop at a spanning cell, at most 200 columns); no vertical rules exist in the table style, so nothing else changes. Highlighting covers fenced blocks only: inline code keeps the
  theme text colour, and unknown languages or blocks over 256 KiB stay plain.
- Math blocks are images: dragging across a display equation selects nothing, and there is no
  equation numbering or copy-as-TeX.
- Diagram blocks are images, so they have no selection geometry.
- The WinUI find has no text box: its query and counter live in the window title, because the trimmed
  `bindings.rs` has no `TextBox` event or text accessor. Qt has a real find bar, and the WinUI
  outline is a menu rather than a dock.
- The WinUI title bar follows Windows, not the in-app theme override.
- The WinUI scrollbar is drawn by the shell, not a WinUI control (no `ScrollBar` in the bindings).
- Scroll frames meet the budgets, but a frame still waits for vsync (3-6 ms in the trace): the
  compositor's clock, not the renderer's.

## 12. What to do next

1. Phase 2 is closed: the user approved the goldens on 2026-11-08.
2. **Phase 3/5 parity**: the Qt and WinUI items from `docs/PORT_CHECKLIST.md` - tabs, folder tree,
   recents, zoom limits, themes, focus mode, session restore. Selection, links, `<details>`, find,
   outline, clipboard copy, math, diagrams and highlighting are done in both shells; a link context
   menu and document tabs are the obvious next shell features.
   - **Qt tabs, Recent and session: done 2026-11-08.** `bins/mdv-qt/src/shell.rs`: a `QTabBar`
     (document mode, closable, movable, auto-hidden with one tab) above one canvas. The showing tab's
     state lives in `Shell`'s fields; other tabs sit in `Page`s and `activate` swaps them, so
     scroll, selection and opened `<details>` survive a switch. Opening an open file switches to
     it; labels come from core's `tab_labels`; closing the last tab shows the empty page. Ctrl+O,
     Ctrl+W/Ctrl+F4, Ctrl+Shift+W, Ctrl+Tab and Ctrl+Shift+Tab work. File > Open recent is rebuilt
     as it opens (missing entries are marked, and clicking one removes it), plus Clear recent files.
     The session (`tabs`, `active`, `recent`) is saved after every open, close and move, not only on
     exit, and saved tabs reopen at startup, missing ones with the not-found text. `MDVIEW_SETTINGS`
     names the file; `--no-session` turns it off; scripted checks never touch the user's file.
     `--tabs-test` checks it and passes on Windows and on the VM (xvfb). A right-click tab
     menu (Close tab, Close other tabs, Copy file path, Show in file manager: `explorer /select,` on
     Windows, the folder through `openUrl` elsewhere) was added 2026-11-10; `--tabs-test` covers it and passes on Linux (offscreen) and on
     the WinApps Windows 11 VM (the menu is built and popped, not clicked). Also done 2026-11-10 (Qt, `--tabs-test` covers them, Linux only so far):
     zoom Ctrl+= / Ctrl+- / Ctrl+0 (60-250%, step 10, View menu), Refresh (F5, File menu),
     focus reading mode (Ctrl+Shift+F, Escape leaves; hides menu bar, status bar, outline, find
     bar) and Help > Keyboard shortcuts (status bar text, no dialog binding yet). Theme (System/Light/Dark,
     System follows `QStyleHints::colorScheme`) and zoom are saved in the session and restored unless
     `--light`/`--dark`/`--zoom` is given. Auto-refresh: an 800 ms canvas timer
     compares each open file's stamp (background tabs too); a change reloads it keeping the scroll
     offset, a deleted file keeps its text and says so, a recreated one reloads (`--tabs-test`).
     Folder tree: File > Open folder (Ctrl+Shift+O, a `QFileDialog` in directory mode) fills a left Files dock (View > Files) with folders first then
     `.md/.markdown/.mdown/.mkd`, hidden names skipped, at most 4000 rows and 8 deep; a click opens
     the file; the folder is saved in the session and restored (`--tabs-test`). It is a scanned
     `QTreeWidget`, not a live `QFileSystemModel`, so it does not update when the folder changes.
     Single instance: `bins/mdv-qt/src/single.rs`, std only. The first
     window listens on a loopback port recorded in `instance.port` beside the settings file; a
     later launch sends its files there and exits, and the first window opens them on its next
     800 ms tick. A unit test and a two-process offscreen run pass on Linux; scripted checks and
     `--no-session` skip it. It does not raise the window. Manage recent files: File > Manage recent files... shows a left dock with the list (missing
     marked) and a toolbar with Open, Remove selected, Clear list and Done; it is a dock and toolbar
     rather than a modal dialog, because no `QDialog` is bound (`--tabs-test`). Not done yet on Qt:
     the empty-state widget with its Open button, raising the window on a second launch.
   - **WinUI zoom and F5: done 2026-11-10.** Ctrl+= / Ctrl+- / Ctrl+0 (also the numpad) zoom 60-250% in
     steps of 10 through `Shell::zoom_step`, and F5 reloads. `--zoom-test` (up 1.1, reset 1.0, floor
     0.6, ceiling 2.5) passes on the WinApps Windows 11 VM, a build without a GPU, so its 3.9 s is
     not a timing. The real Ctrl and F5 key presses were NOT exercised on Windows, and the window
     stays open after the test (kill it). Not saved in a session yet: the WinUI shell has none.
   - **WinUI tabs: not started.** It needs `TabView` bindings in `bins/mdv-winui`; the same `Page`
     swap should carry over.
3. **Bundled fonts**: the user chose on 2026-11-08 to decide later, so system fonts stay for now.
   Bundling would let the golden comparison in `crates/render/tests/golden.rs` run on Linux too;
   today it runs on Windows only.
4. The 2026-11-08 review fixes were checked on the VM: `cargo test --release -p mdview-core -p
   mdview-render -p mdview-engine -p mdview-engine-proto -- --include-ignored` passes (the golden
   compare skips there), the Qt shell builds against Qt 6.8.2 with no authored C++, and its
   `--copy-test`, `--outline-test`, `--find-test`, `--link-test`, `--select-test` and
   `--details-test` all report ok under `xvfb-run`. Not exercised on Linux: a real outline click
   after a rebuild, and a file drop (both covered on Windows or by code review only). On the VM,
   `CARGO_TARGET_DIR` is `/mnt/2b4bc8c0-2e4e-4fc1-b330-27648dcbbf49/target`, so binaries land there,
   not under `bins/mdv-qt/target`; and from PowerShell, upload a script instead of passing
   double-quoted commands to `vm.py run` (PowerShell 5.1 mangles them).

Every Qt/Linux change is tested on the VM as well as on Windows, and nothing counts as working on
Windows until it has run on Windows.

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
- Still to do: the shells' update check (read the manifest, once per 24 h, opt-out), the installer
  run on Windows, and an actual AppImage build.

## 15. Linux budget numbers (measured 2026-11-10)

CachyOS, Qt 6.10 shell built `--release`, **offscreen** platform (no compositor, so real Wayland
memory will be higher), AC power, desktop not otherwise idle. The 20 KB document is the first 20,000
bytes of `generated_1mib.md` (plain prose); the PLAN's "instant" definition is process start to the
first `painted` line.

| Metric | Measured | Plan (likely / aspiration) |
|---|---|---|
| Warm first paint, plain 20 KB | **171 ms** median of 10 after 2 warm-ups (165-177) | 100-160 / 80 ms: **over** |
| Same, showcase x5 (diagrams, math, syntect) | 462 ms | not a plan row; shows the engine and highlighter cost |
| Private memory, plain 20 KB, 3 s in | 37.0 MB | 25-40 empty window: inside, with a document |
| 1 MB document, first paint / private | 747 ms / 40.8 MB | +15-25 MB, +30 ms: **not met** on time |
| Scroll frame, 780x900 @100% | 0.2 mean / 1.6 worst ms | 4 ms p99 |
| Scroll frame, 1170x1350 @150% | 0.3 / 1.8 ms | 4 ms p99 |
| Scroll frame, 3840x2200 @150% | 2.0 / 3.7 ms | 8 ms p99 |
| Idle CPU over 10 s | 0.00 ticks | about 0 (note: an 800 ms poll timer now runs) |

Launch is the open item: render, parse (0.1 ms) and paint (0.1-0.7 ms) are not where the 171 ms goes;
it is Qt and font initialisation, not yet profiled (no `perf` or `strace` on this box). The 1 MB first
paint includes the 4-9 ms height pass plus parse and a full first layout. Not measured: cold launch,
10 tabs, second-launch handoff time, a real Wayland or X11 window.

