# Golden images

The Phase 2 renderer images, from `fixtures/showcase.md` rendered with `mdv-bench --all` on Windows
at 100% and 150%, light and dark, plus screenshots of the first WinUI shell showing the same file on
this PC. They are here for the user's review of the look and of the shell, and as the starting point
for the real golden suite.

`shell-winui-light.png` and `shell-winui-dark.png` are screen captures of `mdview-winui.exe` at
1100x900 on Windows 11, DPI scale 100%, taken with the `--light` and `--dark` flags. They are
evidence that the shell runs on Windows, not a baseline.

They are **not** a pixel test baseline yet:

- They use this host's system fonts (Segoe UI, Consolas), so a render on Linux or another Windows
  build will differ. The plan calls for bundled fonts and a small tolerance; that is still open.
- Images, math and diagrams are placeholders or source text until Phases 2 and 4 finish.
- The renderer has no selection, anchors or viewport virtualization yet.

To regenerate:

```sh
cargo run --release -p mdv-bench -- --all
```

which writes the same four files under `target/goldens/`. `--input`, `--out`, `--width`, `--scale`,
`--theme` and `--max-height` render one page.
