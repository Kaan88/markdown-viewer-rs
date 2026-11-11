# Shell mockups (2026-11-11)

Layout A compiled in three lighter toolkits, to replace Qt. Throwaway code; reasons, numbers and
the decision are in `docs/PLAN.md` ("Next: a lighter shell than Qt") and `docs/HANDOFF.md`
section 28.

| Screenshot | Build |
|---|---|
| `fltk.png` | `spikes/shell-fltk`, fltk-rs 1.5.23, GDI text |
| `soft.png` | `spikes/shell-soft`, first build, fontdue text (superseded) |
| `soft-2.png` | `spikes/shell-soft`, text through `mdview_render::TextEngine` |
| `slint.png` | `spikes/shell-slint`, software renderer (rejected: blurry text) |
| `slint-2.png` | `spikes/shell-slint`, Skia on the GPU (rejected: fringed text) |
| `slint-skia-cpu.png` | the same with `SLINT_BACKEND=winit-skia-software` |

The page in every screenshot is the stand-in image from `spikes/look-shared`.

Run each from its own folder, because the page image is loaded relative to the working directory:

```bash
cd spikes/shell-fltk && ./target/release/shell-fltk.exe
cd spikes/shell-soft && ./target/release/shell-soft.exe
cd spikes/shell-slint && ./target/release/shell-slint.exe
```

Building fltk needs the MSVC environment (`vcvars64.bat`) and CMake.
