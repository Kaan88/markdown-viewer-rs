# Phase 0 results

Written 2026-11-06, after an independent read-only audit of round 2. Where the audit disagreed with a spike's own report, this file uses the audit's figure. Details live in each spike's `RESULT.md`; those files were not rewritten, so some of their wording is superseded here.

**Limits on every number:** a 28-thread desktop with warm caches, other work running, no cold launches, no low-end hardware. Nothing measures a real document launch: every shell draws a ready-made image or a checkerboard, so launch figures are floors. Nothing except WinUI has run on Windows.

## Status

**Go, declared 2026-11-06.** The plan requires G1-G3, G8's cross-build, and a Mermaid route per family, and all are now met. The user accepted the fixed Rust text in G3.

**Decided by the user:** Qt Widgets on Linux; Slint is rejected for its text rendering. Coverage now ranks alongside launch and memory; see "Coverage" in the plan. I took the same answer to accept the 45 MB limit at 1220x820 up to scale 1.5, with larger sizes reported rather than failed.

| Gate | Status | Result | Caveats |
|---|---|---|---|
| G1 Qt from Rust | PASS under the revised limit | Overrides work, no authored C++. Launch 56 ms (skeleton). `unsafe` contained in `src/qt/` | Memory is over 45 MB above 1.5 and in large windows, see below. System icons probably need a small C++ helper in a real product |
| G2 pixel exact | PASS | 42 of 42 exact, 6 of 6 controls fail. Look spikes: 24 of 24 for Qt and Slint | Virtual KWin only. Partial repaints and scrolling untested |
| G3 text quality | PASS, by the user, after a fix | First round failed: the user saw colour fringes and judged Qt better. Cause found: `Format::Subpixel` gave swapped red and blue (Qt left edge R-B +107, Rust -101). Fixed in `text/src/lib.rs`, images in `text/compare-fix/`. The user accepted the fix | Hinting choice still open: it scores closer to Qt at 100% and worse at 150%. Slight differences remain. Rust emoji are visibly larger than Qt's. Default fallback has emoji defects, fixed by naming the font. Windows text is W1. `look-shared` page images predate the fix |
| G4 parser | PASS with a comparator choice | pulldown-cmark 5.2 ms vs comrak 9.8-10.9 ms for 1 MiB. 1.49% GFM misses with autolink on GFM-tagged examples | The viewer's always-on setup misses **2.23%** (15 examples), over the 2% limit. comrak misses 2.08%, so switching doesn't fix it. Official normalizer gives 4.91%, mostly writer form. The 15 break down in the plan's "Coverage": 10 nested `strong` or tag-filter cases we can fix ourselves, 1 autolink escape, 4 deliberate. Fixes not yet written |
| G5 Mermaid | PASS, Linux and one font set | merman on all 8. Showcase worst 16.4 ms. No ELK | Requires our repair pass, real metrics and Gantt gutter. Hard diagrams still have flaws. See plan changes |
| G6 engine | PASS on Linux only | First math about 2 ms, highlight 20-26 ms, hang killed at 5 s, viewer unaffected | No memory limit on Windows yet. The stand-in viewer doesn't paint. Mermaid not included |
| G7 text layout | PASS on the specified document | Height pass about 350-370 ms, visible layouts under 1 MB | Document is about 1% non-Latin, not the 4% the report states, and has no parser. Stress document takes about 530 ms on one thread, about 300 ms on four |
| G8 WinUI build | PASS | Cross-builds; runs in the Windows 11 VM | Not yet on real hardware. Under Wine it fails inside `CoreMessagingXP.dll` |

## Linux shell: Qt vs Slint

Same build profile, same sessions, 12 interleaved rounds per cell. Launch and memory are batch B, light theme, 1220x820.

