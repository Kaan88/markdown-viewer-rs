#!/usr/bin/env bash
# Verify the installed layout and exercise real sibling-engine discovery through the public link.
set -euo pipefail
archive="${1:?usage: bash tests/linux-tarball-smoke.sh <mdview tar.gz>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
source "$root/tests/scratch-profile.sh"
scratch_profile "$scratch/profile"
tar -xzf "$archive" -C "$scratch"
tree="$scratch/$(basename "$archive" .tar.gz)"
prefix="$scratch/prefix"
mkdir -p "$prefix/bin"
printf '%s\n' 'unrelated engine' > "$prefix/bin/mdview-engine"
"$tree/install.sh" "$prefix"
test "$(readlink "$prefix/bin/mdview")" = ../lib/mdview/mdview
test -x "$prefix/lib/mdview/mdview"
test -x "$prefix/lib/mdview/mdview-engine"
grep -qx 'unrelated engine' "$prefix/bin/mdview-engine"
shot() {
    "$prefix/bin/mdview" --behind --no-session --scale 1.0 --light --zoom-diagram 1 \
        --shot "$1" "$root/fixtures/diagrams.md"
}
shot "$scratch/shot.png"
test -s "$scratch/shot.png"

# A successful placeholder shot would make this smoke test meaningless. Prove that the
# same launch fails without its sibling engine, even with an unrelated public file present.
mv "$prefix/lib/mdview/mdview-engine" "$scratch/engine"
status=0
shot "$scratch/missing.png" || status=$?
test "$status" -eq 2
test ! -e "$scratch/missing.png"
mv "$scratch/engine" "$prefix/lib/mdview/mdview-engine"

"$tree/uninstall.sh" "$prefix"
test ! -L "$prefix/bin/mdview"
test ! -e "$prefix/bin/mdview"
test ! -e "$prefix/lib/mdview/mdview"
test ! -e "$prefix/lib/mdview/mdview-engine"
grep -qx 'unrelated engine' "$prefix/bin/mdview-engine"
