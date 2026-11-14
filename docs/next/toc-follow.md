# toc-follow

## README

Feature list: "The outline follows the page: the heading of the section at the top of the window
is highlighted, and its row is kept in view."

No shortcut.

## DESIGN

The outline pane highlights the heading whose section is at the top of the viewport (found by a
binary search over the known heading positions, no layout work) and scrolls itself by the least
amount that brings that row into view. Clicking an outline row jumps and the highlight matches.
The outline does not move while the pointer is on it (rows, backlinks, scrollbar) or while its
scrollbar is dragged; the row is brought into view on the first frame after the pointer leaves.
The same holds when the outline was hidden or not yet drawn when the heading changed, and when
switching tabs (the heading index is tracked per tab).

## Release note

The outline follows the page as you scroll, and leaves the list alone while you are using it.

## Handoff

The highlight and the binary search (`Workspace::current_heading`) and `reveal_outline_row` already
existed. Gaps fixed: the reveal was lost when the outline was not drawn yet or hidden (its height was
0), it moved the list under the pointer, and a tab switch to a document with the same heading index
did not reveal. New: `panes::follow_outline_row`, `last_heading_tab` in `App`. Tests added in
`app/tests.rs`. Not verified: nothing was built or run locally (CI only); no new screenshot, as the
visible states are the existing outline shots; the owner has not looked at it. Budgets cannot be
measured here; the change adds one comparison per frame and nothing at startup.
