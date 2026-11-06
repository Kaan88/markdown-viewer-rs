#!/bin/bash
# Theme and dlopen timing, round-robin so drift hits every variant alike.
# Usage: scripts/run_g1_interleaved.sh [ROUNDS]. Each variant drops its first 2 rounds.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ROUNDS=${1:-12}
BIN=$ROOT/target/release/qt-shell; SO=$ROOT/target/release/libqt_shell.so
LAUNCH=$ROOT/launcher/target/release/mdview-launcher; H=$ROOT/tools/target/release/harness
ARGS="--measure --exit-after-present"
one() { "$H" --runs 1 --drop 0 -- "$@" | sed -n 's/^present_ms: median_after_drop=\([0-9.]*\).*/\1/p'; }
declare -A R
for ((i = 0; i < ROUNDS; i++)); do
  R[default]+=" $(one "$BIN" $ARGS)"
  R[generic]+=" $(QT_QPA_PLATFORMTHEME=generic one "$BIN" $ARGS)"
  R[generic_breeze]+=" $(QT_QPA_PLATFORMTHEME=generic QT_STYLE_OVERRIDE=Breeze one "$BIN" $ARGS)"
  R[dlopen_default]+=" $(one "$LAUNCH" --socket mdvq-absent --so "$SO" $ARGS)"
done
for k in default generic generic_breeze dlopen_default; do
  python3 -c "
import statistics,sys
v=[float(x) for x in sys.argv[2:]]; k=v[2:]
print(f'{sys.argv[1]}: median_after_drop={statistics.median(k):.2f} n={len(k)} min={min(k):.2f} max={max(k):.2f} raw={v}')" "$k" ${R[$k]}
done
