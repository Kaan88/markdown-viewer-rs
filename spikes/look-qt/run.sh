#!/bin/sh
# Opens a live window on your desktop. Stays until you close it. Extra arguments pass
# through, for example: ./run.sh --theme=light   ./run.sh --find-open
cd "$(dirname "$0")" || exit 1
[ -x target/release/look-qt ] || /usr/bin/cargo build --release -j 6 || exit 1
exec target/release/look-qt "$@"
