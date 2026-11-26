# Sourced by the mac-*.sh tests. launch <app-bundle-or-exe> [args...]: starts the application
# the way the Mac starts one, through LaunchServices (`open`), so it is a regular app and comes to
# the front; a bare executable is started directly. Sets $pid. stop: ends it.
source "$(dirname "${BASH_SOURCE[0]}")/mac-profile.sh"
mac_ui_script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/mac-ui.swift"

# Match the literal executable path, not a process name or a path interpreted as regex.
mac_process_ids() {
    local pattern
    pattern="$(printf '%s' "$1" | sed 's/[][\\.^$*+?(){}|]/\\&/g')" || return
    pgrep -f "^$pattern( |\$)"
}

launch() {
    : "${mac_profile:?initialize a scratch profile before launching}"
    local target="$1"
    shift
    if [[ "$target" == *.app ]]; then
        local bundle name exe
        bundle="$(cd "$target" && pwd -P)" || return
        name="$(defaults read "$bundle/Contents/Info" CFBundleExecutable)" || return
        exe="$bundle/Contents/MacOS/$name"
        if mac_process_ids "$exe" >/dev/null; then
            echo "FAIL refusing to reuse an already-running test bundle" >&2
            return 1
        fi
        mac_open -n "$target" --stdout "$out/app.log" --stderr "$out/app.log" --args "$@" || return
        pid=""
        for _ in $(seq 1 40); do
            pid="$(mac_process_ids "$exe" || true)"
            [[ "$pid" =~ ^[1-9][0-9]*$ ]] && break
            sleep 0.25
        done
    else
        "$target" "$@" > "$out/app.log" 2>&1 &
        pid=$!
    fi
    [[ "${pid:-}" =~ ^[1-9][0-9]*$ ]] || return 1
    mac_profile_check_process "$pid" || return
    echo "started scratch PID $pid"
}

# The window server's id of the process's first normal window, once it has one (20 s at most).
window_id() {
    local wid=""
    [[ "${pid:-}" =~ ^[1-9][0-9]*$ ]] || return 1
    for _ in $(seq 1 40); do
        wid=$(swift -e 'import CoreGraphics; let pid = Int32(CommandLine.arguments[1])!; let l = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]; for w in l where (w[kCGWindowOwnerPID as String] as? Int32) == pid && (w[kCGWindowLayer as String] as? Int) == 0 { print(w[kCGWindowNumber as String]!); break }' "$pid" 2>/dev/null)
        [ -n "$wid" ] && break
        sleep 0.5
    done
    [[ "$wid" =~ ^[1-9][0-9]*$ ]] || return 1
    echo "$wid"
}

mac_window_action() {
    local action="$1" exe="$2" title="$3"
    if ! [[ "${pid:-}" =~ ^[1-9][0-9]*$ && "${wid:-}" =~ ^[1-9][0-9]*$ ]]; then
        echo "FAIL no scratch PID/window for $action" >&2
        return 1
    fi
    if [[ "$exe" != "$mac_profile/"* || "$(mac_process_ids "$exe" || true)" != "$pid" ]]; then
        echo "FAIL no unique scratch process for $action" >&2
        return 1
    fi
    mac_profile_check_process "$pid" || return
    swift "$mac_ui_script" "$pid" "$wid" "$exe" "$title" "$action"
}

mac_wait_document() {
    local exe="$1" title="$2"
    for _ in $(seq 1 12); do
        if mac_window_action verify "$exe" "$title" 2>/dev/null; then return 0; fi
        sleep 0.25
    done
    echo "FAIL scratch window did not show $title" >&2
    return 1
}

stop() {
    [[ "${pid:-}" =~ ^[1-9][0-9]*$ ]] && mac_profile_check_process "$pid" || return 0
    kill "$pid" 2>/dev/null
    for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.25; done
    echo "--- app log"
    cat "$out/app.log" 2>/dev/null
}
