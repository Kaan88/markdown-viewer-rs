# Phase 0 spike: G1 and G2 (Qt Widgets through CXX-Qt)

Environment: Qt 6.12.0, cxx-qt 0.10.0, rustc/cargo 1.99.1, KWin 6.8.1, 3440x1440 at 143.98 Hz, real session scale 1.0.
**All timings are noisy.** Other agents were building in parallel (load average 3.5-6 on 28 threads).

## Results

| Gate | Criterion (plan) | Result |
|---|---|---|
| G1, overrides | All overrides work, no authored C++ | **PASS** |
| G1, launch | Under 160 ms | **PASS**: 56-57 ms at scale 1.0, 61 ms at 1.5, 72 ms at 2.2 |
| G1, memory | Private memory under 45 MB | **PASS at 1.0 (25.2 MiB) and 1.5 (39.7 MiB). FAIL at 2.2 (64.5 MiB; 46.1 MiB at best)** |
| G1, `unsafe` | Contained | **PASS**: `src/shell.rs` has `#![forbid(unsafe_code)]` and compiles |
| G2 | Pixel exact at 1.5, 1.7, 2.2 at several offsets | **PASS**: 42 of 42 cases exact, compositor captured |

**G1 needs a decision.** The plan's memory criterion names no scale, so I am not calling G1 a clean pass. It passes at the real desktop's scale and at 1.5, and fails at 2.2. The plan's budget section already expects Wayland shm buffers to cost a lot at high resolution. Whether 45 MB applies at 2.2 is for the user to decide.

The kill criterion was not hit: no authored C++ or QML was needed. C++ is only generated at build time.

These launch numbers are for an **empty skeleton, not the plan's "20 KB document" definition**. There is no parsing, layout or text here, so they are a floor for the budget.

## G1: bridge size and friction

Line counts (measured): `src/qt/ffi.rs` 363 (bridge), `src/qt/mod.rs` 518 (wrapper), `src/shell.rs` 330 (safe logic).
Lines containing `unsafe`: `ffi.rs` 33, `mod.rs` 50 (38 of them `unsafe {` blocks), `shell.rs` 0, plus 1 in the launcher, which is a separate Qt-free program.

All unsafe in the Qt shell is in `src/qt/`. "Contained" does not mean sound: handles are raw pointers that stay valid only because every object here lives until process exit. A real shell that deletes widgets could dangle them. This is the risk the CXX-Qt maintainer warns about.

Friction a real shell will hit, all solved without C++:
- **Constructors:** cxx-qt-lib's `make_unique`/`new_ptr` templates bind directly, and cxx deduces the arguments from the declared signature.
- **Default arguments:** these can't be omitted, so the full signature is needed (`startTimer` needs `Qt::TimerType`, `addWidget` needs `Qt::Alignment`).
- **`qsizetype`:** it is `long long` here, not cxx's `isize`, so it needs its own `ExternType`.
- **Types cxx-qt-lib already owns:** methods can't be added to its aliases, and redeclaring one gives duplicate symbols. This needs a second name for the same C++ class (`RawImage`, `RawPainter`) and a pointer cast.
- **Signal closures must be `Send`,** even on the GUI thread, which needs an `unsafe impl Send` wrapper.
- **Static methods** need `#[Self = "QApplication"]`. A namespace can't be used, because it collides with the class.

### Self-test (measured, real session and offscreen)

`--self-test` sends synthetic events to the `QWindow`, so they go through `QWidgetWindow`'s real hit testing. All passed with correct local coordinates: paintEvent, mouse press and move, wheel (including calling the base `QWidget::wheelEvent`), drag enter, drop, `QAction::triggered`, `QLineEdit::textChanged`.

### Launch (ms, real session, scale 1.0, measured, noisy)

T0 is `CLOCK_MONOTONIC` just before spawn, so fork, exec and linking are included. Each batch is 12 runs, drop 2, median. Raw values are in `results/`.

| Variant | Interleaved batch 1 | Interleaved batch 2 | First pass (sequential) |
|---|---|---|---|
| Direct link, KDE theme | 56.20 | 56.84 | 76.19 |
| Direct link, `generic` (Fusion) | 42.85 | 42.47 | 55.55 |
| Direct link, `generic` + Breeze | 54.84 | 55.92 | 53.05 |
| Qt-free exe + dlopen, KDE theme | 56.11 | 57.30 | 54.41 |

Use the interleaved columns. In the sequential pass, the two KDE-theme rows differ by 22 ms, which is order or noise. Single outliers reached 183 ms.

