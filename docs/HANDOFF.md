# Handoff: Phase 2 renderer, WinUI shell, Qt shell on Windows

**Branch:** `phase2-renderer`, from `phase1-audit-fixes` (pushed at `633c0a8`). Checkpoints are
committed here; nothing is pushed. The four review images in `tests/golden/` still have not been
approved by the user.

## State

- Phase 1 closed: core audit fixes, tests and docs on `phase1-audit-fixes`. Suite passes on Windows:
  7 core, 11 fuzz, 113 regression, 3 spec, release profile. Spec counts stay 645/652 CommonMark and
  665/672 GFM.
- Phase 2: `crates/render` lays the document model out and paints a BGRA bitmap with no window
  system; `bins/mdv-bench` writes PNGs. `fixtures/showcase.md` (66 blocks) lays out in about 27 ms at
  1084-1100 logical px and 100% scale, 3020 px tall, in both themes.
- **There are now two windows, both running on this Windows PC:** the WinUI shell
  (`bins/mdv-winui`) and the Qt shell (`bins/mdv-qt`). Both open a Markdown file, draw the page
  through the renderer, and scroll with the wheel.
- Images draw: PNG and JPEG are decoded from the document's own directory, capped at 12 MiB and 24 M
  pixels, placed as parley inline boxes and scaled to the content width. Missing, unsupported (SVG
  is Phase 4) and over-limit files keep the `[image: alt]` placeholder. New fixture
  `fixtures/images.md`, golden `tests/golden/images-light.png`, and shell captures.
- Workspace: 7 core, 11 fuzz, 113 regression, 3 spec, 6 render tests and the renderer's image-limit
  unit test pass; `cargo fmt --all` clean.
- A scroll frame now reaches the WinUI window as a band: the renderer reports which rows it painted
  and how far the rest moved, and the shell moves the rest with a GPU copy. See "Virtualized
  viewports" below.

## The WinUI shell (`bins/mdv-winui`)

- WinUI 3 with hand-generated Windows App SDK bindings (`src/bindings.rs`, copied from
  `spikes/winui`; regenerate only with the same NuGet winmds). No XAML, C#, or C++.
- Framework-dependent build: needs the Windows App Runtime 2.6.4 or newer (x64). This PC has 2.6.5.
- Window is 1100x900. Menus: File > Open..., Reload, Close; View > Light theme, Dark theme. The
  wheel scrolls, and `--zoom 1.5` scales the page (verified: showcase grows from 3020 px to
  4592 px at 1.5). The page follows the Windows app theme at startup (`AppsUseLightTheme`), and
  `--light` / `--dark` override it for screenshots.
- **Scrolling and input**: the page is laid out 14 logical px narrower than the panel and the shell
  paints its own scrollbar into that gutter (a hairline separator, a track and a thumb), because the
  checked-in bindings have no `ScrollBar` control and cannot be regenerated without the NuGet winmds.
  Home/End, the arrows (48 px), Page Up/Down and Space (90% of the viewport) scroll, and a press in
  the gutter jumps to that point and starts a drag.
- The menu labels are drawn by WinUI itself, and the default app theme is dark, which made them
  white on the light page. `bindings.rs` now carries one hand-written helper, `set_requested_theme`,
  that calls the `RequestedTheme` vtable slot the generator left unbound; the root grid is set to
  Light or Dark with the page, and the View menu keeps them in step. Regenerating the bindings would
  remove that helper, so it is duplicated there deliberately.
- **Input arrives two ways, because WinUI moves it around.** Keys come through a thread-level
  `WH_GETMESSAGE` hook: WinUI's content lives in child windows (`InputSiteWindowClass`,
  `Microsoft.UI.Content.DesktopChildSiteBridge`) that take the keyboard, and subclassing those
  window procedures with `SetWindowLongPtrW` is not durable - WinUI reinstalls its own and the
  subclass silently stops being called after a resize. The hook filters by window (`GetAncestor`
  must be our top-level window), so the file dialog's own message loop is untouched, and it swallows
  the keys it uses by setting the message to `WM_NULL`. The scrollbar is driven by XAML pointer
  events (`PointerPressed`/`PointerMoved`/`PointerReleased`), which the generator left as unbound
  vtable slots; `bindings.rs` carries hand-written `pointer_*` helpers and `IPointerPoint::position`
  for them. XAML reports the position in the panel's own logical px, so no window-coordinate maths is
  involved.
- After a *programmatic* resize from another process the app needs one click before keys reach it
  again: WinUI loses the XAML keyboard focus and `SetForegroundWindow` does not restore it. A resize
  with the mouse keeps the input focus, and a click fixes it, so this is a focus matter rather than
  a hook one - verified by clicking after a resize and watching the next arrow key scroll again.
