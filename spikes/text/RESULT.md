# Text spike: G7 and G3 preparation

## Results

| Gate | Criterion | Result |
|---|---|---|
| G7 | Height pass under 500 ms off-thread; visible layouts under 8 MB | **PASS** on Linux for the specified 1 MB document |
| G3 | Rust text vs QPainter, equal or better at reading distance | **Not judged: the user decides.** Images are in `compare/INDEX.md` |

The G7 pass has three limits:

- The stress document with 50% non-Latin words missed the target on its first pass with one thread (about 534 ms). Four threads brought it to about 298 ms.
- The document is only Markdown-like, and no real parser ran, so parse time is not included.
- Nothing was measured on Windows 11, which is the Windows target.

## G7 numbers

Every number in this section is **measured** and **noisy**: parallel builds were running, with load average about 4 to 7. Each run is a fresh process with the OS font cache warm. "Live MB" comes from the counting global allocator.

The document is seeded and contains 1,045,708 bytes in 1,744 blocks. The width is 900 logical px at 100%. The height pass runs off the main thread and reuses its scratch contexts.

### Height pass

| Document, setup | Runs | First pass in process, ms | Later passes, ms |
|---|---|---|---|
| **Mixed, 1 thread (the gate case)** | 5 | 348.4, 351.1, 358.7, 360.4, 367.5 | 181.9 to 250.1, typically about 184 (20 passes) |
| ASCII only, 1 thread | 3 | 167.4, 168.7, 168.8 | 163.0 to 185.2 (6 passes) |
| Mixed, width 600, 1 thread | 3 | 349.2, 350.2, 362.1 | 182.0 to 185.2 (6 passes) |
| Mixed, 150%, 1 thread | 3 | 350.2, 353.7, 354.7 | 182.5 to 184.5 (6 passes) |
| Mixed, 220%, 1 thread | 3 | 350.9, 351.4, 353.6 | 182.5 to 184.2 (6 passes) |
| Mixed, 4 threads | 3 | 231.4, 234.5, 237.4 | 68.8 to 73.4 (6 passes) |
| **50% non-Latin stress, 1 thread** | 3 | **533.0, 533.9, 537.4 (over target)** | 361.3 to 372.2 (6 passes) |
| 50% non-Latin stress, 4 threads | 3 | 286.0, 298.5, 300.4 | 119.6 to 131.2 (6 passes) |

The mixed document has about 4% non-Latin words. The first pass costs about 170 ms more than later passes on mixed text, but barely more on ASCII, so the extra cost appears to be font fallback lookup (inferred).

Peak extra allocation during the mixed first pass was 0.99 MB, and later passes used 0.18 MB.

### Memory for visible layouts

Target: under 8 MB. Each run sampled 10 random scroll positions at 1220x820, and results were identical across runs.

| Document, scale | Viewport only: median / max MB | Viewport plus one each side: median / max MB |
|---|---|---|
| Mixed, 100% | 0.225 / 0.283 | 0.620 / 0.701 |
| Mixed, 150% | 0.177 / 0.197 | 0.405 / 0.440 |
| Mixed, 220% | 0.114 / 0.126 | 0.238 / 0.255 |
| 50% non-Latin, 100% | 0.617 / 0.827 | 1.383 / 1.604 |

Whole process at the end of a mixed run at 100% (5 runs): RSS 34.5 to 34.8 MB, RssAnon 9.3 MB. RSS includes mapped font files. This run also holds the document text, the 1220x820 framebuffer (about 4 MB) and the glyph cache.

### FontContext creation

Measured in 10 fresh processes, on a machine with 3,039 fonts and 2,117 families.

| Step | Median | Range |
|---|---|---|
| `FontContext::new()` | 25.8 ms | 24.11 to 27.06 ms |
| First layout that needs fallback | 13.6 ms | 13.11 to 14.57 ms |

After that first layout, live memory was 0.53 MB, RSS about 16.4 MB and RssAnon 3.1 MB. This is much cheaper than the plan assumed. It probably benefits from fontconfig's on-disk cache (not verified).

### Frame raster

1220x820 BGRA, repainting the **whole frame** on every frame, which is worse than the plan's strip painting. Times exclude layout and anything that gets pixels to the screen. Each row is 3 or 5 runs, showing the range across runs.

