#!/usr/bin/env bash
# Check the first shot's actual environment without launching a viewer.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cat >"$work/viewer" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
test -z "${MDVIEW_SETTINGS+x}${MDEDIT_SETTINGS+x}${MDV_ENGINE+x}"
if command -v cygpath >/dev/null; then
    test "$LOCALAPPDATA" != inherited
    test -d "$(cygpath -u "$LOCALAPPDATA")"
else
    for name in HOME XDG_DATA_HOME XDG_CONFIG_HOME XDG_STATE_HOME; do
        test "${!name}" != inherited
        test -d "${!name}"
    done
fi
test "$1" = --behind
test "$2" = --no-session
exit 37
EOF
chmod +x "$work/viewer"
status=0
MDVIEW_EXE="$work/viewer" MDVIEW_SETTINGS=inherited MDEDIT_SETTINGS=inherited MDV_ENGINE=inherited \
    LOCALAPPDATA=inherited HOME=inherited XDG_DATA_HOME=inherited XDG_CONFIG_HOME=inherited \
    XDG_STATE_HOME=inherited bash "$root/tests/shots.sh" "$work/shots" || status=$?
test "$status" -eq 37
echo "shots: inherited settings, data and engine isolated"
