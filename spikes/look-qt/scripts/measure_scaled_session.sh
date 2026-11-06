#!/bin/bash
# Runs inside a nested KWin: launch and memory at this scale. Args: SCALE THEME OUTDIR.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
S=$1; T=$2; OUT=$3
export QT_QPA_PLATFORM=wayland
timeout 60 "$ROOT/../qt-shell/tools/target/release/harness" --runs 12 --drop 2 --smaps-after-ms 800 -- \
  "$ROOT/target/release/look-qt" --theme="$T" --measure --stay-ms=1200 > "$OUT/scaled-$T@$S.txt" 2>&1
