# Sourced by the macOS harnesses. No app launch is allowed before this succeeds.
source "$(dirname "${BASH_SOURCE[0]}")/scratch-profile.sh"

mac_profile_init() {
    mac_profile="$(mktemp -d)" || return
    mac_profile="$(cd "$mac_profile" && pwd -P)" || return
    scratch_profile "$mac_profile" || return
    unset MD_SISTER_MDEDIT MD_SISTER_MDVIEW MDV_ENGINE_STALL
    export MDVIEW_SETTINGS="$mac_profile/viewer/settings.ini"
    export MDEDIT_SETTINGS="$mac_profile/editor/settings.ini"
    export MD_SISTER_MDVIEW_SETTINGS="$MDVIEW_SETTINGS"
    export MD_SISTER_MDEDIT_SETTINGS="$MDEDIT_SETTINGS"
    mkdir -p "$mac_profile"/{viewer,editor}
}

mac_profile_cleanup() {
    [ -z "${mac_profile:-}" ] || rm -rf "$mac_profile"
}

mac_open() {
    : "${mac_profile:?initialize a scratch profile before using LaunchServices}"
    command open --env "HOME=$HOME" --env "MDVIEW_SETTINGS=$MDVIEW_SETTINGS" \
        --env "MDEDIT_SETTINGS=$MDEDIT_SETTINGS" \
        --env "MD_SISTER_MDVIEW_SETTINGS=$MD_SISTER_MDVIEW_SETTINGS" \
        --env "MD_SISTER_MDEDIT_SETTINGS=$MD_SISTER_MDEDIT_SETTINGS" "$@"
}

mac_profile_check_process() {
    local listing
    listing="$(ps eww -p "$1" -o command=)" || return
    if [[ "$listing" != *" HOME=$HOME"* ||
          "$listing" != *" MDVIEW_SETTINGS=$MDVIEW_SETTINGS"* ||
          "$listing" != *" MDEDIT_SETTINGS=$MDEDIT_SETTINGS"* ]]; then
        echo "FAIL app did not receive the scratch home and settings" >&2
        return 1
    fi
}

# The toolkit restarts via absolute /usr/bin/open; shell environment alone is not enough.
# Apply this only to scratch bundle copies, then sign the changed copies.
mac_profile_bundle() {
    : "${mac_profile:?initialize a scratch profile before changing a test bundle}"
    python3 - "$1/Contents/Info.plist" "${2:-}" <<'PY'
import os, plistlib, sys

path, bundle_id = sys.argv[1:]
with open(path, "rb") as file:
    info = plistlib.load(file)
env = info.setdefault("LSEnvironment", {})
for name in ("HOME", "MDVIEW_SETTINGS", "MDEDIT_SETTINGS",
             "MD_SISTER_MDVIEW_SETTINGS", "MD_SISTER_MDEDIT_SETTINGS"):
    env[name] = os.environ[name]
if bundle_id:
    info["CFBundleIdentifier"] = bundle_id
with open(path, "wb") as file:
    plistlib.dump(info, file)
PY
}
