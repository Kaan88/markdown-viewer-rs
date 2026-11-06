# Look spike: real Qt Widgets with real Breeze

**Status: PASS for the build and the brief on KDE. NEEDS_USER for two things: whether this look is good enough, and what it means for Windows.**

- **Look:** I think it no longer looks old, but that is your call. Run `./run.sh`, or start with `screenshots/light@1.0-find.png` and `screenshots/dark@1.5-find.png`.
- **Windows 11 (the real target):** nothing here ran on Windows, and Breeze is not a Windows style. See "What this says about Windows 11".

Environment: Qt 6.12.0, breeze 6.8.1, KWin 6.8.1, cxx-qt 0.10.0, cargo 1.99.1. No authored C++ or QML (`find` shows none outside `target/`).

## Results

| Item | Result | Evidence (measured) |
|---|---|---|
| Real Breeze, not Fusion | PASS | `style=breeze` in every `PRESENTED` line, real session included |
| Launch | PASS, under the plan's 160 ms | 63.4 ms median on the real session (38 runs, noisy) |
| Private memory, 1220x820 | See "Memory" | Scale 1.0: 20.0 MiB dirty; total about 26 or 44 MiB, depending on library sharing |
| Stylesheet launch cost | Too small to measure | Widget sheets +0.45 ms, within noise (p10-p90 is about 57-73 ms) |
| Page blitted without resampling | PASS | 8 of 8 captures: 0 differing pixels |
| Ctrl+F, Esc, hover, app menu, submenu, wheel, tab click | PASS | Real compositor input in virtual KWin, 5 sessions. `results/real-input-trace.txt` |
| Follows a live system light/dark change | PASS for the app | A bug found and fixed, see below. The title bar did not change in that test |
| `--theme`, `--measure`, `--screenshot`, `run.sh` | PASS | `--screenshot` output also has 0 differing pixels (1 capture, scale 1.5) |
| Self-test | PASS, 8 of 8 | Real session and virtual KWin. `results/real-session-output.txt` |
| Visual iteration | PASS | 5 rounds of screenshots read, defects below |

## Stylesheet cost: use small per-widget sheets

Measured on the real session at scale 1.0. Each round ran every variant once, for 40 rounds, dropping the first 2 (`results/launch-round-robin.txt`). Noisy: load 1.6-4.

| Variant | Present ms, median | p10-p90 |
|---|---|---|
| No stylesheet (`--qss=none`) | 62.98 | 57.0-75.0 |
| Per-widget sheets (chosen) | 63.43 | 57.1-73.4 |
| Same rules on the whole app (`--qss=app`) | 63.63 | 57.8-83.3 |
| Per-widget, find bar open | 63.46 | 57.0-73.5 |
| Per-widget, find bar open, no shadow | 63.11 | 57.2-71.1 |

Private dirty memory is steadier, and it separates the variants better than the timings do (median of 12 runs, same in each of 3 batches to within 0.01 MiB, except one batch noted below):

| Variant | Private dirty MiB | Added |
|---|---|---|
| No stylesheet | 19.70 | |
| Per-widget sheets | 20.01 | +0.31 |
| App-wide sheet | 20.25 | +0.55 |
| Find bar open, no shadow | 20.18 | +0.48 |
| Find bar open, with shadow | 20.57 (24.32 in batch 3, about one extra window buffer) | +0.87 |
| `qt-shell` skeleton, for reference | 22.03 | It holds a full-window Rust checkerboard, so this isn't a fair floor |

So the look costs almost nothing. The chrome and stylesheets are within about 3 ms of the empty `qt-shell` skeleton (58.3-62.2 ms in the same batches, `results/launch-real-session.txt`). That comparison is rough, since these are batch medians and they overlap.

**These numbers are not a 20 KB document launch.** The page is a ready-made image, so no parsing, layout or text work is included.

## Memory

MiB, 12 runs each, read 800 ms after present.

| Where | Present ms | Private dirty | Private total, median (range) |
|---|---|---|---|
| Virtual KWin, 1.0 | 61.03 | 19.96 | 25.88 (22.0-26.1) |
| Virtual KWin, 1.5 light | 82.19 (noisy, raw 71-124) | 24.51 | 33.25 (24.5-42.0) |
| Virtual KWin, 1.5 dark | 61.72 | 24.52 | 33.29 (33.2-42.0) |
| Virtual KWin, 2.2 | 60.52 | 30.11 | 43.09 (28.8-57.4) |
| Real session, 1.0, chosen variant | 63.43 | 20.01 | 30.05 (26.0-47.4) |

