# G6: lazy engine process

**Result: PASS on Linux. Not run on Windows 11, which is the shipping target for half the app.**

Plan criterion: "First result under 150 ms after paint; hang killed at 5 s; viewer unaffected."

| Criterion | Target | Measured (noisy) | Verdict |
|---|---|---|---|
| Spawn plus first math result | under 150 ms | median 2.1 ms, max 2.3 ms, n=20 | PASS |
| Hang killed | at 5 s | 5001.3 and 5000.4 ms, n=2 in the saved run | PASS |
| Viewer unaffected | host thread keeps ticking | max tick gap 16.2 ms during the hang, tick period 16 ms | PASS, on a stand-in UI thread |

"First result after paint" is measured from job submission on the host's main thread. There is no real viewer or paint in this spike, because it only tests the engine boundary.

## What was built

- `src/engine.rs` (`mdview-engine`): a reader thread and a worker thread with an explicit 8 MiB stack. Renders TeX with ratex 0.1.15 and embedded fonts through resvg 0.49. Highlights with syntect 5.3 and two-face 0.5.2, using the pure Rust fancy-regex backend.
- `src/host.rs` (`host`): the main thread stands in for the UI and only waits on a channel with a 16 ms timeout. A supervisor thread owns the child and enforces the deadline, and a separate writer thread means a stuck engine can't block it.
- `src/proto.rs`: `[u32 len][u8 kind][u32 id][payload]`. Math replies are premultiplied BGRA plus metrics.

Limits applied to the child: `RLIMIT_AS` 512 MiB, `RLIMIT_CORE` 0, `PR_SET_PDEATHSIG` SIGKILL, 5 s job deadline, restart on the next job. The engine also applies 4 KiB TeX, 1 MiB request frame, and 8192 px / 16 MP raster caps.

## Reproduce

Run from `spikes/engine`:

```
/usr/bin/cargo build --release -j 6
target/release/host measure 20
target/release/host cold 10
target/release/host hostile
target/release/host idle
target/release/host pdeath
target/release/host render out
```

Raw output of the runs quoted here is in `results/`.

## Numbers

All are **measured** and **noisy**: parallel builds were running (load average about 5-7 on 28 threads). Latency and memory rows use n=20 after 2 discarded warm-ups, each with a fresh engine.

| Metric | min | median | p95 | max |
|---|---|---|---|---|
| Spawn + first math | 1.8 ms | 2.1 ms | 2.3 ms | 2.3 ms |
| Second math, live engine | 0.5 ms | 0.7 ms | 0.7 ms | 0.7 ms |
| First highlight, live engine | 19.2 ms | 20.2 ms | 21.4 ms | 22.0 ms |
| Spawn + first highlight | 20.0 ms | 21.3 ms | 24.2 ms | 29.8 ms |
| Of which loading syntaxes and theme | 1.2 ms | 1.6 ms | 1.7 ms | 1.7 ms |
| Second highlight, live engine | 0.3 ms | 0.5 ms | 0.6 ms | 0.6 ms |
| Engine binary evicted with fadvise, spawn + math (n=10) | 3.5 ms | 3.9 ms | 4.5 ms | 4.5 ms |
| VmHWM after math | 6.7 MiB | 6.9 MiB | 7.0 MiB | 7.0 MiB |
| VmHWM after math + highlight | 15.5 MiB | 15.5 MiB | 15.7 MiB | 15.7 MiB |
| Private after math, trimmed | 4.5 MiB | 4.6 MiB | 4.7 MiB | 4.7 MiB |
| Private after math + highlight | 13.1 MiB | 13.1 MiB | 13.2 MiB | 13.2 MiB |
| VmPeak (address space) | 212.4 MiB | 212.4 MiB | 262.4 MiB | 264.4 MiB |

Highlight input is the 81-byte Rust block from `fixtures/showcase.md`, giving 18 colour runs. Almost all the first-highlight cost after loading is lazy regex compilation, so other languages will differ. Only Rust was timed.

Binary size: `mdview-engine` is 6,243,864 bytes with test jobs and 6,239,520 bytes without. `host` is 617,728 bytes. **Merman is not included**, so the shipping engine will be larger and will use more memory.

Peak memory by workload (measured, one run each):

| Workload | Engine VmHWM |
|---|---|
| Largest raster allowed, 3972x3972 | 70.5 MiB |
| 900 KB Rust highlight | no higher than 69.6 MiB (HWM was already set by the raster) |
| 900 KB single-line JS highlight | 120.4 MiB |

