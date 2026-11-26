#!/usr/bin/env bash
# The whole self-update on macOS, on a runner: an installed copy of the bundle is offered version
# 99.0.0 by a manifest served over HTTPS from this machine (a throwaway certificate the system is
# told to trust), installs it with `--install-update` (what the update bar's Update and Restart
# buttons do), and the new copy must be in place, the old one set aside, and the new one started.
#
#   tests/mac-update.sh <out-dir> <app-bundle>
set -euo pipefail
[ "${GITHUB_ACTIONS:-}" = true ] || { echo "FAIL this test requires an ephemeral macOS CI runner"; exit 1; }
out="$1"
src="$2"
name="$(basename "$src" .app)"
upper="$(echo "$name" | tr '[:lower:]' '[:upper:]')"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
mkdir "$out/apps" "$out/new" "$out/srv"
port=8443
fail() { echo "FAIL $*"; exit 1; }
source "$(dirname "$0")/mac-profile.sh"
cleanup() {
    [ -z "${pid:-}" ] || kill "$pid" 2>/dev/null || true
    [ -z "${server:-}" ] || kill "$server" 2>/dev/null || true
    if [ -n "${cert_hash:-}" ]; then
        sudo security delete-certificate -Z "$cert_hash" /Library/Keychains/System.keychain ||
            echo "could not remove the temporary test certificate" >&2
    fi
    mac_profile_cleanup
}
trap cleanup EXIT
mac_profile_init

# The installed copy, in a folder this user can write (as /Applications is), and the newer copy:
# the same program with a marker, signed again (a changed bundle must be).
ditto "$src" "$out/apps/$name.app"
ditto "$src" "$out/new/$name.app"
bundle_id="io.github.kaan88.$name.update-test.$$"
mac_profile_bundle "$out/apps/$name.app" "$bundle_id"
mac_profile_bundle "$out/new/$name.app" "$bundle_id"
codesign --force --sign - "$out/apps/$name.app"
echo "the update" > "$out/new/$name.app/Contents/Resources/update-marker"
codesign --force --sign - "$out/new/$name.app"
(cd "$out/new" && ditto -c -k --keepParent "$name.app" "$out/srv/$name-new.zip")
case "$(uname -m)" in arm64) key=macos-aarch64 ;; *) key=macos-x86_64 ;; esac
sha="$(shasum -a 256 "$out/srv/$name-new.zip" | cut -d' ' -f1)"
cat > "$out/srv/update.txt" <<EOF
manifest: 1
version: 99.0.0
notes: The update test.
$key.asset: $name-new.zip
$key.url: https://localhost:$port/$name-new.zip
$key.sha256: $sha
EOF

# A certificate for localhost the system trusts, and a server.
openssl=openssl
for p in /opt/homebrew/opt/openssl@3/bin/openssl /usr/local/opt/openssl@3/bin/openssl; do
    [ -x "$p" ] && openssl="$p" && break
done
"$openssl" req -x509 -newkey rsa:2048 -nodes -days 2 -keyout "$out/key.pem" -out "$out/cert.pem" \
    -subj /CN=localhost -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" \
    -addext "extendedKeyUsage=serverAuth" -addext "basicConstraints=critical,CA:true" 2>/dev/null
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain "$out/cert.pem"
cert_hash="$("$openssl" x509 -in "$out/cert.pem" -noout -fingerprint -sha1 | cut -d= -f2 | tr -d :)"
cat > "$out/serve.py" <<'PY'
import http.server, ssl, sys
root, cert, key, port = sys.argv[1:5]
class Quiet(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k):
        super().__init__(*a, directory=root, **k)
srv = http.server.ThreadingHTTPServer(("127.0.0.1", int(port)), Quiet)
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.load_cert_chain(cert, key)
srv.socket = ctx.wrap_socket(srv.socket, server_side=True)
srv.serve_forever()
PY
python3 "$out/serve.py" "$out/srv" "$out/cert.pem" "$out/key.pem" "$port" > "$out/server.log" 2>&1 &
server=$!
for _ in $(seq 1 40); do curl -s -o /dev/null "https://localhost:$port/update.txt" && break; sleep 0.25; done
curl -sS "https://localhost:$port/update.txt" > /dev/null || fail "the system does not trust the test server"

# The installation, from the installed copy.
exe="$out/apps/$name.app/Contents/MacOS/$name"
code=0
env "${upper}_UPDATE_MANIFEST=https://localhost:$port/update.txt" "$exe" --install-update \
    > "$out/install.log" 2>&1 || code=$?
cat "$out/install.log"
[ "$code" = 0 ] || fail "--install-update exited with $code"
[ -f "$out/apps/$name.app/Contents/Resources/update-marker" ] || fail "the bundle was not replaced"
[ -d "$out/apps/$name.app.old" ] || fail "the old copy was not set aside"
[ ! -e "$out/apps/$name.app.new" ] || fail "the staged copy was left behind"
codesign --verify --deep --strict "$out/apps/$name.app" || fail "the new bundle's signature does not hold"
for f in "$out/apps/$name.app/Contents/MacOS/"*; do
    [ -x "$f" ] || fail "$f lost its executable bit"
done
echo "PASS the update replaced the bundle and set the old one aside"

# The restart: a new process of the installed bundle, started through LaunchServices.
pid=""
for _ in $(seq 1 60); do
    pid="$(pgrep -n -f "^$exe( |\$)" || true)"
    [ -n "$pid" ] && break
    sleep 0.25
done
[ -n "$pid" ] || fail "the new version was not started"
mac_profile_check_process "$pid" || fail "the restarted viewer is not isolated"
echo "PASS the new version started ($(ps -o args= -p "$pid"))"
sleep 3
kill "$pid" 2>/dev/null || true
