# Windows handoff (2026-11-10)

> **Status 2026-11-11:** the first session on the physical PC is done (HANDOFF section 26). Section 1
> builds and passes there. Of section 2, the menu checks no longer apply (there is no menu bar since
> layout A), theme, tabs, find, selection, copy, links and the Files pane were exercised; DPI other
> than 100%, a second monitor, `--measure` and the installer were not. Section 3's bug is moot.
> Section 4's gaps are closed except where HANDOFF section 11 still lists them.

For whoever picks this up on a physical Windows 11 PC. Everything below was run on the WinApps Windows 11
VM (Docker, no GPU, 8 cores). Nothing here has run on real hardware, so that is the first job. Longer
background: `docs/HANDOFF.md` (sections 7, 17, 21, 22, 24).

## 1. Build and test (about 10 minutes cold)

```powershell
cd <repo>
cargo test --release --no-fail-fast --workspace   # needs spikes\parser\data (the spec suites read it)
cargo build --release --workspace                 # also builds target\release\mdview-engine.exe
cargo build --release -p mdview-winui
cargo fmt --all --check
```

State at the VM run: every suite passed (core 17+7+11+117+3, engine 14+7, render 10+22, golden 1, winui args 2).
`cargo fmt --check` was clean on Linux; rustfmt is not installed in the VM, so run it on the PC.
Qt shell (optional on Windows; `bins\mdv-qt`, Qt 6.9.3 MSVC): built and `--tabs-test` passed on the VM.
`system_is_dark()` in the Qt shell now returns false on Windows (see section 4); WinUI follows Windows itself.

Installer: `packaging\` has the Inno Setup script; it compiled and installed on the VM (HANDOFF section 18).
Not run on the PC.

## 2. Hands-on checks to do first (not exercised by a human anywhere)

1. Launch `mdview-winui.exe fixtures\showcase.md`. Does the window appear on screen with a menu bar?
2. **Menu reliability (the user's "menus don't show" report).** Open File, Recent, Edit, Outline and View
   by mouse and by keyboard (Alt, arrows, Enter). Pick an item, then open another menu, repeat ten times.
   See section 3 for what failed on the VM.
3. Theme: View > System / Light / Dark theme. The page, menu text and tab strip should all follow. Restart:
   the choice should persist (System follows Windows).
4. Real keys: PageDown/Space, Home/End, arrows, Ctrl+= / Ctrl+- / Ctrl+0, F5, Ctrl+F then typing, Enter, F3,
   Escape, Ctrl+Tab, Ctrl+W, Ctrl+B, Ctrl+Shift+O.
5. Tabs: open two files, click tabs, close with the X, middle click, restart (session restores both).
6. Files pane: File > Open folder, expand folders with the chevron, click a file; check a deep tree.
7. Selection and copy: drag across text, Ctrl+C, paste elsewhere; click links, `#anchors`, a `<details>` summary.
8. A second launch with another file should open a tab in the first window and bring it forward (the VM
   could not tell whether Windows raised it or only flashed the taskbar).
9. High DPI and a second monitor: 125/150/200% scaling; drag between monitors. DPI scaling other than 100% has
   never been tested.
10. Performance on the real GPU: `--measure`, and scroll with the mouse wheel on a long document.
11. Run the installer, launch from the Start menu, uninstall.

## 3. Open bug: the menu bar stops responding (found on the VM)

Repro (VM, UI Automation plus real `mouse_event` clicks, window moved to 0,0 because the VM desktop is 1024x768):
launch with `fixtures\showcase.md`, then click View > Light theme (works, setting saved), then open View again:
the popup may show, but clicking its items does nothing, keyboard Down/Enter does nothing, and a later
search for the popup items finds none. File/Edit items worked before any theme item was used (View > Edit > Find
fired). Observed several times, not once in the earlier 25/25 popup-open runs (those only opened popups, never
selected an item afterwards).

Ruled out by experiment (each variant was built and run): changing the root background, changing
`RequestedTheme`, saving the session / rebuilding the Recent menu, redrawing the page, and running the theme
change outside the item's `Click` (deferred through a one-shot `SetTimer`). None changed the result, so the
theme work itself is not the cause; something about using a `MenuFlyoutItem` once may break the `MenuBar`
afterward. It may be a VM artifact (software rendering, synthetic input), which is why the first thing to do is
check section 2 item 2 on the PC. If it reproduces there, next ideas: log the MenuBar's open/closed state, try
`MenuBarItem` flyout `Closed` handling, try menu items without `forget()`-ed handlers, or replace the Win32
timer-based work in the handlers.

## 4. Known gaps (WinUI)

- Find has no text box (query and `2 of 4` in the window title; the trimmed bindings have no `TextBox`).
- Outline is a menu, not a dock; the scrollbar is drawn by the shell; the title bar follows Windows, not the
  in-app theme.
- Files pane: a click on a folder row does not expand it (only the chevron). Fixing it needs a `Collapsed` event
  and `IsExpanded` getter in `bindings.rs`, which the generated bindings leave unbound (hand-written like
  `Expanding`). No keyboard navigation test, no resizable pane.
- No self-update (foundation only, HANDOFF section 14). No screen-reader work by decision.
- Theme: View > System theme follows Windows as read at the click; it does not react to Windows switching
  theme later.
- Qt on Windows: dark scheme detection returns light.

## 5. VM gotchas (so they are not rediscovered)

- The scripts are in `~/mdview-winchk` on the Linux host (not in the repo): `sync.cmd` (robocopy to `C:\mv`),
  `winui.cmd` (build), `qt.cmd`, `wtest.cmd`, `shot6.ps1` (the menu repro). Run with
  `winapps manual '\\tsclient\home\mdview-winchk\X.cmd'`.
- `robocopy /MIR` deletes `C:\mv\tmp` (test folders live there) and the sync excludes `spikes`, which the spec
  tests need: copy `spikes\parser\data` too or the two spec suites fail with "path not found".
- Cargo stops at the first failing crate; use `--no-fail-fast`.
- The VM desktop is 1024x768 at times and the window opens at 883,299, mostly off screen: move it with
  `MoveWindow` before scripted clicks or every UI Automation rectangle is off screen.
- The Docker container can be stopped (`docker start WinApps`).

## 6. Linux side (for reference)

AppImage builds in Ubuntu 24.04 (`packaging/appimage/build-in-container.sh`), 35 MB, needs glibc 2.39+ (HANDOFF
section 23). Not run on a clean Ubuntu machine.