Typical math and highlighting sit well below the plan's 30-60 MB engine budget. The largest raster and very large code blocks exceed it, within `RLIMIT_AS`. A lower raster cap would help. Merman may change this.

## Hostile inputs

Full table in `results/hostile.txt`. Every case returned an error or a correct result, the host kept ticking, and the next job succeeded (0 recovery failures across all cases).

| Case | Wall time | Result |
|---|---|---|
| Deep `\frac` x400, braces x2000, `x^{` x1000, `\sqrt` x500, `\left(` x300 | 0.0-1.0 ms | Error: parser recursion limit |
| Deepest accepted nesting: depth 32 (array: 10) | 0.1-3.1 ms | Rendered, VmHWM at most 9 MiB |
| `\def` expansion bomb | 2.5 ms | Error: too many expansions |
| `\rule` 100000em, 1e300em, 1e400em | 0.1 ms | Error: raster cap or non-finite size, before any SVG work |
| `\kern`, `\raisebox`, array row gap at 1e6em; tall `\left(` | 0.1 ms | Error: raster cap |
| `\rule` 124em, 15.8 MP | 79.3 ms | Rendered |
| TeX of 4097 bytes | 0.0 ms | Error: over 4096 |
| 10 MiB frame | 5.2 ms | Error: frame drained, not buffered |
| Invalid UTF-8, syntax error, unknown job, font size 1e9 | 0.0-0.1 ms | Error |
| 900 KB highlight, Rust / single line JS | 2185 / 2820 ms | Finished, ticks 136 / 175, max gap 16.1 ms |
| Panic job | 0.0 ms | Error with the panic message, engine survives |
| Hang (60 s sleep) | 5001.3 ms | Killed, restarted in 2 ms, ticks 311, max gap 16.2 ms |
| Job queued behind a hang | 5000.4 ms, then 2.9 ms | Hang killed, queued job rendered |
| Memory bomb, touched pages | 163.8 ms | Killed by abort (signal 6), restarted |
| Memory bomb, 1 TiB allocation | 34.6 ms | Killed by abort (signal 6), restarted |
| Stack overflow | 45.2 ms | Signal 6, restarted |

The largest RSS of any reaped engine was 442,996 KB, against the 524,288 KB limit (`RUSAGE_CHILDREN`).

`/proc/PID/limits` confirmed the applied limits: address space 536870912 bytes, core 0.

Parent death: after SIGKILL of the host, the engine was gone at the first check, 0.09 ms later (n=1 saved, n=2 total).

## Idle exit

- Exit at 10,000 ms after the last job, exit code 0, `/proc/PID` gone. Rounded to the millisecond, n=3 including the timer reset case, plus ticks 622 and max gap 16.2 ms on the first two.
- A job at 6 s reset the timer, with the same engine and no respawn.
- A job submitted right at the exit boundary (offsets -3, -1, 0, 1 ms, one run each) succeeded every time. One of the four lost the race and was resent automatically.

## Fixture math

All 4 examples in `fixtures/showcase.md` are extracted from the file, not hard-coded, and saved to `out/fixture-0.png` to `fixture-3.png` at 48 px/em. I inspected each with the Read tool, and they are correct: `e^{iπ}+1=0`, `a/b`, the integral, and the sum. `out/extra-4.png` adds a matrix, radical and Turkish `\text` as an extra check, and it is also correct.

PNGs are composited over white for inspection. The wire format stays premultiplied BGRA.

## Problems found

1. **ratex-svg writes sizes in pt.** With usvg's default 96 dpi, every formula came out 1.333x too large. This spike sets `dpi: 72` so one em equals `em_px`. Phase 4 needs the same setting.
2. **CJK in math fails by default here.** `\text{日本語}` returned "glyph could not be embedded" in all 22 runs of the saved measurement, because automatic discovery picked a font without those glyphs. With `RATEX_UNICODE_FONT` set to `NotoSansCJK-Regular.ttc` it rendered (3 runs, 7-14 ms). This needs a font selection step before Phase 4. The engine reports the error rather than drawing wrong glyphs, so the viewer can show source.
3. **First CJK attempt can be slow.** One failed attempt took 1128.7 ms, seen once on the very first run of the session, likely font discovery on cold files (cause not confirmed). In the saved run, failed attempts took 17.2-27.5 ms. It would miss the 150 ms target if it recurs.
4. **`PR_SET_PDEATHSIG` follows the spawning thread, not the process.** Only a long-lived thread may spawn the engine. The supervisor does this. The behaviour of a dying spawner thread was not tested.
5. **The abort message is lost.** Stderr is discarded, so the host reports "signal 6" rather than "out of memory" for a bomb.
6. **900 KB highlights take 2-3 s here.** They finish inside the deadline, but noticeably larger code would be killed and stay plain text. That fits the plan's plain-first behaviour.

