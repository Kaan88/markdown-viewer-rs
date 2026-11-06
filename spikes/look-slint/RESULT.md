# Look spike: Slint 1.18.1, winit backend, software renderer

Environment: slint 1.18.1, winit 0.30.13, softbuffer 0.4.8, rustc 1.99.1, KWin 6.8.1, real session 3440x1440 at 144 Hz, scale 1.0.
**All timings are noisy.** Other work was running (load average 2.6 to 8). Everything was measured on Linux. **Nothing here ran on Windows, which is the actual target.**

## Results

| Item | Result | Evidence |
|---|---|---|
| Dependency tree: winit + software only, no Qt, GPU backend or Skia | **PASS, one note** | `results/dep-proof.txt`. `tiny-skia` is present, see below |
| Pixel exactness at 1.5, 1.7, 2.2 (G2 equivalent) | **PASS** | 57 of 57 cases byte exact in compositor captures; 9 of 9 negative controls fail |
| Wayland server-side decoration | **PASS** | Trace: `set_mode(2)`, then KWin `configure(2)`. Title bar visible in captures |
| Popup menus | **PASS, in-window only** | Real seat input in nested KWin. They cannot leave the window |
| Launch, scale 1.0 | **44.31 ms** (measured, 12 runs, drop 2) | About 12 ms under the Qt skeleton's 56 ms, with more UI drawn |
| Private memory, scale 1.0 | **15.44 MiB** (measured, 12 runs) | Stays at 22.05 MiB at 2.2 |
| Scrolling at 60 Hz | **PASS** (median 6.60 ms at 2.2) | The blit is about 11x slower than memcpy, see below |
| Licence | **Recorded** | Attribution is required unless GPLv3 or commercial, see below |
| Windows 11 build and look | **BLOCKED here, NEEDS_USER** | Not compiled: the Windows Rust std and mingw are not installed |
| Does it look right | **NEEDS_USER** | `./run.sh`, and the images in `screenshots/` |

**Biggest risks for the decision:**
1. **Chrome text is grayscale and unhinted.** The document uses LCD masks, so the two sit side by side with visibly different text. It also isn't DirectWrite, so on Windows it won't match system text.
2. **The renderer drops images scrolled past 4095 device px.** This is avoidable, and the default mode here avoids it, but a real viewer must never hand Slint a tall page.
3. **No Mica or acrylic.** Software rendering gives an opaque surface and I did not attempt a system backdrop. Soft shadows are also unavailable and had to be faked.

## Dependency proof

Features: `default-features = false`, then `std`, `compat-1-2`, `backend-winit`, `backend-winit-wayland`, `renderer-software`, `unstable-winit-030`. `slint-build` is a build dependency only.

`cargo tree -e normal` (316 crates on this host; 237 for `--target x86_64-pc-windows-msvc`) has no Qt, `i-slint-renderer-skia`, femtovg, glutin, wgpu, vello, OpenGL or Vulkan crate. `ldd` and the process's own maps (`results/loaded-libs.txt`) show no libEGL, libGL or libvulkan.

**`tiny-skia` is in the tree,** in two versions. It is a pure-Rust CPU rasteriser, not Skia, and no C++ comes with it. I don't count it as a violation, but a name-based check will find it:
- `tiny-skia 0.11` comes from `sctk-adwaita`, through winit's client-side decoration. Slint's `wayland` feature turns this on and can't be opted out of. It isn't in the Windows tree.
- `tiny-skia 0.12` comes from `resvg`, through `i-slint-common`. It is in the Windows tree too.

Also observed:
- `libX11` and `libxcb` are mapped at runtime although the X11 feature is off. I did not trace which crate loads them.
- `zbus` is a direct dependency of the winit backend, and `xdg-desktop-portal` got activated in the nested session. I did not establish that this app caused the activation.
- The binary is 20.5 MiB (16.5 MiB fully stripped). `target/` is 1.4 GiB.
- Authored code: Rust and `.slint` only, no C++ and no QML. `#![deny(unsafe_code)]` is set, and there is no authored `unsafe`. It is `deny` rather than `forbid` because Slint's generated code contains `allow` attributes that `forbid` rejects.

