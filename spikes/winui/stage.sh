#!/usr/bin/env bash
# Usage: stage.sh framework|self-contained. Builds stage/<mode>/ ready to copy to Windows.
# Runtime files come from the Runtime NuGet's MSIX, unpacked as data; nothing here executes them.
set -euo pipefail
cd "$(dirname "$0")"
mode=${1:?framework or self-contained}
out=stage/$mode
mkdir -p "$out"
cp "target/$mode/x86_64-pc-windows-msvc/release/mdview-winui.exe" "$out/"
cp measure.ps1 W0.md "$out/" 2>/dev/null || true
[ "$mode" = framework ] && exit 0
msix=.cache/msix-x64
if [ ! -d "$msix" ]; then
    mkdir -p "$msix"
    unzip -qo .cache/nuget264/runtime/tools/MSIX/win10-x64/Microsoft.WindowsAppRuntime.2.msix -d "$msix"
fi
# assets/runtime.txt is case-insensitive names from windows-rs reactor-setup.
for entry in "$msix"/*; do
    name=$(basename "$entry")
    if grep -qix -- "$name" <(tr -d '\r' < assets/runtime.txt); then
        cp -r "$entry" "$out/"
    fi
done
du -sh "$out"
