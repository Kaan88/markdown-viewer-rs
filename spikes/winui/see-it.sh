#!/usr/bin/env bash
# Opens the self-contained build in the WinApps Windows 11 VM so you can look at it live.
# It runs C:\mdview-w0\mdview-winui.exe, the copy made by spikes/vm/w1.cmd. Close the window to end it.
set -euo pipefail
docker start WinApps >/dev/null
exec "$HOME/.local/bin/winapps" manual 'C:\mdview-w0\mdview-winui.exe'
