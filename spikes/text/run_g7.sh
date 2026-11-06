#!/bin/sh
# Reproduces results/*.txt. Run from spikes/text.
B=./target/release/g7; O=results
: > $O/fontctx.txt; for i in 1 2 3 4 5 6 7 8 9 10; do $B fontctx >> $O/fontctx.txt; done
run() { n=$1; c=$2; shift 2; : > $O/$n.txt; i=0; while [ $i -lt $c ]; do i=$((i+1)); echo "=== run $i load=$(cut -d' ' -f1 /proc/loadavg)" >> $O/$n.txt; $B all "$@" >> $O/$n.txt 2>&1; done; }
run mixed_s100 5 --passes 5
run ascii_s100 3 --ascii --passes 3
run heavy_s100 3 --heavy --passes 3
run mixed_s150 3 --scale 1.5 --passes 3
run mixed_s220 3 --scale 2.2 --passes 3
run narrow_w600 3 --width 600 --passes 3
run mixed_t4 3 --threads 4 --passes 3
run heavy_t4 3 --heavy --threads 4 --passes 3
run evict_s220_256kb 3 --scale 2.2 --cache-kb 256 --passes 1
echo done > $O/DONE
