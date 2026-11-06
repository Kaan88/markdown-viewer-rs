#!/bin/bash
# Clean release builds, one target dir per renderer, so sizes and times are separate.
cd "$(dirname "$0")" || exit 1
for v in "merman r-merman merman-bin" "merman-cose r-merman-cose merman-bin" "mmdr r-mmdr mmdr-bin"; do
  set -- $v
  rm -rf "target-$1"
  s=$(date +%s.%N)
  /usr/bin/cargo build --release -j 6 --no-default-features --features "$2" --bin "$3" --target-dir "target-$1" > "logs/build-$1.log" 2>&1
  rc=$?
  e=$(date +%s.%N)
  echo "$1 rc=$rc clean_build_wall_s=$(echo "$e - $s" | bc 2>/dev/null || python3 -c "print(round($e-$s,1))") crates=$(grep -c Compiling logs/build-$1.log)"
done
echo DONE