| | Qt 1.0 / 1.5 / 2.2 | Slint 1.0 / 1.5 / 2.2 |
|---|---|---|
| Launch, ms | 62.2 / 62.9 / 65.5 | 45.4 / 46.5 / 60.6 |
| Private (Private_Clean + Dirty), MiB | 24.7 / 32.1 / 47.6 | 13.0 / 15.1 / 19.7 |
| Pss, MiB | 39.1 / 51.3 / 76.6 | 19.0 / 25.9 / 40.4 |
| Owned (anon + window buffers + dirty file pages), MiB | 27.8 / 41.8 / 71.2 | 12.8 / 24.5 / 48.9 |
| Rss, MiB | 90.1 / 106.8 / 142.1 | 32.5 / 44.2 / 68.6 |

- **Launch:** Qt was slower in 164 of 180 paired rounds, with a median gap of 16.6 ms. At 2.2 in batch B the gap is about 6 ms, within one frame, and Slint won 8 of 10 pairs. An earlier batch was slower for unknown reasons (Qt by 23-30 ms, Slint by 3-27 ms). The gap still held there.
- **Memory:** Slint uses a steady 15-22 MiB less. The ratio runs from about 1.5x to 2.8x depending on what is counted. At scale 1.0, Qt's heap and library dirty pages are the main cause. At 2.2, it's mostly window buffers. Figures are before scrolling: Slint's private memory rises by about 8 MiB at 2.2 after scrolling, and Qt's post-scroll figure is unmeasured.
- **Sharing:** only 5.9 of Qt's 55.3 MiB of touched library pages are unique, because Plasma already maps the rest. Without Plasma, Qt's figures would be higher.
- **Bias:** Qt's launch includes the Plasma theme plugin (KIO, Qt Quick). Slint's runs used `--no-kde`, which skips its small `kdeglobals` read.
- **Idle:** both about 0% CPU. Qt wakes once in 30 s. Slint wakes twice a second, from an idle `blocking` pool thread pulled in by `zbus` in Slint's winit backend. It isn't our code.
- **Scrolling:** not compared, because Qt has no scroll benchmark. Slint's measured frame: 3.4 ms median and 7.0 ms p95 at 1.5, 6.6 ms median at 2.2. Its image copy is about 11x slower than memcpy.
- **Code:** both about 2,150 lines. Qt has 109 lines containing `unsafe`, and 55% of its code is binding code. Slint has no `unsafe`.
- **Binary and libraries:** Qt 1.41 MiB plus 139 libraries (156.8 MiB on disk). Slint 13.22 MiB plus 21 libraries (9.0 MiB).
- **Behaviour Qt gets from the toolkit:** real popup menus that leave the window, Breeze's own controls, standard text-field behaviour, keyboard focus, and the system theme.
- **Behaviour Slint would need hand-built or lacks:** popups stay inside the window, no Alt mnemonics, no keyboard focus on tabs and icon buttons, untested clipboard and IME, faked shadows, and a licence attribution requirement. It also blanks images scrolled past about 4095 device px unless given only the visible part, so it needs an extra copy per frame.

Both have grayscale status-bar text next to LCD document text in the captures. Only that region was checked, in the virtual session. Images: `look-compare/pairs/`. Live commands: `look-compare/INDEX.md`.

### The 45 MB limit (42.9 MiB)

| Metric at 2.2 | Qt | Slint |
|---|---|---|
| Private, the metric G1 uses | 47.6 FAIL | 19.7 pass |
| Pss | 76.6 FAIL | 40.4 pass, by 2.5 MiB, before scrolling |
| Owned | 71.2 FAIL | 48.9 FAIL |
| Rss | 142.1 FAIL | 68.6 FAIL |

Qt on the G1 metric: it passes at 1220x820 up to scale 1.5. It reached 44.6 MiB (46.8 MB) at 1.7 in the skeleton, and 49.1 MiB in a nearly maximized 3400x1380 window at scale 1.0, which is close to the user's own desktop.

