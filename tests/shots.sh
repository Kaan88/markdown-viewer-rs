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
shot emoji-light --light --size 900x700 "$fx/emoji.md"
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
shot menu-main-keys --light --menu main-keys "$fx/showcase.md"
shot menu-recent --light --menu recent "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot menu-cascade-light --light --menu cascade "$fx/showcase.md" "$fx/tables.md"
shot menu-cascade-dark --dark --menu cascade "$fx/showcase.md" "$fx/tables.md"
shot menu-tab --dark --menu tab "$fx/showcase.md" "$fx/tables.md"
shot menu-tab-light --light --menu tab "$fx/showcase.md" "$fx/tables.md"
shot menu-page --light --menu page "$fx/showcase.md"
shot menu-find --light --find the --menu find "$fx/showcase.md"
# The main menu 21 ms into its fade-in, about half there and 3 px above its place (the only
# shots that move: --menu-at runs the motion on a clock set by hand).
shot menu-main-fading-light --light --menu main --menu-at 21 "$fx/showcase.md"
shot menu-main-fading-dark --dark --menu main --menu-at 21 "$fx/showcase.md"

# Toasts: one at rest; one 50 ms into its entrance (part way in, below its place); a second
# one pushed 200 ms in: the first has lifted out of its way (167 ms) and the second is coming
# in under it, since a new toast waits for the lift (the owner's choice).
shot toast-light --light --toast "Copied" "$fx/showcase.md"
shot toast-dark --dark --toast "Copied" "$fx/showcase.md"
shot toast-rising-light --light --toast "Copied" --motion-at 50 "$fx/showcase.md"
shot toast-rising-dark --dark --toast "Copied" --motion-at 50 "$fx/showcase.md"
shot toasts-stacking-light --light --toast "Copied" --toast "Link copied" --motion-at 200 "$fx/showcase.md"
shot toasts-stacking-dark --dark --toast "Copied" --toast "Link copied" --motion-at 200 "$fx/showcase.md"
shot toasts-stacked-light --light --toast "Copied" --toast "Link copied" "$fx/showcase.md"
# Three toasts in a short window: the one that would reach above the page is left out.
shot toasts-short-light --light --size 520x220 --toast "Copied" --toast "Link copied" --toast "Code copied" "$fx/showcase.md"
shot toasts-short-dark --dark --size 520x220 --toast "Copied" --toast "Link copied" --toast "Code copied" "$fx/showcase.md"

# Every dialog.
shot dialog-about-dark --dark --dialog about "$fx/showcase.md"
shot dialog-about-light --light --dialog about "$fx/showcase.md"
shot dialog-shortcuts --light --dialog shortcuts "$fx/showcase.md"
shot dialog-recent --dark --dialog recent "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot dialog-edit --light --dialog edit "$fx/showcase.md"

shot dialog-settings-light --light --dialog settings "$fx/showcase.md"
shot dialog-settings-dark --dark --dialog settings "$fx/showcase.md"
shot dialog-settings-scale-150 --light --scale 1.5 --dialog settings "$fx/showcase.md"
shot dialog-settings-min-dark --dark --size 520x360 --dialog settings "$fx/showcase.md"
shot dialog-settings-640 --light --size 640x800 --dialog settings "$fx/showcase.md"
# The Window backdrop row (offered on Windows) with a choice made. A shot has no window, so
# there is no backdrop to show: the chrome is the flat colours.
shot dialog-settings-backdrop --dark --backdrop mica --dialog settings "$fx/showcase.md"
# A switch of the settings dialog (Reading, "Tick task boxes without asking") turned with Space:
# 67 ms into its slide, the knob part way and its fill coming in; and at rest once turned.
for theme in light dark; do
    shot "settings-switch-sliding-$theme" --"$theme" --dialog settings --flip-setting 5 --motion-at 67 "$fx/showcase.md"
done
shot settings-switch-turned-light --light --dialog settings --flip-setting 5 "$fx/showcase.md"
# The Appearance page with "Animations" turned off, from a scratch settings file, at the
# smallest size in both themes.
still="$(mktemp -d)"
printf '[mdview]
motion=0
' >"$still/settings.ini"
for theme in light dark; do
    MDVIEW_SETTINGS="$still/settings.ini" "$exe" --scale 1.0 --shot "$out/settings-appearance-motion-off-min-$theme.png"         --"$theme" --size 520x360 --dialog settings --settings-section appearance "$fx/showcase.md"
