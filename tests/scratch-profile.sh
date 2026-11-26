# Source from a harness, then call scratch_profile <directory>. The caller owns cleanup.
windows_host() {
    case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) return 0;; *) return 1;; esac
}

scratch_profile() {
    local profile="$1" home
    mkdir -p "$profile"/{home,data,config,state}
    home="$(cd "$profile/home" && pwd)"
    unset MDVIEW_SETTINGS MDEDIT_SETTINGS MDV_ENGINE
    if windows_host; then
        local appdata
        appdata="$(cygpath -w "$profile/data")" && [[ -n "$appdata" ]] || {
            echo "scratch profile: cannot set Windows LOCALAPPDATA" >&2
            return 2
        }
        export LOCALAPPDATA="$appdata"
    else
        export HOME="$home"
        export XDG_DATA_HOME="$profile/data" XDG_CONFIG_HOME="$profile/config"
        export XDG_STATE_HOME="$profile/state"
    fi
}
