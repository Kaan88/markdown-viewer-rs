# Shared renderer: Windows shot comparison

`tests/shots.sh` produced 195 PNGs before and after, with `MDVIEW_SHOTS_NO_SESSION=1`
and the respective `mdview-engine` next to the viewer. Repeating both sets with
scratch Windows app-data yielded the same PNGs. `tests/shots-check.sh` found
151 required shots and 0 bad files. Six scratch-profile settings shots were not
run because they load a session. The three conditional font shots were not
selected on this host.

Before the harness redirected `LOCALAPPDATA`, startup attempted to enumerate
`%LOCALAPPDATA%\mdview\crashes` for `crash-*.txt` names. It did not open a report
to read its contents, mark one seen, or write one during the shot runs. No
individual report filename was recorded. This is based on the startup code and
the shot output, not a filesystem trace.

190 images changed, with no image size changes. Side-by-side inspection of
Showcase, Settings and the Notes pane in both themes, plus the light Diagrams
and dark Math shots, showed slightly different glyph edges and no moved or
missing controls, text, diagrams or formulas.

Unchanged: `present-scale-150`, `zoom-diagram-dark`, `zoom-diagram-light`,
`zoom-diagram-pan-dark`, `zoom-diagram-pan-light`.

Changed PNGs (basenames):

```text
both-panes
code-copy-dark
code-copy-done-dark
code-copy-light
code-copy-tip-light
diagrams-light
dialog-about-dark
dialog-about-light
dialog-edit
dialog-library-add-dark
dialog-library-add-light
dialog-recent
dialog-settings-640
dialog-settings-backdrop
dialog-settings-dark
dialog-settings-light
dialog-settings-min-dark
dialog-settings-opening-dark
dialog-settings-opening-light
dialog-settings-scale-150
dialog-shortcuts
emoji-light
empty-dark
empty-light
everything-dark
everything-light
files-pane
files-pane-dark
files-turned-light
files-turning-dark
files-turning-light
find-dark
find-light
find-none
floats-clear-light
floats-light
floats-table-light
focus-mode
focus-mode-dark
folder-search-dark
folder-search-light
folder-search-none-light
formats-light
frame-round-dark
frame-round-dark-2x
frame-round-light
frame-round-light-2x
frame-small-light
frame-square-light
hover-menu
hover-open
hover-page
hover-titlebar
html-extras-dark
images-dark
math-dark
menu-cascade-dark
menu-cascade-light
menu-find
menu-library-groups-dark
menu-library-groups-light
menu-main-dark
menu-main-fading-dark
menu-main-fading-light
menu-main-keys
menu-main-light
menu-page
menu-page-library-dark
menu-page-library-light
menu-recent
menu-tab
menu-tab-library-dark
menu-tab-library-light
menu-tab-light
menu-tall-main
menu-tall-main-end
notes-empty-dark
notes-empty-light
notes-nofolder-dark
notes-nofolder-light
notes-pane-dark
notes-pane-light
notes-pane-min-dark
notes-pane-min-light
notes-switch-files-dark
notes-switch-files-light
outline-focus
outline-pane
outline-pane-dark
page-font-unknown
palette-commands-dark
palette-commands-light
palette-empty-dark
palette-empty-light
palette-export-light
palette-headings-dark
palette-headings-light
palette-history-light
palette-library-dark
palette-library-light
palette-notes-dark
palette-notes-light
palette-print-dark
palette-rising-dark
palette-rising-light
present-bullets-dark
present-bullets-light
present-large-dark
present-overview-dark
present-select-light
present-small-light
present-table-dark
present-title-light
preview-file-dark
preview-file-light
preview-footnote-dark
preview-footnote-light
preview-footnote-long
preview-heading-dark
preview-heading-light
preview-remote
render-more-abbr-card
render-more-light
render-more-narrow-light
scale-100
scale-150
scale-150-menu
scale-200
scale-200-dark
scrolled
settings-appearance-dark
settings-appearance-light
settings-appearance-min-dark
settings-appearance-min-light
settings-reading-dark
settings-reading-light
settings-reading-min-dark
settings-reading-min-light
settings-switch-sliding-dark
settings-switch-sliding-light
settings-switch-turned-light
settings-tabs-dark
settings-tabs-light
settings-tabs-min-dark
settings-tabs-min-light
settings-updates-dark
settings-updates-light
settings-updates-min-dark
settings-updates-min-light
showcase-dark
showcase-light
small
tables-light
tabs
theme-fading-dark
theme-fading-light
theme-fading-settings-dark
theme-fading-settings-light
tip-fading-dark
tip-fading-light
title-hover-dark
title-hover-light
toast-dark
toast-light
toast-rising-dark
toast-rising-light
toasts-short-dark
toasts-short-light
toasts-stacked-light
toasts-stacking-dark
toasts-stacking-light
untitled-live
untitled-live-dark
untitled-tab-menu
untitled-tip
update-available
update-installed
update-installed-plain
update-installing
update-rising-dark
update-rising-light
width-full
width-narrow
width-wide
wiki-embeds-light
zoom-150
zoom-diagram-2x-dark
zoom-diagram-2x-light
zoom-diagram-fit-dark
zoom-diagram-fit-light
```