| Setup | Cold first frame, ms | 40 px scroll, 600 frames: p50 / p99, ms | Random jump, 200 frames: p50 / p99, ms | Peak cache |
|---|---|---|---|---|
| Mixed, 100% (5 runs) | 2.46 to 2.52 | 1.29 to 1.32 / 1.45 to 2.01 | 1.27 to 1.30 / 1.47 to 2.24 | 0.42 MB |
| Mixed, 150% (3 runs) | 2.52 to 2.53 | 1.17 to 1.18 / 1.28 to 1.31 | 1.16 to 1.17 / 1.30 to 1.32 | 0.73 MB |
| Mixed, 220% (3 runs) | 2.40 to 2.45 | 0.99 to 1.00 / 1.10 to 1.14 | 1.01 to 1.02 / 1.20 to 1.22 | 1.58 MB |
| Cache capped at 256 KB, 220% (3 runs) | 2.37 to 2.38 | 0.97 / 2.50 to 2.53 | 1.67 to 1.68 / 2.49 to 2.53 | 0.26 MB |

- **The 4 MB cache never evicted in these runs.** Its peak was 1.58 MB, so 4 MB is more than this document needs.
- The eviction path was only exercised by the 256 KB run, which had about 27,000 evictions and stayed near its cap.
- Cache bytes are mask data plus an **estimated** 96 bytes per entry.
- Laying out newly visible blocks during scrolling is extra: p99 0.25 to 0.33 ms for mixed text (0.83 to 0.86 ms for the stress document), and p99 0.61 to 1.45 ms on random jumps for mixed text (2.9 to 3.0 ms for the stress document).

## G3 preparation

The specimen covers English, Turkish, Japanese, emoji, Arabic and monospace code, in light and dark, at 100, 150, 170 and 220%. That gives 16 side-by-side PNGs: 8 with the curated font stack and 8 with default fallback. The observed defects are listed in `compare/INDEX.md`.

The main findings:

- **Don't rely on fontique's default emoji fallback.** It chose Segoe UI Emoji, and the output had a face with no fill, "TR" instead of a flag, and a black heart. Naming Noto Color Emoji in the stack fixes all three on Linux.
- **Windows 11 risk.** The Segoe UI Emoji file here has COLR version 1, and swash rendered it incompletely. If Windows 11's own copy is the same format, emoji on Windows would show the same fault. The file's origin and the cause are both unconfirmed. Test this at W1, where the plan already allows a DirectWrite `GlyphRaster` backend.
- **Emoji size differs.** With the curated stack, Rust emoji are larger and wider than Qt's.

### Qt reference route

The reference is a small cxx-qt program with no authored C++: a `QWidget` subclass with a Rust `paintEvent`, captured by `QWidget::render` into a QImage at the matching device pixel ratio. The window is never shown.

Painting straight onto a QImage does not match what a widget gets, because that text comes out grayscale. Pixels with visible colour at 100% in light, **measured** with the same ImageMagick threshold:

| Route | Coloured pixels |
|---|---|
| QPainter directly on QImage (only emoji are coloured) | 1,266 |
| QPainter in a widget, via `QWidget::render` | 28,777 |
| Rust renderer | 24,017 |

The reference output was byte-identical for `QT_QPA_PLATFORM=offscreen` and the default Wayland platform on the one image compared (light, 100%). The colour counts also matched at 170% and 220%.

This also matters for the Qt shell: any text drawn through an offscreen QImage will not get LCD antialiasing from Qt.

## Reproduce

Run from `spikes/text`:

```sh
/usr/bin/cargo build --release -j 6
(cd qtref && /usr/bin/cargo build --release -j 6)

./run_g7.sh    # writes results/*.txt; takes a few minutes

mkdir -p compare/raw
./target/release/specimen render compare/raw > compare/raw/fonts_default.txt
CURATED=1 ./target/release/specimen render compare/raw > compare/raw/fonts_curated.txt
timeout 60 qtref/target/release/qtref compare/raw
./target/release/specimen compose compare/raw compare
CURATED=1 ./target/release/specimen compose compare/raw compare
```

Raw output for every G7 run is in `results/`. The `heavy_t4` and `evict_s220_256kb` files came from the same commands now listed in `run_g7.sh`, run by hand after the main batch. They ran at a lower load (about 3.8 for `heavy_t4`).

## Not verified

