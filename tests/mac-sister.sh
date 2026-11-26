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
trap 'stop; mac_profile_cleanup' EXIT
mac_profile_init || exit 1
# Keep testing the bundle-beside-bundle rule, but in scratch. The toolkit starts
# the sister directly, inheriting the viewer's scratch home and instance paths.
ditto "$(dirname "$app")/mdedit.app" "$mac_profile/mdedit.app" || exit 1
ditto "$app" "$mac_profile/mdview.app" || exit 1
editor="$mac_profile/mdedit.app/Contents/MacOS/mdedit"
[ -x "$editor" ] || { echo "FAIL no mdedit.app beside $app"; exit 1; }
viewer="$mac_profile/mdview.app"
launch "$viewer" --no-session "$PWD/fixtures/showcase.md" || exit 1
wid="$(window_id)" || exit 1
exe="$viewer/Contents/MacOS/mdview"
title="showcase.md — Markdown Viewer"
mac_wait_document "$exe" "$title" || exit 1
fail=0
mac_window_action edit "$exe" "$title" || exit 1
edit=""
for _ in $(seq 1 40); do
    edit="$(mac_process_ids "$editor" || true)"
    [[ "$edit" =~ ^[1-9][0-9]*$ ]] && break
    sleep 0.25
done
if [[ "$edit" =~ ^[1-9][0-9]*$ ]] && mac_profile_check_process "$edit"; then
    if (pid="$edit"; wid="$(window_id)" && mac_wait_document "$editor" "showcase.md — mdedit"); then
        echo "PASS Command+E opened showcase.md in scratch editor PID $edit"
    else
        echo "FAIL scratch editor did not show showcase.md"
        fail=1
    fi
else
    echo "FAIL Command+E did not start an isolated mdedit"
    fail=1
fi
sleep 3
screencapture -x "$out/screen.png" && echo "screen captured"
[[ "$edit" =~ ^[1-9][0-9]*$ ]] && mac_profile_check_process "$edit" && kill "$edit" 2>/dev/null
exit $fail