**Meaning of "presented":** the time the frame callback requested with the first canvas buffer returned. `results/wayland-debug-trace.txt` confirms the order: `wl_surface.frame` and commit, then `wl_callback.done`, then `PRESENTED`. KWin's 1220x820 configure arrives after that first commit, and the final commit came 1.3 ms before the callback (one traced run). So the settled buffer may appear up to one refresh later: about 7 ms at 144 Hz and about 17 ms at 60 Hz (estimated). `wp_presentation` was not used.

### Memory (MiB, 12 runs, median, read 800 ms after present, measured)

| Variant | Private total | Private_Dirty | Pss |
|---|---|---|---|
| Scale 1.0, KDE theme | 25.18 | 22.04 | 36.65 |
| Scale 1.0, `generic` + Breeze | 24.30 | 21.35 | 34.37 |
| Scale 1.0, `generic` (Fusion) | 21.41 | 18.16 | 28.89 |
| Scale 1.0, dlopen, KDE theme | 25.53 | 22.04 | 37.23 |
| Scale 1.5, nested KWin | 39.66 (range 30.9-39.9) | 37.69 | 49.34 |
| Scale 2.2, nested KWin | 64.52 (range 46.1-83.0) | 62.68 | 79.47 |

Private total is Private_Clean + Private_Dirty.

The values jump in steps of about one window buffer (about 18.5 MiB at 2.2), so the spread is probably how many shm buffers are alive and how KWin holds them. That is an inference, not a measurement. The spike also holds a full-canvas Rust buffer: 1874x1514x4 = about 11 MiB at 2.2 (calculated). Removing it would leave about 53 MiB at the median (estimated), still over 45.

### Plan decisions these numbers inform

- **Platform theme: keep KDE.** It costs about 1 ms and 0.9 MiB against `generic` + Breeze, well under the plan's 30 ms fallback trigger. The plan's 17 MiB estimate looks too high. Fusion is 13-14 ms faster but isn't the Breeze look.
- **dlopen: no launch penalty, small warm saving.** First launch is within noise of direct linking.
- **Second launch, listener alive (12 runs, drop 2):** Qt-free exe **0.40 ms**, direct-linked binary **4.10 ms**. Both beat the 20 ms budget. dlopen saves about 3.7 ms warm. The plan says to drop it if the saving is too small; the cold case below is the missing input.

## G2: pixel exactness

**Method:** a real Wayland test. Each scale runs in a private `kwin_wayland --virtual` with its own session bus and isolated XDG config. Output is 3840x2400, with the scale seeded through `kwinoutputconfig.json`. `--scale` alone was not enough: it multiplied the output size and left clients at dpr 1. Captures come from `org.kde.KWin.ScreenShot2` at native resolution (`tools/src/bin/kwin_capture.rs`). The nested compositor used OpenGL ES on the real GPU (`results/g2-compositor.txt`).

**Check:** `tools/src/bin/pixelcheck.rs` finds the checkerboard region, counts every non-black/non-white pixel, checks parity on every pixel, and requires the region within 2 px of the expected size.

**Result (measured, `results/g2-results.txt`):** 42 of 42 cases, `non_bw=0`, `parity_errors=0`, regions within 1 px of expected. 7 cases x 2 paint modes x 3 scales:
- **Cases:** base, margins 1/1, margins 3/7 with dock width 201, odd window size, moves to two window positions (101,57 and 333,211, the latter with margins and odd size), and a live resize with repaint.
- **Fractional origins covered:** for example, device origin 307.50,160.50 at 1.5, and window x=101 giving 151.5 device px.
- **Paint modes:** `painter` uses QPainter with a net identity transform and an integer blit. `direct` copies into the backing store's own QImage. Both are exact.

**Negative controls, which must fail, and all 6 did:**
- Naive scaled QPainter draw: 658,293 / 845,049 / 1,406,624 parity errors at 1.5 / 1.7 / 2.2.
- Qt rounding its scale up (`QT_SCALE_FACTOR_ROUNDING_POLICY=Ceil`), so the compositor must downsample: no checkerboard region found at any scale.

## Not verified

