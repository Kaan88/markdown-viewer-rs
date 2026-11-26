#!/usr/bin/env bash
# Verify the environment passed to LaunchServices and embedded in the update bundle,
# without invoking LaunchServices, signing, starting apps, or touching a keychain.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'mac_profile_cleanup; rm -rf "$work"' EXIT
source "$root/tests/mac-launch.sh"
mkdir -p "$work/bin" "$work/Test.app/Contents/MacOS"
cat >"$work/bin/uname" <<'EOF'
#!/bin/sh
echo Darwin
EOF
cat >"$work/bin/open" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" > "$TEST_OPEN_ARGS"
EOF
cat >"$work/bin/ps" <<'EOF'
#!/bin/sh
printf 'fake-app HOME=%s MDVIEW_SETTINGS=%s MDEDIT_SETTINGS=%s\n' \
    "$HOME" "$MDVIEW_SETTINGS" "${TEST_EDITOR_SETTINGS:-$MDEDIT_SETTINGS}"
EOF
printf '#!/bin/sh\necho mdview\n' > "$work/bin/defaults"
printf '#!/bin/sh\necho 123\n' > "$work/bin/pgrep"
if windows_host; then
    # Git Bash's python3 can be a Microsoft Store stub. The local test uses Python.
    printf '#!/bin/sh\nexec python "$@"\n' > "$work/bin/python3"
fi
chmod +x "$work/bin/"*
export PATH="$work/bin:$PATH" TEST_OPEN_ARGS="$work/open"
export MDVIEW_SETTINGS=inherited MDEDIT_SETTINGS=inherited MD_SISTER_MDEDIT=inherited
mac_profile_init
test -z "${MD_SISTER_MDEDIT+x}"
mac_open -n "$work/Test.app" --args --no-session
for name in HOME MDVIEW_SETTINGS MDEDIT_SETTINGS MD_SISTER_MDVIEW_SETTINGS MD_SISTER_MDEDIT_SETTINGS; do
    grep -Fxq -- "$name=${!name}" "$work/open"
done
test "$MDVIEW_SETTINGS" != "$MDEDIT_SETTINGS"
test "$MD_SISTER_MDEDIT_SETTINGS" = "$MDEDIT_SETTINGS"
mac_profile_check_process 123
if TEST_EDITOR_SETTINGS=inherited mac_profile_check_process 123 2>"$work/profile-error"; then
    echo "mac profile: accepted a process that did not inherit editor isolation" >&2
    exit 1
fi
grep -Fq 'did not receive the scratch home and settings' "$work/profile-error"
out="$work"
rm "$work/open"
if launch "$work/Test.app" 2>"$work/launch-error"; then
    echo "mac profile: reused an already-running bundle" >&2
    exit 1
fi
grep -Fq 'refusing to reuse' "$work/launch-error"
test ! -e "$work/open"
cp "$root/packaging/macos/Info.plist" "$work/Test.app/Contents/Info.plist"
mac_profile_bundle "$work/Test.app" io.github.kaan88.test
python3 - "$work/Test.app/Contents/Info.plist" <<'PY'
import os, plistlib, sys
with open(sys.argv[1], "rb") as file:
    info = plistlib.load(file)
assert info["CFBundleIdentifier"] == "io.github.kaan88.test"
assert info["LSEnvironment"] == {
    name: os.environ[name]
    for name in ("HOME", "MDVIEW_SETTINGS", "MDEDIT_SETTINGS",
                 "MD_SISTER_MDVIEW_SETTINGS", "MD_SISTER_MDEDIT_SETTINGS")
}
PY
echo "mac profile: LaunchServices arguments and restart bundle are isolated"
