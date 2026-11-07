# Handoff: Phase 2 renderer and the first WinUI window

**Branch:** `phase2-renderer`, from `phase1-audit-fixes` (pushed at `633c0a8`). The renderer and the
first Windows shell are a checkpoint commit here; nothing is pushed. The four review images in
`tests/golden/` still have not been approved by the user.

## State

- Phase 1 closed: core audit fixes, tests and docs on `phase1-audit-fixes`. Suite passes on Windows:
  7 core, 11 fuzz, 113 regression, 3 spec, release profile. Spec counts stay 645/652 CommonMark and
  665/672 GFM.
- Phase 2: `crates/render` lays the document model out and paints a BGRA bitmap with no window
  system; `bins/mdv-bench` writes PNGs. `fixtures/showcase.md` (66 blocks) lays out in about 27 ms at
  1084 logical px and 100% scale, 3020 px tall, in both themes.
- **New: `bins/mdv-winui` is the first real window.** It runs on this PC, opens a Markdown file,
  draws the page through the renderer into a `SwapChainPanel`, and scrolls with the wheel. This is
  the first time WinUI has run on real Windows (the G8 spike had only run in a VM).
- Workspace: 7 core, 11 fuzz, 113 regression, 3 spec, 4 render tests pass; `cargo fmt --all` clean.

## The WinUI shell (`bins/mdv-winui`)

- WinUI 3 with hand-generated Windows App SDK bindings (`src/bindings.rs`, copied from
  `spikes/winui`; regenerate only with the same NuGet winmds). No XAML, C#, or C++.
- Framework-dependent build: needs the Windows App Runtime 2.6.4 or newer (x64). This PC has 2.6.5.
  The self-contained staging from the spike was not carried over; that needs the NuGet packages.
- Window is 1100x900. Menus: File > Open..., Reload, Close; View > Light theme, Dark theme. The
  wheel scrolls. The page follows the Windows app theme at startup (`AppsUseLightTheme`), and
  `--light` / `--dark` override it for screenshots.
- `src/render.rs` keeps one laid-out `Bitmap` and copies the visible slice into the swap chain
  buffer each frame, 1:1 with physical pixels. Re-layout happens when the width, DPI scale or theme
  changes; `max_height` is capped at 30,000 logical px as a guard until virtualization exists.
- `--trace` writes `mdview.trace.log` beside the exe; `--measure` writes one `result=ok` line with
  first-frame time and private bytes, then exits.
- Measured on this PC (RTX 4070 Ti SUPER, 96 DPI): first frame at 251.9 ms from process start,
  renderer layout 27.0 ms, parse 0.1 ms; `--measure` reports first frame at 259.3 ms in the spike and
  251.9 ms here, both including DLL loading.
- Screenshots: `tests/golden/shell-winui-light.png`, `tests/golden/shell-winui-dark.png`.

## What has and has not run

Run on Windows: the WinUI spike (`spikes/winui`, framework build, `--measure`: result=ok at 259 ms,
private 168 MB with the real GPU), the new shell at 1100x900 with `fixtures/showcase.md`, wheel
scrolling, system-theme selection, `--light`/`--dark`, `--trace`, `--measure`.

Not tested yet by hand: the File > Open dialog and the View theme items were wired but not clicked;
resize reflow and DPI scaling other than 100%; keyboard input (there is none yet); the spike's
self-contained flavor.

## Qt on Windows, in progress

The user asked to "compile linux variant on windows for testing", so the Qt shell is being brought
up on Windows as well:

- Qt 6.9.3 for MSVC 2022 is installed at `C:\Qt\6.9.3\msvc2022_64` with `aqtinstall` (qtbase only).
  `aqt` is in the Python user install; it also installed `cmake` and `ninja` wheels.
- `cxx-qt` 0.10 and its closure were fetched into the cargo cache (the same Python seeding trick as
  the text dependencies; script is `target/seed-lock.py`, not committed).
- `spikes/qt-shell` is the reference bridge (`src/qt/mod.rs` wrapper and `ffi.rs` C++ declarations).
  It contains Linux-only pieces (`libc::clock_gettime`, `libc::_exit`, `/proc` timing) that a Windows
  build must not use.
- **Nothing Qt has been built on Windows yet.** This section is the current attempt, not a result.

## Linux target from Windows

`rustup target add x86_64-unknown-linux-gnu` works in this shell (rustup's downloader is fine, only
cargo's is not). `cargo check --target x86_64-unknown-linux-gnu -p mdview-core` passes. The renderer
does not check yet: `yeslogic-fontconfig-sys` needs `pkg-config` and a Linux sysroot, and there is no
Linux linker here, so this only ever proves compilation, never a runnable Linux binary. Qt cannot be
cross-compiled from this machine at all.

## Cargo in this sandbox

Cargo cannot reach crates.io: `schannel: AcquireCredentialsHandle failed: SEC_E_NO_CREDENTIALS`.
Python's and `gh`'s TLS work. New dependencies were fetched with `target/seed-lock.py` (takes any
`Cargo.lock`, writes the sparse index cache and `.crate` files in cargo's layout), so plain
`cargo build`/`cargo test` work with `--offline`. If another dependency appears, seed it the same way.

## Known gaps, in the order they matter

- **No virtualization.** The whole document is laid out into one bitmap; the 1 MB scroll budget is
  unmeasured, and the 30,000 px guard truncates very long files.
- **Images** are `[image: alt]`; no PNG/JPEG/SVG loading, limits or cache.
- **No selection, links, anchors or details toggling.** `BlockBox` is there for hit tests but unused.
- **No scrollbar**, find box, tabs or tree; the wheel is the only scroll input, and there is no
  keyboard handling.
- Table `colspan`/`rowspan` ignored; math, Mermaid and highlighting show source.
- No bundled fonts, so goldens depend on system fonts.
- The window's title bar follows Windows, not the in-app theme override, so `--dark` on a light
  desktop leaves a light title bar (and the reverse).

## What to do next

1. Finish the Qt shell on Windows (or report the exact blocker) and show both windows to the user.
2. Get the four `tests/golden/` images and the two shell screenshots approved.
3. Add image loading and drawing; then virtualization and the 1 MB budget.
4. Add selection and link/anchor hit-testing, then details toggling and a scrollbar.
5. Decide bundled fonts, then turn the goldens into a pixel suite.
