# live-preview-unsaved: a document mdedit has not saved (mdview side)

Shared commit: `0ba3b6129b7ccd60c62275652e46c297ec5b5fc4` (branch `f/live-preview-unsaved` of
markdown-shared-rs). The editor's side is on the branch of the same name in markdown-editor-rs.

## README

Change the "With mdedit" item of the feature list to say:

- **With mdedit:** Edit in mdedit (Ctrl+E) opens the file in the editor at the line you are
  reading, and when mdedit previews a file here it shows the editor's unsaved text as you type,
  also for a document that was never saved.

No new shortcut.

## DESIGN

**A document that has no file.** mdedit can hand over a document it has not saved (`?doc`,
`?title`, `?open` and the text; the shared `docs/SISTER.md` has the wire format).

- It is shown in a tab named as the editor's tab ("Untitled 1"), with the dot of a live tab. The
  tab's tooltip and the palette say "not saved yet: shown as mdedit has it" in place of a path.
- Inside, the tab is keyed by a path that cannot exist on disk (under a folder whose name starts
  with a NUL character, then the document's name and title). Nothing is read from or written to
  that path, and it is never watched.
- Relative images and links have nothing to resolve against: an image is drawn as a missing
  image, and following a relative link says "not a file: <destination>". Absolute paths, web
  links and anchors inside the document work as usual.
- Later texts for the same name replace the text in place and keep the place read; the line the
  editor sends is followed as for a file. A text without `?open` for a name that has no tab does
  nothing.
- When the editor says the document was saved (`?doc` with a path), the tab becomes that file's
  tab: same position in the row, the file is read from disk, the live state carries on if the
  editor keeps sending. If the file is already open in another tab, the untitled tab is closed
  and that tab is shown: never two tabs for one file.
- When the preview ends (`live_off`), or the editor is found gone, the tab stays and keeps the
  last text, without the dot; if it is the tab in front when the editor goes, the toast is
  "mdedit closed: this document was not saved". There is no file to fall back to.
- Not for such a tab: Reload (says "This document has no file yet: it is shown as mdedit has
  it."), Copy file path and Show in file manager (disabled in the tab menu), the session and the
  recent list (the tab is left out, so it is not restored), the file watcher, a new window's
  start folder.
- Edit in mdedit asks the running editor for the document by its name, at the line being read;
  with no editor running it says "mdedit is not running" (nothing can be started for a document
  that has no file).

## Release note

- A document that mdedit has not saved yet can be previewed: it appears in a tab under the
  editor's name, follows the typing, and becomes the file's tab when it is saved. Needs the
  mdedit of the same release.

## Handoff

Built:

- `bins/mdv-app/src/workspace.rs`: `Tab.untitled`, `untitled_path`, `is_untitled_path`,
  `Workspace::document_text`, `document_off`, `document_saved`; the exclusions listed above.
- `bins/mdv-app/src/app/commands.rs`: `handed_over` routes requests that carry a document; the
  tab commands for such a tab.
- `crates/core/src/commands.rs`: the strings `LIVE_EDITOR_GONE_UNSAVED`, `UNTITLED`,
  `UNTITLED_HINT`, `UNTITLED_NO_FILE`, `EDITOR_NOT_RUNNING`.
- Argument for shots: `--demo-untitled <file>` shows the text of a file as such a tab.
- Shots: `untitled-live`, `untitled-live-dark`, `untitled-tab-menu`, `untitled-tip`.

Decisions taken, for the owner to confirm:

- The tab stays with its last text when the preview ends or the editor is gone, instead of
  closing.
- The tab is not kept in the session or the recent list.
- Relative links say "not a file: ..." instead of trying a folder (the editor's working folder
  would be a guess).
- `--demo-untitled` was added as a launch argument.

Tried in real windows on Windows 11 together with the editor: see the editor's file of the same
name. Also seen there: with the editor gone the tab kept its text and the toast appeared.

Budgets, measured on this PC on release builds of the base (`e909adc`) and of this branch, ten
launches each in turn (`MDVIEW_TRACE=1`, `--scroll-steps`): first frame of `fixtures/showcase.md`
75 to 82 ms before and 76 to 82 ms after; scroll frame median 0.56 to 0.57 ms before and 0.57
to 0.61 ms after. Nothing runs for the feature until the editor hands such a document over.
Not measured: memory.

Not verified: Linux in real windows; "Edit in mdedit" from such a tab in real windows (app-level
tests only); an older mdedit needs nothing new (it never sends a document).
