# Independent comparison: look-qt and look-slint

**Status: PASS for the Linux measurements. NEEDS_USER for the look. BLOCKED for Windows 11: neither toolkit ran there, so nothing here settles the real target.**

Slint launched faster and used less memory in every cell of all three batches (18 of 18 launch medians). Both are pixel exact. Qt idles slightly lower, though both are negligible. Start with `INDEX.md`.

Environment: Qt 6.12.0, KWin 6.8.1, slint 1.18.1, cxx-qt 0.10.0, rustc 1.99.1, 28 threads, `powersave` governor. Edge, the WinApps VM and desktop apps were running throughout, so **all timings are noisy**, though no builds were running.

## Build

Both built from unchanged source with `-j 6`, with the profile set from outside so neither spike's files changed:

```sh
export CARGO_PROFILE_RELEASE_OPT_LEVEL=3 CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_PANIC=abort CARGO_PROFILE_RELEASE_DEBUG=false
(cd ../look-qt    && CARGO_TARGET_DIR=$PWD/../look-compare/target/qt    cargo build --release -j 6 --locked)
(cd ../look-slint && CARGO_TARGET_DIR=$PWD/../look-compare/target/slint cargo build --release -j 6 --locked)
```

The profiles did differ before this. Slint had thin LTO, 4 codegen units and `strip = debuginfo`. Its binary drops from 20.5 MiB to **13.22 MiB** (13,857,888 bytes). The Qt binary is byte-identical to the one it had: **1.41 MiB** (1,481,000 bytes).

## Method

- **Environment:** `scripts/session.sh` starts a private `kwin_wayland --virtual`, one per scale and theme. Output is 3840x2400 for every scale, with the same seeded `kdeglobals`. Slint gets `--no-kde`, see "Problems" below.
- **Launch:** `scripts/inner.py`. 12 rounds, with Qt and Slint alternating in the same KWin. The first 2 rounds per app are dropped, so n=10.
- **Memory:** read from outside, from `smaps_rollup` and `smaps`, 800 ms after `PRESENTED`.
- **Cross-check:** the existing `qt-shell/tools` harness ran 12 more runs per app per cell, not interleaved (`results/launch*/harness-*.txt`).
- **Idle:** 5 s after present, then a 30 s window. Per-thread `schedstat` deltas, 3 runs per scale per app, plus 1 run per app with the find caret blinking, at 1.0.

```sh
scripts/run_all.sh     # batch A, T0 checks, traces, captures, menus, first idle run
scripts/run_ab.sh      # output size and XDG location (needs TMPX=<dir>)
scripts/run_more.sh    # batches B and C, idle (needs over 10 min: run it without a timeout)
python3 scripts/analyze.py && python3 scripts/pixel.py && python3 scripts/stitch.py
```

## Do the timers start at process start? Yes, for both

Measured at light@1.0 with 7 runs per row, first dropped. These ran during the slower period, so compare within a row group, not with the tables below.

| Check | Qt | Slint |
|---|---|---|
| `present_ms` as reported | 87.00 | 65.82 |
| Outside clock: T0 to reading the line from the pipe | 87.06 | 65.83 |
| Kernel start time minus T0 (10 ms ticks, rounded down) | -4.55 | -4.84 |
| `sleep 0.1` before `exec` | 196.85 (+110) | 177.32 (+112) |
| No `MDV_T0_NS`, falls back to `/proc` start time | 99.03 | 74.67 |

So T0 is taken before spawn, as the source says, and it includes fork, exec and dynamic linking. `first_paint` is an internal stamp and I did not check it this way.

**Same definition of "presented":** `results/trace/` holds 18 `WAYLAND_DEBUG` runs (3 per app per scale). In the ones I read line by line (1 per app, at 1.5), the sequence is first full-size `attach`, `commit`, `wl_callback.done`, then `PRESENTED`. `analyze.py` found a frame `done` before `PRESENTED` in all 18. Neither is scanout or `wp_presentation`.

**A flaw that doesn't affect these numbers:** Slint's fallback subtracts boot-based ticks from `CLOCK_MONOTONIC`, so its reading would be wrong after a suspend. Qt uses `CLOCK_BOOTTIME`. I read this in source and did not test it. The harness path doesn't use it.

## Launch (measured, noisy)

Median ms, min-max in brackets, n=10.

