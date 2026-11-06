#!/usr/bin/python3
"""Independent pixel check. For each capture, finds the shared page by exact row match and compares
every byte of the canvas area, leaving out a right strip (each app's scrollbar). Includes controls."""
import glob, os, re, sys
import numpy as np
from PIL import Image
HERE = os.path.dirname(os.path.abspath(__file__)); SH = f"{HERE}/../../look-shared"; SHOTS = f"{HERE}/../results/shots"
DIM = {"1.0": (780, 2400), "1.5": (1170, 3600), "2.2": (1716, 5280)}
def page(T, S):
    w, h = DIM[S]
    return np.fromfile(f"{SH}/content-{T}@{S}-{w}x{h}.bgra", dtype=np.uint8).reshape(h, w, 4)[:, :, [2, 1, 0]]
def check(cap, want):
    h, w, _ = want.shape
    # Pick the row with the most horizontal detail: a uniform rule would match anywhere.
    x0 = w // 4
    mid = want[:, x0:x0 + w // 2]
    ry = int(np.argmax((mid[:, 1:] != mid[:, :-1]).any(axis=2).sum(axis=1)))
    slice_ = mid[ry].tobytes()
    best = None
    for yy in range(cap.shape[0]):
        i = cap[yy].tobytes().find(slice_)
        while i >= 0 and i % 3: i = cap[yy].tobytes().find(slice_, i + 1)
        if i < 0: continue
        x, y = i // 3 - x0, yy - ry
        if y < 0 or x < 0 or y + h > cap.shape[0] or x + w > cap.shape[1]: continue
        n = int((cap[y:y + h, x:x + w] != want).any(axis=2).sum())
        if best is None or n < best[0]: best = (n, x, y)
    return best
out = open(f"{HERE}/../results/pixel.txt", "w")
def say(s): print(s); out.write(s + "\n")
tot = {}
for f in sorted(glob.glob(f"{SHOTS}/.*-*@*-*.full.png")):
    m = re.match(r"\.(qt|slint)-(light|dark)@([\d.]+)-(plain|scrolled)(-odd)?\.full\.png", os.path.basename(f))
    if not m: continue
    app, T, S, case, odd = m.groups(); sc = float(S)
    log = f"{SHOTS}/.{app}-{T}@{S}-{case}.log"
    line = next(l for l in open(log, errors="replace") if l.startswith("PRESENTED"))
    if app == "qt":
        cw, ch = map(int, re.search(r"canvas_logical=(\d+)x(\d+)", line).groups()); vw, vh = round(cw * sc), round(ch * sc)
    else:
        vw, vh = map(int, re.search(r"canvas_device_size=(\d+)x(\d+)", line).groups())
    pg = page(T, S); pw, ph = pg.shape[1], pg.shape[0]
    scroll = 900 if case == "scrolled" else 0
    # Common comparison: skip 4 px each side (Qt's canvas is narrower than the page) and the 14 logical px scrollbar strip.
    left = 4; right = int(14 * sc) + 4
    h = min(ph - scroll, vh); want = pg[scroll:scroll + h, left:pw - right]
    # The top rows of the canvas can be covered by nothing, so compare all h rows.
    cap = np.array(Image.open(f).convert("RGB"))
    b = check(cap, want)
    r = "FAIL not-found" if b is None else f"{'PASS' if b[0]==0 else 'FAIL'} differing={b[0]} of {want.shape[0]*want.shape[1]} at={b[1]},{b[2]}"
    say(f"{app} {T}@{S} {case}{odd or ''}: canvas={vw}x{vh} {r}")
    tot.setdefault(app, [0, 0]); tot[app][1] += 1; tot[app][0] += (b is not None and b[0] == 0)
    # Control: same capture against the page shifted by one row must differ.
    if not odd and case == "plain" and b is not None:
        w2 = pg[scroll + 1:scroll + 1 + h - 1, left:pw - right]
        n = int((cap[b[2]:b[2] + h - 1, b[1]:b[1] + want.shape[1]] != w2).any(axis=2).sum())
        say(f"  control, page shifted 1 row at same place: differing={n} (must be large)")
say(f"TOTAL {tot}")
