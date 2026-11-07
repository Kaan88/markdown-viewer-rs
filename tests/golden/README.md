# Golden images

The Phase 2 renderer images, from `fixtures/showcase.md` rendered with `mdv-bench --all` on Windows
at 100% and 150%, light and dark, plus screenshots of the first WinUI shell showing the same file on
this PC. They are here for the user's review of the look and of the shell, and as the starting point
for the real golden suite.

`shell-winui-light.png` and `shell-winui-dark.png` are screen captures of `mdview-winui.exe` at
1100x900 on Windows 11, DPI scale 100%, taken with the `--light` and `--dark` flags.
`shell-qt-light.png` and `shell-qt-scrolled.png` are captures of `mdview-qt.exe` (the Qt variant,
built against Qt 6.9.3 for MSVC) at the same size, before and after wheel scrolling.
`shell-qt-linux.png` is the same shell built against Qt 6.8.2 on Debian 13, running under Xvfb on
the test VM. They are evidence that both shells run, not a baseline.

`images-light.png` is `fixtures/images.md` through `mdv-bench`: a real PNG drawn as a block and
inline, next to the placeholders for a missing file and an SVG. `shell-winui-images.png` and
`shell-qt-images.png` show the same file in both shells.

They are **not** a pixel test baseline yet:

- They use this host's system fonts (Segoe UI, Consolas), so a render on Linux or another Windows
  build will differ. The plan calls for bundled fonts and a small tolerance; that is still open.
- Math and diagrams are still source text until Phase 4.
- The renderer has no selection, anchors or viewport virtualization yet.

To regenerate:

```sh
cargo run --release -p mdv-bench -- --all
```

which writes the same four files under `target/goldens/`. `--input`, `--out`, `--width`, `--scale`,
`--theme` and `--max-height` render one page.