## Not verified

- **Windows 11: nothing here was built or run on it.** Job Object limits are not implemented; `apply_limits` is an empty stub on non-Unix. Unix-only code is behind `cfg`, but that gating was not compiled for Windows, because the system Rust has no Windows standard library. All numbers above are Linux only. Process creation on Windows is usually slower and Defender may scan a new exe, so the 150 ms margin there is unknown.
- **A real viewer.** The UI thread is a stand-in, so Qt or WinUI event loop behaviour is untested. `pre_exec` forces fork instead of posix_spawn, and fork cost in a large Qt host was not measured.
- **True cold start.** The fadvise test evicts only the engine binary, not shared libraries. A cold boot was not measured (no root, no reboot).
- **Merman and SVG jobs.** They are not in this engine. Their latency, memory and hostile inputs belong to G5 and Phase 4.
- **Pathological regex in syntect.** Large inputs were tested, but no input was crafted to cause backtracking. The deadline should catch it, as the hang test shows, though that exact case did not run.
- **Unmeasured:** the onig backend (rejected here because it is C), and languages other than Rust and JS.
- **Tail latency.** Most rows have n=20 and hostile cases n=1, so tails are not well characterised.

## Verification

Independent audit, 2026-11-06. Load average was about 2, but a Windows VM and an unrelated Wine rustc were running. Raw output is in `results/verify/`.

**G6 verdict: PASS on Linux against a stand-in viewer. Not satisfied for Windows 11, because the memory limit does not exist there.**

| Criterion | Report | Re-run | Result |
|---|---|---|---|
| Spawn + first math, median / max | 2.1 / 2.3 ms | 2.0 / 2.1 ms | Confirmed |
| Spawn + first highlight, median / max | 21.3 / 29.8 ms | 21.5 / 24.7 ms | Confirmed |
| Hang killed | 5001.3, 5000.4 ms | 5001.0, 5000.9 ms | Confirmed |
| Stand-in thread max tick gap | 16.2 ms | 16.2 ms | Confirmed |
| Memory bombs | killed, signal 6 | killed, signal 6, 0 recovery failures | Confirmed |
| Idle exit | 10,000 ms | 10,000 ms | Confirmed |
| Parent death | gone at 0.09 ms | gone at 0.09 ms | Confirmed |

**Math PNGs:** I read `out/fixture-0` to `fixture-3` and `extra-4` myself. All are correct: Euler's identity, a/b, the integral equal to 1/3, the sum equal to n(n+1)/2, and the matrix, radical and `Türkçe` text. A re-render of `fixture-2` is byte-identical.

**Flags:**

- **"Viewer unaffected" is close to true by construction.** The stand-in thread only waits on a channel and does no painting or real work, so the 16 ms tick could hardly fail. It proves the threads are separate, not that Qt or WinUI stays smooth.
- **"After paint" is not what was measured.** The clock starts at job submission, and there is no paint. The report says so.
- **The host is tiny.** Spawn time from a large Qt or WinUI process is not represented, and `pre_exec` forces fork on Linux.
- **The hang job is a sleep,** not hostile TeX. No TeX input was found that hangs, so the deadline has only been exercised by the test job. SIGKILL doesn't depend on what the engine is doing, so I consider this a small gap.
- **Merman is not in this engine.** Mermaid latency, memory and binary size are outside these numbers. Standalone with resvg, the merman binary is 16.6 MB and peaks at 18-24 MiB per diagram (see `spikes/mermaid/RESULT.md`).
- **Not reproduced:** the 1128.7 ms CJK failure. The re-run saw 16.8-52.8 ms, so the cause is still unknown. CJK in math still fails in every run without a configured font.
- **Idle race:** the re-run used 0 retries where the report saw 1, so the race is timing dependent and the retry path was not exercised this time.
- **Weakened criterion or estimated number presented as measured:** none found. The stand-in limit is disclosed.

**Windows 11:**

- It type-checks for `x86_64-pc-windows-msvc` (`cargo check`), which answers the report's "gating was not compiled". It was not linked or run.
- `apply_limits` is an empty stub there and `RLIMIT_AS_BYTES` is unused, so **a memory bomb on Windows is currently unlimited**.
- Job Object support has to be written and tested before this gate can be called passed for Windows 11. Parent-death cleanup also needs that Job Object.
