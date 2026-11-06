#!/bin/bash
# G2 equivalent: compositor-captured pixel exactness in a private virtual KWin per scale.
# Same method as spikes/qt-shell/scripts/run_g2.sh. Usage: scripts/run_g2.sh OUTDIR [SCALES...]
# INNER=other_session.sh runs a different script inside the nested KWin.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=$(realpath -m "$1"); shift
SCALES=${*:-1.5 1.7 2.2}
mkdir -p "$OUT"; [ -z "$INNER" ] && : > "$OUT/results.txt"
XDG="$OUT/xdg"
for S in $SCALES; do
  rm -rf "$XDG"; mkdir -p "$XDG/config" "$XDG/data" "$XDG/cache"
  cat > "$XDG/config/kwinoutputconfig.json" <<J
[{"name":"outputs","data":[{"connectorName":"Virtual-0","mode":{"basic":{"width":3840,"height":2400,"refreshRate":60000},"flags":1},"scale":$S,"transform":"Normal"}]},
 {"name":"setups","data":[{"lidClosed":false,"outputs":[{"enabled":true,"outputIndex":0,"position":{"x":0,"y":0},"priority":0,"replicationSource":""}]}]}]
J
  SOCK="mdvs-$S"
  env -u DISPLAY WAYLAND_DISPLAY="$SOCK" XDG_CONFIG_HOME="$XDG/config" XDG_DATA_HOME="$XDG/data" XDG_CACHE_HOME="$XDG/cache" \
    KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 \
    timeout ${KWIN_TIMEOUT:-300} dbus-run-session -- kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --no-kactivities \
      --socket "$SOCK" --width 3840 --height 2400 --exit-with-session "$ROOT/scripts/${INNER:-g2_session.sh} $S $OUT" \
      > "$OUT/kwin-$S.log" 2>&1 < /dev/null
  # Stop only what this run activated: processes carrying this run's XDG_CONFIG_HOME.
  for p in /proc/[0-9]*; do
    if grep -qz "^XDG_CONFIG_HOME=$XDG/config\$" "$p/environ" 2>/dev/null; then kill "${p#/proc/}" 2>/dev/null; fi
  done
done
[ -z "$INNER" ] && cat "$OUT/results.txt"
