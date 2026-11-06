# compare_page.py CAPTURE.png SHARED_DIR THEME SCALE SCROLL APP.LOG
# Finds the document region in a compositor capture and compares it byte for byte with the
# shared raw file: every RGB byte of every visible pixel.
import re, sys
import numpy as np
from PIL import Image

cap_path, shared, theme, scale, scroll, log = sys.argv[1:7]
scroll = int(scroll)
dims = {"1.5": (1170, 3600), "1.7": (1326, 4080), "2.2": (1716, 5280), "1.0": (780, 2400)}
pw, ph = dims[scale]
raw = np.fromfile(f"{shared}/content-{theme}@{scale}-{pw}x{ph}.bgra", dtype=np.uint8).reshape(ph, pw, 4)
page = raw[:, :, [2, 1, 0]]
line = next(l for l in open(log) if l.startswith("PRESENTED"))
vw, vh = map(int, re.search(r"canvas_device_size=(\d+)x(\d+)", line).groups())
page_x = int(re.search(r"page_x=(\d+)", line).group(1))
scroll = min(scroll, max(ph - vh, 0))
w, h = min(pw, vw), min(ph - scroll, vh)
want = page[scroll:scroll + h, 0:w]
cap = np.array(Image.open(cap_path).convert("RGB"))
# Locate: use the most detailed row of the expected region, then its bytes in the capture.
trans = (want[1:] != want[:-1]).any(axis=2).sum(axis=1) + (want[:, 1:] != want[:, :-1]).any(axis=2).sum(axis=1)[:-1]
ry = int(np.argmax(trans))
tmpl = want[ry].tobytes()
# The row can repeat (table borders, identical headings), so try every match and keep the best.
best = None
for yy in range(cap.shape[0]):
    i = cap[yy].tobytes().find(tmpl)
    if i < 0 or i % 3:
        continue
    x, y = i // 3, yy - ry
    if y < 0 or y + h > cap.shape[0] or x + w > cap.shape[1]:
        continue
    n = int((cap[y:y + h, x:x + w] != want).any(axis=2).sum())
    if best is None or n < best[0]:
        best = (n, x, y)
if best is None:
    print(f"FAIL region_not_found scroll={scroll} expected={w}x{h}")
    sys.exit(0)
n, x, y = best
print(f"{'PASS' if n == 0 else 'FAIL'} at=({x},{y}) region={w}x{h} scroll={scroll} page_x={page_x} pixels={w*h} differing={n}")
