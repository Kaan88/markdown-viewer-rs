# Shared document page for the look spikes

Status: **PASS** for producing the files. This is a stand-in page, not the future renderer, and it says nothing about Windows text quality.

## Files

Each image is one 780 x 2400 logical px page. Files are in this folder, named by theme and device scale.

| Scale | Exact pixels | PNG | Raw file | Raw bytes |
|---|---|---|---|---|
| 1.0 | 780 x 2400 | `content-<theme>@1.0.png` | `content-<theme>@1.0-780x2400.bgra` | 7,488,000 |
| 1.5 | 1170 x 3600 | `content-<theme>@1.5.png` | `content-<theme>@1.5-1170x3600.bgra` | 16,848,000 |
| 1.7 | 1326 x 4080 | `content-<theme>@1.7.png` | `content-<theme>@1.7-1326x4080.bgra` | 21,640,320 |
| 2.2 | 1716 x 5280 | `content-<theme>@2.2.png` | `content-<theme>@2.2-1716x5280.bgra` | 36,241,920 |

`<theme>` is `light` or `dark`, so there are 8 PNGs (3.5 MB total) and 8 raw files (157 MB total).

The raw format is 8-bit B, G, R, A, top-down, with stride = width x 4 and no header. Alpha is 255 on every pixel, so the premultiplied and straight forms are the same.

`.gitignore` excludes `*.bgra` and `target/` because of their size. Regenerate them with the command below.

## Displaying them

- **Show them 1:1.** Text uses RGB subpixel (LCD) masks, so any scaling or a fractional offset gives colour fringes. Use the file that matches the window's scale and put it on whole device pixels.
- **The background is opaque,** so Mica or acrylic won't show through the page. To blend the page with its surroundings, use the same background colour in the shell.

| Colour | Light | Dark |
|---|---|---|
| Page background | `#FBFBFB` | `#202020` |
| Body text | `#242424` | `#E4E4E4` |
| Accent | `#005FB8` | `#60CDFF` |
| Hairlines | `#E0E0E0` | `#3D3D3D` |
| Code and table header fill | `#F3F3F3` | `#2B2B2B` |

Content is 700 logical px wide, with 40 px side margins. It ends about 2309 to 2319 logical px down (it varies by scale because of rounding), and the rest of the page is blank background. See `generate.log`.

## Content

The page comes from `page.md`, which is `fixtures/showcase.md` with these changes:

- **Added prose:** one wrapped paragraph, a plain bullet list with its lead-in sentence, and a plain blockquote. The fixture has no text that wraps at this width. All of it is my own filler.
- **Removed to fit the height:** the Class, Entity relationship, Gantt, Pie and Mindmap sections.
- **Removed because they aren't drawn:** the `<p align="center">` HTML block with its badge image, and the `<details>` and `<summary>` tags. The text inside `<details>` stays as a normal paragraph.

Not rendered, and shown differently from the real viewer:

- Mermaid and `math` blocks appear as source in code blocks.
- Inline and display math appear as TeX source.
- The missing image appears as `[image: Missing image]`.
- Rust colouring is a small keyword and string matcher, not a real highlighter.

## How it is drawn

`src/main.rs` parses `page.md` with pulldown-cmark, then does one top-down layout pass. Glyphs go through `Renderer::draw_layout` in `spikes/text`, which is reused through a path dependency and was not modified. Boxes, checkboxes, inline-code chips, underlines and strikethroughs are drawn by this crate, because the text renderer doesn't draw them.

Fonts actually used (measured, `generate.log`): Segoe UI (Regular, Bold, Italic, Semibold), Consolas, and Yu Gothic UI for the Japanese. These are Windows font files copied to this Linux machine.

## Limits

- Rendered on Linux. There is no hinting, and the LCD order is assumed to be RGB. Windows 11 DirectWrite or ClearType text will look different, so these images can't be used to judge text quality on Windows.
- The renderer's gamma and contrast (1.4, 0.5) were chosen in the text spike. They are not tuned to a specific monitor.
- Line positions are rounded per scale, so block positions differ slightly in logical px between scales. Line wrapping wasn't compared across scales.

## Reproduce

Run from this folder. The command writes the 16 image files here:

```sh
/usr/bin/cargo build --release -j 6
./target/release/look-shared > generate.log
```

## Checks

All measured, 1 generation run plus 1 repeat:

- `identify` reports the dimensions above for all 8 PNGs.
- All 8 raw files have the expected byte count, and every alpha byte is 255.
- Each raw file's RGB hash matches its PNG's RGB hash, using `magick` and `sha256sum`.
- Repeating the generation gave byte-identical PNGs (8 of 8).
- I viewed full-resolution crops with the Read tool: `light@1.0` from top to bottom, `dark@1.7` from the top, and `dark@2.2` from the middle. Headings, body text, the table, the list, blockquotes, code and the footnote all appear as intended. I didn't view the two `@1.5` files, `light@1.7`, `light@2.2` or `dark@1.0`; those were only checked by the dimension and hash checks above.

One full generation of all 8 pages took 0.69 s on Linux, a single run during other builds (noisy, load average about 2 to 4). Treat it as an indication only: it was not a target and it isn't a renderer benchmark.
