# G3 comparison images

G3 is the user's decision. Nothing here claims a pass or a fail.

In every PNG, the left side is Rust (parley 0.12 + swash 0.2.10, LCD masks, 1/4 px horizontal positioning, gamma 1.4 / contrast 0.5 LUT, no hinting). The right side is Qt 6.12 (QPainter in a real `QWidget::paintEvent`, captured with `QWidget::render` into a QImage).

View them at 1:1. An image viewer that scales the PNG will blur the differences you are judging.

## curated: start here

The Rust font stack is `Noto Sans, Noto Color Emoji, sans-serif`. This is closest to what the product would ship on Linux.

| Scale | Light | Dark |
|---|---|---|
| 100% | [curated_light_100.png](curated_light_100.png) | [curated_dark_100.png](curated_dark_100.png) |
| 150% | [curated_light_150.png](curated_light_150.png) | [curated_dark_150.png](curated_dark_150.png) |
| 170% | [curated_light_170.png](curated_light_170.png) | [curated_dark_170.png](curated_dark_170.png) |
| 220% | [curated_light_220.png](curated_light_220.png) | [curated_dark_220.png](curated_dark_220.png) |

## default: fontique's automatic fallback

The Rust font stack is `Noto Sans, sans-serif`. These show the emoji defects listed below.

| Scale | Light | Dark |
|---|---|---|
| 100% | [default_light_100.png](default_light_100.png) | [default_dark_100.png](default_dark_100.png) |
| 150% | [default_light_150.png](default_light_150.png) | [default_dark_150.png](default_dark_150.png) |
| 170% | [default_light_170.png](default_light_170.png) | [default_dark_170.png](default_dark_170.png) |
| 220% | [default_light_220.png](default_light_220.png) | [default_dark_220.png](default_dark_220.png) |

`raw/` holds each side as a separate PNG. `raw/fonts_default.txt` and `raw/fonts_curated.txt` list the fonts the Rust side picked for each line.

## Objective defects seen

I inspected zoomed crops at 100% and 220% in light, and the composites for dark 150% and 170%. I did not inspect every image at zoom.

**Rust, default fallback**

- Fontique picks Segoe UI Emoji (a copy in `/usr/local/share/fonts/WindowsFonts`) instead of Noto Color Emoji.
- 😀 loses its yellow fill and shows only dark outlines. That font has COLR version 1, so I suspect swash renders only part of it. This cause is inferred, not confirmed.
- 🇹🇷 appears as the letters "TR". Qt shows a flag.
- ❤️ appears as a black DejaVu Sans heart, ignoring the emoji variation selector. Qt shows a red heart.

**Rust, curated stack**

- All of the above are fixed: flag, red heart and yellow face are correct.
- Emoji are visibly larger and wider than Qt's. The emoji line ends about 23 px further right at 100% (estimated from a zoomed crop).

**Both sides**

- Japanese uses MS Gothic on the Rust side, and Qt looks the same. `fc-match :lang=ja` also returns MS Gothic on this machine, so the product should name a CJK font explicitly.
- Arabic is shaped, joined and ordered right to left on both sides, using DejaVu Sans on the Rust side. Qt looks the same, but I did not query which font Qt chose.
- 👨‍👩‍👧‍👦 is a single grey-boxed glyph on both sides with Noto Color Emoji, so it comes from the font or its sequence handling and is not specific to swash.

**No defect seen**

- Turkish İıĞğŞş: no missing glyphs or wrong dots on either side.
- Monospace code: no missing glyphs.
- The `fi fl ffi` text and the mixed Arabic and English line look the same on both sides.

## Differences that are not defects

- Rust has no hinting. Qt follows the fontconfig settings here (`hintstyle=1`, slight), at least at 100%.
- The LUT values are a first guess and have not been tuned against Qt.