- **Real user input.** Mouse, wheel and drag were synthetic events sent to the QWindow, not from a seat or a real drag source.
- **Menu bar appearance.** In nested captures, the "File View Help" labels are present but very dim. The cause is not established (possibly an inactive-window palette). I did not visually check it on the real desktop, and did not open a menu popup: only `QAction::trigger()` was exercised.
- **Fractional scale on the real desktop and DRM scanout.** The real session is scale 1.0, so G2 ran only in virtual KWin. ScreenShot2 reads the compositor's scene, not what reaches the monitor.
- **Partial repaints and scrolling.** Every G2 paint covers the whole canvas.
- **`direct` mode with overlapping widgets.** It bypasses Qt's clip, which needs care. `painter` mode doesn't have this issue.
- **Cold launch.** Dropping caches needs root, so all launch numbers are warm. This is also why dlopen's cold saving is unknown.
- **Other things not tried:** global menu, wp_presentation timing, teardown (the spike exits with `_exit`), and a run without parallel builds.

## Reproduce

```sh
cd spikes/qt-shell
cargo build --release -j 6
(cd launcher && cargo build --release -j 6)
(cd tools && cargo build --release -j 6)

target/release/qt-shell --measure --exit-after-present --self-test   # needs a Wayland session
scripts/run_g1.sh 12                     # G1 sequential: time, memory, second launch
scripts/run_g1_interleaved.sh 12         # G1 round-robin: theme and dlopen comparison
INNER=g1_scaled_session.sh scripts/run_g2.sh OUTDIR 1.5 2.2   # memory at fractional scale
scripts/run_g2.sh OUTDIR                 # G2: all scales, cases and controls
```

`run_g1*` opens short-lived windows on the current desktop. `run_g2.sh` stays inside virtual KWin.

## Notes from the run

- An early trial capture with spectacle activated a stray `xdg-desktop-portal-kde` on my private bus, with `WAYLAND_DISPLAY` pointing at the real desktop. I confirmed it was mine from its environment, stopped only that PID, and dropped spectacle. Scripts now set the nested socket before the bus starts, and clean up only processes carrying that run's `XDG_CONFIG_HOME`. The user's `~/.config/kwinoutputconfig.json` was unchanged.
- Build size: `target/` is 914 MiB, `tools/target/` 254 MiB (ignored by git, safe to delete).
- The relayed request (Windows 11 target, no accessibility unless useful) changes nothing here: this is the plan's Linux shell, and no accessibility bridge was built.

## Verification

Independent re-check by a separate agent, 2026-11-06. Built and run from a scratch copy, so nothing in this tree changed except this section. The machine was **not quiet**: load 3-7, with an unrelated `dxc.exe` at about 100% CPU, plus msedge and baloo. Timings are still noisy.

**Verdict: G1 overrides, no C++, launch and `unsafe` verified. G1 memory is not a clean pass and still needs the user's decision. G2 verified.**

### Build and source

- Clean `cargo build --release` gives a `qt-shell` **byte-identical** to `target/release/qt-shell` (sha256 `c73405b4...`).
- **No authored C++ confirmed:** no C, C++, header, QML or UI files outside `target/`, no `[patch]`, git or path dependencies, and `build.rs` lists only `src/qt/ffi.rs`. `Canvas` with its `override` methods exists only in the generated `ffi.cxxqt.h`.
- **Timer start is genuine:** the harness takes `CLOCK_MONOTONIC` before spawn and passes it in `MDV_T0_NS`, so fork, exec and linking count. A separate Python parent timing to the arrival of the stdout line agreed: 55.33 vs 55.30 ms, largest difference 0.04 ms.
- **Fallback T0 caveat:** without the env var, T0 comes from `/proc` starttime, which has 10 ms resolution and counts from boot, not `CLOCK_MONOTONIC`. It would be wrong after a suspend. This affects one-shot runs only, not the harness numbers (inferred from the code, not tested).

### "Presented" (12 traced runs)

In 12 of 12 runs there was exactly one frame callback, it completed before `PRESENTED`, and no commit fell between the two. So the timer measures what the section above says: frame callback, not scanout. `wl_callback.done` is a compositor hint, so this is not proof of display. The last buffer commit came 2.7-8.5 ms before the callback (median 4.4), not the single 1.3 ms traced above. In separate untimed traces, one of three runs reached `PRESENTED` before KWin's 1220x820 configure, so the settled buffer came later, as the section above warns.

### Overrides (instrumented copy, counters at the Rust entry of each override)

| Sent to the QWindow | Count change |
|---|---|
| Press + release + wheel over the left dock (control) | 0 on all canvas input overrides |
| 3 press | press +3 |
| 5 hover moves, no button | move +5 |
| Press, 5 moves, release | press +1, move +5 |
| Press, 5 moves outside the canvas (grab) | press +1, move +5 |
| 7 wheel | wheel +7 |
| 2 drag + drop | dragEnter +2, drop +2 |

