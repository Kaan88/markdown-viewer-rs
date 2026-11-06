#!/bin/bash
# Runs INSIDE a nested KWin. Per-frame scrolling cost and launch/memory at this scale.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCALE=$1; OUT=$2
BIN=$ROOT/target/release/look-slint
TOOLS=$ROOT/../qt-shell/tools/target/release
F="$OUT/bench-$SCALE.txt"; : > "$F"
for a in "--doc-mode=viewport --pixel=rgb" "--doc-mode=viewport --pixel=rgba" "--doc-mode=full --pixel=rgb" "--doc-mode=full --pixel=rgba"; do
  echo "## $a" >> "$F"
  timeout 30 "$BIN" --bench-scroll=300 $a >> "$F" 2>&1
done
echo "## big window, viewport rgb" >> "$F"
timeout 30 "$BIN" --bench-scroll=300 --win=1700x1000 >> "$F" 2>&1
echo "## launch and memory" >> "$F"
"$TOOLS/harness" --runs 12 --drop 2 --smaps-after-ms 800 -- "$BIN" --measure >> "$F" 2>&1
# Software renderer's texture offset is Fixed<u16,4>, so check scroll beyond 4095 px.
echo "## full mode past 4095 device px" >> "$F"
timeout 8 "$BIN" --stay-ms=3000 --doc-mode=full --hide-overlays --theme=light --win=800x400 --scroll-px=99999 > "$OUT/overflow-$SCALE.log" 2>&1 &
P=$!; sleep 1.2; "$TOOLS/kwin_capture" "$OUT/overflow-full-$SCALE.png" >/dev/null 2>&1; kill $P; wait $P 2>/dev/null
timeout 8 "$BIN" --stay-ms=3000 --hide-overlays --theme=light --win=800x400 --scroll-px=99999 > "$OUT/overflow-vp-$SCALE.log" 2>&1 &
P=$!; sleep 1.2; "$TOOLS/kwin_capture" "$OUT/overflow-vp-$SCALE.png" >/dev/null 2>&1; kill $P; wait $P 2>/dev/null
grep -h PRESENTED "$OUT/overflow-$SCALE.log" | cut -c1-200 >> "$F"