## Licence (checked 2026-11-06)

Slint 1.18.1 declares `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`. I read the licence file shipped in the crate, `LICENSES/LicenseRef-Slint-Royalty-free-2.0.md`. This is my reading, not legal advice.

**Royalty-free 2.0, section 2:** you may distribute Slint as part of an application if you do either of these:
- **(a)** Display the `AboutSlint` widget "in an 'About' screen or dialog that is accessible from the top level menu of the Application". With no such screen, it goes in the splash screen.
- **(b)** Display the Slint attribution badge (`MadeWithSlint-logo-whitebg.png`) "on a public webpage, preferably where the binaries of your Application can be downloaded from", where any visitor can easily find it.

**Section 3 limits:** no distributing Slint alone, no embedded systems, no application that exposes Slint's APIs, and no removing licence notices from the source.

**Other routes:** GPLv3 (the whole app must then comply with GPLv3) or the paid commercial licence.

This spike does (a): Help, About mdview shows `AboutSlint`, as a large white #MadeWithSlint logo that can't be restyled to match (`screenshots/dark-about-1.0.png`). A real app would probably prefer option (b).

## Pixel exactness (G2 equivalent)

**Method:** same as `spikes/qt-shell`. Each scale runs in a private `kwin_wayland --virtual`, with `org.kde.KWin.ScreenShot2` captures. `pixelcheck` and `kwin_capture` are the existing binaries from `qt-shell/tools`. `scripts/compare_page.py` is new.

**Result (measured, one full run on the final binary, `results/g2-results.txt`), 23 cases at each of 3 scales:**

| Case type | Count | Result |
|---|---|---|
| 1 px checkerboard: base, odd pane widths (x2), odd window size, 2 window moves, live resize, RGBA, no origin snap, smooth filter | 10 x 3 | 30 of 30, `non_bw=0 parity_errors=0` |
| Real document, every RGB byte against the shared `.bgra`: light and dark, several scroll offsets, odd panes, wide window, viewport and full-page modes | 9 x 3 | 27 of 27, `differing=0` |
| Negative controls: naive logical-size stretch; image 1 device px too wide (nearest and smooth) | 3 x 3 | 9 of 9 fail as they must, 580,608 to 1,426,856 parity errors |
| Information only: `SLINT_SCALE_FACTOR=2` | 1 x 3 | Passes, but isn't a control: the window just draws larger, with no resampling |

**How it works:** the image's size in `phx` equals its buffer, so the renderer's scale is exactly 1. `no-snap` and `smooth` also pass, so origin snapping and the filter setting aren't what makes it exact: the renderer rounds the target rectangle. I kept the snapping so the code states its intent.

**Limits:**
- At 1.7 and 2.2 the window is about 1 logical px larger than requested (1221.2x821.2, 1220.9x820.9), because Slint rounds up. The document region is 2 device px wider than the page, and the page centres in it.
- ScreenShot2 reads the compositor's scene, not the monitor. This is the same limit as the Qt spike.
- Live scale changes and Windows DWM were not tested.

## Per-frame scroll cost (measured, noisy)

`--bench-scroll=300`: one run per row, 300 frames, step 37 device px, driven by frame callbacks. "Total" runs from the start of the frame to after present and **includes** the fill. Medians, with p95 in brackets, in ms.

| Scale | Viewport px | Fill (read + convert) | Total, viewport RGB | Total, full-page RGB | Plain memcpy, same area |
|---|---|---|---|---|---|
| 1.0 (real, 144 Hz) | 780x756 | 0.50 (0.98) | 1.54 (3.00) | 1.05 (1.93) | 0.10 |
| 1.5 (nested, 60 Hz) | 1170x1134 | 1.17 (2.46) | 3.43 (7.04) | 2.27 (4.10) | 0.23 |
| 1.7 (nested, 60 Hz) | 1328x1287 | 1.20 (2.29) | 3.82 (6.83) | 2.91 (5.51) | 0.25 |
| 2.2 (nested, 60 Hz) | 1718x1665 | 2.09 (3.41) | 6.60 (9.55) | 4.61 (7.87) | 0.39 |
| 2.2, large window | 2772x2059 | 2.70 (3.61) | 7.85 (10.54) | not run | 1.00 |

