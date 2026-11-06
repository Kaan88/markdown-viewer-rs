#!/usr/bin/env python3
"""Checks a window screenshot for a sharp 1 px checkerboard. Usage: check_board.py shot.png
Finds the panel by its pixel pattern, then counts pixels that are pure black or white and whose
4-neighbours are the opposite colour. Prints raw counts; anything grey means scaling blurred it."""
import sys
from PIL import Image

im = Image.open(sys.argv[1]).convert("RGB")
w, h = im.size
px = im.load()
BLACK, WHITE = (0, 0, 0), (255, 255, 255)

# Rows and columns where the pattern alternates perfectly over a long run bound the panel.
def is_board(x, y):
    a, b = px[x, y], px[x + 1, y]
    return {a, b} == {BLACK, WHITE}

rows = [y for y in range(h - 1) if sum(is_board(x, y) for x in range(w // 2, w // 2 + 200)) == 200]
if not rows:
    print(f"{sys.argv[1]} size={w}x{h}: no checkerboard rows found")
    sys.exit(1)
top, bottom = rows[0], rows[-1]
y0 = (top + bottom) // 2
left = w // 2
while left > 0 and is_board(left - 1, y0):
    left -= 1
right = w // 2
while right < w - 2 and is_board(right + 1, y0):
    right += 1

total = good = bad_colour = bad_phase = 0
phase = None
for y in range(top, bottom + 1):
    for x in range(left, right + 1):
        total += 1
        p = px[x, y]
        if p not in (BLACK, WHITE):
            bad_colour += 1
            continue
        odd = (x + y) & 1
        expected = (p == BLACK) ^ bool(odd)
        if phase is None:
            phase = expected
        if expected == phase:
            good += 1
        else:
            bad_phase += 1
print(
    f"{sys.argv[1]} size={w}x{h} board x={left}..{right} y={top}..{bottom} "
    f"pixels={total} exact={good} wrong_colour={bad_colour} wrong_phase={bad_phase} "
    f"exact_pct={100 * good / total:.3f}"
)
