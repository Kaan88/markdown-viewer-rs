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
test -z "${MD_SISTER_MDEDIT+x}${MD_SISTER_MDVIEW+x}${MD_SISTER_MDEDIT_SETTINGS+x}${MD_SISTER_MDVIEW_SETTINGS+x}${MD_SISTER_OTHER_SETTINGS+x}"
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
    MD_SISTER_MDEDIT=inherited MD_SISTER_MDVIEW=inherited MD_SISTER_MDEDIT_SETTINGS=inherited \
    MD_SISTER_MDVIEW_SETTINGS=inherited MD_SISTER_OTHER_SETTINGS=inherited \
    LOCALAPPDATA=inherited HOME=inherited XDG_DATA_HOME=inherited XDG_CONFIG_HOME=inherited \
    XDG_STATE_HOME=inherited bash "$root/tests/shots.sh" "$work/shots" || status=$?
test "$status" -eq 37

# Emulate a Linux/WSL shell in a scratch copy of the harness. A stale Windows
# binary must neither win the default selection nor be accepted as an override.
mkdir -p "$work/host" "$work/repo/tests" "$work/repo/target/release"
cp "$root/tests/"{shots,scratch-profile}.sh "$work/repo/tests/"
printf '#!/bin/sh\nprintf "Linux\\n"\n' > "$work/host/uname"
printf '#!/bin/sh\nexit 91\n' > "$work/repo/target/release/mdview.exe"
printf '#!/bin/sh\nexit 37\n' > "$work/repo/target/release/mdview"
chmod +x "$work/host/uname" "$work/repo/target/release/"*
(
    export PATH="$work/host:$PATH"
    unset MDVIEW_EXE
    status=0
    bash "$work/repo/tests/shots.sh" "$work/default" || status=$?
    test "$status" -eq 37
    mkdir "$work/repo/target/shots-notes"
    status=0
    bash "$work/repo/tests/shots.sh" "$work/stale" 2>"$work/stale.log" || status=$?
    test "$status" -eq 2
    grep -Fq 'shots-notes already exists; remove it' "$work/stale.log"
    test -d "$work/repo/target/shots-notes"
    rmdir "$work/repo/target/shots-notes"
    rm "$work/repo/target/release/mdview"
    status=0
    bash "$work/repo/tests/shots.sh" "$work/no-native" || status=$?
    test "$status" -eq 2
    status=0
    MDVIEW_EXE="$work/repo/target/release/mdview.exe" \
        bash "$work/repo/tests/shots.sh" "$work/explicit-pe" || status=$?
    test "$status" -eq 2
    printf 'MZ' > "$work/repo/target/release/mdview"
    chmod +x "$work/repo/target/release/mdview"
    status=0
    bash "$work/repo/tests/shots.sh" "$work/renamed-pe" || status=$?
    test "$status" -eq 2
)
echo "shots: inherited overrides isolated and Windows binaries refused on Linux"