| Batch and time | Cell | Qt | Slint |
|---|---|---|---|
| A, 19:08-19:14 | light 1.0 / 1.5 / 2.2 | 88.7 (65-102) / 90.9 (59-133) / 95.4 (76-129) | 65.4 (54-76) / 73.2 (55-82) / 73.3 (65-89) |
| A | dark | 88.0 (61-100) / 94.2 (70-106) / 99.4 (88-123) | 63.0 (58-71) / 74.8 (52-93) / 75.5 (66-103) |
| **B, 19:39-19:45** | **light** | **62.2 (55-76) / 62.9 (58-74) / 65.5 (62-75)** | **45.4 (40-47) / 46.5 (42-61) / 60.6 (51-69)** |
| B | dark | 62.8 (59-93) / 69.2 (59-77) / 76.7 (71-91) | 47.2 (41-58) / 54.5 (43-72) / 72.2 (61-79) |
| C, 4 nice-19 spinners, 19:45-19:53 | light | 64.2 (58-79) / 61.7 (58-64) / 72.7 (63-113) | 45.4 (40-52) / 45.7 (42-59) / 60.2 (55-64) |
| C | dark | 62.6 (58-82) / 70.4 (57-94) / 67.9 (60-96) | 45.5 (41-55) / 46.7 (42-63) / 58.9 (50-63) |

- **Batch A is about 20-25 ms slower for both apps, and I couldn't establish why.** The system load was lowest then. In a separate A/B, output size and XDG location did not explain it (`results/ab/`). Later on, Qt went to about 60-75 ms and Slint to about 45 ms with no setting changed. Spinners held Qt within 59.8-60.7 and Slint within 45.5-46.6 ms over 3 rounds, but the unspun rounds beside them were close for Slint too, so I don't claim spinners are the cause.
- **The Slint-minus-Qt gap held in every cell,** from 4.5 ms (B dark@2.2) to 25.1 ms (A dark@1.0). It is smallest at 2.2.
- **Timings are not stable to better than one 16.7 ms frame.** Medians jump by about a frame between cells. Don't read small differences.
- **Harness cross-check agrees in direction:** batch B light, Qt 60.19 / 60.70 / 70.10, Slint 44.75 / 44.73 / 59.78. Batch A and dark runs are in `results/summary.txt`.
- **Warm starts only.** No parsing, layout or real text work is included in either. Cold start wasn't tested.
- **On Linux, the Qt number includes the Plasma theme plugin**, which pulls in KIO and Qt Quick. That won't happen on Windows, and nothing here says how Qt would launch there.

**Run interruption:** my 590 s command limit killed `run_more.sh` during batch C, and a leftover session overlapped a restart for under a minute. I stopped my own processes, deleted batch C's dark files and reran them with `scripts/run_rest.sh`. Batch C's light cells finished before that.

## Memory (measured)

Batch B, light, n=10, MiB, 800 ms after present. Private_Dirty matches across batches A, B and C, and the idle reads hold steady to 35 s.

| | Qt 1.0 | Qt 1.5 | Qt 2.2 | Slint 1.0 | Slint 1.5 | Slint 2.2 |
|---|---|---|---|---|---|---|
| Private_Dirty | 19.96 | 24.52 | 34.32 | 5.33 | 7.45 | 11.97 |
| Private_Clean | 4.76 | 7.57 | 13.30 | 7.67 | 7.64 | 7.75 |
| Pss | 39.14 | 51.26 | 76.60 | 19.01 | 25.90 | 40.43 |
| Rss | 90.06 | 106.77 | 142.10 | 32.53 | 44.20 | 68.60 |
| **Unique now** (Private_Clean + Private_Dirty) | 24.71 | 32.09 | 47.64 | 13.00 | 15.10 | 19.72 |
| **Owned** (anon + shm + dirty file pages) | 27.82 | 41.79 | 71.19 | 12.83 | 24.50 | 48.86 |
| Anon + heap, Rss | 11.53 | 11.38 | 11.40 | 4.66 | 6.78 | 11.30 |
| Window buffers (shm), Rss | 12.01 | 26.13 | 55.50 | 7.63 | 17.18 | 37.02 |
| Page file mapped, Rss | 2.25 | 5.06 | 10.88 | 0 (in anon) | 0 | 0 |
| Own binary, Rss | 0.81 | 0.82 | 0.81 | 7.87 | 7.82 | 7.95 |
| Library pages, Rss | 55.3 | 55.3 | 55.3 | 4.8 | 4.8 | 4.8 |
| Library pages, Pss | 14.6 | 14.4 | 14.4 | 0.6 | 0.6 | 0.6 |
| Library pages unique to this process | 5.92 | 5.95 | 5.92 | 0.26 | 0.26 | 0.26 |

- **Private_Dirty alone misleads.** Qt keeps 3 full-window shm buffers against Slint's 2. KWin maps some of them, and those become Shared_Dirty. All of Slint's are shared here. Qt's Private_Dirty therefore includes a buffer, and it can jump: at dark@2.2 in batch A the median was 52.82 MiB, with a 15.86-52.86 range. The owned figure is steadier, though it also has outliers (52.67-89.67 at dark@2.2 in batch B).
- **"Unique" is a snapshot:** a library page counts as unique only if nothing else has it mapped at that moment. The kernel figure can't say what happens without Plasma. Rss is the upper bound, and the real value on another system lies between (inference).
- **Not comparable pages:** Qt memory-maps the page and Slint reads the visible rows into its own buffer, so a real renderer will change both.

