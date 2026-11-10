#!/usr/bin/env bash
# Build the mdview AppImage (Linux, x86_64). See README.md in this directory.
#
#   build-appimage.sh [--appdir-only] [--fetch-tools] [--out DIR]
#
#   --appdir-only  build the binaries and assemble the AppDir, then stop before linuxdeploy
#   --fetch-tools  download linuxdeploy + the Qt plugin into ./tools/ (checked against tools.sha256)
#   --out DIR      build output (default: ./out, git-ignored)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$here/out"
appdir_only=0
fetch=0

LD_URL="https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
LDQT_URL="https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-x86_64.AppImage"
LDAI_URL="https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage"
UPDATE_INFO="gh-releases-zsync|Kaan88|markdown-viewer-rs|latest|mdview-*x86_64.AppImage.zsync"

while [ $# -gt 0 ]; do
  case "$1" in
    --appdir-only) appdir_only=1 ;;
    --fetch-tools) fetch=1 ;;
    --out) shift; out="$(realpath -m "${1:?--out needs a directory}")" ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

fetch_tools() {
  mkdir -p "$here/tools"
  local name url want got
  for pair in "linuxdeploy|$LD_URL" "linuxdeploy-plugin-qt|$LDQT_URL" "linuxdeploy-plugin-appimage|$LDAI_URL"; do
    name="${pair%%|*}"; url="${pair#*|}"
    echo "downloading $url"
    curl -fL --proto '=https' -o "$here/tools/$name.part" "$url"
    got="$(sha256sum "$here/tools/$name.part" | cut -d' ' -f1)"
    want="$(awk -v n="$name" '$1 !~ /^#/ && $2 == n {print $1}' "$here/tools.sha256")"
    if [ "$want" != "$got" ]; then
      rm -f "$here/tools/$name.part"
      echo "REFUSING $name: sha256 is $got but tools.sha256 says '${want:-<missing>}'." >&2
      echo "Review the download yourself, then put the hash in tools.sha256 and rerun." >&2
      exit 1
    fi
    chmod +x "$here/tools/$name.part"
    mv "$here/tools/$name.part" "$here/tools/$name"
  done
}

[ "$fetch" = 1 ] && fetch_tools
[ -d "$here/tools" ] && PATH="$here/tools:$PATH"

if [ "$appdir_only" = 0 ]; then
  missing=0
  for t in linuxdeploy linuxdeploy-plugin-qt linuxdeploy-plugin-appimage; do
    command -v "$t" >/dev/null || missing=1
  done
  if [ "$missing" = 1 ]; then
    cat >&2 <<M
linuxdeploy and/or linuxdeploy-plugin-qt are not on PATH. Nothing is downloaded silently.
Official releases:
  $LD_URL
  $LDQT_URL
Download them, verify with:  sha256sum <file>   (compare against a hash you trust; see tools.sha256),
chmod +x them, and put them on PATH as 'linuxdeploy' and 'linuxdeploy-plugin-qt'.
Or run this script with --fetch-tools to download into $here/tools/ (verified against tools.sha256).
Use --appdir-only to assemble the AppDir without them.
M
    exit 1
  fi
fi

version="$(sed -n "s/^pkgver=//p" "$root/packaging/arch/PKGBUILD")"
target="$out/target"
appdir="$out/AppDir"
mkdir -p "$out"

export QMAKE="${QMAKE:-/usr/bin/qmake6}" CARGO_TARGET_DIR="$target"
(cd "$root" && cargo build --release --locked -p mdview-engine)
(cd "$root/bins/mdv-qt" && cargo build --release --locked)

rm -rf "$appdir"
install -Dm755 "$target/release/mdview-qt" "$appdir/usr/bin/mdview"
install -Dm755 "$target/release/mdview-engine" "$appdir/usr/bin/mdview-engine"
install -Dm644 "$here/mdview.desktop" "$appdir/usr/share/applications/mdview.desktop"
install -Dm644 "$here/mdview.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/mdview.svg"
# The viewer looks for mdview-engine beside its own executable (crates/render/src/engine.rs), and
# linuxdeploy leaves usr/bin alone, so no MDV_ENGINE or AppRun tweak is needed.
# QT_QPA_PLATFORM is deliberately not set: the Qt plugin's hook picks xcb/wayland itself.

if [ "$appdir_only" = 1 ]; then
  echo "AppDir ready: $appdir"
  exit 0
fi

cd "$out"
# xcb comes by default. Wayland sessions need libqwayland (Qt 6.12 ships one plugin; older Qt 6 had
# libqwayland-generic.so and libqwayland-egl.so, so adjust the names there), and offscreen lets the scripted checks run the
# finished image without a display.
export EXTRA_PLATFORM_PLUGINS="libqwayland.so;libqoffscreen.so"
export LDAI_UPDATE_INFORMATION="$UPDATE_INFO"
export LDAI_OUTPUT="mdview-$version-x86_64.AppImage"
export LDAI_VERSION="$version"
export APPIMAGE_EXTRACT_AND_RUN="${APPIMAGE_EXTRACT_AND_RUN:-1}"   # no FUSE needed for the tools
# linuxdeploy-plugin-qt deploys every Qt plugin it finds, and distro-added ones (KDE's kimg_* image
# formats) can need libraries that are not installed, which aborts the build. mdview only needs Qt's
# own image formats, so the plugin is pointed at a copy of the plugin tree without kimg_*, through a
# qmake wrapper that reports that tree as QT_INSTALL_PLUGINS.
plugins_src="$("$QMAKE" -query QT_INSTALL_PLUGINS)"
plugins="$out/qt-plugins"
rm -rf "$plugins"; mkdir -p "$plugins"
# Also left out: the PDF image plugin and the on-screen keyboard, which drag in Qt Pdf, Qt Quick and
# Qt Qml (about 60 MB) for features mdview does not have.
for d in "$plugins_src"/*/; do
  name="$(basename "$d")"
  case "$name" in
    imageformats|platforminputcontexts)
      mkdir -p "$plugins/$name"
      for f in "$d"*; do
        case "$(basename "$f")" in kimg_*|libqpdf.so|libqtvirtualkeyboardplugin.so) ;; *) ln -s "$f" "$plugins/$name/";; esac
      done ;;
    *) ln -s "$d" "$plugins/$name" ;;
  esac
done
cat > "$out/qmake-wrapper" <<W
#!/bin/sh
# Same answers as the real qmake, except for where the plugins are.
case "\$*" in
  *"QT_INSTALL_PLUGINS") echo "$plugins" ;;
  *-query*) "$QMAKE" "\$@" | sed "s|^QT_INSTALL_PLUGINS:.*|QT_INSTALL_PLUGINS:$plugins|" ;;
  *) exec "$QMAKE" "\$@" ;;
esac
W
chmod +x "$out/qmake-wrapper"
export QMAKE="$out/qmake-wrapper"
linuxdeploy --appdir "$appdir" \
  --desktop-file "$appdir/usr/share/applications/mdview.desktop" \
  --icon-file "$here/mdview.svg" \
  --custom-apprun "$here/AppRun" \
  --plugin qt --output appimage

image="$out/$LDAI_OUTPUT"
[ -f "$image" ] || { echo "expected $image was not produced" >&2; exit 1; }
# The appimage plugin already writes the .zsync when update info is set; make sure it exists.
if [ ! -f "$image.zsync" ]; then
  zsyncmake -u "$(basename "$image")" -o "$image.zsync" "$image"
fi
sha256sum "$(basename "$image")" "$(basename "$image").zsync" > SHA256SUMS
echo "done:"; ls -l "$image" "$image.zsync" SHA256SUMS
