#!/usr/bin/env bash
# Builds the AppImage inside Ubuntu 24.04 (glibc 2.39), so the result runs on that and anything newer.
# An AppImage carries its own Qt but not glibc, so building on a rolling distro (glibc 2.43 on Arch) makes
# an image that only runs on equally new systems. Needs docker and network (apt, rustup, crates).
#
#   build-in-container.sh TOOLS_DIR OUT_DIR
#     TOOLS_DIR  holds linuxdeploy, linuxdeploy-plugin-qt, linuxdeploy-plugin-appimage (see tools.sha256)
#     OUT_DIR    receives mdview-<version>-x86_64.AppImage, .zsync and SHA256SUMS
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
tools="$(realpath "${1:?usage: build-in-container.sh TOOLS_DIR OUT_DIR}")"
out="$(realpath -m "${2:?usage: build-in-container.sh TOOLS_DIR OUT_DIR}")"
mkdir -p "$out"
# lld matters: cxx-qt picks the linker by looking for `lld`, and falls back to gold, which drops its init symbols.
# Optional limits for a busy machine: DOCKER_LIMITS="--cpus=1 --cpu-shares=2" JOBS=1
docker run --rm ${DOCKER_LIMITS:-} \
  -e JOBS="${JOBS:-4}" \
  -v "$root":/src:ro -v "$tools":/tools:ro -v "$out":/out \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  ubuntu:24.04 bash -euo pipefail -c '
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends \
      build-essential lld pkg-config curl ca-certificates git file zsync desktop-file-utils patchelf \
      qt6-base-dev qt6-base-dev-tools qt6-svg-dev qt6-wayland libqt6svg6 \
      libgl1-mesa-dev libvulkan-dev libxkbcommon-dev libxkbcommon-x11-0 libfontconfig1-dev libfreetype-dev \
      libxcb-cursor0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 libxcb-render-util0 libxcb-xkb1 \
      libxcb-shape0 libxcb-xinerama0 libxcb-randr0 libxcb-sync1 libxcb-xfixes0 libdbus-1-3 >/dev/null
    curl -sSf --proto =https https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null
    . "$HOME/.cargo/env"
    mkdir -p /work && cp -a /src/. /work/ && cd /work
    rm -rf target bins/mdv-qt/target
    export CXXFLAGS=-O2 CARGO_BUILD_JOBS="$JOBS"
    export PATH="/tools:$PATH" QMAKE=/usr/lib/qt6/bin/qmake6 NO_STRIP=1 APPIMAGE_EXTRACT_AND_RUN=1
    packaging/appimage/build-appimage.sh --out /out
    chown -R "$HOST_UID:$HOST_GID" /out
  '
