# Source from a harness, then call scratch_profile <directory>. The caller owns cleanup.
scratch_profile() {
    local profile="$1" home
    mkdir -p "$profile"/{home,data,config,state}
    home="$(cd "$profile/home" && pwd)"
    unset MDVIEW_SETTINGS MDEDIT_SETTINGS MDV_ENGINE
    if command -v cygpath >/dev/null; then
        export LOCALAPPDATA="$(cygpath -w "$profile/data")"
    else
        export HOME="$home"
        export XDG_DATA_HOME="$profile/data" XDG_CONFIG_HOME="$profile/config"
        export XDG_STATE_HOME="$profile/state"
    fi
}
