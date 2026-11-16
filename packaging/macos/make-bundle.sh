#!/usr/bin/env bash
# Makes mdview.app from a release build, on a Mac: the executable and the engine in
# Contents/MacOS, Info.plist (the Markdown document types, macOS 12 or later), the icon made from
# assets/mdview.svg, and the ad-hoc signature the system requires of Apple Silicon code (no
# Developer ID: see docs/MACOS.md, decision 2).
#
#   packaging/macos/make-bundle.sh <target-dir> <out-dir>    # e.g. target/release dist
set -euo pipefail
bin="${1:?usage: make-bundle.sh <target-dir> <out-dir>}"
out="${2:?out-dir}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
app="$out/mdview.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin/mdview" "$bin/mdview-engine" "$app/Contents/MacOS/"
"$root/packaging/macos/make-icns.sh" "$root/assets/mdview.svg" "$app/Contents/Resources/mdview.icns"
sed "s/@VERSION@/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
codesign --force --sign - "$app/Contents/MacOS/mdview-engine"
codesign --force --sign - "$app"
echo "$app ($version)"