**Corrections to what was said in chat:** Slint "clears the limit at 2.2 while Qt misses" holds only for private memory and Pss. The Slint wakeup cause, reported as unknown, is now identified.

## WinUI (Windows 11 VM, software rendering over RDP, 100% scale)

| Measure | Result |
|---|---|
| First frame, warm | About 130 ms (126.2 for runs 2-10, 130.9 under the plan's skip of 2) |
| Full UI, warm | About 175 ms (171.9 or 178.4) |
| Private memory | 30.2 MiB at launch, about 31.7 MiB after settling (one reading) |
| Working set | 116.7 MiB at launch, about 128.9 MiB after settling (one reading) |
| First run after a fresh copy | 466 ms (one run, not a true cold boot) |

Working: tabs, menu, lazy tree, flyout, typing and wheel via synthetic input, and an exact checkerboard at 100%.

**Don't compare these with the plan's 200-350 ms and 50-90 MB.** They differ from the plan's definition:
- There is no document, parser, layout or font work.
- The window is 2580x1026, not 1220x820.
- "First frame" is two XAML ticks after the first Present, not confirmed by DWM.
- The build isn't the release profile, and Mica is absent.
- The same binary gave 85-158 ms across harnesses, so don't trust it to better than tens of ms.

**Never run:** real GPU, other scales, framework build, resize, live scale change, theme switch, real mouse and keyboard, true cold boot.

**Evidence corrections:**
- The input runs have 3 distinct results in one row, not 4: run 5's file is a copy of run 4's.
- The band moved one net notch, not two.
- The 314.0 ms first-run figure has no retained log.

### W0 fixes from the audit, then re-run in the VM

- A failed first draw now shows a message box and exits with code 1. It was forced with `--test-fail-draw` and checked both normally and in measure mode.
- The window opens at 1220x820. `measure.ps1` now uses 12 runs with 2 warm-ups, a correct median and honest labels, and warns on battery. `W0.md` covers scale, power and how to run it.
- Same VM, 10 warm samples at 1220x820: **first frame 94.0 ms** (79-111), **full UI 135.8 ms** (118-156), private 22.4 MiB, working set 76.5 MiB. These replace the larger-window figures above, and all the limits above still apply except window size and sample count.
- Input test passed on this build: wheel, tree, flyout, typing and File > Close, one run.

W0 on the user's PC is still to do. Everything under "Never run" above remains unrun.

## Plan changes these results call for

- **Diagram limit:** the 500-line limit can't hold. merman fails cleanly at about 320-400 lines in 5 of 8 families on synthetic inputs, sometimes from raster size limits. Source is shown above the real ceiling. Medium diagrams exceed 300 ms (150 nodes about 310-350 ms).
- **Engine memory:** 30-60 MB doesn't hold with merman. Hard diagrams reach about 71 MiB, probes about 150 MiB, and a hostile label about 297 MiB. All stayed within the 512 MiB limit.
- **Mermaid is our code too:** the repair pass, metric measurer, Gantt gutter and line limit are required, and are tied to merman 0.9.3. The merman binary adds about 13 MB in a non-plan profile. ELK-requesting documents show source or the default layout.
- **Font policy:** fallback must be chosen per script for merman/resvg, RaTeX and parley. The plan has no shared policy. Segoe can't be assumed on Linux, and the Mermaid route is unverified visually with Liberation.
- **Windows engine limits:** Job Object and parent-death cleanup are unwritten.
- **Text layout:** pure CJK or Arabic probably needs a multi-threaded height pass or estimated heights (inference).
- **Delayed Qt loading:** `dlopen` saves only about 4 ms on a warm second launch. The plan says drop it if the saving is too small. Cold launch is unmeasured.
- **Cold launch:** no gate measured it, on any OS.

## Housekeeping

- `mermaid/out/` holds 32 stale files with old clipped labels. The verified output is `mermaid/verification/patched/`.
- Nothing is committed. The private repo gets created at Go.
