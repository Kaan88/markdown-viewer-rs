#!/usr/bin/env bash
# Exercise the Mac harness guards without launching an app, Swift, or touching a keychain.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/tests/mac-launch.sh"
work="$(mktemp -d)"
trap 'mac_profile_cleanup; rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$work/mdview.app/Contents/Resources"
cp "$root/packaging/macos/Info.plist" "$work/mdview.app/Contents/"
cat >"$work/bin/uname" <<'EOF'
#!/bin/sh
echo Darwin
EOF
cat >"$work/bin/swift" <<'EOF'
#!/usr/bin/env bash
[[ "$2" = 12345 && "$3" = 987 && "$4" = "$TEST_EXE" ]] || exit 3
[[ "$5" = "$TEST_WINDOW_TITLE" ]] || exit 4
printf '%s\n' "$6" >>"$TEST_UI_LOG"
EOF
cat >"$work/bin/pgrep" <<'EOF'
#!/bin/sh
printf '%s\n' "$TEST_MATCHES"
EOF
cat >"$work/bin/ps" <<'EOF'
#!/bin/sh
printf 'scratch HOME=%s MDVIEW_SETTINGS=%s MDEDIT_SETTINGS=%s\n' \
    "${TEST_PROCESS_HOME:-$HOME}" "$MDVIEW_SETTINGS" "$MDEDIT_SETTINGS"
EOF
cat >"$work/bin/ditto" <<'EOF'
#!/bin/sh
if [ "$1" = -c ]; then printf mock > "$5"; else cp -R "$1" "$2"; fi
EOF
printf '#!/bin/sh\nexit 0\n' >"$work/bin/codesign"
printf '#!/bin/sh\nprintf "0000000000  mock\\n"\n' >"$work/bin/shasum"
cat >"$work/bin/openssl" <<'EOF'
#!/bin/sh
if [ "$1" = x509 ]; then printf '%s\n' "sha1 Fingerprint=${TEST_FINGERPRINT}"; fi
EOF
cat >"$work/bin/sudo" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >>"$TEST_KEYCHAIN_LOG"
# Simulate an add that changed trust before failing. No command is actually run.
if [ "$2" = add-trusted-cert ]; then exit 79; fi
EOF
if windows_host; then
    printf '#!/bin/sh\nexec python "$@"\n' >"$work/bin/python3"
fi
chmod +x "$work/bin/"*
export PATH="$work/bin:$PATH"
mac_profile_init
pid=12345 wid=987
export TEST_EXE="$mac_profile/mdview.app/Contents/MacOS/mdview"
export TEST_MATCHES="$pid" TEST_UI_LOG="$work/ui" TEST_WINDOW_TITLE="tables.md — Markdown Viewer"
reject() {
    if "$@" 2>/dev/null; then
        echo "mock unexpectedly succeeded: $1" >&2
        exit 1
    fi
}
wid= reject mac_window_action edit "$TEST_EXE" "$TEST_WINDOW_TITLE"
TEST_MATCHES=999 reject mac_window_action edit "$TEST_EXE" "$TEST_WINDOW_TITLE"
TEST_MATCHES=$'12345\n999' reject mac_window_action quit "$TEST_EXE" "$TEST_WINDOW_TITLE"
TEST_PROCESS_HOME=inherited reject mac_window_action menu "$TEST_EXE" "$TEST_WINDOW_TITLE"
reject mac_window_action verify "$TEST_EXE" "math.md — Markdown Viewer"
test ! -e "$TEST_UI_LOG"
for action in verify edit menu quit; do
    mac_window_action "$action" "$TEST_EXE" "$TEST_WINDOW_TITLE"
done
diff -u <(printf '%s\n' verify edit menu quit) "$TEST_UI_LOG"

# No broad name-based AppleScript or process-command logging in the callers.
if grep -En 'osascript|pgrep.*-.*l|ps .*args=' "$root/tests/"{mac-open,mac-sister}.sh; then
    exit 1
fi

export TEST_KEYCHAIN_LOG="$work/keychain" GITHUB_ACTIONS=true
for runner in "" self-hosted; do
    RUNNER_ENVIRONMENT="$runner" reject bash "$root/tests/mac-update.sh" "$work/denied" "$work/mdview.app"
    test ! -e "$work/denied" && test ! -e "$TEST_KEYCHAIN_LOG"
done
export TEST_FINGERPRINT="AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD"
status=0
RUNNER_ENVIRONMENT=github-hosted bash "$root/tests/mac-update.sh" "$work/hosted" "$work/mdview.app" || status=$?
test "$status" -eq 79
sed -n '1p' "$TEST_KEYCHAIN_LOG" | grep -Fq 'security add-trusted-cert'
sed -n '2p' "$TEST_KEYCHAIN_LOG" | grep -Eq '^security delete-certificate -Z [[:xdigit:]]{40} /Library/Keychains/System.keychain$'
echo "mac review: PID/document guards, hosted-only trust, and cleanup after a failed add"
