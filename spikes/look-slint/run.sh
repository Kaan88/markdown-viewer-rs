#!/bin/bash
# Opens a live window with no timeout. Extra arguments are passed on (--theme=dark, --find).
cd "$(dirname "$0")" || exit 1
[ -x target/release/look-slint ] || /usr/bin/cargo build --release -j 6 || exit 1
exec target/release/look-slint "$@"
