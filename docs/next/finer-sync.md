# finer-sync: the preview follows the editor inside long blocks (mdview side)

Shared commit: `368aef1` (branch `f/finer-sync` of markdown-shared-rs: `docs/SISTER.md` and one
doc comment; no code changed there). The editor's side is on the branch of the same name in
markdown-editor-rs. This branch is on top of `f/live-preview-unsaved`.

## README

Change the "With mdedit" item of the feature list to end with:

  ... The preview follows the editor's caret line by line, inside a long paragraph, list item,
  code block or table too, and along a paragraph written as one long line.

No new shortcut, no setting.

## DESIGN

**A line of the file is a place inside its block.** Until now a source line was shown by the
block it is in: the block's first row went below the top edge, so a caret walking through a
300-line code block or a long paragraph did not move the page. Now:

- A line of a code block (fenced or indented) is shown exactly: that row of the code.
- A line of a paragraph or list item written over several lines is shown where its text begins
  on the page. The page joins and wraps those lines anew, so that is somewhere in a row.
- A table is shown by its row, a list by its item, as before.
- With a column (the byte in the line, from 1, which mdedit sends since this release) the place
  is that far along the line's text: a paragraph written as one long line, which wraps to many
  rows on the page, is shown at the row the caret is in. Markup that shows nothing (`**`, a
  link's address) is counted with the text before the next mark, so inside such a line the place
  can be some words off (not measured how far at worst; marks are 64 bytes apart).
- A place before the line's first text (an indent, a list marker, a fence) is that first text; a
  line with no text of its own (a blank line inside a block quote, an image alone on its line)
  is the nearest line before it; a block with no text at all (a rule) is the block.

**How.** While parsing, the builder notes where lines of the file are in the document's text
(`Document::marks`, `LineMark { line, col, text }`): one mark for each line a run of text or code
begins on, and one more every 64 bytes along a line. `Document::text_at_line(line, col)` finds the
place by a binary search in the marks of the block `block_at_line` gives; the workspace hands it
to the renderer as `Jump::Text(offset)`, which already existed (find-in-page uses it) and reads
the row from the lines the layout has. No layout pass is added: the jump measures the same one
block a jump to its start measured.

**The other way.** `Document::line_at_text(offset)` gives the line and byte of a place in the
text. "Edit in mdedit" sends the line of the file that is in the first row of the page, and the
column when that row begins past the line's first text (a row inside a line that wraps), so the
editor's caret lands in the row that was at the top, not at the paragraph's start.

**A follow moves by a step.** As before, a live update that would move the page by less than one
arrow-key step (48 logical px) leaves it where it is, in both directions. With places inside
blocks that is what keeps the page still while the caret moves within the top rows: walking down
a code block the page moves every second or third line, by that many rows. Typing at the end of
a document adds rows below; the page moves down when a step's worth has been added and never
back (tested: 400 letters typed at the end, with line breaks, never move the page up).

**Older builds.** No new key and no new answer. `?col=` was always carried and skipped by a
viewer that had no use for it, so an mdedit that sends it to an older mdview gets the old
behaviour (the block's start), and an older mdedit that sends no column to this mdview gets the
line's beginning.

## Release note

- The side-by-side preview follows the caret inside long paragraphs, lists, code blocks and
  tables, not only from block to block. Needs the mdedit of the same release for paragraphs
  written as one long line.

## Handoff

Built:

- `crates/core/src/model.rs`: `Document::marks`, `LineMark`, `text_at_line`, `line_at_text`.
- `crates/core/src/parse.rs`: `Builder::mark`, `Builder::code_text`, `MARK_GAP`.
- `bins/mdv-app/src/workspace.rs`: `SourcePlace` (line and column), `scroll_to_line` /
  `follow_line` / `follow` / `live_text` / `document_text` take it; `top_source_place`,
  `source_place`.
- `bins/mdv-app/src/app/commands.rs`: requests give their column, "Edit in mdedit" sends one.
- Tests: five in `crates/core/tests/core_tests.rs` (the mapping: paragraph, list, code, table,
  one long line, every fixture), four in `workspace.rs` (a line inside a long block, a place
  along a wrapped line, a caret walking down and up, typing at the end), one in `app/tests.rs`
  (a hand-over with a column). `parse_cost` (ignored) prints what a parse costs.

Started by an earlier session (the marks, the two `Document` functions, the plumbing); this
session renamed the type (it clashed with the history's `Place`), rewrote `text_at_line` as a
binary search, and took out one thing the earlier session had begun: a smaller step upwards
(24 px) than downwards. It made an existing test fail
(`live_text_replaces_the_page_keeps_the_place_and_ends_on_live_off`: a live text whose caret is
on the line at the top edge moved the page), so the step is the old one in both directions.

Decisions taken, for the owner to confirm:

- Marks are made at every parse, for every document, used or not: 12 bytes for each line of the
  file that has text, and a parse about 3% longer (below). Making them only when the editor asks
  would need a second parse.
- A column is a byte in the line, as the editor's `--goto line:column` already was.
- The follow step stays 48 px both ways. One consequence: while the caret walks up, its row can
  be the one just above the top edge for one line before the page follows.
- No shot: nothing new is visible in a still picture.

Budgets, measured on this PC (Windows 11, release builds):

- Parse, `parse_cost` on 8 MB of `fixtures/showcase.md` repeated: 73.0 / 74.4 ms (min / median of
  9) before, 75.5 / 78.4 ms after; a second pair of runs 73.4 / 75.8 and 76.1 / 77.1. 8 MB of
  one-line paragraphs: 36.8 / 37.3 ms before, 37.6 / 38.2 ms after.
- First frame and scrolling, the release before (`f/live-preview-unsaved`) and after, three runs
  each of `MDVIEW_TRACE=1 mdview --no-session --scroll-steps 120`: `fixtures/showcase.md` first
  frame after 73.4 / 74.3 / 75.3 ms before, 73.7 / 73.8 / 80.1 ms after; a scroll frame's own
  work, median 0.57 ms before and 0.56 to 0.57 ms after (p90 0.83 to 0.88 and 0.84 to 0.89).
  The 610-line test document: first frame 53.2 to 55.3 ms before, 53.6 to 55.6 ms after; scroll
  median 0.56 to 0.57 ms before, 0.56 to 0.61 ms after. No difference beyond the runs' own spread.

Tried in real windows on Windows 11: a script started mdedit on a 610-line test
document with `--run PreviewSideBySide`, which started this mdview beside it (both with scratch
settings), posted arrow, Home and End keys to the editor's window and saved each window's own
picture after each. Seen: the caret on line 150, 190 and 230 of a 300-line code block has that
line of code in the second row of the page each time (before, the page stayed at the block's
start); the caret at byte 9000 of a 27000-byte one-line paragraph has the row with that word
second from the top, End shows the paragraph's end, Home its first row; 100 lines up into a
300-line paragraph the page shows the row that line's text begins in. Not tried in a real window:
"Edit in mdedit" from a row inside a wrapped line (tested headless), typing with live text on a
long line, and anything by hand.

Not verified: Linux in real windows (the tests run there on CI; the unix-only test
`edit_in_mdedit_starts_the_editor_at_the_top_line` was changed to expect a column when the top
row is inside a wrapped line, and was not run here); a person's judgement of how the following
feels; right-to-left text; a document over the live-text cap (the line still maps through the
file on disk).
