#!/bin/bash
# Launch and memory on the current desktop, round robin. Each run opens a short-lived
# window and closes itself. Usage: scripts/measure.sh OUTFILE [BATCHES] [RUNS]
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=$1; BATCHES=${2:-3}; RUNS=${3:-12}
H=$ROOT/../qt-shell/tools/target/release/harness
BIN=$ROOT/target/release/look-qt
export QT_QPA_PLATFORM=wayland
: > "$OUT"
for b in $(seq 1 "$BATCHES"); do
  for v in "skeleton" "qss=none" "qss=widgets" "qss=app" "find-open" "find-open-noshadow"; do
    case "$v" in
      skeleton) cmd=("$ROOT/../qt-shell/target/release/qt-shell" --measure --exit-after-present --stay-ms 1200) ;;
      qss=none) cmd=("$BIN" --measure --stay-ms=1200 --qss=none) ;;
      qss=widgets) cmd=("$BIN" --measure --stay-ms=1200) ;;
      qss=app) cmd=("$BIN" --measure --stay-ms=1200 --qss=app) ;;
      find-open) cmd=("$BIN" --measure --stay-ms=1200 --find-open) ;;
      find-open-noshadow) cmd=("$BIN" --measure --stay-ms=1200 --find-open --no-shadow) ;;
    esac
    { echo "### batch=$b variant=$v load=$(cut -d' ' -f1-3 /proc/loadavg)"
      timeout 120 "$H" --runs "$RUNS" --drop 2 --smaps-after-ms 800 -- "${cmd[@]}"; } >> "$OUT" 2>&1
  done
done
