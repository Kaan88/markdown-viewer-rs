#!/bin/bash
# Runs inside a nested KWin: launch time and memory at a fractional scale.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export QT_QPA_PLATFORM=wayland
timeout 90 "$ROOT/tools/target/release/harness" --runs 12 --drop 2 --smaps-after-ms 800 -- \
  "$ROOT/target/release/qt-shell" --measure --exit-after-present --stay-ms 1200 > "$2/g1-scaled-$1.txt" 2>&1