## Idle CPU (measured)

30 s window, starting 5 s after present, n=3 per scale. CPU ms over the window, from per-thread `schedstat`:

| Scale | Qt | Slint |
|---|---|---|
| 1.0 | 0.04, 0.04, 0.13 | 0.85, 0.74, 0.46 |
| 1.5 | 0.03, 0.06, 0.03 | 0.40, 0.49, 0.46 |
| 2.2 | 0.06, 0.06, 0.02 | 0.74, 0.89, 0.94 |
| Voluntary wakeups in 30 s | 1 | 60 (2 per second) |

Both round to 0%. I did not trace Slint's 2 Hz wakeup.

With a blinking caret at 1.0: Qt 46.77 and 123.23 ms, Slint 17.21 and 55.21 ms, one run per app in each of the two idle passes. These are too few to rank closely, though Slint was lower in both.

**My first idle run was invalid.** It summed live threads, and Qt lets 4 threads exit during the window, so its deltas came out negative. That run is kept in `results/idle-invalid-first-run/`. The higher caret figures above come from it, so they are less reliable. All other idle values use per-thread deltas.

## Pixel exactness (measured)

`scripts/pixel.py`, my own comparison, on fresh compositor captures from these binaries: every RGB byte against the shared `.bgra`. Light and dark, at 1.0, 1.5 and 2.2, scrolled 0 and 900, at window positions (40,40) and (41,43). The odd position is a fractional device origin at 1.5 and 2.2. **Qt 24 of 24, Slint 24 of 24, 0 differing pixels.** Controls with the page shifted by 1 row at the same spot differ by 51,742-135,675 pixels, so the check can fail. My first version picked a uniform row and falsely failed everything; that's fixed.

The comparison leaves out 4 device px on the left, and a strip on the right for the scrollbar (14 logical px + 4 device px). It covers neither window resize nor scale changes, and the find bar was closed.

## Problems found in the two RESULT files

**look-qt:**
- **Confirmed:** real Breeze (`style=breeze`), 0 differing pixels, launch about 60 ms in the better batches, Private_Dirty 19.96 at 1.0, unstable Private_Clean.
- **Extended:** exactness also holds at 2.2, which it didn't test, and at odd window positions.
- **Not stated:** the canvas is narrower than the page (778x754 against 780x756 logical at 1.0, 1167 against 1170 device px at 1.5, 1712 against 1716 at 2.2). The page is centred and clipped by up to about 2 device px per side. Only margin is lost, so nothing visible.
- **Its 2.2 memory is low:** it measured with a 2400x1600 output, only 1091 logical px wide, smaller than the 1220 px window. It got 30.11 Private_Dirty and I got 34.32 at 3840x2400. The cause is inferred, not separately tested.
- **"1.5 MB binary" is its own code only:** at run time it loads 139 libraries, 156.8 MiB on disk.
- **Its comparison with the empty skeleton** and its stylesheet costs were not re-measured.

**look-slint:**
- **Confirmed:** Launch of about 45 ms in the better batches, Private_Dirty and Pss, no real popups, generic title-bar icon behaviour traced to no `set_app_id`.
- **Not like-for-like:** its table compares with `qt-shell`'s skeleton on another day, under a different profile. Its 1.0 row ran on the real desktop and the others nested.
- **"Isolated XDG config" isn't fully isolated:** `kde.rs` reads `$HOME/.config/kdeglobals` and ignores `XDG_CONFIG_HOME`. So its nested captures picked up your real accent: `screenshots/dark-1.0.png` has olive pixels and no blue. Qt's captures use blue. That's why my runs pass `--no-kde`.
- **Risk 1 applies to both:** it says Slint's chrome text is grayscale next to LCD document text. In these captures Qt's status bar text is grayscale too (0 coloured pixels in both). Only that region was cleanly checked, and your real desktop's font settings may differ.
- **Its scroll bench, 4095 px limit, input tests and Windows crate count** were not re-measured. Qt has no matching scroll bench, so scroll cost isn't compared.

## Not done

- **Anything on Windows 11:** launch, memory, fonts, title bars, DWM, Mica, `windows11` style.
- **The live windows on your real desktop.** The same arguments ran in virtual KWin only.
- **Cold launch, a physical mouse and keyboard, real IME, resize and scale changes.**
- **Reading every image.** I read the 4 required pairs, `light@1.0-menu2`, `dark@1.5-menu2`, and zooms `light@1.0-topleft`, `light@1.0-topright`, `dark@1.0-topright`, `light@1.5-topright`, `dark@1.5-topleft`. The `plain` and other menu pairs and the other zooms were generated but not read. Scale 1.5 pairs were displayed downscaled, so I judged fine detail at 1.5 from the zoom crops.
- **Native behaviour tests:** I only re-checked popups, app ID and bound Wayland protocols from traces. Other entries in the "reported" row come from the spikes' own RESULT files and source.