`paintEvent` ran 3 times on Wayland, `timerEvent` once, and `eventFilter` on each window event. gdb shows `QWidget::event` calling `Canvas::wheelEvent` and `Canvas::paintEvent`, so Qt really dispatches to them.

- 5 moves with Left set in `buttons` but no prior press were **not** delivered. That input state can't happen with a real mouse, so I don't count it as a defect.
- The self-test asserts only that press and move arrived, not their coordinates. The printed values are correct by inspection: (426,344) is the centre of 852x688.
- All of this is still synthetic input.

### Launch and memory (12 runs; launch drops 2, memory is the harness median of all 12)

| Variant | present_ms | Private total (MiB), range |
|---|---|---|
| Scale 1.0, KDE theme | 56.19 (worst 128.45) | 22.83, 18.7-26.8 |
| Scale 1.0, `generic` (Fusion) | 45.17 | 19.23, 15.2-23.2 |
| Scale 1.0, `generic` + Breeze | 52.56 | 22.12, 18.2-26.0 |
| Scale 1.0, dlopen, KDE theme | 58.89 | 23.33, 19.4-27.4 |
| Scale 1.5, nested | 59.82 | 34.13, 29.5-38.3 |
| Scale **1.7**, nested (not measured above) | 60.51 | **44.63**, 33.4-44.7 |
| Scale 2.2, nested | 63.70 | **63.25**, 27.1-63.3 |
| Scale 1.0, real session, **3400x1380 window** (not measured above) | 68.31 | **49.13**, 31.6-67.7 |

Dropping the first 2 runs changes memory medians by at most 0.11 MiB (for example 44.62 at 1.7, 63.25 at 2.2).

Pss was 30.95 / 45.31 / 55.06 / 77.47 MiB at 1.0 / 1.5 / 1.7 / 2.2. Private total leaves out shm buffers that are shared with KWin (Shared_Dirty 8-19 MiB).

Second launch: Qt-free 0.41 ms (reproduces), direct-linked 5.19 ms.

### G2 (rebuilt binaries, fresh nested KWin, OpenGL ES on the real GPU)

- **Original 42 of 42 pass.** All 6 negative controls fail, with the same naive-draw parity counts as above: 658,293 / 845,049 / 1,406,624.
- **36 added cases:** 6 seeded random geometries (window size, dock width, margins, window position) x 2 paint modes x 3 scales. All have `non_bw=0` and `parity_errors=0`. 4 of them report FAIL at 2.2 on size only, because my chosen windows ran off the 3840x2400 output (the regions end exactly at the screen edge). That is my test's fault, not a defect.
- **Separate numpy checker, sharing no code with `pixelcheck`:** 78 of 78 non-control captures exact. Lines just outside each region show no leftover checker pattern (at most 0.4% transitions, against 45-67% for the naive control), so the 2 px size tolerance is not hiding a blurred edge.
- Moves really happened: regions shift between the base and moved cases.

### Claims above that did not reproduce

- **"46.1 MiB at best" at 2.2, range 46.1-83.0:** I saw 27.1-63.3, with the same median verdict. Memory steps with the number of live buffers, so a single best or range isn't stable across batches.
- **Scale 1.5 at 39.66 MiB:** I got 34.13. Both pass, but the batch-to-batch spread is about 5 MiB.
- **Scale 1.0 at 25.18 MiB:** I got 22.83, for the same reason.
- **Second launch, direct-linked, 4.10 ms:** I got 5.19, so dlopen saves about 4.8 ms warm rather than 3.7.
- **KDE theme costing about 1 ms:** I got 3.6 ms against `generic` + Breeze, in a sequential run, which the section above warns is order-sensitive. Still far under the 30 ms trigger.
- **`shell.rs` has 0 lines containing `unsafe`:** it has 1, the `#![forbid(unsafe_code)]` line. No unsafe code.

### Gaps in the memory verdict

- **MB vs MiB:** the plan says 45 MB, which is 42.9 MiB. RESULT compares MiB against 45. Scale 1.7 at 44.63 MiB is 46.8 MB, so it **fails** in decimal MB and barely passes in MiB. Scale 1.5 passes either way (at most 41.6 MB on either batch's median).
- **Window size:** every memory figure above used 1220x820. A near-maximized window at scale 1.0 has a 49 MiB median, over the limit at the real desktop's own scale.
