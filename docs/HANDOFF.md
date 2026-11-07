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

## The WinUI shell (`bins/mdv-winui`)

- WinUI 3 with hand-generated Windows App SDK bindings (`src/bindings.rs`, copied from
  `spikes/winui`; regenerate only with the same NuGet winmds). No XAML, C#, or C++.
- Framework-dependent build: needs the Windows App Runtime 2.6.4 or newer (x64). This PC has 2.6.5.
- Window is 1100x900. Menus: File > Open..., Reload, Close; View > Light theme, Dark theme. The
  wheel scrolls. The page follows the Windows app theme at startup (`AppsUseLightTheme`), and
  `--light` / `--dark` override it for screenshots. The menu row is painted the page background.
- `src/render.rs` keeps one laid-out `Bitmap` and copies the visible slice into the swap chain
  buffer each frame, 1:1 with physical pixels. Re-layout happens when the width, DPI scale or theme
  changes; `max_height` is capped at 30,000 logical px until virtualization exists.
- `--trace` writes `mdview.trace.log` beside the exe; `--measure` writes one `result=ok` line and
  exits.
- Measured on this PC (RTX 4070 Ti SUPER, 96 DPI): first frame at 251.9 ms from process start,
  renderer layout 27.0 ms, parse 0.1 ms.
- Screenshots: `tests/golden/shell-winui-light.png`, `tests/golden/shell-winui-dark.png`.

## The Qt shell (`bins/mdv-qt`)

- Qt Widgets through CXX-Qt 0.10, ported from `spikes/qt-shell`: `src/qt/mod.rs` (the only module
  with `unsafe`) and `src/qt/ffi.rs` (C++ declarations, no authored C++). `src/shell.rs` is
  `#![forbid(unsafe_code)]`.
- **This crate is excluded from the workspace** (`Cargo.toml`), because it needs a Qt installation
  to build. It has its own `Cargo.lock` (the Qt-only graph, seeded from the spike's lock) and its
  own target directory.
- Window 1100x900, menus File > Quit, View > Light/Dark theme, Help > About, a status bar with the
  file name and block count. A file comes from the command line or is dropped on the window
  (Qt file URIs, `%20` decoded). There is no File > Open dialog yet.
- `src/shell.rs` renders the page with `mdview-render` for the canvas width and device pixel ratio,
  copies the visible slice into a BGRX buffer and blits it through QPainter with an identity
  transform. The wheel moves the scroll offset and repaints.
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
- Measured here: parse 0.1 ms, layout 27.0 ms at 1100 dip, 100% scale, page 1100x3020; window opens
  with the page drawn and the wheel scrolls it (checked from screenshots).
- Screenshots: `tests/golden/shell-qt-light.png` (page top) and `shell-qt-scrolled.png`.
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
  across frames), `layout(doc, options) -> PageLayout` (one height pass over every top-level block)
  and `view(doc, options, &PageLayout, top, height) -> Slice` (one viewport-sized bitmap). `render`
  still paints a whole page for tests; `render_slice` is the one-shot layout+view form.
- Measured on `spikes/parser/data/generated_1mib.md` (1.0 MB, 25,060 blocks, 897,583 bytes of text,
  583,619 px page): parse 8.3 ms; height pass 8.9 ms; 40 scroll frames at 780x900 at 100%: mean
  5.9 ms, worst 7.8 ms; glyph cache 99% hit, peak 0.2 MiB. The bitmap is the viewport (2.8 MB) rather
  than the page (1.8 GB at 780x583,619x4).
- The plan's budget is 4 ms p99 per scroll frame at 150%. **Not met yet**: every frame re-shapes the
  visible blocks. Caching each block's laid-out text between frames is the next step.
- The **Qt shell is wired to this API**: it keeps a `DocumentRenderer` and a `PageLayout`, paints
  one viewport slice per frame, and scrolls without laying the page out again. On this PC it opens
  `generated_1mib.md` in 284 ms (height pass at 1100 dip) with a 583,619 px page. The WinUI shell
  still paints a whole page, capped at 30,000 px.

## What has and has not run

Run on Windows: the WinUI spike (`spikes/winui`, framework build, `--measure`: result=ok at 259 ms),
the WinUI shell on `fixtures/showcase.md` (wheel scrolling, system theme, `--light`/`--dark`,
`--trace`), and the Qt shell on the same file (layout, first paint, wheel scrolling).

Run on Linux: the Qt shell on the VM, built with Qt 6.8.2, drawn and captured under Xvfb.

Not tested by hand: the WinUI File > Open dialog and View theme items (wired, not clicked); the Qt
menus (Quit, Light/Dark, About) and drag and drop; window resizing and reflow on both; DPI scaling
other than 100%; keyboard input (neither shell has any).

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

- **Virtualization is in Qt only.** The WinUI shell still paints a whole page (30,000 px cap).
  Scroll frames average 5.9 ms on the 1 MB fixture against a 4 ms p99 budget, because visible
  blocks are re-shaped every frame.
- **Images**: SVG (and anything but PNG/JPEG) is still a placeholder; images are decoded once per
  render rather than shared across renders; `<picture>` light/dark sources and `width`/`height`
  attributes are ignored; a blocked image is retried on every render.
- **No selection, links, anchors or details toggling.** `BlockBox` is there for hit tests but unused.
- **No scrollbar or keyboard** in either shell; the wheel is the only scroll input. No File > Open in
  the Qt shell; no find, tabs or tree in either.
- Table `colspan`/`rowspan` ignored; math, Mermaid and highlighting show source.
- No bundled fonts, so goldens depend on system fonts.
- The WinUI title bar follows Windows, not the in-app theme override.

## What to do next

1. Get the four `tests/golden/` images and the shell screenshots approved.
2. Wire the WinUI shell to `DocumentRenderer`/`PageLayout`, then cache laid-out blocks to reach
   the 4 ms scroll budget.
3. Add File > Open to the Qt shell, a scrollbar and keyboard scrolling to both.
4. Add selection and link/anchor hit-testing, then details toggling.
5. Add a shared image cache and SVG support when Phase 4 lands.
6. Decide bundled fonts, then turn the goldens into a pixel suite.

Every Qt/Linux change is now tested on the VM above as well as on Windows.
