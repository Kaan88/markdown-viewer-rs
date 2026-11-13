#!/usr/bin/env bash
# Builds the Windows release files of mdview on Linux: the same files
# packaging\windows\build-installer.ps1 puts in target\release-files.
#
#   packaging/windows-cross/build.sh [--notes 'One line.'] [--no-installer]
#
# Output in target/dist-cross/:
#   mdview.exe, mdview-engine.exe   built with +crt-static, as the PowerShell script does
#   mdview-setup-<version>.exe      the Inno Setup installer (ISCC.exe run under Wine)
#   mdview-update.txt               the manifest, Windows entry only (release-all.sh writes the one
#                                   with both platforms)
#
# The exes are cross-compiled in packaging/windows-cross/Dockerfile with cargo-xwin, which
# downloads the MSVC C runtime and the Windows SDK from Microsoft on its first run (cached under
# $MD_BUILD_CACHE/xwin). USING THEM MEANS ACCEPTING MICROSOFT'S LICENCE
# (https://go.microsoft.com/fwlink/?LinkId=2086102). cargo-xwin 0.23.1 has no accept flag or
# variable of its own: it accepts for you when it downloads, so running this script is that
# acceptance.
#
# Caches (default ~/.cache/md-build, override with MD_BUILD_CACHE), all safe to delete:
#   mdview/cargo        cargo registry and git checkouts
#   mdview/target-win   cargo's target directory for the Windows build
#   xwin                the downloaded CRT and SDK
#   innosetup           a copy of the Inno Setup folder (from $INNO_SETUP_DIR, default the one on the
#                       mounted Windows drive, "/mnt/C/Program Files/Inno Setup 7")
#   wine-prefix         the WINEPREFIX ISCC.exe runs in
# Needs on the host: docker, wine (for the installer), sha256sum.
set -euo pipefail

app=mdview
image=mdview-windows-cross
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cache="${MD_BUILD_CACHE:-$HOME/.cache/md-build}"
notes=""
installer=1
while [ $# -gt 0 ]; do
    case "$1" in
        --notes) notes="$2"; shift 2 ;;
        --no-installer) installer=0; shift ;;
        *) echo "unknown argument $1" >&2; exit 2 ;;
    esac
done
cd "$root"

version="$(awk '/^\[workspace\.package\]/{p=1;next} /^\[/{p=0} p&&/^version[ ]*=/{gsub(/[" ]/,"",$3);print $3;exit}' Cargo.toml)"
[ -n "$version" ] || { echo "cannot read the version from Cargo.toml" >&2; exit 1; }
dist="$root/target/dist-cross"
mkdir -p "$cache/$app/cargo" "$cache/$app/target-win" "$cache/xwin" "$dist"

# 1. The executables. +crt-static, so there is no VC++ redistributable to ship.
docker build -q -t "$image" packaging/windows-cross >/dev/null
docker run --rm --user "$(id -u):$(id -g)" \
    -v "$root:/src:ro" \
    -v "$cache/$app/cargo:/cache/cargo" \
    -v "$cache/$app/target-win:/cache/target" \
    -v "$cache/xwin:/cache/xwin" \
    -e CARGO_HOME=/cache/cargo -e CARGO_TARGET_DIR=/cache/target -e XWIN_CACHE_DIR=/cache/xwin \
    -e MD_LLVM_RC=/usr/bin/llvm-rc -e CARGO_TERM_COLOR=never \
    -e RUSTFLAGS='-C target-feature=+crt-static' \
    -w /src "$image" \
    bash -c 'PATH=/opt/cargo/bin:$PATH cargo xwin build --release --locked --target x86_64-pc-windows-msvc -p mdview -p mdview-engine'
out="$cache/$app/target-win/x86_64-pc-windows-msvc/release"
for f in mdview.exe mdview-engine.exe; do
    [ -f "$out/$f" ] || { echo "build left no $out/$f" >&2; exit 1; }
done
rm -f "$dist/mdview.exe" "$dist/mdview-engine.exe" "$dist/mdview-setup-$version.exe" "$dist/mdview-update.txt"
cp "$out/mdview.exe" "$out/mdview-engine.exe" "$dist/"

# 2. The installer: ISCC.exe under Wine, on the same .iss the PowerShell script uses. The script
# names its inputs relative to itself (..\..\target\dist\release\*.exe), so a scratch tree with the
# same layout holds the cross-built exes where the script looks for them.
if [ "$installer" = 1 ]; then
    iscc_dir="$cache/innosetup"
    if [ ! -f "$iscc_dir/ISCC.exe" ]; then
        src="${INNO_SETUP_DIR:-/mnt/C/Program Files/Inno Setup 7}"
        [ -f "$src/ISCC.exe" ] || { echo "no Inno Setup at $src (set INNO_SETUP_DIR)" >&2; exit 1; }
        mkdir -p "$iscc_dir"
        cp -r "$src/." "$iscc_dir/"
    fi
    stage="$root/target/cross-installer"
    rm -rf "$stage"
    mkdir -p "$stage/packaging/windows" "$stage/assets" "$stage/target/dist/release"
    cp packaging/windows/mdview.iss "$stage/packaging/windows/"
    cp assets/mdview.ico "$stage/assets/"
    cp "$out/mdview.exe" "$out/mdview-engine.exe" "$stage/target/dist/release/"
    export WINEPREFIX="$cache/wine-prefix" WINEARCH=win64 WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
    wine "$iscc_dir/ISCC.exe" "/DAppVersion=$version" "$(winepath -w "$stage/packaging/windows/mdview.iss" 2>/dev/null)" >/dev/null
    cp "$stage/target/installer/mdview-setup-$version.exe" "$dist/"
fi

# 3. The manifest, Windows entry only.
args=(--version "$version" --out "$dist" --windows-exe "$dist/mdview.exe" --windows-engine "$dist/mdview-engine.exe")
[ -z "$notes" ] || args+=(--notes "$notes")
packaging/write-manifest.sh "${args[@]}"

files=(mdview.exe mdview-engine.exe mdview-update.txt)
[ "$installer" = 1 ] && files+=("mdview-setup-$version.exe")
( cd "$dist" && ls -l "${files[@]}" && sha256sum "${files[@]}" )
