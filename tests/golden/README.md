# Golden images

The Phase 2 renderer images, from `fixtures/showcase.md` rendered with `mdv-bench --all` on Windows
at 100% and 150%, light and dark, plus screenshots of the first WinUI shell showing the same file on
this PC. They are here for the user's review of the look and of the shell, and as the starting point
for the real golden suite.

`shell-winui-light.png` and `shell-winui-dark.png` are captures of `mdview-winui.exe` at 1100x900 on
Windows 11, DPI scale 100%, taken with the `--light` and `--dark` flags; both show the shell's own
scrollbar in the right-hand gutter. `shell-winui-scrolled.png` is the same window after six 40 px
scroll frames (`--scroll-steps 6`), so it is the view the shifted-frame path produces.
`shell-qt-light.png` and `shell-qt-scrolled.png` are captures of `mdview-qt.exe` (the Qt variant,
built against Qt 6.9.3 for MSVC) at the same size, at the top and after a click in its scrollbar.
`shell-qt-open.png` is the Qt File > Open dialog, opened in `fixtures/`.
`shell-qt-linux.png` is the same shell built against Qt 6.8.2 on Debian 13, running under Xvfb on
the test VM. `shell-qt-1mb.png` is the Qt shell showing `spikes/parser/data/generated_1mib.md`, the
virtualization test document. They are evidence that both shells run, not a baseline.

`shell-qt-selection.png` and `shell-winui-selection.png` show the word "Markdown" selected in the
showcase heading. The Qt one is from `--select-hold`, which drags through the real Qt mouse path and
keeps the window open; the WinUI one is after a real press-drag-release on the running window. The
selection is drawn over the frame in both shells and is not part of the frame delta.
`shell-winui-anchor.png` is the WinUI window after a click on the `#diagrams` link, scrolled so the
Diagrams heading sits just below the top edge. `shell-winui-details.png` is the WinUI window after
`--details-test` opened the showcase `<details>`: the chevron points down and the body is visible.
`shell-qt-svg.png` and `shell-winui-svg.png` show the showcase's SVG badge, with its text, drawn
through `mdview-engine` in both shells. `shell-winui-two-svg.png` is `fixtures/two-svg.md`, where
both badges come from one engine process; the process count was 1 three seconds in and 0 after the
10 s idle exit, with the viewer still open.
`shell-qt-math.png` and `shell-winui-math.png` are `fixtures/math.md`: inline math on the text
baseline, centred display math, a matrix, a heading formula and formulas inside a table. All seven
came from one shared engine process.
`shell-qt-mermaid.png` and `shell-winui-mermaid.png` are the showcase at scroll 1100: flowchart,
sequence and class diagrams drawn by merman in both shells, again from the one engine process.
`shell-qt-highlight.png` and `shell-winui-highlight.png` are the showcase at scroll 500, where the
Rust fence is coloured by syntect in both shells.
`shell-qt-find.png` and `shell-winui-find.png` show find for "Markdown": four hit rectangles and the
second one active, with the Qt status bar and the WinUI title carrying `2 of 4`.
`shell-qt-outline.png` shows the Qt outline dock with all 15 headings, scrolled to Diagrams;
`shell-winui-outline.png` shows the same jump with the Outline menu in the bar.

Math and Mermaid changed the showcase: the page is now 4071 px at 100% (taller formula lines and
diagram images instead of code boxes) and the Mathematics and Diagrams sections show rendered output,
so the four `showcase-*` images were regenerated. The engine must be built (or `MDV_ENGINE` set) when
regenerating, because SVG, math and Mermaid go through it. Syntax highlighting changed only the Rust
block's glyph colours, so the page height stayed 4071 px. Dark images use Mermaid's dark palette with
contrast overrides, gantt section labels clear the task bars and date ticks no longer collide, and
mindmap labels match their node size.

`images-light.png` is `fixtures/images.md` through `mdv-bench`: a real PNG drawn as a block and
inline, a missing file's placeholder, and the SVG badge that the engine now draws in the last line.
`shell-winui-images.png` and `shell-qt-images.png` show the same file in both shells, with the badge
drawn both ways.

**Pixel test.** The four `showcase-*.png` images are a baseline: `crates/render/tests/golden.rs`
renders the showcase at the same four settings and fails when more than 0.1% of pixels differ by more
than 24 in any channel, writing the actual image to `target/golden-actual-<name>.png`. A red/blue
subpixel swap alone changes 0.94% and fails it. Limits:

- They use this host's system fonts (Segoe UI, Consolas), so the comparison runs on Windows only;
  elsewhere it passes with a note. Bundled fonts would make it portable; that is still open.
- Only the four showcase renders are compared. The `shell-*` captures and `images-light.png` are
  evidence that the shells run, not a baseline.
- Fenced code is coloured from syntect's bundled syntaxes and two themes; unknown languages, inline
  code and blocks over 256 KiB stay plain.
- The window captures include the 31 px title bar, which follows Windows; compare the client area.

After an intended visual change, regenerate the baseline with
`MDV_BLESS=1 cargo test --release -p mdview-render --test golden`, look at the new images, and commit
them only once they are approved.

To regenerate:

```sh
cargo run --release -p mdv-bench -- --all
```

which writes the same four `showcase-*.png` files under `target/goldens/`. `--input`, `--out`,
`--width`, `--scale`, `--theme` and `--max-height` render one page; `--width` with `--viewport`,
`--scroll` and `--repeat` measures scroll frames instead (see `docs/HANDOFF.md`, section 6).

