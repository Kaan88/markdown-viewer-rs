# Handoff

`mdview` is a Rust remake of `Kaan88/markdown-viewer-cpp`: a Markdown viewer with its own renderer,
**WinUI 3 on Windows** and **Qt Widgets (CXX-Qt) on Linux**, no webview, no authored C++ or XAML.

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
| images (PNG/JPEG, limits, inline) | done; SVG is Phase 4 |
| virtualized viewport, scroll budget | done: 0.2-2.6 ms per scroll frame (budget 4 ms at 150%, 8 ms at 4K) |
| selection, link/anchor hit-testing | done in the renderer and both shells; no clipboard copy yet |
| `<details>` toggling | **not started** |
| exit gate: goldens approved by the user | **open**: the images in `tests/golden/` have not been approved yet |

So the immediate queue is `<details>` toggling and the approval; then Phase 3 (Qt shell parity),
Phase 4 (engine: math, Mermaid, highlighting, SVG), Phase 5 (WinUI parity, W1-W3), Phase 6 (polish,
installer). The shells already exist and run, which is ahead of the plan's order - the user asked for
the UIs early - so Phase 3 and 5 become "make the existing shells match the checklist" rather than
"build them".

## 2. Repository

- `Kaan88/markdown-viewer-rs`, private, branch `phase2-renderer`.
- Recent commits: `09e5622` docs, `8442100` WinUI input, `5377a2b` band upload, `21d4f39` shifted
  frame, `0013782` WinUI scrollbar/keys, `5e41238` Qt scrollbar/keys.
- Workspace members: `crates/core`, `crates/render`, `bins/mdv-bench`, `bins/mdv-winui`.
  `bins/mdv-qt` is **excluded** from the workspace because it needs Qt to build; it has its own
  `Cargo.lock` and its own `target/`.
- `spikes/` is throwaway reference code and is excluded from the workspace.
- `target/` is git-ignored; the helper scripts in it (`vm.py`, `seed-lock.py`, `cap.ps1`) are **not
  committed**. Section 5 and section 9 describe what they do in case they have to be rewritten.

## 3. Build and test on Windows

Toolchain here: rustc/cargo 1.99.1, MSVC 2022, Qt 6.9.3 MSVC at `C:\Qt\6.9.3\msvc2022_64`.

```powershell
cd C:\Users\kaank\Desktop\mv
cargo test --release --offline --workspace     # 7 core, 11 fuzz, 113 regression, 3 spec,
                                               # 2 render unit, 16 render integration
cargo fmt --all --check
cargo build --release --offline -p mdview-winui
```

**Cargo cannot reach crates.io from this machine** (`schannel: AcquireCredentialsHandle failed:
SEC_E_NO_CREDENTIALS`), so always pass `--offline`. If a dependency is missing from the cache,
`target/seed-lock.py` (not committed) reads `Cargo.lock` files, writes the sparse index entry for
every crate and downloads the locked `.crate` files:

```powershell
python target/seed-lock.py Cargo.lock bins/mdv-qt/Cargo.lock spikes/qt-shell/Cargo.lock
```

All locks in use must be passed: the index cache is shared.

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
`--zoom <x>`, `--drop-test <path>` (sends one `QDragEnterEvent` and `QDropEvent` through the window
and exits 0 when the dropped path was loaded; it exists because Qt uses OLE drag and drop on Windows,
so a posted `WM_DROPFILES` from another process is ignored), `--select-test` (drags over "Markdown"
through the real mouse path, prints the selection and rect count, exits), `--select-hold` (the same
but leaves the window open for a screenshot) and `--link-test` (clicks the `#diagrams` link and
compares the scroll row with what `y_of` says).

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
cd ~/mdview && cargo test --release -p mdview-core -p mdview-render   # 7 core, 11 fuzz, 113
                                                                      # regression, 3 spec, 2+11 render
cd ~/mdview && xvfb-run -a /mnt/2b4bc8c0-2e4e-4fc1-b330-27648dcbbf49/target/release/mdview-qt \
    --drop-test fixtures/linked.md                                   # prints "drop test ok: ..."
