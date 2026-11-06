#!/usr/bin/env bash
# Cross-builds both deployment flavours. Run from anywhere.
set -euo pipefail
cd "$(dirname "$0")"
export RUSTUP_HOME=$HOME/.local/rust-win/rustup CARGO_HOME=$HOME/.local/rust-win/cargo
export PATH=$HOME/.local/rust-win/cargo/bin:$PATH XWIN_ACCEPT_LICENSE=true
export XWIN_CACHE_DIR=$PWD/.cache/xwin
T=x86_64-pc-windows-msvc
cargo xwin build -j 6 --release --target $T --target-dir target/framework
cargo xwin build -j 6 --release --target $T --target-dir target/self-contained --features self-contained
