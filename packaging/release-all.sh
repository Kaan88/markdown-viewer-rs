#!/usr/bin/env bash
# Builds every file of an mdview release on Linux, the Windows and Linux files together, and with
# --publish creates the GitHub release. Everything is written to target/release-files-all/:
#
#   mdview-<version>-linux-x86_64.AppImage  packaging/linux/build-packages.sh (both binaries inside; it also
#   mdview-<version>-linux-x86_64.tar.gz    makes the .tar.gz, .deb, .rpm and PKGBUILD below)
#   mdview_<version>_amd64.deb, mdview-<version>-1.x86_64.rpm, PKGBUILD
#   mdview.exe, mdview-engine.exe           packaging/windows-cross/build.sh (cross-built)
#   mdview-setup-<version>.exe              packaging/windows-cross/build.sh (ISCC under Wine)
#   mdview-update.txt                       packaging/write-manifest.sh, both platforms
#
#   packaging/release-all.sh --notes 'One short sentence.'                        build and check only
#   packaging/release-all.sh --notes '...' --notes-file FILE --publish            also publish
#   packaging/release-all.sh --notes '...' --notes-file FILE --publish --dry-run  print the gh command
#
# --notes is the manifest's one line (what the in-app update bar shows; optional); --notes-file
# holds the GitHub release text. It refuses a dirty tree (changes under docs/ do not count) and a
# HEAD that no remote branch contains. The release is created on the current commit as
# `v<version>`, marked latest, with all the assets (the Linux packages included;
# a release from the CI workflow is the one meant for users, see docs/CI.md).
set -euo pipefail

app=mdview
slug=Kaan88/markdown-viewer-rs
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
notes="" notes_file="" publish=0 dry=0
while [ $# -gt 0 ]; do
    case "$1" in
        --notes) notes="$2"; shift 2 ;;
        --notes-file) notes_file="$2"; shift 2 ;;
        --publish) publish=1; shift ;;
        --dry-run) dry=1; shift ;;
        *) echo "unknown argument $1" >&2; exit 2 ;;
    esac
done
cd "$root"
fail() { echo "release-all: $*" >&2; exit 1; }
if [ "$publish" = 1 ]; then
    [ -n "$notes_file" ] && [ -f "$notes_file" ] || fail "--publish needs --notes-file FILE (the release text)"
fi

version="$(awk '/^\[workspace\.package\]/{p=1;next} /^\[/{p=0} p&&/^version[ ]*=/{gsub(/[" ]/,"",$3);print $3;exit}' Cargo.toml)"
[ -n "$version" ] || fail "cannot read the version from Cargo.toml"

# A clean tree and a pushed HEAD.
dirty="$(git status --porcelain -- . ':(exclude)docs')"
[ -z "$dirty" ] || fail "working tree is dirty:
$dirty"
sha="$(git rev-parse HEAD)"
git fetch --quiet origin
[ -n "$(git branch -r --contains "$sha")" ] || fail "HEAD $sha is on no remote branch; push it first"

all="$root/target/release-files-all"
rm -rf "$all"
mkdir -p "$all"

packaging/linux/build-packages.sh
packaging/windows-cross/build.sh ${notes:+--notes "$notes"}
cp "target/dist-linux/$app-$version-linux-x86_64.AppImage" "target/dist-linux/$app-$version-linux-x86_64.tar.gz" \
   "target/dist-linux/${app}_${version}_amd64.deb" "target/dist-linux/$app-$version-1.x86_64.rpm" target/dist-linux/PKGBUILD \
   target/dist-cross/mdview.exe target/dist-cross/mdview-engine.exe \
   "target/dist-cross/$app-setup-$version.exe" "$all/"
packaging/write-manifest.sh --version "$version" ${notes:+--notes "$notes"} --out "$all" \
    --windows-exe "$all/mdview.exe" --windows-engine "$all/mdview-engine.exe" \
    --linux "$all/$app-$version-linux-x86_64.AppImage"

echo
echo "release files for $app $version (commit $sha) in $all:"
( cd "$all" && for f in *; do printf '%12d  %s  %s\n' "$(stat -c %s "$f")" "$(sha256sum "$f" | cut -d' ' -f1)" "$f"; done )

if [ "$publish" = 0 ]; then
    echo "not published (no --publish)."
    exit 0
fi
cmd=(gh release create "v$version" "$all/$app-$version-linux-x86_64.AppImage" "$all/mdview.exe"
     "$all/mdview-engine.exe" "$all/$app-setup-$version.exe" "$all/$app-update.txt" "$all/$app-$version-linux-x86_64.tar.gz"
     "$all/${app}_${version}_amd64.deb" "$all/$app-$version-1.x86_64.rpm" "$all/PKGBUILD"
     --repo "$slug" --target "$sha" --title "$app $version" --notes-file "$notes_file" --latest)
if [ "$dry" = 1 ]; then
    printf 'dry run, would run:'; printf ' %q' "${cmd[@]}"; echo
    exit 0
fi
"${cmd[@]}"
echo "published https://github.com/$slug/releases/tag/v$version"