```

Capturing a Linux window (`Xvfb` has no window manager, so the window sits at 0,0):

```sh
Xvfb :99 -screen 0 1600x1200x24 &
DISPLAY=:99 /mnt/.../target/release/mdview-qt fixtures/showcase.md & sleep 6
DISPLAY=:99 import -window root /tmp/shot.png
```

Linux font metrics differ from Windows (the showcase page is 3041 px tall there, 3020 here), so Linux
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
| `.hit_test(doc, &Options, &PageLayout, x, y) -> Option<Hit>` | the document offset under a point, plus the link or footnote there; lays out one block on demand |
| `.selection_rects(doc, &Options, &PageLayout, from, to) -> Vec<Rect>` | one page rectangle per selected line, for the shell to draw |
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

Tests that pin this down (`crates/render/tests/render.rs`, 16 tests):

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

- **Keys** come from a thread-level `WH_GETMESSAGE` hook installed on the UI thread. WinUI's content
  lives in child windows (`InputSiteWindowClass`, `Microsoft.UI.Content.DesktopChildSiteBridge`) that
  take the keyboard, and subclassing those window procedures with `SetWindowLongPtrW` is not durable:
  WinUI reinstalls its own and the subclass stops being called after a resize. The hook filters by
  window (`GetAncestor(.., GA_ROOT)` must be our top-level window), so the file dialog's own message
  loop is untouched, and it swallows the keys it uses by setting the message to `WM_NULL`.
- **The scrollbar** is driven by XAML pointer events (`PointerPressed`/`PointerMoved`/
  `PointerReleased`). The generator left those, and `IPointerPoint::Position`, as unbound vtable slots,
  so `bindings.rs` has hand-written `pointer_pressed/moved/released` and `IPointerPoint::position`.
  XAML reports the position in the panel's own logical px, so no window-coordinate maths is involved.

Keys: Home/End, arrows (48 px), Page Up/Down and Space (90% of the viewport). Mouse: a press in the
gutter jumps there and starts a drag; the drag follows the pointer.

**Selection and links.** A press on the page starts a selection through the same XAML pointer events;
moving extends it and release keeps it. If the pointer never moved, release follows the link or
footnote under the press: a `#fragment` scrolls with `PageLayout::y_of`, another local Markdown file
is loaded and then scrolled to its fragment, and `http(s):`/`mailto:` goes to `ShellExecuteW`.
Escape clears the selection. The selection is `Option<(u32, u32)>` over `Document::text`, and its
rectangles are CPU-blended into the **back buffer** just before the gutter strip, so the page texture
- and with it `band`/`shift` - never contains selection pixels.

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
`unsafe`, `src/qt/ffi.rs` holds the C++ declarations (no authored C++), `src/shell.rs` is
`#![forbid(unsafe_code)]`.

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
document only. A release without movement follows the link or footnote under the press: a
`#fragment` scrolls with `PageLayout::y_of`, another local Markdown file is loaded and scrolled to
its fragment after layout, and `http(s)`/`mailto` goes through `QDesktopServices::openUrl`. The
cursor becomes a pointing hand over a link or footnote mark. `--select-test`, `--select-hold` and
`--link-test` drive these through Qt's real event path for scripted checks.

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

**These are for review, not a pixel baseline yet.** They use this host's system fonts (Segoe UI,
Consolas), so another machine or Linux will differ; the plan calls for bundled fonts and a tolerance
before they become a suite. The four review images still have **not been approved by the user**.

Renderer goldens regenerate byte-identically:

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
`--select-test` (selection 7..15 = "Markdown", one rect) and `--link-test` (scrolled to 1222, the row
`y_of` reports for the Diagrams heading), plus a captured `--select-hold`; WinUI through a real
press-drag-release on the running window (a drag from the heading into the next paragraph, two
selection rows) and a click on the `#diagrams` link, both captured. The Qt shell on the same files with layout,
first paint, wheel, scrollbar, keys, File > Open..., themes, `--zoom`, a live resize to 3400x1380 and
`--drop-test`.

**Linux:** the Qt shell built with Qt 6.8.2 on the VM, run and captured under Xvfb, `--drop-test`
passing, and `cargo test --release -p mdview-core -p mdview-render` passing.

**Not run:** the Qt View/Help menu items; the Qt scrollbar and keys on Linux (Windows only); DPI scales
other than 100% (zoom is tested, DPI scaling is not); a real OLE drag from Explorer (the drop test drives
Qt's event path instead); the new selection and link checks on Linux (Windows only so far).

## 11. Known gaps

- **No copy or toggling yet.** Selection is drawn but Ctrl+C does not copy it, and `<details>` still
  does not open or close. `BlockBox` and the hit map are what a toggle needs next.
- **SVG** (and anything that is not PNG/JPEG) is a placeholder; `<picture>` light/dark sources and
  `width`/`height` attributes are ignored; a blocked image is retried on every render.
- No find, tabs, folder tree, outline or recent files in either shell; no bundled fonts.
- Table `colspan`/`rowspan` ignored; math, Mermaid and highlighting show their source (Phase 4).
- The WinUI title bar follows Windows, not the in-app theme override.
- The WinUI scrollbar is drawn by the shell, not a WinUI control (no `ScrollBar` in the bindings).
- Scroll frames meet the budgets, but a frame still waits for vsync (3-6 ms in the trace): the
  compositor's clock, not the renderer's.

## 12. What to do next

1. **Get the `tests/golden/` images approved by the user** - it is the Phase 2 exit gate. Show
   `showcase-{light,dark}@{1.0,1.5}.png` plus the shell captures (including the new selection and
   anchor shots) and ask for a yes or a list of changes.
2. **`<details>` toggling** is the last unstarted Phase 2 row. The model already keeps
   `Details { open }` and `Document::hidden_ranges(is_open)` / `details_keys()`; the renderer's
   `details()` currently uses `details_open_as_written`. Give the renderer an open-state callback,
   hit-test the summary row (the `LineGeom` for the summary is cached like any other), and invalidate
   just that block: the paint cache is per top-level block, and `PageLayout` is cheap to rebuild, so
   a toggle can do one height pass and redraw. Keep open state in the shell, keyed by
   `details_keys()`.
3. **Phase 4, SVG**: a new `mdview-engine` process (the plan's Phase 4) that renders SVG with resvg; the
   image cache is already shared across frames, so a decoded SVG slots into `images` where a PNG does.
4. **Then the Qt/WinUI parity items** from `docs/PORT_CHECKLIST.md`: tabs, folder tree, outline, find,
   recents, zoom limits, themes, focus mode, session restore. Selection is done in both shells; copy
   to the clipboard and a link context menu are the obvious next shell features.
5. **Bundled fonts**, then turn the goldens into a real pixel suite (the plan's Phase 6).

Every Qt/Linux change is tested on the VM as well as on Windows, and nothing counts as working on
Windows until it has run on Windows.
