#!/bin/bash
cd "$(dirname "$0")/.." || exit 1
R=results; L=$R/run2.log
say() { echo "$(date +%T) load=$(cut -d' ' -f1-3 /proc/loadavg) $*" | tee -a $L; }
P=""; for i in 1 2 3 4; do nice -n 19 timeout 300 sh -c 'while :; do :; done' & P="$P $!"; done
T=dark; for S in 1.0 1.5 2.2; do say C $S $T; scripts/session.sh measure $S $T $R/launch-c 180 >> $L 2>&1; done
kill $P 2>/dev/null; wait $P 2>/dev/null
for S in 1.0 1.5 2.2; do say idle $S; scripts/session.sh idle $S light $R/idle 420 >> $L 2>&1; done
say finished
