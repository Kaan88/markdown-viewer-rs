#!/usr/bin/env bash
# Run the fixture build script with a fake toolchain, not another Cargo build.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
source "$root/tests/scratch-profile.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/toolchain/bin" "$work/out" "$work/shared-build"
native() { if windows_host; then cygpath -w "$1"; else printf '%s\n' "$1"; fi; }
suffix=
if windows_host; then suffix=.exe; fi
cat >"$work/tool.rs" <<'RS'
use std::{env, fs, path::PathBuf};

fn main() {
    let args: Vec<_> = env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--print") {
        println!("{}", env::var("TEST_SYSROOT").unwrap());
        return;
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("nested-build-dir"),
        env::var("CARGO_BUILD_BUILD_DIR").unwrap_or_default(),
    ).unwrap();
    let arg = |name: &str| args.windows(2).find(|v| v[0] == name).unwrap()[1].clone();
    let artifact = PathBuf::from(arg("--target-dir"))
        .join(arg("--target")).join("release")
        .join(format!("mdview-engine{}", env::consts::EXE_SUFFIX));
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    fs::write(artifact, b"fixture, never executed").unwrap();
}
RS
rustc --edition 2024 "$work/tool.rs" -o "$work/compiler$suffix"
cp "$work/compiler$suffix" "$work/toolchain/bin/cargo$suffix"
source="${1:-$root/tests/engine-fixture/build.rs}"
CARGO_MANIFEST_DIR="$(native "$root/tests/engine-fixture")" \
    rustc --edition 2024 "$source" -o "$work/build-script$suffix"
target=x86_64-unknown-linux-gnu
if windows_host; then target=x86_64-pc-windows-msvc; fi
TEST_SYSROOT="$(native "$work/toolchain")" RUSTC="$(native "$work/compiler$suffix")" \
    OUT_DIR="$(native "$work/out")" TARGET="$target" \
    CARGO_BUILD_BUILD_DIR="$(native "$work/shared-build")" "$work/build-script$suffix" >"$work/log"

failed=0
if [ "$(cat "$work/out/nested-build-dir")" != "$(native "$work/out/engine/build")" ]; then
    echo "fixture: nested build directory is not private" >&2
    failed=1
fi
if ! diff -u <(printf '%s\n' build.rs ../../Cargo.toml ../../Cargo.lock ../../bins/mdv-app/Cargo.toml) \
    <(sed -n 's/^cargo:rerun-if-changed=//p' "$work/log" | tr -d '\r'); then
    echo "fixture: expected only manifest-relative rerun paths" >&2
    failed=1
fi
[ "$failed" = 0 ]
echo "fixture: private nested build directory and relative input paths"
