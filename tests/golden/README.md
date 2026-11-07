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

`images-light.png` is `fixtures/images.md` through `mdv-bench`: a real PNG drawn as a block and
inline, next to the placeholders for a missing file and an SVG. `shell-winui-images.png` and
`shell-qt-images.png` show the same file in both shells, with the badge drawn both ways.

They are **not** a pixel test baseline yet:

- They use this host's system fonts (Segoe UI, Consolas), so a render on Linux or another Windows
  build will differ. The plan calls for bundled fonts and a small tolerance; that is still open.
- Math and diagrams are still source text until Phase 4.
- The shells virtualize, but the renderer still has no selection or anchors.
- The window captures include the 31 px title bar, which follows Windows; compare the client area.

To regenerate:

```sh
cargo run --release -p mdv-bench -- --all
```

which writes the same four `showcase-*.png` files under `target/goldens/`. `--input`, `--out`,
`--width`, `--scale`, `--theme` and `--max-height` render one page; `--width` with `--viewport`,
`--scroll` and `--repeat` measures scroll frames instead (see `docs/HANDOFF.md`, section 6).
