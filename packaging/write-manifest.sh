#!/usr/bin/env bash
# Writes mdview-update.txt for one or both platforms, re-reads it and checks every hash.
#
#   packaging/write-manifest.sh --version 0.2.3 [--notes 'One line.'] --out DIR \
#       [--windows-exe DIR/mdview.exe --windows-engine DIR/mdview-engine.exe] \
#       [--linux DIR/mdview-0.2.3-linux-x86_64.AppImage]
#
# At least one platform; Windows needs both its files. Asset names are the file names given; the
# URLs are where GitHub serves the assets of tag v<version>. The grammar is mdshared-app's update.rs
# (manifest: 1, https only, at most 8 KiB, notes at most 1024 bytes without control characters).
# Windows carries the engine keys (the engine is a second file); Linux carries none, the engine is
# inside the AppImage. The manifest is written to DIR/mdview-update.txt.
set -euo pipefail

slug=Kaan88/markdown-viewer-rs
app=mdview
version="" notes="" out="" wexe="" weng="" lin=""
while [ $# -gt 0 ]; do
    case "$1" in
        --version) version="$2"; shift 2 ;;
        --notes) notes="$2"; shift 2 ;;
        --out) out="$2"; shift 2 ;;
        --windows-exe) wexe="$2"; shift 2 ;;
        --windows-engine) weng="$2"; shift 2 ;;
        --linux) lin="$2"; shift 2 ;;
        *) echo "unknown argument $1" >&2; exit 2 ;;
    esac
done
fail() { echo "write-manifest: $*" >&2; exit 1; }
[ -n "$version" ] && [ -n "$out" ] || fail "--version and --out are required"
[ -n "$wexe$weng$lin" ] || fail "give --windows-exe and --windows-engine, and/or --linux"
{ [ -n "$wexe" ] && [ -n "$weng" ]; } || { [ -z "$wexe$weng" ] || fail "Windows needs --windows-exe and --windows-engine"; }
printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' || fail "bad version $version"
if printf '%s' "$notes" | LC_ALL=C grep -q '[[:cntrl:]]'; then fail "notes: one line, no control characters"; fi
[ "$(printf '%s' "$notes" | wc -c)" -le 1024 ] || fail "notes: at most 1024 bytes"
mkdir -p "$out"

base="https://github.com/$slug/releases/download/v$version"
manifest="$out/$app-update.txt"
sha() { sha256sum "$1" | cut -d' ' -f1; }
name_ok() { printf '%s' "$1" | grep -Eq '^[A-Za-z0-9._-]{1,128}$' || fail "bad asset name $1"; }
for f in "$wexe" "$weng" "$lin"; do
    [ -z "$f" ] || { [ -f "$f" ] || fail "$f does not exist"; name_ok "$(basename "$f")"; }
done
{
    echo "manifest: 1"
    echo "version: $version"
    [ -z "$notes" ] || echo "notes: $notes"
    if [ -n "$wexe" ]; then
        echo "windows-x86_64.asset: $(basename "$wexe")"
        echo "windows-x86_64.url: $base/$(basename "$wexe")"
        echo "windows-x86_64.sha256: $(sha "$wexe")"
        echo "windows-x86_64.engine.url: $base/$(basename "$weng")"
        echo "windows-x86_64.engine.sha256: $(sha "$weng")"
    fi
    if [ -n "$lin" ]; then
        echo "linux-x86_64.asset: $(basename "$lin")"
        echo "linux-x86_64.url: $base/$(basename "$lin")"
        echo "linux-x86_64.sha256: $(sha "$lin")"
    fi
} > "$manifest"

# Re-read what was written: header, keys, URLs, and every hash against the file it names.
[ "$(head -1 "$manifest")" = "manifest: 1" ] || fail "header wrong"
[ "$(wc -c < "$manifest")" -le 8192 ] || fail "manifest larger than 8 KiB"
[ "$(wc -l < "$manifest")" -le 64 ] || fail "manifest has more than 64 lines"
get() { grep -m1 "^$1: " "$manifest" | sed "s/^$1: //"; }
[ "$(get version)" = "$version" ] || fail "version does not match"
[ -z "$notes" ] || [ "$(get notes)" = "$notes" ] || fail "notes do not match"
checked=0
check() { # key file: the URL and hash lines of one file
    [ "$(get "$1.url")" = "$base/$(basename "$2")" ] || fail "$1.url does not match"
    [ "$(get "$1.sha256")" = "$(sha "$2")" ] || fail "$1.sha256 does not match $2"
    checked=$((checked + 1))
}
if [ -n "$wexe" ]; then
    [ "$(get windows-x86_64.asset)" = "$(basename "$wexe")" ] || fail "windows-x86_64.asset does not match"
    check windows-x86_64 "$wexe"
    check windows-x86_64.engine "$weng"
fi
if [ -n "$lin" ]; then
    [ "$(get linux-x86_64.asset)" = "$(basename "$lin")" ] || fail "linux-x86_64.asset does not match"
    check linux-x86_64 "$lin"
fi
unknown="$(grep -Ev '^(manifest|version|notes|(windows|linux)-x86_64\.(asset|url|sha256)|windows-x86_64\.engine\.(url|sha256)): ' "$manifest" || true)"
[ -z "$unknown" ] || fail "unexpected lines: $unknown"
[ -z "$lin" ] || ! grep -q '^linux-x86_64\.engine' "$manifest" || fail "Linux must not carry engine keys"
echo "wrote $manifest ($(wc -c < "$manifest") bytes, $checked file hash(es) checked)"
