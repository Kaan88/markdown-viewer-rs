#!/bin/bash
# Screenshots in a private virtual KWin, so nothing on the real desktop is touched.
# Usage: scripts/shots.sh OUTDIR SCALE:THEME... (THEME is light, dark or system).
# "system" seeds a read-only copy of the user's kdeglobals and passes no --theme.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=$(realpath -m "$1"); shift
WORK="${WORK:-${TMPDIR:-/tmp}/look-qt-shots.$$}"
mkdir -p "$OUT" "$WORK"
for PAIR in "$@"; do
  S=${PAIR%%:*}; T=${PAIR##*:}
  XDG="$WORK/xdg-$S-$T"
  rm -rf "$XDG"; mkdir -p "$XDG/config" "$XDG/data" "$XDG/cache"
  cat > "$XDG/config/kwinoutputconfig.json" <<J
[{"name":"outputs","data":[{"connectorName":"Virtual-0","mode":{"basic":{"width":2400,"height":1600,"refreshRate":60000},"flags":1},"scale":$S,"transform":"Normal"}]},
 {"name":"setups","data":[{"lidClosed":false,"outputs":[{"enabled":true,"outputIndex":0,"position":{"x":0,"y":0},"priority":0,"replicationSource":""}]}]}]
J
  case "$T" in
    light) cp /usr/share/color-schemes/BreezeLight.colors "$XDG/config/kdeglobals"
           printf '\n[General]\nAccentColor=0,95,184\n' >> "$XDG/config/kdeglobals" ;;
    dark)  cp /usr/share/color-schemes/BreezeDark.colors "$XDG/config/kdeglobals"
           printf '\n[General]\nAccentColor=96,205,255\n' >> "$XDG/config/kdeglobals" ;;
    system) cp "$HOME/.config/kdeglobals" "$XDG/config/kdeglobals" ;;
  esac
  # Breeze takes its hover and focus tints from kdeglobals, not from the palette.
  case "$T" in
    light) sed -i -E 's/^(DecorationFocus|DecorationHover)=.*/\1=0,95,184/' "$XDG/config/kdeglobals" ;;
    dark)  sed -i -E 's/^(DecorationFocus|DecorationHover)=.*/\1=96,205,255/' "$XDG/config/kdeglobals" ;;
  esac
  SOCK="mdlookqt-$$-$S-$T"
  env -u DISPLAY WAYLAND_DISPLAY="$SOCK" XDG_CONFIG_HOME="$XDG/config" XDG_DATA_HOME="$XDG/data" XDG_CACHE_HOME="$XDG/cache" \
    KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
    timeout 150 dbus-run-session -- kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --no-kactivities \
      --socket "$SOCK" --width 2400 --height 1600 --exit-with-session "$ROOT/scripts/${INNER:-shots_session.sh} $S $T $OUT" \
      > "$OUT/.kwin-$S-$T.log" 2>&1 < /dev/null
  echo "session $S $T exit=$?"
  # Stop only what this run started: processes carrying this run's XDG_CONFIG_HOME.
  for p in /proc/[0-9]*; do
    if grep -qz "^XDG_CONFIG_HOME=$XDG/config\$" "$p/environ" 2>/dev/null; then kill "${p#/proc/}" 2>/dev/null; fi
  done
done
