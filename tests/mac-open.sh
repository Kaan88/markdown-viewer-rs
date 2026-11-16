#!/usr/bin/env bash
# On a macOS runner: files handed to the running bundle the way Finder hands them (an
# open-documents event, here through `open -a`) open in its window, and Command+Q (the menu bar's
# Quit, which is the Exit command) ends it. Prints PASS or FAIL lines; exits non-zero on a failure.
#
#   tests/mac-open.sh <out-dir> <mdview.app>
set -uo pipefail
out="${1:?usage: tests/mac-open.sh <out-dir> <app>}"
app="${2:?app}"
mkdir -p "$out"
source "$(dirname "$0")/mac-launch.sh"
launch "$app" --no-session
wid="$(window_id)"
echo "window id: ${wid:-none}"
sleep 1
fail=0
# The event goes to the running copy: no second process starts.
open -a "$(cd "$app" && pwd)" "$PWD/fixtures/tables.md"
sleep 3
n=$(pgrep -f "$(cd "$app" && pwd)/Contents/MacOS/mdview\$" | wc -l | tr -d ' ')
if [ "$n" = "1" ]; then echo "PASS one process after open -a"; else echo "FAIL $n processes after open -a"; fail=1; fi
[ -n "${wid:-}" ] && screencapture -x -o -l "$wid" "$out/opened.png" && echo "opened captured"
osascript -e 'tell application "System Events" to keystroke "q" using command down'
for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.25; done
if kill -0 "$pid" 2>/dev/null; then echo "FAIL Command+Q left the process running"; fail=1; else echo "PASS Command+Q quits"; fi
stop
exit $fail
