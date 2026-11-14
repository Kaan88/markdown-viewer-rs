# watch-folder

## README
No change to the feature list: the Files pane already follows created, deleted and renamed files and folders in the root and expanded folders (OS notifications, no polling). Bursts of changes are now folded into one check.

## DESIGN
The viewer already used OS notifications (`mdshared_app::watch::FileWatcher`, the `notify` crate): parent folders of open files and notes, plus the listing of the Files pane's root and expanded folders while the pane is shown. A wake no longer checks at once: it sets `watch_due` 150 ms ahead; the timer then runs the check (open tabs reload, note index, `tree.refresh()`). The watcher stays pending until the check, so a git checkout's events cause one wake and one check. Expansion is kept by `FileTree::refresh`; the pane clamps its scroll on draw; the open document's row is derived from the active tab. Nothing polls and no thread is added; a hidden window costs nothing because the OS wakes only on a change.

## Release note
A burst of file changes in the open folder (a git checkout) refreshes the Files pane once.

## Handoff
Built: the 150 ms debounce (`WATCH_DEBOUNCE`, `watch_due`, `watch_check`) and a test. The rest of the requested feature was already present at the base commit (found by reading the code), so nothing else was changed. Not verified: behaviour in a real window, Windows/Linux real watcher timing. Left out: a poll fallback when the OS watcher cannot start (the shell has none; a hidden-window poll was not added because the existing mechanism covers the request).