- **Frame pacing held.** Interval medians were 6.94 ms on the real display and about 16.68 ms nested. The worst single intervals were about 32 ms, consistent with an occasional missed frame.
- **The blit is the main cost.** It takes about 4.5 ms at 2.2 against 0.39 ms for memcpy, about 11x. `draw_texture_line` loops per pixel with a bounds-checked slice, and there is no memcpy path even at scale 1 (read from source).
- **At 144 Hz and 2.2 it would be tight** (estimate: the budget is 6.94 ms, and the 6.60 ms median was measured under 60 Hz nested). There is headroom at 1.0, where this desktop runs.
- **RGB and RGBA cost the same,** within noise. 3 bytes per pixel saves memory (30.31 against 35.80 MiB private at 2.2 after scrolling).
- **Partial rendering works.** Each scrolled frame damages only the document rectangle: `damage_buffer(230, 40, 780, 756)`, `results/wayland-trace-bench.txt`.

### The full-page image limit (measured)

`SceneTextureExtra.off_y` is `Fixed<u16, 4>`, so offsets above about 4095 source px don't fit. I read that in source, then confirmed it: at 2.2 with an 800x400 window scrolled to 4541, **the document turned blank** (`results/bench/overflow-full-2.2.png`), with no error or log line. Viewport mode at the same place is byte exact (`differing=0`). At 1.5 and 1.7, where the offsets stay under the limit, both modes are exact.

Viewport mode is therefore the default: Rust reads only the visible rows into a viewport-sized buffer, and swaps between two buffers so a scroll doesn't allocate. It costs an extra copy per frame, but it is the only safe path, and it keeps memory low. A better design would own the surface through a custom `slint::platform` and skip the second copy. I did not try that.

## Launch and memory (measured)

`qt-shell/tools` harness, 12 runs, drop 2 for time, read 800 ms after present, 1220x820. Qt numbers are copied from `spikes/qt-shell/RESULT.md` (first run, then its re-check where there is one) and were taken on a different, also noisy, day's load. The Qt skeleton is also a different UI. Launch is warm in both, and neither has parsing or real text layout, so these are floors.

| Scale | Present ms | Private MiB | Private_Dirty | Shared_Dirty (shm) | Pss MiB | Qt private MiB | Qt Pss MiB |
|---|---|---|---|---|---|---|---|
| 1.0 (real) | 44.31 | 15.44 | 5.51 | 7.63 | 20.82 | 25.18 / 22.83 | 36.65 / 30.95 |
| 1.5 (nested) | 59.99 | 17.54 | 7.61 | 17.18 | 27.21 | 39.66 / 34.13 | 49.34 / 45.31 |
| 1.7 (nested) | 48.22 | 18.65 | 8.74 | 22.12 | 30.78 | 44.63 (re-check) | 55.06 |
| 2.2 (nested) | 60.49 | 22.05 | 12.13 | 37.02 | 41.67 | 64.52 / 63.25 | 79.47 / 77.47 |

- **Read Pss rather than private when comparing.** Slint's shm buffers land in Shared_Dirty here, which private leaves out. The memory ranges are tight, unlike Qt's stepped ranges: 15.26 to 15.54 at 1.0, 21.92 to 22.16 at 2.2.
- **Private memory rises after scrolling,** because the second viewport buffer gets allocated: 17.07 MiB at 1.0 and 30.31 MiB at 2.2 at the end of a bench run.
- **Launch timing is bimodal in nested runs,** about 44-48 or 60 ms, one frame apart at 60 Hz. Raw values are in `results/bench/`.
- **The first launch in each fresh nested KWin** was slower: 200.73, 215.74, 212.01 ms (3 runs). The harness runs came later in the same session. Cause not established.
- **Where time goes (one traced run):** entry 3 ms, `App::new` done at 34 ms, first paint at 37 ms, present at 45 ms. So about 31 ms is inside `App::new`. I did not break that down further.

