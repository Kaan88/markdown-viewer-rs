#!/bin/bash
# Runs INSIDE a nested KWin. Args: SCALE OUTDIR.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCALE=$1; OUT=$2
BIN=$ROOT/target/release/look-slint
TOOLS=$ROOT/../qt-shell/tools/target/release
qdbus6 org.kde.KWin /KWin supportInformation > "$OUT/kwin-support-$SCALE.txt" 2>&1
wayland-info > "$OUT/wayland-info-$SCALE.txt" 2>&1

kwin_script() { # NAME JS: runs JS in KWin against the spike window as `w`
  local js="$OUT/$1.js"
  cat > "$js" <<J
var all = workspace.windowList();
for (var i = 0; i < all.length; i++) {
  var w = all[i];
  if (w.caption.indexOf("mdview look spike") >= 0) { $2 }
}
J
  local id
  id=$(qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$js" "$1")
  qdbus6 org.kde.KWin "/Scripting/Script$id" org.kde.kwin.Script.run >/dev/null 2>>"$OUT/script-errors.txt"
  sleep 0.6
  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript "$1" >/dev/null
}
move_window() { kwin_script "mdvmove$1x$2" "var g = w.frameGeometry; w.frameGeometry = {x: $1, y: $2, width: g.width, height: g.height};"; }
resize_window() { kwin_script "mdvsize$1x$2" "var g = w.frameGeometry; w.frameGeometry = {x: g.x, y: g.y, width: g.width + $1, height: g.height + $2};"; }

start_app() { # ID ARGS...
  local id=$1; shift
  LOG="$OUT/$id.log"
  timeout 14 "$BIN" --stay-ms=8000 "$@" > "$LOG" 2>&1 &
  PID=$!
  for _ in $(seq 1 100); do grep -q PRESENTED "$LOG" && break; sleep 0.05; done
  sleep 0.5
}
stop_app() { kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; sleep 0.2; }

checker_case() { # NAME MOVE_X MOVE_Y ARGS...
  local name=$1 mx=$2 my=$3; shift 3
  local id="s$SCALE-checker-$name"
  start_app "$id" --checker "$@"
  local dw=0 dh=0
  if [ "$mx" = "resize" ]; then resize_window 7 5; dw=7; dh=5
  elif [ "$mx" != "-" ]; then move_window "$mx" "$my"; fi
  "$TOOLS/kwin_capture" "$OUT/$id.png" >> "$LOG" 2>&1
  local line size w h
  line=$(grep PRESENTED "$LOG")
  size=$(echo "$line" | grep -o 'canvas_device_size=[^ ]*' | cut -d= -f2)
  w=$(python3 -c "print(round(${size%x*} + $dw * $SCALE))"); h=$(python3 -c "print(round(${size#*x} + $dh * $SCALE))")
  echo "$id $(echo "$line" | grep -oE '(dpr|canvas_device_origin|window_phys|window_logical)=[^ ]*' | tr '\n' ' ') :: $("$TOOLS/pixelcheck" "$OUT/$id.png" "$w" "$h")" >> "$OUT/results.txt"
  stop_app
}

page_case() { # NAME THEME SCROLL ARGS...
  local name=$1 theme=$2 scroll=$3; shift 3
  local id="s$SCALE-page-$name"
  start_app "$id" --hide-overlays --theme="$theme" --scroll-px="$scroll" "$@"
  "$TOOLS/kwin_capture" "$OUT/$id.png" >> "$LOG" 2>&1
  echo "$id :: $(python3 -I "$ROOT/scripts/compare_page.py" "$OUT/$id.png" "$ROOT/../look-shared" "$theme" "$SCALE" "$scroll" "$LOG")" >> "$OUT/results.txt"
  stop_app
}

checker_case base - -
checker_case panes231x207 - - --left-width=231 --right-width=207
checker_case panes233x211 - - --left-width=233 --right-width=211
checker_case oddsize - - --win=1221x821
checker_case move101x57 101 57
checker_case move333x211 333 211 --left-width=235 --right-width=205 --win=1223x817
checker_case resized resize 0
checker_case rgba - - --pixel=rgba
checker_case nosnap - - --no-snap --left-width=231 --right-width=207
checker_case smooth - - --smooth --left-width=231 --right-width=207

page_case light-top-viewport light 0
page_case dark-mid-viewport dark 733
page_case dark-bottom-viewport dark 99999
page_case light-mid-odd-panes light 1234 --left-width=233 --right-width=211
page_case light-mid-wide light 500 --win=1400x900
page_case light-top-full light 0 --doc-mode=full
page_case light-mid-full light 733 --doc-mode=full
page_case light-deep-full light 3000 --doc-mode=full
page_case light-bottom-full light 99999 --doc-mode=full

# Negative controls: these must fail, or the check proves nothing.
checker_case control-naive - - --naive
checker_case control-stretch1 - - --stretch1
checker_case control-stretch1-smooth - - --stretch1 --smooth
# Not a control: the toolkit scale is forced to 2, so the window is drawn larger (no resample).
SLINT_SCALE_FACTOR=2 checker_case info-forced-scale2 - -
