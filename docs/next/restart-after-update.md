# Restart after an update (mdview side), as merged into `f/frosted-window`

The branch `feat/restart-after-update` (`534abad`, built and reviewed earlier) is merged into this
branch; the shared pin is `merge/editor-next` (`c9ba573`). `docs/UPDATE.md` is already edited for
it (the branch did that itself); this file holds only what the merge added and what was tried.

## README

Nothing new beyond what `docs/UPDATE.md` says: after an update is installed the bar has a
**Restart** button.

## DESIGN

Beside the Restart button the bar now reads "mdview X is installed. Restart to use it." (shared
`Updates::bar_label_with`). Where there is no file to start (a copy that cannot update itself) it
keeps "It starts the next time you open mdview." and has no button. Shots `update-installed` and
`update-installed-plain` show the two.

## Release note

- After an update is installed, the bar has a Restart button that closes mdview and starts the
  new version.

## Handoff

Tried by script on Windows 11 with this branch's release build and a scratch settings file: a
window was open; a second process without the mark handed its file to that window and returned
in 0.1 s; a second process started with `--restarted` (what Restart starts) had no window for
the 3 s the first one stayed open, and opened its own window, with the session's tabs and its
file, once the first had gone.

Not tried: a real update followed by a click on Restart (it needs a second build with a higher
version on an https host); the click itself in a real window (`--update-bar installed` has no
installed file to start); an AppImage.

Found on the way, **not changed** (it is in the released 0.3.1 too and belongs to no feature
here): a close request that comes from the system (`WM_CLOSE`, as the taskbar's "Close window"
sends it) sets `App::exit`, but the window goes only at the next event (a pointer move over it
is enough). Measured on the `main` build: still open 8 s after the request, gone right after a
posted pointer move. mdedit does not have this: its `CloseRequested` also asks for a frame
(`self.redraw()`), which is probably all mdview needs in `window_event`.