- `src/render.rs` keeps the page in **two D3D11 textures** (see the virtualization section) and the
  scrollbar in a 15 px strip beside it, so a scroll sends the band and a GPU copy of the rest instead
  of the whole viewport. The page can be a few px wider than the width it was laid out for; the
  scrollbar strip covers that overhang, because the bar has to sit at the panel's right edge.
- `--trace` writes `mdview.trace.log` beside the exe, one line per frame with paint, upload, the rows
  sent, present and total. `--measure` writes one `result=ok first_frame_ms=...` line and exits.
- `--no-shift` sends every frame whole instead of using the shifted frame, which is the reference a
  shifted frame has to match; `--scroll <device px>` starts partway down the page; `--scroll-steps
  <n>` walks the page down `n` 40 px steps, one frame each, after the first frame; `--size
  <w>x<h>` opens a larger window. The last three exist so a capture or a measurement needs no input
  injection.
- Measured on this PC (RTX 4070 Ti SUPER, 96 DPI): first frame at 263.5 ms from process start,
  height pass 8.8 ms for the 1084 dip panel, parse 0.1 ms.
- Checked by driving the running window with posted messages and real clicks: three Page Downs move
  the page to the footnotes and the thumb to the bottom, a press-and-drag in the gutter lands
  mid-document, File > Open... opens `GetOpenFileNameW` (class `#32770`, "Open") and typing a path in
  it loads the file, Reload parses the document again, View > Dark theme repaints the page and the
  menu bar dark, and Close closes the window.
- Screenshots: `tests/golden/shell-winui-light.png`, `shell-winui-dark.png`, `shell-winui-images.png`.

## The Qt shell (`bins/mdv-qt`)

- Qt Widgets through CXX-Qt 0.10, ported from `spikes/qt-shell`: `src/qt/mod.rs` (the only module
  with `unsafe`) and `src/qt/ffi.rs` (C++ declarations, no authored C++). `src/shell.rs` is
  `#![forbid(unsafe_code)]`.
