#!/bin/bash
# G1 timing and memory, on the current Wayland session. Windows close themselves.
# Usage: scripts/run_g1.sh [RUNS]
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNS=${1:-12}
BIN=$ROOT/target/release/qt-shell
SO=$ROOT/target/release/libqt_shell.so
LAUNCH=$ROOT/launcher/target/release/mdview-launcher
H=$ROOT/tools/target/release/harness
ARGS="--measure --exit-after-present --stay-ms 1200"
M="--runs $RUNS --drop 2 --smaps-after-ms 800"
echo "load before: $(cat /proc/loadavg)"
echo "=== one-shot info"; timeout 10 "$BIN" --measure --exit-after-present | cut -c1-250
echo "=== A direct-linked, default platform theme"
timeout 120 "$H" $M -- "$BIN" $ARGS
echo "=== B direct-linked, QT_QPA_PLATFORMTHEME=generic (Fusion)"
QT_QPA_PLATFORMTHEME=generic timeout 120 "$H" $M -- "$BIN" $ARGS
echo "=== C direct-linked, generic + QT_STYLE_OVERRIDE=Breeze"
QT_QPA_PLATFORMTHEME=generic QT_STYLE_OVERRIDE=Breeze timeout 120 "$H" $M -- "$BIN" $ARGS
echo "=== D Qt-free launcher + dlopen, default theme (socket miss first)"
timeout 120 "$H" $M -- "$LAUNCH" --socket mdvq-absent --so "$SO" $ARGS
echo "=== E second launch, Qt-free launcher, listener alive"
"$LAUNCH" --listen mdvq-g1 --secs 25 > /dev/null &
LP=$!
sleep 0.3
timeout 30 "$H" --exit-only --runs "$RUNS" --drop 2 -- "$LAUNCH" --socket mdvq-g1 --so "$SO"
echo "=== F second launch, direct-linked binary (ld.so loads Qt first), listener alive"
timeout 30 "$H" --exit-only --runs "$RUNS" --drop 2 -- "$BIN" --second-launch-probe mdvq-g1
kill "$LP" 2>/dev/null; wait "$LP" 2>/dev/null
echo "=== self-test on this session"
timeout 15 "$BIN" --measure --exit-after-present --self-test | cut -c1-200
echo "load after: $(cat /proc/loadavg)"