**"Presented" means what it does in the Qt spike:** the frame callback for the first commit, not scanout, and not `wp_presentation`. winit holds a requested redraw until that callback, and the trace confirms the order: `commit`, then `wl_callback.done`, then `PRESENTED` (`results/wayland-trace-measure.txt`, 1 traced run). KWin's activated-state configure arrived after the first commit and caused a second commit of the same size, so the settled frame can be one refresh later.

## Custom components (lines in the `.slint` file, measured with `wc -l`)

| Component | Lines | Notes |
|---|---|---|
| Tabs | 81 | No overflow or scrolling: too many tabs just run under the buttons |
| Tree plus outline | 133 | Rust holds expansion state and supplies visible rows (about 15 Rust lines) |
| Splitter | 30 | Takes no layout space, so the document keeps its full width |
| Find bar | 74 | Custom field around `TextInput` |
| App menu | 167 | Cascading, with its own keyboard handling |
| Document view and overlay scrollbar | 79 | |
| Icons and icon button | 62 | Stroked `Path`, crisp at all scales |
| Theme, hairlines, fake shadow | 50 | |
| App layout, context menu, About | 420 | |
| **Total `.slint`** | **1096** | |

Rust: `main.rs` 827 (including options, bench, doc buffers), `kde.rs` 92, `selftest.rs` 162.

## Theme

`~/.config/kdeglobals` is read at startup: `Colors:Window`, `Colors:View`, `Colors:Selection`, `General/AccentColor`, `General/font`. Dark mode is the luminance of the window background. Checked by sampling pixels of `--screenshot` output, 1 run each:

