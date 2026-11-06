#!/bin/bash
# Runs inside a nested KWin: exercises --screenshot and --self-test. Args: SCALE THEME OUTDIR.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
S=$1; T=$2; OUT=$3
export QT_QPA_PLATFORM=wayland
timeout 12 "$ROOT/target/release/look-qt" --theme="$T" --find-open --screenshot="$OUT/flag-$T@$S.png" > "$OUT/.flag-$T@$S.log" 2>&1
timeout 12 "$ROOT/target/release/look-qt" --theme="$T" --self-test > "$OUT/.selftest-$T@$S.log" 2>&1
timeout 12 "$ROOT/target/release/look-qt" --theme="$T" --screenshot="$OUT/flag-plain-$T@$S.png" >> "$OUT/.flag-$T@$S.log" 2>&1
