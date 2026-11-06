#!/bin/bash
# One private virtual KWin. Usage: session.sh MODE SCALE THEME OUTDIR [KWIN_TIMEOUT_S]
# Same recipe as spikes/qt-shell/scripts/run_g2.sh, with one fixed 3840x2400 output for every
# scale and one seeded kdeglobals, so both apps get the same environment.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE=$1; S=$2; T=$3; OUT=$(realpath -m "$4"); LIMIT=${5:-300}
mkdir -p "$OUT"
W=${OUT_W:-3840}; H=${OUT_H:-2400}
XDG="${XDG_BASE:-$ROOT/.xdg}/$MODE-$S-$T-$$"
rm -rf "$XDG"; mkdir -p "$XDG/config" "$XDG/data" "$XDG/cache"
cat > "$XDG/config/kwinoutputconfig.json" <<J
[{"name":"outputs","data":[{"connectorName":"Virtual-0","mode":{"basic":{"width":$W,"height":$H,"refreshRate":60000},"flags":1},"scale":$S,"transform":"Normal"}]},
 {"name":"setups","data":[{"lidClosed":false,"outputs":[{"enabled":true,"outputIndex":0,"position":{"x":0,"y":0},"priority":0,"replicationSource":""}]}]}]
J
case "$T" in
  light) cp /usr/share/color-schemes/BreezeLight.colors "$XDG/config/kdeglobals"; A=0,95,184 ;;
  dark)  cp /usr/share/color-schemes/BreezeDark.colors  "$XDG/config/kdeglobals"; A=96,205,255 ;;
esac
printf '\n[General]\nAccentColor=%s\n' "$A" >> "$XDG/config/kdeglobals"
sed -i -E "s/^(DecorationFocus|DecorationHover)=.*/\1=$A/" "$XDG/config/kdeglobals"
SOCK="mdcmp-$$-$S-$T"
env -u DISPLAY -u QT_STYLE_OVERRIDE -u QT_QPA_PLATFORMTHEME WAYLAND_DISPLAY="$SOCK" \
  XDG_CONFIG_HOME="$XDG/config" XDG_DATA_HOME="$XDG/data" XDG_CACHE_HOME="$XDG/cache" \
  KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
  timeout "$LIMIT" dbus-run-session -- kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --no-kactivities \
    --socket "$SOCK" --width $W --height $H \
    --exit-with-session "$ROOT/scripts/inner.py $MODE $S $T $OUT" \
    > "$OUT/.kwin-$MODE-$S-$T.log" 2>&1 < /dev/null
echo "session $MODE $S $T exit=$?"
# Stop only what this run started: processes carrying this run's XDG_CONFIG_HOME.
for p in /proc/[0-9]*; do
  if grep -qz "^XDG_CONFIG_HOME=$XDG/config\$" "$p/environ" 2>/dev/null; then kill "${p#/proc/}" 2>/dev/null; fi
done
rm -rf "$XDG"
