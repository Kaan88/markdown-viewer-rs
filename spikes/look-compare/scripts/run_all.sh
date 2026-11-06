#!/bin/bash
# Everything, inside private virtual KWin sessions. About 25 minutes. Nothing opens on the real desktop.
cd "$(dirname "$0")/.." || exit 1
R=results; mkdir -p $R/shots $R/trace $R/launch $R/idle $R/t0
L=$R/run.log; : > $L
say() { echo "$(date +%T) load=$(cut -d' ' -f1-3 /proc/loadavg) $*" | tee -a $L; }
for T in light dark; do for S in 1.0 1.5 2.2; do
  say measure $S $T; scripts/session.sh measure $S $T $R/launch 180 >> $L 2>&1
done; done
say t0; scripts/session.sh t0 1.0 light $R/t0 180 >> $L 2>&1
for S in 1.0 1.5 2.2; do say trace $S; scripts/session.sh trace $S light $R/trace 60 >> $L 2>&1; done
for T in light dark; do for S in 1.0 1.5 2.2; do
  say shots $S $T; scripts/session.sh shots $S $T $R/shots 180 >> $L 2>&1
done; done
for T in light dark; do for S in 1.0 1.5; do
  say menu $S $T; scripts/session.sh menu $S $T $R/shots 90 >> $L 2>&1
done; done
for S in 1.0 1.5 2.2; do say idle $S; scripts/session.sh idle $S light $R/idle 420 >> $L 2>&1; done
say done
