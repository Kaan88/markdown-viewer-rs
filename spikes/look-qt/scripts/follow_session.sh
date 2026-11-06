#!/bin/bash
# Runs inside a nested KWin that was seeded dark: does the app follow a live scheme change?
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
S=$1; T=$2; OUT=$3
export QT_QPA_PLATFORM=wayland
log="$OUT/.follow.log"
timeout 12 "$ROOT/target/release/look-qt" --trace --stay-ms=9000 > "$log" 2>&1 &
pid=$!
sleep 1.5
"$ROOT/../qt-shell/tools/target/release/kwin_capture" "$OUT/follow-before.png" >> "$log"
cp /usr/share/color-schemes/BreezeLight.colors "$XDG_CONFIG_HOME/kdeglobals"
printf '\n[General]\nAccentColor=0,95,184\n' >> "$XDG_CONFIG_HOME/kdeglobals"
dbus-send --session --type=signal /KGlobalSettings org.kde.KGlobalSettings.notifyChange int32:0 int32:0
sleep 1.5
"$ROOT/../qt-shell/tools/target/release/kwin_capture" "$OUT/follow-after.png" >> "$log"
kill $pid 2>/dev/null; wait $pid 2>/dev/null
