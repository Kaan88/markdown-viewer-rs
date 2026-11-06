#!/usr/bin/python3
"""Side by side PNGs at 1:1 device pixels, Qt left, Slint right, no resampling, labelled on a band above."""
import os, sys, subprocess
from PIL import Image, ImageDraw, ImageFont
HERE = os.path.dirname(os.path.abspath(__file__)); SH = f"{HERE}/../results/shots"; OUT = f"{HERE}/../pairs"
os.makedirs(OUT, exist_ok=True)
def pair(T, S, case, name=None):
    a, b = [Image.open(f"{SH}/{app}-{T}@{S}-{case}.png").convert("RGB") for app in ("qt", "slint")]
    sc = float(S); band = int(30 * sc); gap = int(16 * sc)
    bg = (250, 250, 250) if T == "light" else (20, 20, 20); fg = (20, 20, 20) if T == "light" else (235, 235, 235)
    w = a.width + gap + b.width; h = band + max(a.height, b.height)
    im = Image.new("RGB", (w, h), bg); im.paste(a, (0, band)); im.paste(b, (a.width + gap, band))
    d = ImageDraw.Draw(im)
    f = ImageFont.truetype("/usr/local/share/fonts/WindowsFonts/segoeuib.ttf", int(16 * sc))
    d.text((int(8 * sc), int(4 * sc)), f"Qt Widgets + Breeze   {T}@{S}   {case}", fill=fg, font=f)
    d.text((a.width + gap + int(8 * sc), int(4 * sc)), f"Slint software   {T}@{S}   {case}", fill=fg, font=f)
    p = f"{OUT}/{name or f'{T}@{S}-{case}'}.png"; im.save(p); print(p, im.size)
for T in ("light", "dark"):
    for S in ("1.0", "1.5"):
        for c in ("find", "plain", "menu", "menu2"):
            if os.path.exists(f"{SH}/qt-{T}@{S}-{c}.png") and os.path.exists(f"{SH}/slint-{T}@{S}-{c}.png"):
                pair(T, S, c)
