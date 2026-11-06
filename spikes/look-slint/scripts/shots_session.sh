#!/bin/bash
# Runs INSIDE a nested KWin. Takes compositor captures for the look comparison.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCALE=$1; OUT=$2
BIN=$ROOT/target/release/look-slint
TOOLS=$ROOT/../qt-shell/tools/target/release
SHOTS=${SHOTS:-$ROOT/screenshots}
shot() { # NAME ARGS...
  local name=$1; shift
  timeout 8 "$BIN" --stay-ms=4000 "$@" > "$OUT/shot-$name.log" 2>&1 &
  local pid=$!
  for _ in $(seq 1 100); do grep -q PRESENTED "$OUT/shot-$name.log" && break; sleep 0.05; done
  sleep 0.5
  "$TOOLS/kwin_capture" "$OUT/full.png" > /dev/null 2>&1
  magick "$OUT/full.png" -trim +repage "$SHOTS/$name-$SCALE.png"
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; sleep 0.2
}
for t in light dark; do
  shot "$t" --theme=$t --find
  shot "$t-menu" --theme=$t --menu=2
done
shot "light-scrolled" --theme=light --scroll-px=900
shot "dark-about" --theme=dark --about
shot "dark-context" --theme=dark --context
shot "dark-small-window-menu" --theme=dark --menu=2 --win=640x360
