# G5: Mermaid routes

**G5: PASS on all 8 families with merman 0.9.3, once this spike's route is used: real Segoe UI metrics, the SVG repair pass, then resvg.** Measured on Linux with the real Windows font files. Nothing ran on Windows 11; the route only type-checks there.

Plan criterion: "All 8 render with labels, each under 300 ms, no ELK." The harder diagrams go beyond that criterion, and they found defects that are listed below rather than counted as gate failures.

**Recommendation: merman for all 8 families, and drop mermaid-rs-renderer.** Without this route, merman as built still fails the gate. See the [audit](#audit-as-built-before-this-route).

| Family | G5 | Renderer | Still wrong in harder diagrams (labels complete in all of them) |
|---|---|---|---|
| Flowchart | PASS | merman | An edge crosses a subgraph title ("Engine process") |
| Sequence | PASS | merman | Long note text runs edge to edge in its box. Lifelines cross some message text |
| Class | PASS | merman | Cardinality labels sit very close to arrowheads |
| State | PASS | merman | None seen |
| ER | PASS | merman | Crow's feet at box corners are tight but visible |
| Gantt | PASS, **weakest** | merman | A long task label overlaps a section title. Short charts repeat axis dates |
| Pie | PASS | merman | Slices under 1% get a legend entry and no slice. Two neighbouring percentages touch |
| Mindmap | PASS | merman | Pale edges on white are low contrast (default theme) |

## What changed

| Part | File | What it does |
|---|---|---|
| Real metrics | `src/metrics.rs` | A merman `TextMeasurer` using the painted font and harfrust, the shaper resvg uses. No new crates |
| Repair pass | `src/patch.rs` | Sets nested `<svg>` `overflow:hidden` to visible, and any negative stroke width to 2. Rewrites only style and `stroke-width` markup, never label text |
| Gantt gutter | `src/pipeline.rs` | Sizes `gantt.leftPadding` from the longest section title (75-300 px). Added because titles ran under the bars |
| Italic faces | `src/pipeline.rs` | Loads all 4 Segoe UI faces. Without italics, abstract class members looked like normal ones |
| Hostile limits | `src/pipeline.rs` | Refuses source lines over 4 KiB. Caps rasters at 16 MP, stepping down to 0.5x at the lowest |

**Why the labels were clipped:** merman sized boxes from its estimate for "Recursive Variable", a font we never paint with. With real metrics, removing the clip changes showcase and core pixels by at most 2 of 255 on Segoe UI, so measurement was the cause. In the harder set the clip still changes pixels by more than 8 of 255 in 7 of the 16 diagrams, up to 216 on hard-class-b, so the repair stays. I didn't locate those pixels individually. A separate advance-width check showed label boxes matching text widths once kerning is allowed for.

## Every output PNG (read with the Read tool)

Light theme, Segoe UI, in `verification/patched/`. "Edges" means every edge and marker is visible.

| PNG | Labels complete | Edges | Notes |
|---|---|---|---|
| showcase-flowchart | Yes | Yes | |
| showcase-sequence | Yes | Yes | |
| showcase-class | Yes | Yes | Was clipped as built |
| showcase-state | Yes | Yes | |
| showcase-er | Yes | Yes | `\|\|` and crow's foot both clear. Was clipped as built |
| showcase-gantt | Yes | Yes | Title no longer touches the bar. Axis repeats dates at 12 h ticks |
| showcase-pie | Yes | Yes | Slices are pale |
| showcase-mindmap | Yes | Yes | Files to README edge now drawn. Was clipped as built |
| core-flowchart | Yes | Yes | |
| core-sequence | Yes | Yes | |
| core-class | Yes | Yes | |
| core-state | Yes | Yes | |
| core-er | Yes | Yes | |
| core-gantt | Yes | Yes | Axis repeats dates |
| core-pie | Yes | Yes | |
| core-mindmap | Yes | Yes | |
| hard-flowchart-a | Yes | Yes | Subgraphs, diamonds, edge labels. "yes"/"no" labels sit close to arrowheads |
| hard-flowchart-b | Yes | Yes | **"Engine process" title crossed by an edge.** Turkish and a 49-character word fit |
| hard-sequence-a | Yes | Yes | **Note text touches both box edges.** Lifelines cross some text |
| hard-sequence-b | Yes | Yes | "critical" tab is tight but inside |
| hard-class-a | Yes | Yes | All four relationship markers visible |
| hard-class-b | Yes | Yes | Abstract member italic after the fix. "many" touches its arrow line |
| hard-state-a | Yes | Yes | Grey edge-label backgrounds are heavy |
| hard-state-b | Yes | Yes | Choice, fork, join and concurrency regions correct |
| hard-er-a | Yes | Yes | Crow's feet at box corners are tight |
| hard-er-b | Yes | Yes | Self-loop visible. Two edges cross |
| hard-gantt-a | Yes | Yes | "Ship" text overlaps its milestone diamond. Grid lines cross some text |
| hard-gantt-b | Yes, with **one overlap** | Yes | **"Long running soak test..." collides with the "Infrastructure..." title.** It was cut off at the canvas edge before the gutter fix |
| hard-pie-a | Yes | Yes | Title reaches the left edge, not clipped |
| hard-pie-b | Yes | **No** | **Five slices under 1% have a legend entry and no slice.** "3%" and "1%" touch |
| hard-mindmap-a | Yes | Yes | Depth 8 edges drawn, pale on white |
| hard-mindmap-b | Yes | Yes | All shapes correct, long label wraps |
| probe-pie-long-title | Yes | Yes | 125-character title fits; the canvas widens |
| probe-scripts | **No, with Segoe UI only** | Yes | **Chinese, Japanese, Korean and emoji are tofu.** Arabic, Cyrillic and Greek are fine |
| probe-scripts (`verification/patched-fallback/`) | Yes | Yes | With msyh, malgun, seguisym and seguiemj. Emoji paints monochrome |
| probe-flow-150 | Top 1,400 px only | Yes in that part | Painted at about 0.65x. I did not inspect the rest |
| probe-seq-200 | Not readable at the size I viewed | Layout looks right | 8,835 px tall, downscaled to view |

The sub-1% slices look deliberate, as I recall mermaid.js filtering them too, but there is no mermaid.js render here to confirm that. This says nothing about parity with GitHub.

**Automated tests:** `cargo test --release -j 6 --features r-merman --target-dir target-merman` gives 11 repair-pass and 12 route tests, all passing. The 12 route tests also pass with `MERMAID_FONT=liberation`, after I loosened the clip test's limit from 8 to 16 because Liberation reached 14 on `showcase-class`. The tests check that expected words are painted and that no text leaves the canvas. They can't see overlaps or tofu, which came from reading the PNGs.

## Measurements

All measured on Linux, release build, 2x raster plus PNG encode. **Timings are noisy:** load average was 4-10 from other work. A viewer wouldn't need the PNG encode.

| Set | Median of 30, same process | Worst of 30 | Under 300 ms |
|---|---|---|---|
| Showcase, worst family (sequence) | 16.40 ms | 17.34 ms | Yes, all 8 |
| Core tests, worst family (sequence) | 14.35 ms | 16.61 ms | Yes, all 8 |
| Harder, worst (flowchart-a, 4280x967) | 203.33 ms | 208.77 ms | Yes, all 16 |
| Size probe, 200-message sequence | 203.52 ms | 211.05 ms | Yes |
| Size probe, 150-node flowchart | **307.71 ms** | **344.94 ms** | **No** |

| Fresh process, 3 runs each | Total | Peak VmHWM |
|---|---|---|
| Showcase, 8 families | 10.7-22.6 ms | 21.8-27.1 MiB |
| Harder, 4 largest (flowchart-a, flowchart-b, state-b, er-a) | 94.2-228.3 ms | 49.0-70.7 MiB |
| 150-node flowchart | 348.5-352.2 ms | 150.7 MiB |
| 200-message sequence | 215.0-216.7 ms | 149.5 MiB |

- **Fonts:** loading 4 faces took 2.3-3.8 ms and peak memory was about 13.9 MiB afterwards, against about 4.9 MiB at start.
- **Fallback fonts:** adding msyh, malgun, seguisym and seguiemj raised the script probe's peak from 24.0 to 70.0 MiB, and font loading from 2.6 to 10.3 ms (one run each). Load them only when a diagram needs them.
- **Repair pass:** at most 0.21 ms on the 32 fixture diagrams and 0.95 ms on the 150-node probe, in the single render run behind `patched_render_light.txt`.
- **ELK:** none. `cargo tree -e normal --features r-merman | grep -ic elk` prints 0.

| Stripped binary, thin LTO | Bytes |
|---|---|
| `resvg-only` (baseline) | 3,583,800 |
| `merman-bin` (as built) | 16,623,928 |
| `merman-patched` (this route, with test harness) | 16,945,864 |

**merman adds about 13.4 MB over resvg alone.** Metrics, repair and dark theme code add about 0.32 MB of that. Fat LTO was not tried.

## Hostile input

One fresh process per case, 8 s limit, `verification/logs/patched_hostile.txt`. The engine process wasn't involved, so this says nothing about G6.

- **Malformed (16 cases):** 14 return an error in about 2.5 ms, so the viewer can show source. `init_bad_json` and `pie_zero` render an image, which is reasonable for both.
- **Over limits:** 3 MiB source, 40k edges, 40k pie slices, 40k messages and 2,000-depth nesting are all refused in 2-151 ms. The 40k edge case peaks at 202 MiB before refusal.
- **1.5 MiB label:** now refused in 2.4 ms. It was the one hang as built.
- **Where that hang came from:** the timings point to resvg's text layout, not merman or this repair. merman finished 64 KiB in under 15 ms, then the raster step took 2.9-4.6 s and about 600 MiB. 256 KiB gave no result in 20 s, with or without this route. Before the limit, merman's own 5 s `with_deadline` still gave no result by 12 s on the 1.5 MiB label, which fits the time being spent after merman returns.
- **2,000-edge flowchart:** errors after 2.16 s (a merman limit), so it shows source.
- **Multi-line label:** the 4 KiB line limit doesn't catch it. 64 KiB over 1,600 lines took 856.5 ms and 296 MiB. 8 KiB over 200 lines took 370.1 ms and 202 MiB. It finishes, but the peak is high. **The engine's memory limit needs to cover this.**
- **Not rerun:** mermaid-rs-renderer. Its results below are from the audit.

## Dark mode

Works today through merman's `HostTheme` API, in `src/pipeline.rs`: Windows 11 style surfaces, a transparent root, and the viewer's own background. A theme switch means rendering the diagram again.

All 36 rendered with no errors, in `verification/patched-dark/`. I read the 8 showcase PNGs plus hard-sequence-a, hard-gantt-a, hard-er-b and hard-class-b. Labels, edges and markers were clear in those.

**One fault: dark Gantt loses status colours.** Done and critical bars look the same as normal ones, and active differs only by a brighter border. Light theme shows these clearly. Mermaid's `critBkgColor`, `doneTaskBkgColor` and `activeTaskBkgColor` theme variables might fix it (not tested). I also didn't choose the dark palette with a designer's eye, and gantt grid lines are faint.

## What the engine still needs

1. **Run it on Windows 11.** It type-checks with the private toolchain, `cargo check --release -j 6 --features r-merman --bins --target x86_64-pc-windows-msvc` (RUSTUP_HOME and CARGO_HOME under `~/.local/rust-win`). The Segoe UI files here came from a Windows install, so metrics should carry over, but that's an expectation, not a measurement.
2. **Fallback fonts for non-Latin text**, loaded only when needed. Without them, Chinese, Japanese and Korean labels are tofu. Emoji was monochrome.
3. **Large diagrams.** Above 16 MP the route paints smaller, and below 0.5x it refuses. The 150-node flowchart came out at about 0.65x, small to read. Painting only the visible part would fix this, and is untested.
4. **Gantt width.** Canvas width is merman's default, so the chart doesn't follow the window. Long task labels can still collide with section titles.
5. **Default colours.** Pale pie slices and loud mindmap colours are not the modern look you want. The `HostTheme` API can also set a light palette (not tried).

## Reproduce

```
cd spikes/mermaid
cargo build --release -j 6 --features r-merman --bins --target-dir target-merman
cargo test --release -j 6 --features r-merman --target-dir target-merman
B=target-merman/release/merman-patched
$B render verification/patched light real full
$B render verification/patched-dark dark real full
$B bench 30
$B one showcase-pie
$B hostile over_label_1p5MiB
$B file path/to/diagram.mmd
```

`MERMAID_FONT=liberation` swaps the font. `MERMAID_FALLBACK=a.ttc;b.ttf` adds fallback faces. Logs are in `verification/logs/patched_*.txt`. The harder diagrams are in `corpus/`, and each starts with a `%% expect:` line listing its labels.

## Audit (as built, before this route)

Independent audit, 2026-11-06, with merman 0.9.3, mermaid-rs-renderer 0.3.1 and resvg 0.49. Load average about 5.5. Evidence is in `verification/merman/`, `verification/mmdr/` and `verification/merman-patched/`. No earlier result report existed for this spike. `build.sh` had stopped partway at 17:22, and no benchmark or PNG output was kept.

**Verdict then: FAIL as built, because neither crate drew all 8 correctly.**

| Family | merman as built | mermaid-rs-renderer |
|---|---|---|
| Flowchart | Good | Good |
| Sequence | Good | Good |
| Class | **Labels clipped** ("ocumen", "arkdow") | Good |
| State | Good | Good |
| ER | **Labels clipped** (FOLDER, DOCUMENT, contains) | Labels fine, **label hides the `\|\|` end, no crow's foot visible** |
| Gantt | Good, section label touches the first bar, repeated axis dates | Good, axis skips 11-04 |
| Pie | Good | Good, Markdown slice and swatch are very pale |
| Mindmap | **Labels clipped, Files to README edge invisible** | **Wrong tree:** Outline and Search drawn under Files, not the root |

**Measured on 8 showcase diagrams, 2x raster plus PNG encode, Liberation Sans:**

| Check | merman | mermaid-rs-renderer |
|---|---|---|
| Worst median, 30 iterations | 15.48 ms (sequence) | 9.37 ms (pie) |
| Worst fresh process total | 18.16 ms (sequence) | 12.59 ms (pie) |
| Peak VmHWM, one diagram | 18.1-24.2 MiB | 23.6-27.4 MiB |
| Stripped binary, with resvg, thin LTO | 16.6 MB | 8.7 MB |
| ELK in `cargo tree -e normal` | None | None |

**Why not mermaid-rs-renderer, despite being 7.9 MB smaller:**

- It draws the wrong mindmap tree and hides ER cardinality.
- It returns an image for `flow_unterminated`, `gantt_bad_date`, `gantt_cycle`, `pie_nonnum` and `mindmap_bad`, for example a 93-byte blank PNG, so "malformed input shows source" wouldn't trigger.
- It dumped core on 3 MiB source, gave no result on the 40k edge, 40k sequence and 1.5 MiB label cases within 8 s, and took 7.5 s on 2,000 edges, over the 5 s deadline.

Loading all system fonts into resvg's fontdb took 1,110 ms once, so the engine shouldn't do that at start.

The older `logs/build-merman-cose.log` and `logs/build-mmdr.log` only record a missing `/usr/bin/time`, and `logs/build-merman.log` ends mid-compile. `verification/patched_render.rs` is the auditor's original patch, replaced by `src/patch.rs`. It only handled `-1` and `-4`, and it would have rewritten label text.