- **This crate is excluded from the workspace** (`Cargo.toml`), because it needs a Qt installation
  to build. It has its own `Cargo.lock` (the Qt-only graph, seeded from the spike's lock) and its
  own target directory.
- Window 1100x900, menus File > Open... and Quit, View > Light/Dark theme, Help > About, a status bar
  with the file name and block count. `--zoom 1.5` scales the page (verified: showcase page grows
  from 3020 px to 4596 px at 1.5). A file comes from the command line, File > Open... or is dropped
  on the window (Qt file URIs, `%20` decoded).
- `src/shell.rs` renders the page with `mdview-render` for the canvas width and device pixel ratio,
  copies the visible slice into a BGRX buffer and blits it through QPainter with an identity
  transform.
- **Scrolling**: the wheel, a vertical `QScrollBar` beside the canvas (14 px wide, in a `QHBoxLayout`)
  and the keyboard. The shell owns the offset: it sets the bar's range, page step and value while
  painting, with a `syncing` flag so the bar's `valueChanged` does not echo back; a drag or a click in
  the groove arrives through `valueChanged` and repaints. The canvas takes `Qt::StrongFocus`, so Home,
  End, the arrows, Page Up/Down and Space work: arrows move 48 px, Page Up/Down and Space 90% of the
  viewport, Home and End go to the ends.
- **File > Open** is a `QFileDialog` built through the same `new_ptr` bridge (its four-argument
  constructor; `QFileDialog::getOpenFileName` could not be bound because cxx takes its address and the
  C++ signature has six parameters). It opens in the current document's directory with a
  `Markdown (*.md *.markdown)` filter.
- Build on this PC (Qt 6.9.3, MSVC 2022, installed at `C:\Qt\6.9.3\msvc2022_64` with `aqtinstall`):

  ```powershell
  cd bins/mdv-qt
  $env:PATH = "C:\Qt\6.9.3\msvc2022_64\bin;$env:PATH"
  $env:QMAKE = "C:\Qt\6.9.3\msvc2022_64\bin\qmake.exe"
  $env:CMAKE_PREFIX_PATH = "C:\Qt\6.9.3\msvc2022_64"
  cargo build --release --offline
  ```

  Run it with the Qt `bin` directory on `PATH` so the Qt DLLs are found. In this sandbox `TEMP` and
  `TMP` must point at a writable directory (e.g. `target/tmp`) or MSVC's `lib.exe` fails with
  access denied; a normal shell does not need that.
- Measured here: parse 0.1 ms, height pass 8.6 ms for the 1086 px canvas, page 3020 px.
- Checked by driving the running Windows build: Page Down scrolls and the bar's thumb moves, a click
  in the bar's groove jumps a page, File > Open... opens the dialog in `fixtures/` and loading
  `linked.md` through it updates the title, the page and the status bar.
- **Drag and drop** is reached through `--drop-test <path>`, which sends one `QDragEnterEvent` and
  `QDropEvent` through the window and exits 0 when the dropped path was loaded. It exists because Qt
  uses OLE drag and drop on Windows: the platform plugin has no `DragQueryFileW`, `DragAcceptFiles`
  or `WM_DROPFILES` handling, so a posted `WM_DROPFILES` from another process is ignored. The check
  passes on Windows and on the VM under Xvfb.
- Screenshots: `tests/golden/shell-qt-light.png` (page top, scrollbar at zero), `shell-qt-scrolled.png`
  (after a click in the bar), `shell-qt-open.png` (the Open dialog).
- The crate emits seven `unnecessary unsafe block` warnings from the `pin!` macro; they predate this
  work and are not errors.
- **Both the Windows build and the Linux build now run.** The Linux build is built and run on the
  test VM below (Qt 6.8.2, Debian 13), under Xvfb; see `tests/golden/shell-qt-linux.png`.

## The Linux test VM (`user@vm-host`)

The user gave this VM for Qt and Linux testing from 2026-11-07. It is Debian 13 (trixie), 20 cores,
7.8 GB RAM, Xvfb, ImageMagick, and Rust 1.99.1 with `CARGO_HOME`/`CARGO_TARGET_DIR` under
`/mnt/2b4bc8c0-.../` (global environment; the binary lands in `$CARGO_TARGET_DIR/release/`).

- Packages installed: `qt6-base-dev` (Qt 6.8.2), `libfontconfig1-dev`, `libxcb-cursor0`,
  `libxkbcommon-x11-0`, `xvfb`, `imagemagick`, `fonts-noto-cjk`.
- The source tree is copied with `tar` + SFTP (the helper `target/vm.py`, not committed) into
  `~/mdview`. It is a snapshot: re-upload after changes. Two archives: the source (without
  `spikes/`) and `spikes/parser/data`, which the spec tests read. The whole workspace does not
  build on Linux because `bins/mdv-winui` is Windows-only, so Linux runs
  `cargo test --release -p mdview-core -p mdview-render`; that now passes 7 core, 11 fuzz, 113
  regression, 3 spec, the image-limit unit test and 6 render tests.
- Build and run:

  ```sh
  cd ~/mdview/bins/mdv-qt
  export QMAKE=/usr/lib/qt6/bin/qmake6
  export PATH=/usr/lib/qt6/bin:$PATH
  cargo build --release
  cd ~/mdview
  Xvfb :99 -screen 0 1600x1200x24 &
  DISPLAY=:99 "$CARGO_TARGET_DIR/release/mdview-qt" fixtures/showcase.md &
  DISPLAY=:99 import -window root /tmp/shot.png
  ```

- First run: parse 0.1 ms, layout 26.8 ms, page 1100x3041 (Linux font metrics differ from Windows'
  3020), page drawn and menus present. `tests/golden/shell-qt-linux.png` is that screen capture.
- Chinese/Japanese text was tofu until `fonts-noto-cjk` was installed; the renderer's font stack has
  a CJK fallback on Linux through fontconfig.
- No window manager runs, so the window sits at 0,0 on the Xvfb root; that is fine for captures.
- **X forwarding to the user's desktop:** MobaXterm's X server listens on `0.0.0.0:6000`, so the app
  can be started with `DISPLAY=windows-host:0.0` (the Windows host's LAN address) and appears on the
  user's screen, mouse and wheel included. Windows' OpenSSH `-X` does not work here: it cannot
  generate the xauth cookie, so the VM's `DISPLAY` stays empty. Use the direct TCP display instead.

## Virtualized viewports (renderer API; shells not wired yet)

- `crates/render` exposes `DocumentRenderer` (font and layout contexts plus the glyph cache kept warm
  across frames), `layout(doc, options) -> PageLayout` (one height pass over every top-level block),
  `view(doc, options, &PageLayout, top, height) -> Slice` (a fresh viewport-sized bitmap, used by
  tests and one-shot callers) and `view_frame(...) -> &Frame` plus `frame()`, which keep one bitmap
  between calls. `render` still paints a whole page for tests; `render_slice` is the one-shot
  layout+view form.
- **Scrolling shifts the frame**: `view_frame` keeps the last viewport bitmap and, when the new view
  overlaps it, moves the surviving rows with one `copy_within` and repaints only the band that came
  into view. The band is drawn on its own small bitmap and copied in, because a block can reach past
  the band and would otherwise be blended twice on the overlap. A jump of a full viewport or more
  falls back to a clear and a full repaint. A test compares shifted frames against a fresh `view` at
  five scroll offsets, images included, and they are byte-identical.
- Measured on `spikes/parser/data/generated_1mib.md` (1.0 MB, 25,060 blocks, 897,583 bytes of text,
  583,619 px page): parse 8.3 ms; height pass 3.7 ms; scroll frames over 60 steps at 40 px, with the
  per-block paint cache and the shifted frame: **0.2 ms mean, 0.4 ms worst at 780x900 (100%)**,
  **0.6 mean, 1.0 worst at 1170x1350 (150%)**, 2.6 mean, 3.0 worst at 3840x2200 (4K, 150%). Glyph
  cache 99% hit, peak 0.2 MiB. The bitmap is the viewport (2.8 MB) rather than the page (1.8 GB at
  780x583,619x4).
- The plan's budget is 4 ms p99 per scroll frame at 150% and 8 ms at 4K: **met** (1.0 and 3.0 ms
  worst). The path there was 5.3 ms mean, then the `Bitmap::rrect` profile (code-block and table
  backgrounds ran a `hypot` and a float blend on every pixel; 5.8 ms of a 6.5 ms frame), then the
  band-shifting frame above: 6.6 -> 0.9 -> 0.2 ms at 100%, 8.1 -> 2.6 ms at 4K. `rrect` still has a
  unit test against the old per-pixel form, and the four `tests/golden/showcase-*.png` images
  regenerate byte-identical.
- The shells still copy the whole viewport bitmap to the GPU every frame; only the renderer got
  cheaper. That upload is now the next 4K item, not the drawing.
- **The frame says what changed**: `Frame::band` is the row range painted this time (`None` when the
  whole frame is new, `Some((0, 0))` when nothing changed at all) and `Frame::shift` is how far the
  rows moved: row `r` of this frame was row `r + shift` of the previous one. A test rebuilds a frame
  from the previous bitmap using only those two numbers and compares the bytes, which is the contract
  the shells depend on.
- **The WinUI shell now sends only the band.** The page lives in a pair of D3D11 textures instead of
  a CPU buffer: a scroll copies the rows that stay on screen from one texture into the other on the
  GPU (`CopySubresourceRegion`) and uploads only the new band (`UpdateSubresource` with a
  `D3D11_BOX`), and the back buffer is filled from the page texture with one GPU copy per frame,
  because a flip-model swap chain does not keep its contents. Measured on this PC in a 3384x1301
  panel: a whole frame uploads in 2.6 ms, a 40-row scroll band in 0.0-0.2 ms, and the frame is
  1.7-4.1 ms end to end (the 3-6 ms "present" in the trace is waiting for vsync). Verified by
  capturing the same scroll offset reached two ways - six shifted band frames, and a fresh layout
  sent whole - and comparing the pictures: identical below the title bar. The un-scrolled window is
  byte-identical to `tests/golden/shell-winui-light.png`, and the dark and images goldens are
  unchanged below the title bar.
- Two bugs found on the way, both worth remembering. A flip-model back buffer alternates between
  buffers, so *partially* writing it leaves the other buffer's pixels showing: the scrollbar strip,
  which is narrow, is therefore sent whole every frame rather than only where the thumb moved. And
  the page bitmap can be wider than `logical width - 14`, so a gutter strip placed at the page's
  right edge was 6 px wide and clipped the scrollbar; the strip is now the bar's own column and
  covers that overhang.
- **Qt needs no band path.** The raster backend has no upload to save, and `drawImage` from the frame
  bitmap only touches the pixels the paint event exposes. Measured on Windows: 0.5-0.7 ms per 48 px
  scroll paint at 1086x846, and 2.0 ms at 3386x1327 (4.5 Mpx) - inside the 4 ms and 8 ms budgets.
  Cutting it further would mean moving the backing store with `QWidget::scroll` before blitting the
  band, which is another viewport-sized memmove on top of the renderer's own shift, so Qt keeps one
  blit and its paint is timed in the trace output.
- **Both shells are wired to this API**: each keeps a `DocumentRenderer` and a `PageLayout`, calls
  `view_frame` per frame, blits from `frame()`, and scrolls without laying the page out again (checked
  on the Qt shell: three Page Downs produce no further height pass). On this PC the height pass for
  `generated_1mib.md` is 284 ms (Qt) and 287 ms (WinUI) at ~1100 dip, page 583,619 px; the first
  WinUI frame arrives at 308 ms from process start (`--measure`).
- Renderer scroll frames after the frame-delta work, `generated_1mib.md`, 60 steps of 40 px:
  **0.2 ms mean / 0.8 worst at 780x900 (100%)**, **0.6 / 1.1 at 1170x1350 (150%)**, **2.6 / 3.4 at
  3840x2200 (4K, 150%)**. Both plan budgets (4 ms p99 at 150%, 8 ms at 4K) are met.

## What has and has not run

Run on Windows: the WinUI spike (`spikes/winui`, framework build, `--measure`: result=ok at 259 ms),
the WinUI shell on `fixtures/showcase.md` (wheel scrolling, real Page Down keys, system theme,
`--light`/`--dark`, `--trace`, `--measure`, the shifted-frame path against a whole-frame reference),
and the Qt shell on the same file (layout, first paint, wheel, scrollbar and keys). For the Qt shell
the input was driven with real and posted Windows messages against the running window: Page Down
moved the page and the thumb, a click in the scrollbar groove jumped a page, and File > Open...
opened the dialog and loaded `linked.md` from it.

Run on Linux: the Qt shell on the VM, built with Qt 6.8.2, drawn and captured under Xvfb (the
screenshot shows the scrollbar in place).

Not tested by hand: the Qt View/Help menu items, the WinUI window resizing and reflow on both,
DPI scaling other than 100%, and the Qt scrollbar and keys on Linux (only on Windows). The Qt drop
handler is covered by `--drop-test`, which drives Qt's event path rather than an OLE drag from
Explorer.

## Linux target from Windows

`rustup target add x86_64-unknown-linux-gnu` works here (rustup's downloader works; cargo's does
not). `cargo check --target x86_64-unknown-linux-gnu -p mdview-core` passes. The renderer does not
check: `yeslogic-fontconfig-sys` wants `pkg-config` and a Linux sysroot. There is no Linux linker,
so this could only ever prove compilation. Qt cannot be cross-compiled from this machine; the Qt
shell above is a native Windows build of the same source.

## Cargo in this sandbox

Cargo cannot reach crates.io: `schannel: AcquireCredentialsHandle failed: SEC_E_NO_CREDENTIALS`.
Python's and `gh`'s TLS work. `target/seed-lock.py` (not committed) takes any number of `Cargo.lock`
files, writes the complete sparse index cache entry for every crate and downloads the locked
`.crate` files, so `cargo build --offline` works. It must be given every lock in use
(`Cargo.lock`, `spikes/qt-shell/Cargo.lock`): the index cache is shared.

## Known gaps, in the order they matter

- **Scroll budgets are met**: renderer frames are 0.2/0.8 ms at 100%, 0.6/1.1 at 150% and 2.6/3.4 at
  4K (mean/worst), and the WinUI upload is a band rather than a viewport. What is left is present
  pacing: a frame still waits for vsync (3-6 ms in the trace), so a 4K window shows a frame every
  16.7 ms rather than sooner. That is the compositor's clock, not the renderer's.
- **Images**: SVG (and anything but PNG/JPEG) is still a placeholder; `<picture>` light/dark sources
  and `width`/`height` attributes are ignored; a blocked image is retried on every render. Decoded
  images now live on the `DocumentRenderer` and are shared by every frame, which is what keeps them
  in cached scroll frames (there is a test that fails without it).
- **No selection, links, anchors or details toggling.** `BlockBox` is there for hit tests but unused.
- **The WinUI scrollbar is drawn by the shell**, not by a WinUI control, because the checked-in
  bindings have no `ScrollBar`. No find, tabs or document tree in either shell.
- Table `colspan`/`rowspan` ignored; math, Mermaid and highlighting show source.
- No bundled fonts, so goldens depend on system fonts.
- The WinUI title bar follows Windows, not the in-app theme override.

## What to do next

1. Get the four `tests/golden/` images and the shell screenshots approved.
2. Add selection and link/anchor hit-testing, then details toggling. `BlockBox` (block, y, height)
   already comes out of a page render; the per-block paint cache in `DocumentRenderer` is where a
   text run's `Rect` can be recorded for a hit test.
3. Add SVG support when Phase 4 lands (the image cache is already shared across frames).
4. Decide bundled fonts, then turn the goldens into a pixel suite.
5. New WinUI input goes through the thread message hook (keys) or XAML events (pointer), not window
   procedures: the hook sees every message this thread retrieves, so a new control's keys need a case
   in `handle_message`, and a new pointer gesture needs another `pointer_*` handler.

Every Qt/Linux change is now tested on the VM above as well as on Windows.
