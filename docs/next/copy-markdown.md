# Copy as Markdown (`f/copy-markdown`)

## Built

- `mdview_core::rich::selection_markdown(doc, source, from, to)`: the source lines of every
  top-level block the selection touches, whole, cut from the file's text (bytes as written, CRLF
  kept). Tests are in `rich.rs` (`markdown_tests`, 14).
- `Workspace::selected_markdown()`: the text builder (no clipboard). The model keeps no source, so
  it uses the text a live or presented tab holds, else reads the file again and only accepts it
  when its stamp equals the one the page was parsed at. A file changed on disk since, an untitled
  or missing file: a status message, nothing copied.
- Command `CopyMarkdown` ("Copy as Markdown", no key) in the table in `crates/core/src/commands.rs`
  (menu `None`, so it is in the palette and not in a menu bar menu), in the page's right-click
  menu right under Copy (greyed with no selection) and in the palette (greyed with no selection).
  It writes plain text only and shows the status "Copied".

## The line map and the block kinds (first step)

The map gives each block's start line (`Block::line`) and no end line. A top-level block is taken
from its start to the line before the next top-level block starts, less blank lines at the end.
Checked by test, each selected from inside the block: front matter (the `---` lines kept),
paragraphs, lists (whole list from one nested item), fenced code (inner blank lines kept), math
`$$` blocks, tables (whole, from one cell), `<details>` (from the summary or the body), HTML
blocks, footnote definitions, the last block of the file; and every file in `fixtures/` for the
whole-page selection ends where the file ends. Not exact, widened: text between two blocks that is
no block (a link reference definition, an HTML comment) goes with the block above it.

## Decisions

- Whole top-level blocks only: a selection inside one list item gives the whole list.
- The last line's terminator and trailing blank lines are dropped; blank lines between blocks stay.
- A block with no text (a rule, an image-only paragraph) counts when the selection runs across it;
  a rule at the very start of the selection belongs to the text after it.
- No shortcut: Ctrl+Shift+C and friends are not obviously free; the right-click menu and palette
  carry it.
- The page menu has one more row, so `menu-page` differs from before.

## Run

`cargo fmt --all`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, and
`cargo test --locked --release --workspace` pass. Looked at the shots `menu-page` and
`palette` (`>copy`, made by hand with the same flags as `tests/shots.sh`): the new row fits, greyed.

## NOT verified

- A real selection with the mouse followed by the command in the running app, and a paste of the
  result anywhere; the system clipboard was never written (tests use the hook of the code button).
- Live (editor) and presented tabs, which use their held source: not tested.
- Selections across a file with a BOM, or whose line breaks are only `\r`.
- `tests/shots.sh` was not run in full.

## Release note

Right-click a selection and choose "Copy as Markdown" (also in the command palette) to copy the
Markdown source of the selected blocks exactly as it is in the file.
