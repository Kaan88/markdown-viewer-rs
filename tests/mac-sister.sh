#!/usr/bin/env bash
# On a macOS runner: with mdedit.app beside mdview.app, Command+E (Edit in mdedit) in the viewer
# starts the editor's bundle on the open file. Prints PASS or FAIL; exits non-zero on a failure.
#
#   tests/mac-sister.sh <out-dir> <mdview.app>     # mdedit.app in the same folder
set -uo pipefail
out="${1:?usage: tests/mac-sister.sh <out-dir> <app>}"
app="${2:?app}"
mkdir -p "$out"
editor="$(cd "$(dirname "$app")" && pwd)/mdedit.app/Contents/MacOS/mdedit"
[ -x "$editor" ] || { echo "FAIL no mdedit.app beside $app"; exit 1; }
source "$(dirname "$0")/mac-launch.sh"
launch "$app" --no-session "$PWD/fixtures/showcase.md"
wid="$(window_id)"
echo "window id: ${wid:-none}"
sleep 1
fail=0
osascript -e 'tell application "System Events" to keystroke "e" using command down'
edit=""
for _ in $(seq 1 40); do
    edit="$(pgrep -n -f "^$editor( |\$)" || true)"
    [ -n "$edit" ] && break
    sleep 0.25
done
if [ -n "$edit" ]; then
    echo "PASS Command+E started mdedit.app ($(ps -o args= -p "$edit"))"
else
    echo "FAIL Command+E started no mdedit"
    fail=1
fi
sleep 3
screencapture -x "$out/screen.png" && echo "screen captured"
[ -n "$edit" ] && kill "$edit" 2>/dev/null
stop
exit $fail
