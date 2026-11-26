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
trap mac_profile_cleanup EXIT
mac_profile_init || exit 1
# Give Finder's document hand-off a bundle identity no existing viewer can claim.
ditto "$app" "$mac_profile/mdview.app" || exit 1
app="$mac_profile/mdview.app"
mac_profile_bundle "$app" "io.github.kaan88.mdview.test.$$" || exit 1
codesign --force --sign - "$app" || exit 1
launch "$app" --no-session || exit 1
wid="$(window_id)"
echo "window id: ${wid:-none}"
sleep 1
fail=0
# The event goes to the running copy: no second process starts.
mac_open -a "$app" "$PWD/fixtures/tables.md" || exit 1
sleep 3
n=$(pgrep -f "^$(cd "$app" && pwd)/Contents/MacOS/mdview( |\$)" | wc -l | tr -d ' ')
pgrep -lf "Contents/MacOS/mdview" || true
if [ "$n" = "1" ]; then echo "PASS one process after open -a"; else echo "FAIL $n processes after open -a"; fail=1; fi
[ -n "${wid:-}" ] && screencapture -x -o -l "$wid" "$out/opened.png" && echo "opened captured"
# The application menu, opened from the menu bar.
osascript -e 'tell application "System Events" to tell process "mdview" to click menu bar item "mdview" of menu bar 1' || echo "no application menu"
sleep 1
screencapture -x "$out/app-menu.png" && echo "app menu captured"
osascript -e 'tell application "System Events" to key code 53'
sleep 0.5
osascript -e 'tell application "System Events" to keystroke "q" using command down'
for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.25; done
if kill -0 "$pid" 2>/dev/null; then echo "FAIL Command+Q left the process running"; fail=1; else echo "PASS Command+Q quits"; fi
stop
# Launched for a file, as a double-click in Finder launches it: the file comes with the launch's
# open-documents event.
mac_open -a "$app" "$PWD/fixtures/math.md" || exit 1
pid=""
for _ in $(seq 1 40); do
    pid="$(pgrep -n -f "^$(cd "$app" && pwd)/Contents/MacOS/mdview( |\$)" || true)"
    [ -n "$pid" ] && break
    sleep 0.25
done
[ -n "$pid" ] && mac_profile_check_process "$pid" || exit 1
wid="$(window_id)"
sleep 3
[ -n "${wid:-}" ] && screencapture -x -o -l "$wid" "$out/launched.png" && echo "launched captured"
screencapture -x "$out/launched-screen.png"
stop
exit $fail
