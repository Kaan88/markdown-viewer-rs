#!/usr/bin/env bash
# On a macOS runner: with mdedit.app beside mdview.app, Command+E (Edit in mdedit) in the viewer
# starts the editor's bundle on the open file. Prints PASS or FAIL; exits non-zero on a failure.
#
#   tests/mac-sister.sh <out-dir> <mdview.app>     # mdedit.app in the same folder
set -uo pipefail
out="${1:?usage: tests/mac-sister.sh <out-dir> <app>}"
app="${2:?app}"
mkdir -p "$out"
source "$(dirname "$0")/mac-launch.sh"
trap mac_profile_cleanup EXIT
mac_profile_init || exit 1
# Keep testing the bundle-beside-bundle rule, but in scratch. The toolkit starts
# the sister directly, inheriting the viewer's scratch home and instance paths.
ditto "$(dirname "$app")/mdedit.app" "$mac_profile/mdedit.app" || exit 1
ditto "$app" "$mac_profile/mdview.app" || exit 1
editor="$mac_profile/mdedit.app/Contents/MacOS/mdedit"
[ -x "$editor" ] || { echo "FAIL no mdedit.app beside $app"; exit 1; }
launch "$mac_profile/mdview.app" --no-session "$PWD/fixtures/showcase.md" || exit 1
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
if [ -n "$edit" ] && mac_profile_check_process "$edit"; then
    echo "PASS Command+E started mdedit.app ($(ps -o args= -p "$edit"))"
else
    echo "FAIL Command+E did not start an isolated mdedit"
    fail=1
fi
sleep 3
screencapture -x "$out/screen.png" && echo "screen captured"
[ -n "$edit" ] && kill "$edit" 2>/dev/null
stop
exit $fail