done
# Settings, every page at the default size and the smallest, in both themes. "tabs-on" is the
# Tabs page with the brief-tab rule on (its seconds enabled), read from a scratch settings file
# (a shot reads one named by MDVIEW_SETTINGS and never the user's); "tabs" is the rule off.
brief="$(mktemp -d)"
printf '[mdview]\nskip_brief=1\n' >"$brief/settings.ini"
for page in appearance reading tabs updates tabs-on; do
    section="${page%-on}"
    for theme in light dark; do
        for size in default min; do
            suffix="-$theme"
            dims="1280x800"
            if [ "$size" = min ]; then
                suffix="-min-$theme"
                dims="520x360"
            fi
            args=(--"$theme" --size "$dims" --dialog settings --settings-section "$section" "$fx/showcase.md")
            if [ "$page" = tabs-on ]; then
                MDVIEW_SETTINGS="$brief/settings.ini" "$exe" --scale 1.0 --shot "$out/settings-$page$suffix.png" "${args[@]}"
            else
                shot "settings-$page$suffix" "${args[@]}"
            fi
        done
    done
done
rm -rf "$brief"


# The command palette: tabs and recent files, commands by prefix, headings by prefix.
shot palette-empty-light --light --palette "" "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot palette-empty-dark --dark --palette "" "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
# 41 ms into its fade-in (about half there, a few px below its place); --motion-at starts the
# clock before its first frame, which a still run would draw in place at once.
shot palette-rising-light --light --palette "" --motion-at 41 "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot palette-rising-dark --dark --palette "" --motion-at 41 "$fx/showcase.md" "$fx/tables.md" "$fx/math.md"
shot palette-commands-light --light --palette ">zoom" "$fx/showcase.md"
shot palette-commands-dark --dark --palette ">zoom" "$fx/showcase.md"
shot palette-history-light --light --palette ">back" "$fx/showcase.md"
shot palette-export-light --light --palette ">export" "$fx/showcase.md"
shot palette-print-dark --dark --palette ">print" "$fx/showcase.md"
shot palette-headings-light --light --palette "#" "$fx/showcase.md"
shot palette-headings-dark --dark --palette "#" "$fx/showcase.md"

# Find in folder: results by file with the matches marked; nothing found.
shot folder-search-light --light --files --folder "$fx" --folder-search "table" "$fx/showcase.md"
shot folder-search-dark --dark --files --folder "$fx" --folder-search "table" "$fx/showcase.md"
shot folder-search-none-light --light --files --folder "$fx" --folder-search "zzzqq" "$fx/showcase.md"

# Panes, find, focus mode, tabs.
shot files-pane --light --files --folder "$fx" "$fx/showcase.md"
shot files-pane-dark --dark --files --folder "$fx/wiki" "$fx/linked.md"
# A folder's chevron turning as it opens, 67 ms into the turn (about a third of the way round),
# and the folder open at rest.
shot files-turning-light --light --files --folder "$fx" --turn wiki --motion-at 67 "$fx/showcase.md"
shot files-turning-dark --dark --files --folder "$fx" --turn wiki --motion-at 67 "$fx/showcase.md"
shot files-turned-light --light --files --folder "$fx" --turn wiki "$fx/showcase.md"
shot outline-pane --light --outline "$fx/showcase.md"
shot outline-pane-dark --dark --outline "$fx/everything.md"
shot outline-focus --light --outline --outline-focus 3 "$fx/showcase.md"
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
# The Open button's tip 41 ms into its fade-in (83 ms), once the pointer has rested.
for theme in light dark; do
    shot "tip-fading-$theme" --"$theme" --hover 46,63 --motion-at 41 "$fx/showcase.md"
done
shot hover-page --light --hover 640,300 "$fx/showcase.md"
shot hover-menu --light --menu main --hover 1100,120 "$fx/showcase.md"

# Hover cards: a footnote, a long footnote (cut), a heading of the page, a local file, in both
# themes; a web address keeps only its hint.
shot preview-footnote-light --light --size 1000x600 --hover 411,191 "$fx/previews.md"
shot preview-footnote-dark --dark --size 1000x600 --hover 411,191 "$fx/previews.md"
shot preview-footnote-long --light --size 1000x600 --hover 288,284 "$fx/previews.md"
shot preview-heading-light --light --size 1000x600 --hover 390,217 "$fx/previews.md"
shot preview-heading-dark --dark --size 1000x600 --hover 390,217 "$fx/previews.md"
shot preview-file-light --light --size 1000x600 --hover 800,217 "$fx/previews.md"
shot preview-file-dark --dark --size 1000x600 --hover 800,217 "$fx/previews.md"
shot preview-remote --light --size 1000x600 --hover 240,242 "$fx/previews.md"

