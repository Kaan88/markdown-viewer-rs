# Handoff: continuing mdview on Windows

Written when work paused and moved from the Linux machine to Windows. Read this first, then [PLAN.md](PLAN.md).

## Where things stand

- **Done:** Phase 0 (spikes, Go declared) and the Phase 1 core crate, `crates/core`. It has a parser, document model, find and session logic. There is no renderer or app yet, so nothing can be opened or seen.
- **Tests on Linux:** 120 pass. CommonMark matches 645 of 652 and GFM 665 of 672, and every miss is a listed exception. These use a comparison reduced to what our allow-list can show, not byte-exact HTML; see [COVERAGE.md](COVERAGE.md).
- **Speed:** a 1 MiB document parses in about 5.9 ms on a 28-thread desktop, with a 10.91 MiB peak and 2.08 MiB retained.
- **Phase 1 is not closed.** The audit found defects that are not fixed yet. See "Do next".

**Never run on Windows:** the core crate's tests. It type-checks for `x86_64-pc-windows-msvc`, tests included, but `cfg(windows)` code has never executed, including `canonical_paths_have_one_identity_on_windows`.

**WinUI:** only an empty test skeleton exists, in `spikes/winui`. It ran in a Windows 11 VM with no GPU, never on real hardware.

## Decisions already made (don't re-ask)

- Launch speed, low memory and Markdown coverage rank equally first. Coverage means how much real-world Markdown renders correctly. Coverage work goes in lazily loaded code.
- Windows shell: WinUI 3, code only, committed regardless of measured cost. Linux shell: Qt Widgets through CXX-Qt with no authored C++. Slint was compared and rejected for its text rendering; don't reopen that on numbers alone.
- All authored code in Rust. Custom renderer, no webview. Windows 11 only. No screen-reader work. Private repo, no CI.
- A paired `<script>`, `<style>` or `<iframe>` shows as literal text, as GitHub does. **Not yet changed in code.**
- All exclusions in COVERAGE.md are accepted. Keep its "Not implemented" list current.
- Rust text must use the channel order from `spikes/text/src/lib.rs`, `Format::CustomSubpixel([0.3, 0.0, -0.3])`, not `Format::Subpixel`, which swapped red and blue and caused visible colour fringes. The real renderer needs a regression test for this. Whether to hint at lower scales is still open.

## Do next

1. **Fix the audit findings** in [AUDIT_PHASE1.md](AUDIT_PHASE1.md): 19 confirmed and 11 unverified. Reproduce each unverified one before fixing it. The largest:
   - `<object>`, `<embed>`, `<head>` and `<template>` drop their content silently, where GitHub keeps it.
   - Footnotes aren't collected at the end of the document in reference order, as GitHub does. COVERAGE.md overstates this.
   - Valid alerts are rejected when a heading or rule follows a blank line.
   - The scaling test can miss small quadratic costs (unverified).
2. **Change the script, style and iframe handling** to literal text, and update `tests/common/visible.rs`, which currently hides the difference.
3. **Run the core tests on Windows** (below). Treat any failure as a real bug, and close Phase 1.
4. **Ask the user about phase order.** The plan builds the Linux Qt shell (Phase 3) before WinUI (Phase 5), but the Qt shell can't be built on Windows. Working from Windows, the natural order is Phase 2, then a first WinUI window. It is the user's call. They want something they can open real files in as early as possible, so build a bare window as soon as the renderer can draw a page.

## Setting up on Windows

1. Install Git, the GitHub CLI, Visual Studio Build Tools with the C++ workload and Windows SDK, and rustup. Linux used Rust 1.99.1.
2. Clone and test:

```powershell
gh repo clone Kaan88/markdown-viewer-rs
cd markdown-viewer-rs
cargo test --release -p mdview-core
```

`.gitattributes` keeps the test data on LF line endings. This has not been tried on a real Windows clone.

**WinUI spike:** its scripts are for cross-building from Linux and won't run on Windows as they are: `build-win.sh` (uses `cargo xwin`), `stage.sh` and `see-it.sh`. The two `cargo xwin build` lines there should become plain `cargo build`, untested. `stage.sh` shows how to lay out the folder and needs porting to PowerShell. Built output is not in the repo. `spikes/winui/W0.md` is the checklist for a first run on real hardware, and it has never been done.

## What stayed on the Linux machine

- Claude's memory and this session's history. The parts that matter are copied here and in PLAN.md.
- Build outputs, the Docker `WinApps` VM, and the private toolchain in `~/.local/rust-win`.
- Probe code for the audit, in `~/.cache/mdview-audit`. The audit quotes the inputs, so nothing depends on it.

The Linux desktop froze during this work with heavy builds running. The cause was never found. On any machine, keep build jobs moderate and don't copy the repo into a RAM-backed temp folder.
