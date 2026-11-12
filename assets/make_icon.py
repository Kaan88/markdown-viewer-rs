"""Writes the app mark as an SVG and a multi-size .ico from one description of its shapes.

    python assets/make_icon.py            # the viewer: mdview.svg, mdview.ico
    python assets/make_icon.py mdedit     # the editor's mark, into the current folder

Needs Pillow. The mark is drawn on a 256 x 256 canvas: a rounded tile from 8 to 248, and white
shapes centred on it. The viewer's is the Markdown "M" and a down arrow; the editor's is the same
"M" and a pencil.
"""

import math
import sys
from pathlib import Path

from PIL import Image, ImageDraw

SIZES = [16, 24, 32, 48, 64, 128, 256]
SCALE = 0.96  # the shapes below are drawn a little large and scaled about the tile's centre

M = [(44, 176), (44, 80), (72, 80), (100, 116), (128, 80), (156, 80), (156, 176),
     (128, 176), (128, 122), (100, 158), (72, 122), (72, 176)]
ARROW = [(178, 128), (154, 128), (192, 172), (230, 128), (206, 128), (206, 80), (178, 80)]


def pencil(cx, cy, length, width, degrees):
    """A pencil with its point down, turned clockwise by `degrees`: eraser, body, point."""
    h, w = length / 2, width / 2
    parts = [
        [(-w, -h), (w, -h), (w, -h + 12), (-w, -h + 12)],
        [(-w, -h + 17), (w, -h + 17), (w, h - 26), (0, h), (-w, h - 26)],
    ]
    a = math.radians(degrees)
    turn = lambda x, y: (cx + x * math.cos(a) - y * math.sin(a), cy + x * math.sin(a) + y * math.cos(a))
    return [[turn(x, y) for x, y in part] for part in parts]


MARKS = {
    "mdview": {"tile": "#2b5fab", "shapes": [M, ARROW]},
    "mdedit": {"tile": "#1d7d6b", "shapes": [M, *pencil(197, 128, 92, 28, 28)]},
}


def placed(shapes):
    """The shapes scaled and moved so their bounding box is centred on the tile."""
    xs = [x for shape in shapes for x, _ in shape]
    ys = [y for shape in shapes for _, y in shape]
    dx = 128 - (min(xs) + max(xs)) / 2 * SCALE
    dy = 128 - (min(ys) + max(ys)) / 2 * SCALE
    return [[(x * SCALE + dx, y * SCALE + dy) for x, y in shape] for shape in shapes]


def svg(name, mark, shapes):
    paths = "".join(
        "M" + " ".join(f"{x:.2f} {y:.2f}" for x, y in shape) + "z" for shape in shapes
    )
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">\n'
        f"  <!-- The {name} mark. Written by make_icon.py; change the shapes there. -->\n"
        f'  <rect x="8" y="8" width="240" height="240" rx="52" fill="{mark["tile"]}"/>\n'
        '  <rect x="8" y="8" width="240" height="120" rx="52" fill="#fff" opacity="0.08"/>\n'
        f'  <path d="{paths}" fill="#fff"/>\n'
        "</svg>\n"
    )


def raster(mark, shapes, size):
    k = 8 if size <= 64 else 4  # supersampling
    n = size * k
    f = n / 256
    tile = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ImageDraw.Draw(tile).rounded_rectangle((8 * f, 8 * f, 248 * f, 248 * f), 52 * f, fill=mark["tile"])
    sheen = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    ImageDraw.Draw(sheen).rounded_rectangle((8 * f, 8 * f, 248 * f, 128 * f), 52 * f, fill=(255, 255, 255, 20))
    tile.alpha_composite(sheen)
    draw = ImageDraw.Draw(tile)
    for shape in shapes:
        draw.polygon([(x * f, y * f) for x, y in shape], fill="#ffffff")
    return tile.resize((size, size), Image.LANCZOS)


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "mdview"
    mark = MARKS[name]
    out = Path(__file__).parent if name == "mdview" else Path.cwd()
    shapes = placed(mark["shapes"])
    (out / f"{name}.svg").write_text(svg(name, mark, shapes), encoding="utf-8", newline="\n")
    images = [raster(mark, shapes, size) for size in SIZES]
    images[-1].save(out / f"{name}.ico", format="ICO", append_images=images[:-1],
                    sizes=[(s, s) for s in SIZES])
    images[-1].save(out / f"{name}.png")
    print(f"wrote {name}.svg, {name}.ico and {name}.png to {out}")


if __name__ == "__main__":
    main()
