#!/bin/bash
# Runs inside a nested KWin. Args: SCALE THEME OUTDIR.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
S=$1; T=$2; OUT=$3
BIN=$ROOT/target/release/look-qt
CAP=$ROOT/../qt-shell/tools/target/release/kwin_capture
export QT_QPA_PLATFORM=wayland
TARG="--theme=$T"; [ "$T" = system ] && TARG=""
mapfile -t CASES < "${CASES_FILE:-$ROOT/scripts/cases.txt}"

kwin_script() {
  local js; js=$(mktemp --suffix=.js)
  cat > "$js" <<J
var all = workspace.windowList();
for (var i = 0; i < all.length; i++) {
  var w = all[i];
  if (w.caption.indexOf("mdview") >= 0) { $1 }
}
J
  local id
  id=$(qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$js" "look$RANDOM")
  qdbus6 org.kde.KWin "/Scripting/Script$id" org.kde.kwin.Script.run >/dev/null 2>&1
  sleep 0.4
}

for line in "${CASES[@]}"; do
  [ -z "$line" ] && continue
  name=${line%% *}; args=${line#"$name"}; [ "$args" = "$line" ] && args=""
  id="$T@$S-$name"; log="$OUT/.$id.log"
  # shellcheck disable=SC2086
  timeout 14 "$BIN" $TARG --stay-ms=6000 $args > "$log" 2>&1 &
  pid=$!
  for _ in $(seq 1 80); do grep -q PRESENTED "$log" && break; sleep 0.05; done
  sleep 0.3
  kwin_script "var g = w.frameGeometry; w.frameGeometry = {x: 40, y: 40, width: g.width, height: g.height};"
  sleep 1.5
  "$CAP" "$OUT/.$id.full.png" >> "$log" 2>&1
  python3 - "$S" "$OUT/.$id.full.png" "$OUT/$id.png" "$log" <<'P'
import sys,subprocess
s=float(sys.argv[1]); full,out=sys.argv[2],sys.argv[3]
# Window at logical (40,40). Keep a margin; the title bar height isn't known here.
x,y=int(24*s),int(24*s); w,h=int(1252*s),int((820+36+32)*s)
subprocess.run(["magick",full,"-crop",f"{w}x{h}+{x}+{y}","+repage",out],check=True)
P
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  sleep 0.2
done
