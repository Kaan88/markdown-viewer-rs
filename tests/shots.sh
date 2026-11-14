#!/usr/bin/env bash
# Renders mdview's screenshot set with --shot into the directory given as the only argument.
# The no-pixel-change check for a refactor or a pin move: run it before and after into two
# directories and compare, every PNG must be byte-identical.
#
#   tests/shots.sh target/shots/base      # before the change
#   tests/shots.sh target/shots/final     # after it
#   for f in target/shots/base/*.png; do cmp "$f" target/shots/final/$(basename "$f"); done
#
# Needs a release build (cargo build --release -p mdview). Every shot uses --no-session, so the
# user's session is neither read nor written, and none loads remote images, so none needs the
# network.

set -euo pipefail

out="${1:?usage: tests/shots.sh <output-dir>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
exe="${MDVIEW_EXE:-target/release/mdview.exe}"
[ -x "$exe" ] || exe="target/release/mdview"
fx="fixtures"
mkdir -p "$out"

shot() {
    local name="$1"
    shift
    # The scale is pinned, so the system's display scale cannot change the pictures; a shot
    # that asks for its own --scale comes later on the line and wins.
    "$exe" --no-session --scale 1.0 --shot "$out/$name.png" "$@"
}

# Both themes.
shot showcase-light --light --size 1280x800 "$fx/showcase.md"
shot showcase-dark --dark --size 1280x800 "$fx/showcase.md"
shot everything-light --light --size 1280x800 "$fx/everything.md"
shot everything-dark --dark --size 1280x800 "$fx/everything.md"
shot tables-light --light "$fx/tables.md"
shot math-dark --dark "$fx/math.md"
shot diagrams-light --light --size 1280x1000 "$fx/diagrams.md"
shot html-extras-dark --dark "$fx/html-extras.md"
shot floats-light --light "$fx/floats.md"
shot formats-light --light "$fx/formats.md"
shot images-dark --dark "$fx/images.md"
shot scrolled --light --scroll 900 "$fx/showcase.md"

# The empty state.
shot empty-light --light
shot empty-dark --dark

# Every menu, in both themes where it matters; `cascade` is Open recent beside the main menu.
shot menu-main-light --light --menu main "$fx/showcase.md"
shot menu-main-dark --dark --menu main "$fx/showcase.md"
shot menu-recent --light --menu recent "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot menu-cascade-light --light --menu cascade "$fx/showcase.md" "$fx/tables.md"
shot menu-cascade-dark --dark --menu cascade "$fx/showcase.md" "$fx/tables.md"
shot menu-tab --dark --menu tab "$fx/showcase.md" "$fx/tables.md"
shot menu-page --light --menu page "$fx/showcase.md"
shot menu-find --light --find the --menu find "$fx/showcase.md"

# Every dialog.
shot dialog-about-dark --dark --dialog about "$fx/showcase.md"
shot dialog-about-light --light --dialog about "$fx/showcase.md"
shot dialog-shortcuts --light --dialog shortcuts "$fx/showcase.md"
shot dialog-recent --dark --dialog recent "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot dialog-edit --light --dialog edit "$fx/showcase.md"

shot dialog-settings-light --light --dialog settings "$fx/showcase.md"
shot dialog-settings-dark --dark --dialog settings "$fx/showcase.md"
shot dialog-settings-scale-150 --light --scale 1.5 --dialog settings "$fx/showcase.md"
shot dialog-settings-760 --dark --size 760x800 --dialog settings "$fx/showcase.md"
shot dialog-settings-640 --light --size 640x800 --dialog settings "$fx/showcase.md"

# The command palette: tabs and recent files, commands by prefix, headings by prefix.
shot palette-empty-light --light --palette "" "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot palette-empty-dark --dark --palette "" "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot palette-commands-light --light --palette ">zoom" "$fx/showcase.md"
shot palette-commands-dark --dark --palette ">zoom" "$fx/showcase.md"
shot palette-headings-light --light --palette "#" "$fx/showcase.md"
shot palette-headings-dark --dark --palette "#" "$fx/showcase.md"

# Panes, find, focus mode, tabs.
shot files-pane --light --files --folder "$fx" "$fx/showcase.md"
shot files-pane-dark --dark --files --folder "$fx/wiki" "$fx/linked.md"
shot outline-pane --light --outline "$fx/showcase.md"
shot outline-pane-dark --dark --outline "$fx/everything.md"
shot both-panes --light --files --outline --folder "$fx" "$fx/showcase.md"
shot find-light --light --find the "$fx/showcase.md"
shot find-dark --dark --find table "$fx/tables.md"
shot find-none --light --find zzzqqq "$fx/showcase.md"
shot focus-mode --light --focus-mode "$fx/showcase.md"
shot focus-mode-dark --dark --focus-mode "$fx/everything.md"
shot tabs --dark "$fx/showcase.md" "$fx/tables.md" "$fx/math.md" "$fx/formats.md" "$fx/floats.md"

# Page widths, window sizes, zoom.
shot width-narrow --light --page-width narrow "$fx/showcase.md"
shot width-wide --light --page-width wide "$fx/showcase.md"
shot width-full --dark --page-width full "$fx/showcase.md"
shot small --light --size 700x600 "$fx/showcase.md"
shot zoom-150 --light --zoom 150 "$fx/showcase.md"

# Device scales.
shot scale-100 --light --scale 1.0 "$fx/showcase.md"
shot scale-150 --light --scale 1.5 "$fx/showcase.md"
shot scale-150-menu --dark --scale 1.5 --menu cascade "$fx/showcase.md" "$fx/tables.md"
shot scale-200 --light --scale 2.0 "$fx/showcase.md"
shot scale-200-dark --dark --scale 2.0 --focus-mode "$fx/showcase.md"

# Pointer positions: a title-bar button, a link in the text, a menu row, the page.
shot hover-titlebar --light --hover 1200,16 "$fx/showcase.md"
shot hover-open --light --hover 46,63 "$fx/showcase.md"
shot hover-page --light --hover 640,300 "$fx/showcase.md"
shot hover-menu --light --menu main --hover 1100,120 "$fx/showcase.md"

# The update bar.
shot update-available --light --update-bar available "$fx/showcase.md"
shot update-installing --dark --update-bar installing "$fx/showcase.md"
shot update-installed --dark --update-bar installed

# A menu taller than its window: kept below the title row, scrolling, and scrolled to its end.
shot menu-tall-main --light --size 1000x520 --menu main
shot menu-tall-main-end --light --size 1000x520 --menu main-end

# A page font. Only where the family is installed (fc-list lists it, so Windows skips these): a
# serif for the text, a monospace for code, with emoji and symbols still coming from the default
# stack, and the settings dialog showing both rows filled.
families="$(fc-list : family 2>/dev/null || true)"
if grep -qx "DejaVu Serif" <<<"$families"; then
    shot page-font-serif --light --page-font "DejaVu Serif" --code-font "DejaVu Sans Mono" "$fx/showcase.md"
    shot page-font-serif-dark --dark --page-font "DejaVu Serif" "$fx/everything.md"
    shot dialog-settings-fonts --light --page-font "DejaVu Serif" --code-font "DejaVu Sans Mono" --dialog settings "$fx/showcase.md"
fi
# An unknown family: the same page as the default, since the default stack stays behind it.
shot page-font-unknown --light --page-font "No Such Family Anywhere" --code-font "Nor This One" "$fx/showcase.md"

ls "$out" | wc -l