**Private total is not stable on the real session.** Private_Clean jumps between about 6 and 23 MiB while Private_Dirty does not move. The `qt-shell` skeleton does the same. My inference, not tested, is that library pages count as private when no other process has them mapped at that moment. Compare builds on Private_Dirty.

The plan's limit is 45 MB, which is 42.9 MiB. The high real-session values (43-47 MiB) and the 2.2 median (43.09 MiB) sit just over it.

The page is memory-mapped, so only what is shown gets touched. That counts as Private_Clean: about 2.2 MiB at 1.0 and 4.9 MiB at 1.5 (calculated from the visible area). A real renderer will cost differently.

## Pixel exactness

`scripts/pixelcheck.py` finds the page in a full KWin capture and compares every canvas pixel except the 12 px scrollbar strip. Light and dark, at 1.0 and 1.5, at the top and scrolled by 900: 8 of 8 with 0 differing pixels, 577,564 or 1,299,519 compared each (`results/pixelcheck.txt`). The find bar was closed in these captures.

## Where Qt Widgets resisted

Most of the modern look came cheaply, from palettes, flat `QToolButton`s and frameless trees. These parts fought back:

1. **System icons can't be loaded properly from Rust.** `QIcon::fromTheme` returns `QIcon` by value, and cxx won't accept that without a C++ helper. I load SVGs from `/usr/share/icons/breeze{,-dark}/actions/16/` instead. This ignores a custom icon theme, and those paths don't exist on Windows. **A real shell has to solve this.** Binding `KIconEngine` might work on KDE, but I didn't try it.
2. **Default Breeze tabs are the "old" look.** See `screenshots/light@1.0-noqss.png`: boxed grey tabs, even in document mode. Slim pill tabs needed a stylesheet.
3. **Tree branch lines are drawn by Breeze** and are a user-level setting. Removing them for this app needed a stylesheet, and that sheet must then name its own arrow icons.
4. **The selected row starts at the text, not at the row's edge.** Asking for full-row selection (`show-decoration-selected`) drew a separate square block beside the pill, so I took it out. Fixing this properly needs a custom delegate.
5. **Stylesheets block later palette changes.** After a live dark-to-light switch the trees stayed dark and the arrows disappeared. The sheet now has to be removed, the palette set, and the sheet reapplied.
6. **`changeEvent` doesn't receive the application palette change.** The app has to watch the `QWindow`'s events instead.
7. **The overlay scrollbar is only partly an overlay.** It's a `QScrollBar` placed over the page, with a stylesheet. It is always visible, with no fade or animated widening, because I didn't build those. The hover style is written but wasn't captured.
8. **Find bar:** rounded corners need a stylesheet and the shadow needs `QGraphicsEffect`. It can't be real acrylic, and it covers the end of the H1 at the default width.
9. **A menu holding only submenus is sized too tightly.** The arrows touched the labels until I set a minimum width.
10. **A 1 px splitter is also a 1 px drag target,** so it may be hard to grab. Drag was not tested.
11. **A negative blit position rounds the other way on fractional origins.** Drawing at `y = -scroll` shifted the page 1 px at scale 1.5, found by the pixel check. It now draws from the scroll row at `y = 0`.
12. **Breeze takes hover and focus tints from `kdeglobals`, not the palette.** So `--theme` can't fully override them. The screenshot sessions seed a matching file. A hovered selected row also turns lighter than a selected one.
13. **Two accents on your real setup.** Your system accent is a dark green, while the shared page's links are blue. The chrome also has to adjust that green to stay readable on dark. See `screenshots/system@1.0-*.png`. In the `--theme` screenshots, the palette uses the page's accent, so there's only one.

Small bridge issues: `setGraphicsEffect` needs the exact pointer type, and a `fill_rect` binding clashed with a cxx-qt-lib symbol until renamed.

## What this says about Windows 11

**This spike can't answer how Qt looks on Windows 11.**

