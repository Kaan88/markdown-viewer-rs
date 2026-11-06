#!/bin/bash
# Why is Qt slower here than look-qt/RESULT.md's 61 ms? Tests output size and where XDG dirs live, 2 rounds.
cd "$(dirname "$0")/.." || exit 1
TMPX=${TMPX:?}; R=results/ab; mkdir -p $R $TMPX
for round in 1 2; do
  AB_TAG=big-disk-$round                                                scripts/session.sh ab 1.0 light $R 80
  AB_TAG=big-tmp-$round  XDG_BASE=$TMPX                                 scripts/session.sh ab 1.0 light $R 80
  AB_TAG=small-disk-$round OUT_W=2400 OUT_H=1600                        scripts/session.sh ab 1.0 light $R 80
  AB_TAG=small-tmp-$round  OUT_W=2400 OUT_H=1600 XDG_BASE=$TMPX         scripts/session.sh ab 1.0 light $R 80
done
