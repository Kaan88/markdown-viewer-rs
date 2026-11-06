#!/bin/bash
# Runs INSIDE a nested KWin. Real seat events (KWin fake-input), not Slint dispatch_event.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCALE=$1; OUT=$2
BIN=$ROOT/target/release/look-slint
TOOLS=$ROOT/../qt-shell/tools/target/release
FI="python3 -I $ROOT/scripts/fakeinput.py"
LOG="$OUT/input-$SCALE.log"; : > "$LOG"
timeout 40 "$BIN" --stay-ms=30000 --log-state --theme=dark > "$LOG" 2>&1 &
PID=$!
for _ in $(seq 1 100); do grep -q PRESENTED "$LOG" && break; sleep 0.05; done
sleep 0.5
# Put the window at a known place, and ask KWin where its client area is.
js="$OUT/geom.js"
cat > "$js" <<J
var all = workspace.windowList();
for (var i = 0; i < all.length; i++) {
  var w = all[i];
  if (w.caption.indexOf("mdview look spike") >= 0) {
    var g = w.frameGeometry; w.frameGeometry = {x: 100, y: 100, width: g.width, height: g.height};
    print("MDVGEOM " + w.clientGeometry.x + " " + w.clientGeometry.y);
  }
}
J
id=$(qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$js" geom)
qdbus6 org.kde.KWin "/Scripting/Script$id" org.kde.kwin.Script.run >/dev/null
sleep 0.6
# Client origin in compositor logical px: frame at 100,100 plus borders measured below.
X=${CX:-101}; Y=${CY:-130}
mark() { echo "--- $1" >> "$LOG"; sleep 0.25; }
cap() { "$TOOLS/kwin_capture" "$OUT/input-$SCALE-$1.png" >/dev/null 2>&1; }
p() { python3 -c "print($X+$1, $Y+$2)" | tr ' ' ':'; }

mark "ctrl+f"; $FI key:29:down tap:33 key:29:up; sleep 0.3; cap find
mark "type ab"; $FI tap:30 tap:48; sleep 0.3
mark "select all + type c"; $FI key:29:down tap:30 key:29:up tap:46; sleep 0.3; cap typed
mark "click the find field, then esc"; $FI move:$(p 900 60) click:left; sleep 0.2; $FI tap:1; sleep 0.3
mark "click hamburger"; $FI move:$(p 20 20) click:left; sleep 0.3; cap menu
mark "hover View"; $FI move:$(p 80 124); sleep 0.3; cap flyout
mark "click Files sidebar"; $FI move:$(p 300 124) click:left; sleep 0.3; cap noleft
mark "toggle sidebar back with button"; $FI move:$(p 1166 20) click:left; sleep 0.3
mark "open menu, click outside"; $FI move:$(p 20 20) click:left; sleep 0.2; $FI move:$(p 900 500) click:left; sleep 0.3
mark "wheel"; $FI move:$(p 600 400) axis:30 sleep:400 axis:30; sleep 0.6; cap wheel
mark "page down key"; $FI tap:109; sleep 0.5
mark "home key"; $FI tap:102; sleep 0.8
mark "tab 2 click"; $FI move:$(p 208 24) click:left; sleep 0.3; cap tab2
mark "splitter drag"; $FI move:$(p 230 400) btn:left:down move:$(p 262 400) btn:left:up; sleep 0.3; cap split
mark "tree collapse click"; $FI move:$(p 100 146) click:left; sleep 0.3; cap tree
mark "scrollbar drag"; $FI move:$(p 262 500); $FI move:$(p 1000 100) ; cap sbhover
mark "right click doc"; $FI move:$(p 700 300) click:right; sleep 0.3; cap context
mark "context down x3, enter"; $FI tap:108 tap:108 tap:108 tap:28; sleep 0.3; cap contextfind
mark "esc"; $FI tap:1; sleep 0.2
mark "F1 about"; $FI tap:59; sleep 0.3; cap about
mark "esc"; $FI tap:1; sleep 0.2
mark "ctrl+w"; $FI key:29:down tap:17 key:29:up; sleep 0.3
mark "end"; kill $PID; wait $PID 2>/dev/null