- **Won't carry over:** Breeze itself, and so all of its look, the icon paths, `.colors` parsing, `kdeglobals`, and items 3, 5 (untested there) and 12 above.
- **Should carry over, but untested:** the layout, palette-derived colours, small stylesheets, the blit and the bridge.
- **Decision for you:** if Windows 11 is the only target, this comparison should be repeated with Qt's `windows11` style on Windows before it decides the toolkit.

## Not verified

- **How it looks on your real desktop.** All screenshots come from virtual KWin, including the one with a copy of your `kdeglobals`. On the real session I only ran the self-test, the measurements and a short `run.sh` start, without viewing the result.
- **A physical mouse and keyboard.** Input was KWin's fake-input protocol, which does go through the compositor and real Wayland events.
- **Not tried:** splitter drag, tab drag, the close button by click, scrollbar drag and hover, tooltips, the sidebar buttons with real input (self-test clicks only).
- **Title bar on a live scheme change.** It stayed dark in my test, because that test told only the app, not KWin. On a real scheme change KWin should update it (expected, not seen).
- **Scales 1.7 and 2.2 visually.** 2.2 was only measured. Other scales have no matching image, so the nearest is shown 1:1.
- **Cold launch.** All runs are warm.
- **Not real features:** find doesn't search ("2 of 14" is fixed text), the outline's current heading doesn't track scrolling, tree clicks do nothing, tabs show the same page, zoom is a label, and most menu commands do nothing. Only Find and Close Find Bar are wired.

## Reproduce

```sh
cd spikes/look-qt
cargo build --release -j 6
./run.sh                                  # live window, no timeout; try --theme=light, --find-open
target/release/look-qt --measure          # prints PRESENTED, PRESENT_MS, MEMORY, then exits
target/release/look-qt --screenshot=a.png # QWidget::render to PNG, then exits

# Needs spikes/qt-shell/tools built (kwin_capture, harness). All of these stay in virtual KWin:
scripts/shots.sh OUT 1.0:light 1.5:light 1.0:dark 1.5:dark 1.0:system
INNER=input_session.py scripts/shots.sh OUT 1.0:dark
INNER=follow_session.sh scripts/shots.sh OUT 1.0:dark
INNER=measure_scaled_session.sh scripts/shots.sh OUT 1.5:light
scripts/pixelcheck.py OUT/.light@1.5-plain.full.png ../look-shared/content-light@1.5-1170x3600.bgra 1170 3600 0 1167 1131

# These open short-lived windows on your real desktop:
scripts/measure.sh results/launch-real-session.txt 3 12
scripts/measure_rr.py 40
```

Other options: `--qss=none|widgets|app`, `--no-shadow`, `--scroll=N`, `--win=WxH`, `--self-test`, `--trace`, `--stay-ms=N`.

`src/shell.rs` is safe Rust (`#![forbid(unsafe_code)]`). All `unsafe` is in `src/qt/`, with the same caveat as `qt-shell`: handles are raw pointers that stay valid only because nothing is ever deleted. The build prints dead-code warnings for bindings left unused after iterating. Size: 2,150 lines of Rust, a 1.5 MB binary, and `target/` at 199 MiB (ignored by git).

## Visual iterations

1. **First capture:** branch lines looked dated, and the checked toolbar buttons were heavy grey boxes. Trees got branch rules and the buttons became plain flat ones. The dark selected tab was nearly invisible, so it got its own fill.
2. **Full-row selection** drew a detached square, so I removed it. The current heading now has a tint and bold text.
3. **Selected heading** lost its accent colour, because selection overrides the item's text colour. Fixed through the outline's palette. With your green accent it was unreadable on dark, so the accent is now adjusted to at least 4.5:1 contrast.
4. **Real input:** the app menu popup never appeared with synthetic clicks, probably because a Wayland popup needs a real input event (inferred: Qt created the surface, then destroyed it within about 1 ms). I built the fake-input driver, which then showed the cramped menu (fixed).
5. **Live scheme change:** trees stayed dark and the arrows vanished (fixed). Tab close buttons got more padding.

`screenshots/` holds 58 virtual KWin captures. `follow-live-dark-to-light.png` shows the live switch, with the title bar still dark. From this final set I read 5 at full size and 9 more on one reduced contact sheet. The other 44 were not viewed individually, though many repeat the same states seen in earlier rounds, so a defect that appears in only one of them could be missed.
