#!/usr/bin/env bash
# On a macOS runner: files handed to the running bundle the way Finder hands them (an
# open-documents event, here through `open -a`) open in its window, and its native menu's Quit
# item ends it. Prints PASS or FAIL lines; exits non-zero on a failure.
#
#   tests/mac-open.sh <out-dir> <mdview.app>
set -uo pipefail
out="${1:?usage: tests/mac-open.sh <out-dir> <app>}"
app="${2:?app}"
mkdir -p "$out"
source "$(dirname "$0")/mac-launch.sh"
trap 'stop; mac_profile_cleanup' EXIT
mac_profile_init || exit 1
# Give Finder's document hand-off a bundle identity no existing viewer can claim.
ditto "$app" "$mac_profile/mdview.app" || exit 1
app="$mac_profile/mdview.app"
mac_profile_bundle "$app" "io.github.kaan88.mdview.test.$$" || exit 1
codesign --force --sign - "$app" || exit 1
launch "$app" --no-session || exit 1
exe="$app/Contents/MacOS/mdview"
wid="$(window_id)" || exit 1
mac_wait_document "$exe" "Markdown Viewer" || exit 1
# The event goes to the running copy: no second process starts.
mac_open -a "$app" "$PWD/fixtures/tables.md" || exit 1
title="tables.md — Markdown Viewer"
mac_wait_document "$exe" "$title" || exit 1
ids="$(mac_process_ids "$exe" || true)"
n="$(printf '%s\n' "$ids" | grep -c '^[0-9]' || true)"
echo "scratch PID $pid; matching process count: $n"
[ "$ids" = "$pid" ] || { echo "FAIL open -a did not use just the original scratch process"; exit 1; }
screencapture -x -o -l "$wid" "$out/opened.png" || exit 1
# The application menu, opened from the menu bar.
mac_window_action menu "$exe" "$title" || exit 1
screencapture -x "$out/app-menu.png" || exit 1
mac_window_action quit "$exe" "$title" || exit 1
for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.25; done
if kill -0 "$pid" 2>/dev/null; then echo "FAIL Quit left the scratch process running"; stop; exit 1; fi
echo "PASS tables.md reached the original scratch window, and its Quit item closed it"
# Launched for a file, as a double-click in Finder launches it: the file comes with the launch's
# open-documents event.
mac_open -a "$app" "$PWD/fixtures/math.md" || exit 1
pid=""
for _ in $(seq 1 40); do
    pid="$(mac_process_ids "$exe" || true)"
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] && break
    sleep 0.25
done
wid="$(window_id)" || exit 1
mac_wait_document "$exe" "math.md — Markdown Viewer" || exit 1
echo "PASS math.md opened in scratch PID $pid"
screencapture -x -o -l "$wid" "$out/launched.png" || exit 1
screencapture -x "$out/launched-screen.png"
