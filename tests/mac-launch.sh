# Sourced by the mac-*.sh tests. launch <app-bundle-or-exe> [args...]: starts the application
# the way the Mac starts one, through LaunchServices (`open`), so it is a regular app and comes to
# the front; a bare executable is started directly. Sets $pid. stop: ends it.
source "$(dirname "${BASH_SOURCE[0]}")/mac-profile.sh"

launch() {
    : "${mac_profile:?initialize a scratch profile before launching}"
    local target="$1"
    shift
    if [[ "$target" == *.app ]]; then
        local bundle name exe
        bundle="$(cd "$target" && pwd -P)" || return
        name="$(defaults read "$bundle/Contents/Info" CFBundleExecutable)" || return
        exe="$bundle/Contents/MacOS/$name"
        if pgrep -f "^$exe( |\$)" >/dev/null; then
            echo "FAIL refusing to reuse an already-running test bundle" >&2
            return 1
        fi
        mac_open -n "$target" --stdout "$out/app.log" --stderr "$out/app.log" --args "$@" || return
        pid=""
        for _ in $(seq 1 40); do
            pid="$(pgrep -n -f "^$exe( |\$)" || true)"
            [ -n "$pid" ] && break
            sleep 0.25
        done
    else
        "$target" "$@" > "$out/app.log" 2>&1 &
        pid=$!
    fi
    echo "started $target as ${pid:-nothing}"
    [ -n "${pid:-}" ] && mac_profile_check_process "$pid"
}

# The window server's id of the process's first normal window, once it has one (20 s at most).
window_id() {
    local wid=""
    for _ in $(seq 1 40); do
        wid=$(swift -e 'import CoreGraphics; let pid = Int32(CommandLine.arguments[1])!; let l = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]; for w in l where (w[kCGWindowOwnerPID as String] as? Int32) == pid && (w[kCGWindowLayer as String] as? Int) == 0 { print(w[kCGWindowNumber as String]!); break }' "$pid" 2>/dev/null)
        [ -n "$wid" ] && break
        sleep 0.5
    done
    echo "$wid"
}

stop() {
    kill "$pid" 2>/dev/null
    for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.25; done
    echo "--- app log"
    cat "$out/app.log" 2>/dev/null
}