- **Windows 11:** nothing here ran on Windows. Fontique uses a different font backend there, so all timings and fallback choices are Linux only.
- **Real Markdown:** no pulldown-cmark, no markers stripped, tables laid out as monospace lines, and no images, math or diagrams.
- **Cold disk:** the OS page cache was warm, and I can't drop it without sudo. Cold-start font numbers are unknown.
- **Quiet machine:** all timings were taken alongside other builds.
- **On-screen Qt:** the reference was captured through `QWidget::render`, not from a window on the Plasma compositor. I assume it matches but did not screenshot one.
- **Qt settings:** the Qt result depends on this machine's fontconfig (`rgba=1`, `hintstyle=1`).
- **Renderer gaps:** no hinting, no synthetic bold or italic, RGB subpixel order only, and no underline or strikethrough. Colour glyph blending assumes straight alpha from swash; it looks right but I did not check the swash source.
- **Layout parity:** Rust and Qt each did their own shaping, so glyph positions can differ. I did not measure line widths apart from the emoji estimate.
- **Peak cache with a longer session:** the frame test covers 800 frames, not long use across many documents or zoom levels.

## Verification

Independent audit, 2026-11-06. Load average was about 1.5-2, but a Windows VM and an unrelated Wine rustc were running. Raw output is in `results/verify/`. I did not overwrite `results/*.txt`.

**G7 verdict: PASS for the specified document on Linux.** **G3 preparation: complete. The judgement stays with you.**

| Check | Report | Re-run | Result |
|---|---|---|---|
| Mixed, 1 thread, first pass | 348.4-367.5 ms | 346.8-394.0 ms | Confirmed, under 500 ms |
| Mixed, later passes | about 184 ms | 179.3-179.8 ms | Confirmed |
| 50% non-Latin, 1 thread, first pass | 533.0-537.4 ms | 527.1-528.3 ms | Confirmed, **still misses on a quieter machine** |
| 50% non-Latin, 4 threads, first pass | 286.0-300.4 ms | 293.7-306.2 ms | Confirmed |
| Visible layouts, viewport max | 0.283 MB | 0.283 MB | Confirmed, identical |
| Visible layouts, plus one each side, max | 0.701 MB | 0.701 MB | Confirmed, identical |
| `FontContext::new()` | median 25.8 ms | 24.6-30.9 ms | Confirmed |

The miss on the stress document is not caused by noise, so the single-thread margin depends on the text. A document that is mostly CJK or Arabic wasn't tested and would probably miss on one thread (inferred). The plan needs threads, or accepts that.

**Flags:**

- **Description error:** "about 4% non-Latin" is wrong. `word()` gives about 3% Turkish words, which are Latin script, and about 1% from the real non-Latin list. Code blocks have none. So the gate document is about 1% non-Latin at most, which makes it an easier test than the text suggests.
- **What the benchmark leaves out:** no parser, `**` and backtick markers left in the text, and tables as monospace lines rather than one layout per cell. It is close to the criterion but cheaper than a real document. Parsing adds about 5 ms. Per-cell tables are unmeasured.
- **Memory:** this counts requested bytes for retained layouts. It excludes allocator overhead, and the raster and glyph caches. The margin is about 5x, so that does not change the result.
- **Coloured pixel counts cannot be re-derived.** The table says "same ImageMagick threshold" but records neither the threshold nor the command. With my own thresholds, Qt is ahead of Rust at every level, as reported, but the exact numbers don't match.
- **Weakened criterion:** none. The gate uses the slower first pass, which is the honest choice.
- **Estimated presented as measured:** none. The 96 byte entry size and 23 px emoji offset are labelled as estimates.

**G3 images (no judgement made):**

- All 16 composites exist: 4 scales x light/dark x curated/default, and the dimensions match the specimen at each scale. Every row in `compare/INDEX.md` links to a real file.
- All 16 composites match their raw Rust and Qt images pixel for pixel. Re-running `specimen` and `qtref` reproduced all 24 raw PNGs byte for byte.
- `qt6-base` is 6.12.0, and `raw/fonts_*.txt` match the fonts INDEX names.
- I viewed `curated_light_100`, `default_light_100` and `curated_dark_170`. INDEX's defects are visible as described: the outline face, "TR" and the black heart with default fallback, and larger emoji with the curated stack.
- **The composites depend on Windows fonts copied to this Linux machine** (MS Gothic, Segoe UI Emoji), and you are judging Linux text. Windows 11 text quality is W1.

**Windows 11:** it type-checks for `x86_64-pc-windows-msvc` (`cargo check`). Nothing was run there, so timings and fallback choices stay Linux only, and the COLR emoji risk is still open.
