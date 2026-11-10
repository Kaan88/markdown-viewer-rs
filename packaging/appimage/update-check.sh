#!/usr/bin/env bash
# Check for (or apply) an mdview AppImage update with appimageupdatetool.
#
#   update-check.sh            check, at most once per 24 h; prints ONE status line
#   update-check.sh --force    check now, ignoring the stamp
#   update-check.sh --apply    download and apply the update (delta via zsync)
#
# Status lines: "not-appimage", "disabled", "skipped (checked recently)", "no-tool",
#               "update-available", "up-to-date", "error (<exit code>)", "updated", "update-failed (<code>)".
# Exit code is 0 for everything except --apply failing (1). The opt-out is the file
# ${XDG_CONFIG_HOME:-$HOME/.config}/mdview/no-update-check, or MDVIEW_NO_UPDATE_CHECK=1.
set -uo pipefail

mode=check
case "${1:-}" in --force) mode=force ;; --apply) mode=apply ;; "") ;; *) echo "usage: $0 [--force|--apply]" >&2; exit 2 ;; esac

if [ -z "${APPIMAGE:-}" ] || [ ! -f "$APPIMAGE" ]; then echo "not-appimage"; exit 0; fi
if [ -n "${MDVIEW_NO_UPDATE_CHECK:-}" ] || [ -e "${XDG_CONFIG_HOME:-$HOME/.config}/mdview/no-update-check" ]; then
  [ "$mode" = apply ] || { echo "disabled"; exit 0; }
fi

tool="$(command -v appimageupdatetool || command -v AppImageUpdate || true)"
if [ -z "$tool" ]; then echo "no-tool"; [ "$mode" = apply ] && exit 1; exit 0; fi

if [ "$mode" = apply ]; then
  "$tool" "$APPIMAGE" >/dev/null 2>&1
  rc=$?
  if [ "$rc" = 0 ]; then echo "updated"; exit 0; fi
  echo "update-failed ($rc)"; exit 1
fi

state="${XDG_STATE_HOME:-$HOME/.local/state}/mdview"
stamp="$state/update-check.stamp"
if [ "$mode" = check ] && [ -f "$stamp" ]; then
  now="$(date +%s)"; last="$(stat -c %Y "$stamp" 2>/dev/null || echo 0)"
  if [ $((now - last)) -lt 86400 ]; then echo "skipped (checked recently)"; exit 0; fi
fi
mkdir -p "$state" && touch "$stamp"

"$tool" --check-for-update "$APPIMAGE" >/dev/null 2>&1
rc=$?
case "$rc" in
  0) echo "up-to-date" ;;
  1) echo "update-available" ;;
  *) echo "error ($rc)" ;;
esac
exit 0
