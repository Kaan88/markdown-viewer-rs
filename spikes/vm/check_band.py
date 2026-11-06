#!/usr/bin/env python3
"""Prints where the blue wheel band sits in each screenshot. Usage: check_band.py shot.png..."""
import sys
from PIL import Image
BAND = {(0, 0, 0x40), (0x60, 0x90, 0xFF)}
for f in sys.argv[1:]:
    im = Image.open(f).convert("RGB")
    w, h = im.size
    x = w // 2
    ys = [y for y in range(h) if im.getpixel((x, y)) in BAND]
    print(f, f"size={w}x{h}", f"band y={ys[0]}..{ys[-1]} rows={len(ys)}" if ys else "no band")