# The update bar.
shot update-available --light --update-bar available "$fx/showcase.md"
shot update-installing --dark --update-bar installing "$fx/showcase.md"
shot update-installed --dark --update-bar installed
shot update-installed-plain --light --update-bar installed-plain

# A document mdedit previews before it has a file: a tab under the editor's name, live, with
# its images and links shown as missing; its tab menu (no path to copy or show) and its tooltip.
shot untitled-live --light --demo-untitled "$fx/images.md" "$fx/tables.md"
shot untitled-live-dark --dark --demo-untitled "$fx/showcase.md"
shot untitled-tab-menu --light --menu tab --demo-untitled "$fx/images.md" "$fx/tables.md"
shot untitled-tip --dark --hover 70,18 --demo-untitled "$fx/images.md"

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

# The window as the client-drawn margin shows it (--frame): the shadow and the rounded corners over
# a flat backdrop, at scale 1 and 2 in both themes, and the other two choices of Window corners.
# The settings dialogs above show the row.
shot frame-round-light --light --size 520x360 --frame round "$fx/showcase.md"
shot frame-round-dark --dark --size 520x360 --frame round "$fx/showcase.md"
shot frame-round-light-2x --light --size 520x360 --scale 2 --frame round "$fx/showcase.md"
shot frame-round-dark-2x --dark --size 520x360 --scale 2 --frame round "$fx/showcase.md"
shot frame-small-light --light --size 520x360 --frame small "$fx/showcase.md"
shot frame-square-light --light --size 520x360 --frame square "$fx/showcase.md"
# The copy button of a code block: under the pointer in the block, with its tip on the button, and
# with its check mark after a copy.
shot code-copy-light --light --scroll 400 --hover 700,420 "$fx/showcase.md"
shot code-copy-tip-light --light --scroll 400 --hover 971,339 "$fx/showcase.md"
shot code-copy-done-dark --dark --scroll 400 --hover 971,339 --copied "$fx/showcase.md"
shot code-copy-dark --dark --scroll 1200 --hover 640,475 "$fx/showcase.md"
# Presentation mode: a slide, in both themes, and at a small and a large window.
shot present-title-light --light --present 1 "$fx/slides.md"
shot present-bullets-light --light --present 2 "$fx/slides.md"
shot present-bullets-dark --dark --present 2 "$fx/slides.md"
shot present-table-dark --dark --present 3 "$fx/slides.md"
shot present-small-light --light --size 700x500 --present 2 "$fx/slides.md"
shot present-large-dark --dark --size 1920x1080 --present 2 "$fx/slides.md"
shot present-scale-150 --light --scale 1.5 --present 3 "$fx/slides.md"
shot present-overview-dark --dark --present 3 --present-overview "$fx/slides.md"
shot present-select-light --light --present 2 --present-select "$fx/slides.md"

# The render gaps: markup in a wiki-link label and embeds, `<br clear>`, a table beside a float.
shot wiki-embeds-light --light --size 900x700 "$fx/wiki-embeds/Embeds.md"
shot floats-clear-light --light --size 900x700 "$fx/floats-clear.md"
shot floats-table-light --light --size 900x700 "$fx/floats-table.md"

# A large diagram opened at a reading size: as it opens (the first diagram of the fixture), the
# whole of it fitted, and zoomed in and panned; in both themes.
shot zoom-diagram-light --light --size 1280x800 --zoom-diagram 1 "$fx/wide-diagram.md"
shot zoom-diagram-dark --dark --size 1280x800 --zoom-diagram 1 "$fx/wide-diagram.md"
shot zoom-diagram-fit-light --light --size 1280x800 --zoom-diagram 1 --zoom-level 0.42 "$fx/wide-diagram.md"
shot zoom-diagram-fit-dark --dark --size 1280x800 --zoom-diagram 1 --zoom-level 0.42 "$fx/wide-diagram.md"
shot zoom-diagram-pan-light --light --size 1280x800 --zoom-diagram 1 --zoom-level 1.8 --zoom-pan -900,-500 "$fx/wide-diagram.md"
shot zoom-diagram-pan-dark --dark --size 1280x800 --zoom-diagram 1 --zoom-level 1.8 --zoom-pan -900,-500 "$fx/wide-diagram.md"
# At a device scale of 2 it opens at the same size against the page as at 1, each raster pixel on
# one device pixel (audit 4, E2: it opened at four times the page's size).
shot zoom-diagram-2x-light --light --scale 2.0 --size 1280x800 --zoom-diagram 1 "$fx/wide-diagram.md"
shot zoom-diagram-2x-dark --dark --scale 2.0 --size 1280x800 --zoom-diagram 1 "$fx/wide-diagram.md"

