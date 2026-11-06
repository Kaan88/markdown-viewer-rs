#!/bin/bash
# Runs INSIDE a nested KWin. Args: SCALE OUTDIR. Runs every paint mode and offset case.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCALE=$1; OUT=$2
BIN=$ROOT/target/release/qt-shell
TOOLS=$ROOT/tools/target/release
export QT_QPA_PLATFORM=wayland
qdbus6 org.kde.KWin /KWin supportInformation > "$OUT/kwin-support-$SCALE.txt" 2>&1
wayland-info > "$OUT/wayland-info-$SCALE.txt" 2>&1

kwin_script() { # NAME JS: runs JS in KWin against the spike window as `w`
  local js="$OUT/$1.js"
  cat > "$js" <<J
var all = workspace.windowList();
for (var i = 0; i < all.length; i++) {
  var w = all[i];
  if (w.caption.indexOf("mdview spike") >= 0) { $2 }
}
J
  local id
  id=$(qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$js" "$1")
  qdbus6 org.kde.KWin "/Scripting/Script$id" org.kde.kwin.Script.run >/dev/null 2>>"$OUT/script-errors.txt"
  sleep 0.6
  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript "$1" >/dev/null
}
move_window() { # X Y
  kwin_script "mdvmove$1x$2" "var g = w.frameGeometry; w.frameGeometry = {x: $1, y: $2, width: g.width, height: g.height};"
}
resize_window() { # DW DH
  kwin_script "mdvsize$1x$2" "var g = w.frameGeometry; w.frameGeometry = {x: g.x, y: g.y, width: g.width + $1, height: g.height + $2};"
}

run_case() { # NAME MODE MOVE_X MOVE_Y SHELL_ARGS...
  local name=$1 mode=$2 mx=$3 my=$4; shift 4
  local id="s$SCALE-$mode-$name" log="$OUT/$id.log"
  timeout 12 "$BIN" --paint-mode "$mode" --stay-ms 6000 "$@" > "$log" 2>&1 &
  local pid=$!
  for _ in $(seq 1 60); do grep -q PRESENTED "$log" && break; sleep 0.05; done
  sleep 0.5
  if [ "$mx" = "resize" ]; then resize_window 7 5
  elif [ "$mx" != "-" ]; then move_window "$mx" "$my"; fi
  "$TOOLS/kwin_capture" "$OUT/$id.png" >> "$log" 2>&1
  local line size w h
  line=$(grep PRESENTED "$log")
  size=$(echo "$line" | grep -o 'canvas_device_size=[^ ]*' | cut -d= -f2)
  # The resize case grows the frame by 7x5 logical px after PRESENTED was printed; docks
  # are fixed width, so the canvas takes all of it.
  local dw=0 dh=0
  if [ "$mx" = "resize" ]; then dw=7; dh=5; fi
  w=$(python3 -c "print(round(float('${size%x*}') + $dw * $SCALE))"); h=$(python3 -c "print(round(float('${size#*x}') + $dh * $SCALE))")
  echo "$id $(echo "$line" | grep -oE '(dpr|canvas_in_window|canvas_device_origin|window_logical)=[^ ]*' | tr '\n' ' ') :: $("$TOOLS/pixelcheck" "$OUT/$id.png" "$w" "$h")" >> "$OUT/results.txt"
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  sleep 0.2
}

for mode in painter direct; do
  run_case resized     "$mode" resize 0
  run_case base        "$mode" - -
  run_case margin1     "$mode" - -         --left-margin 1 --top-margin 1
  run_case margin3x7   "$mode" - -         --left-margin 3 --top-margin 7 --dock-width 201
  run_case oddsize     "$mode" - -         --win 1221x821
  run_case move101x57  "$mode" 101 57
  run_case move333x211 "$mode" 333 211     --left-margin 5 --top-margin 2 --win 1223x817
done
# Negative controls: both must FAIL, or the check above proves nothing.
run_case control-naive painter - - --paint-mode naive
QT_SCALE_FACTOR_ROUNDING_POLICY=Ceil run_case control-qt-ceil-scale painter - -
