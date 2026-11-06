#!/bin/bash
# Launch batch B (nothing else added), batch C (4 nice-19 spinners, which held launch steady in the A/B), then idle with per-thread deltas.
cd "$(dirname "$0")/.." || exit 1
R=results; L=$R/run2.log; : > $L
say() { echo "$(date +%T) load=$(cut -d' ' -f1-3 /proc/loadavg) $*" | tee -a $L; }
for T in light dark; do for S in 1.0 1.5 2.2; do
  say B $S $T; scripts/session.sh measure $S $T $R/launch-b 180 >> $L 2>&1
done; done
P=""; for i in 1 2 3 4; do nice -n 19 timeout 500 sh -c 'while :; do :; done' & P="$P $!"; done
for T in light dark; do for S in 1.0 1.5 2.2; do
  say C $S $T; scripts/session.sh measure $S $T $R/launch-c 180 >> $L 2>&1
done; done
kill $P 2>/dev/null; wait $P 2>/dev/null
for S in 1.0 1.5 2.2; do say idle $S; scripts/session.sh idle $S light $R/idle 420 >> $L 2>&1; done
say finished