# More HTML and notes: a wrapping ruby base, a figure with its margin, `<abbr title>` (and its hover card).
shot render-more-light --light --size 640x640 "$fx/render-more.md"
shot render-more-narrow-light --light --size 430x560 "$fx/render-more.md"
shot render-more-abbr-card --light --size 640x640 --hover 89,254 "$fx/render-more.md"

# The Notes face of the Files pane and "Add this file to the notes library…", on scratch copies
# of mdedit's settings and libraries (MDEDIT_SETTINGS names a settings file of mdedit's; a shot
# never reads the owner's). `notes-library` is the fixture; `one` lacks Work.md, `empty` has no
# library file, `nowhere` names a folder that is not there.
nl="$root/target/shots-notes"
rm -rf "$nl"
abs() { (cd "$1" && (pwd -W 2>/dev/null || pwd)); }
mkdir -p "$nl/full" "$nl/one" "$nl/empty" "$nl/small"
cp -r "tests/notes-library/." "$nl/full/"
cp -r "tests/notes-library/." "$nl/one/"
rm -f "$nl/one/Work.md"
notes_settings() { # name library-folder
    mkdir -p "$nl/$1"
    printf '[preferences]\nlibrary_dir=%s\n' "$2" >"$nl/$1/settings.ini"
    echo "$nl/$1/settings.ini"
}
full="$(notes_settings s-full "$(abs "$nl/full")")"
one="$(notes_settings s-one "$(abs "$nl/one")")"
empty="$(notes_settings s-empty "$(abs "$nl/empty")")"
nowhere="$(notes_settings s-nowhere "$(abs "$nl")/not-made")"
for theme in light dark; do
    # The face, with a note of the library open (its row selected); a group folded is in the unit test.
    MDEDIT_SETTINGS="$full" shot "notes-pane-$theme" --"$theme" --files --notes "$nl/full/Personal/Budget.md"
    MDEDIT_SETTINGS="$full" shot "notes-pane-min-$theme" --"$theme" --size 520x360 --files --notes "$nl/full/Personal/Budget.md"
    # The Files face beside it, with the two switches in its head.
    MDEDIT_SETTINGS="$full" shot "notes-switch-files-$theme" --"$theme" --files --folder "$fx" "$fx/showcase.md"
    MDEDIT_SETTINGS="$empty" shot "notes-empty-$theme" --"$theme" --files --notes "$fx/showcase.md"
    MDEDIT_SETTINGS="$nowhere" shot "notes-nofolder-$theme" --"$theme" --files --notes "$fx/showcase.md"
    # Add this file: the tab's menu, the palette, the question, the choice of group; then the
    # page's menu with it, and the palette listing the library's notes by title.
    MDEDIT_SETTINGS="$full" shot "menu-tab-library-$theme" --"$theme" --menu tab "$fx/showcase.md" "$fx/tables.md"
    MDEDIT_SETTINGS="$full" shot "palette-library-$theme" --"$theme" --palette ">notes library" "$fx/showcase.md"
    MDEDIT_SETTINGS="$one" shot "dialog-library-add-$theme" --"$theme" --dialog library-add "$fx/showcase.md"
    MDEDIT_SETTINGS="$full" shot "menu-library-groups-$theme" --"$theme" --menu library "$fx/showcase.md"
    MDEDIT_SETTINGS="$full" shot "menu-page-library-$theme" --"$theme" --menu page "$fx/showcase.md"
    MDEDIT_SETTINGS="$full" shot "palette-notes-$theme" --"$theme" --palette "" "$fx/showcase.md"
done
rm -rf "$nl"

ls "$out" | wc -l
