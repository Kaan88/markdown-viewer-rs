# history: Back and forward

## README

Feature list: **Back and Forward.** Following a link to another file or to an anchor, a footnote,
an outline or palette heading, or a backlink remembers where you were; Alt+Left / Alt+Right (or
the mouse's Back / Forward buttons, or the command palette) return there at the exact scroll
position.

Shortcut table: `Alt+Left` Back, `Alt+Right` Forward.

## DESIGN

- Support already existed (0.2.x): one history for the window, up to 100 places (file and scroll
  position), Alt+Left / Alt+Right and mouse buttons 4/5. It is window-wide and not per tab on
  purpose: a link to another file opens it in a new tab (or switches to it), so a per-tab history
  would never hold the jump and Back could not return to the place the link was clicked. Back
  switches to that file's tab, opening it again if it was closed, and restores the scroll.
- Recorded by: links (same-file anchor, other Markdown file with or without anchor, wiki links),
  footnote jumps, outline and palette `#` headings, backlinks. A jump that did not move the
  reader is not recorded. A new jump drops what Forward held. Files that vanished are skipped.
- Not persisted across restarts.
- New in this branch: commands `Back` and `Forward` in the table (menu None, shortcuts
  Alt+Left / Alt+Right, so the Keyboard shortcuts dialog lists them), in the command palette
  (greyed when there is nowhere to go), icons BACK / FORWARD.
- No toolbar buttons (the bar has no natural place) and not in the main menu: with two more rows it
  would no longer fit the default 800 px window without scrolling.

## Release note

Back and Forward are in the command palette and the Keyboard shortcuts list (Alt+Left, Alt+Right).

## Handoff

Built: the two commands, palette rows with enabled state, icons, key-table and app tests, shot
`palette-history-light`. Decided: keep the window-wide history (see DESIGN). Not verified:
anything outside CI; the main-menu fit is computed by hand. Left out: toolbar buttons, main-menu
entries, history of find-in-document matches (not jumps), find-in-folder results (another
feature; it should call the workspace's `remember` as backlinks do), persistence.