| Arguments | Window | Accent |
|---|---|---|
| none | 32,35,38 (KDE, dark) | 89,120,30 (KDE) |
| `--theme=dark` | 32,35,38 (KDE) | KDE |
| `--theme=light` | 243,243,243 (built in, since KDE's scheme is dark) | KDE |
| `--no-kde` | 243,243,243 | 0,95,184 |

- **Defect: without kdeglobals on Linux, it stays light even on this dark desktop.** The fallback is winit's `theme()`, which apparently gave no answer here (inferred from the result). Windows should answer through the same call, but that is untested.
- **The accent clashes a little.** Your olive KDE accent is on the chrome, while the shared document images use blue links and checkboxes. `--no-kde` shows a matched accent. On Windows the system accent would need reading from the registry, which isn't implemented.
- Your kdeglobals has no `font=` key, so the UI uses Segoe UI, as the document does. It does not watch for live theme changes.

## Look: defects found and fixed

From reading the screenshots, in rounds:
1. **Sidebars empty:** custom components with `x` set collapse to zero width. Fixed with explicit widths. The find field's text was also top-aligned, and the three toggle buttons were heavy tinted blocks, now a small accent underline.
2. **No shadow on the find bar or menus:** `drop-shadow-*` is silently ignored, because the software renderer's `draw_box_shadow` body is `// TODO`. Replaced with 7 stacked translucent rectangles. Inactive tab labels were also misaligned with the active one.
3. **Banded, too-dark first shadow; About dialog in the wrong place, then with no background:** popups take their position when shown, and my panel had no size. The fake shadow is now acceptable at 1.0 and 1.5, but is a stack of rectangles, not a blur.

Compositor captures, with the title bar, are in `screenshots/`: light and dark at 1.0 and 1.5, plus menu, About, context menu, scrolled and small-window variants. On the final binary I viewed `dark-1.5`, `light-1.0` and `dark-small-window-menu-1.0`. I last viewed `dark-about-1.0` and `light-menu-1.5` one rebuild earlier, and the later changes (a focus fix and a state log) don't affect drawing.

## What won't behave like a native app

**Tested with real seat input** (KWin fake-input in nested KWin at scale 1.0, `scripts/input_session.sh`, 1 run, state log in `results/input/input-1.0.log`), all as expected: Ctrl+F, typing, Ctrl+A then typing, Esc, hamburger click, hover flyout, menu item click, click outside to dismiss, toolbar toggle, wheel, Page Down, Home, tab click, splitter drag, tree collapse, right-click menu, arrows and Enter in that menu, F1, Ctrl+W. Plus 24 of 24 steps in `--self-test`, which uses Slint's synthetic `dispatch_event` (`results/selftest-real-session.txt`).

Fake-input isn't a physical device. Real mouse-wheel line steps and touchpad pixel scrolling are unverified.

| Area | Behaviour |
|---|---|
| **Popups** | `PopupWindow` is drawn **inside** the parent window on winit (`ChildWindow` location; the winit adapter has no native popup path). It can't extend past the window edge, has no `xdg_popup`, and no compositor shadow. Clipping in a small window was not actually shown: my menu just fits at the 640x360 minimum |
| **Popup API traps** | No "closed" callback, so I made the menu popup cover the window to catch outside clicks. Code that runs after `close()` from inside a popup's repeater **silently does nothing**. Closing restores the old focus, overriding a focus set just before, so the find field needs a 1 ms timer to take focus back. All three cost debugging time |
| **Text field** | `TextInput` worked for typing, select-all and Backspace. Not tested: mouse drag selection, double click, copy and paste, undo. There is no right-click menu on it, because I didn't build one. Windows text-field behaviour would be Slint's own, not the system's |
| **IME** | Trace shows `zwp_text_input_v3` `enable`, `set_content_type` and `set_cursor_rectangle` on focus, and the backend's source handles preedit and commit. **No real input method was run,** so composition display is unverified, and Windows IME is untested |
| **Keyboard navigation** | Find field, tree and menus work, by hand-written code. Tabs and icon buttons have no keyboard focus or focus ring, and Tab order was not tested. Alt alone does nothing, there are no mnemonic underlines, and I bound Alt+F, E, V, H by hand. Every shortcut is a manual string match |
| **Text rendering** | Grayscale and soft at scale 1.0 (seen in zoomed captures; `swash` is the crate in the tree), and different from the document's LCD text. Windows ClearType or DirectWrite won't apply |
| **Windows surface** | Opaque software buffer. Mica and acrylic weren't attempted. Snap layouts, title bar theming and rounded corners are untested |
| **Accessibility** | Out of scope, and the feature is off |
| **Stand-in content** | Tab switching doesn't change the document, and zoom only changes the status bar number. Outline positions are read by eye. Find results are a static count |

## Not done

- **Windows:** no compile or run. `cargo tree --target x86_64-pc-windows-msvc` only. The `cfg(windows)` file-read shim has never been compiled.
- Cold launch, `wp_presentation`, DRM scanout, fractional scale on the real desktop (the real session is 1.0), and live monitor scale change.
- Long-running use, teardown, multiple windows.

## Reproduce

```sh
cd spikes/look-slint
cargo build --release -j 6
(cd ../qt-shell/tools && cargo build --release -j 6)    # kwin_capture, pixelcheck, harness

./run.sh                                                # live window, no timeout
target/release/look-slint --self-test                   # 24 synthetic checks; exits by itself
target/release/look-slint --measure --theme=dark        # PRESENTED and MEM lines, then exits
target/release/look-slint --screenshot=FILE             # Slint's own snapshot, no title bar

scripts/run_g2.sh results/g2                            # pixel exactness at 1.5, 1.7, 2.2
INNER=bench_session.sh scripts/run_g2.sh results/bench 1.5 1.7 2.2     # frame cost, launch, memory
INNER=shots_session.sh scripts/run_g2.sh results/shots 1.0 1.5         # captures into screenshots/
KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 INNER=input_session.sh scripts/run_g2.sh results/input 1.0   # real seat input
```

`run_g2.sh` stays inside virtual KWin, with an isolated XDG config. Only `run.sh`, `--measure` and `--self-test` open windows on the real desktop. Other options: `--theme=light|dark`, `--find`, `--menu=N`, `--about`, `--context`, `--scroll-px=N`, `--win=WxH`, `--doc-mode=viewport|full`, `--pixel=rgb|rgba`, `--checker`, `--bench-scroll=N`, `--no-kde`, `--log-state`.
