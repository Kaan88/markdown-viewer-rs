#!/usr/bin/python3
"""Is the shared page on screen byte for byte? Args: FULL_CAPTURE.png RAW.bgra W H SCROLL CANVAS_W CANVAS_H
Finds the page in a full compositor capture by exact match of a patch taken from the
page, then compares every pixel in the matched viewport, except the given exclusions."""
import subprocess, sys
import numpy as np

cap, raw, W, H, scroll = sys.argv[1], sys.argv[2], *map(int, sys.argv[3:6])
page = np.fromfile(raw, dtype=np.uint8).reshape(H, W, 4)[:, :, [2, 1, 0]]
info = subprocess.run(["magick", "identify", "-format", "%w %h", cap], capture_output=True, text=True).stdout.split()
cw, ch = int(info[0]), int(info[1])
rgb = subprocess.run(["magick", cap, "-depth", "8", "rgb:-"], capture_output=True).stdout
img = np.frombuffer(rgb, dtype=np.uint8).reshape(ch, cw, 3)

# A patch with structure: rows from the first table and body text.
scale = W / 780
px, py = int(60 * scale), scroll + int(140 * scale)
pw, ph = int(300 * scale), int(60 * scale)
patch = page[py:py + ph, px:px + pw]
first = patch[0]
found = None
for y in range(ch - ph):
    row = img[y]
    hits = np.where((row[: cw - pw, 0] == first[0, 0]) & (row[: cw - pw, 1] == first[0, 1]) & (row[: cw - pw, 2] == first[0, 2]))[0]
    for x in hits:
        if np.array_equal(img[y:y + ph, x:x + pw], patch):
            found = (x, y)
            break
    if found:
        break
if not found:
    print(f"NOT-FOUND patch {pw}x{ph} at page ({px},{py})")
    sys.exit(1)
x0, y0 = found[0] - px, found[1] - py  # capture position of page (0,0)
print(f"page origin in capture: x={x0} y={y0}")
# The canvas sits at a known offset from the page: see the blit in src/shell.rs.
vw, vh = int(sys.argv[6]), int(sys.argv[7])  # canvas size in device px
cx, cy = x0 - (vw - W) // 2, y0 + scroll
sb = int(12 * scale + 0.999)
left, right = max(cx, x0), min(cx + vw - sb, x0 + W)
top, bottom = cy, cy + vh
a = img[top:bottom, left:right]
b = page[top - y0:bottom - y0, left - x0:right - x0]
diff = np.any(a != b, axis=2)
print(f"compared {diff.size} pixels, region x {left}-{right} y {top}-{bottom}: differing={int(diff.sum())}")
if diff.any():
    ys, xs = np.where(diff)
    print(f"first diffs at capture ({xs[0] + left},{ys[0] + top}), bbox x {xs.min() + left}-{xs.max() + left} y {ys.min() + top}-{ys.max() + top}")
