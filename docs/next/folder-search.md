# Find in folder (folder-search)

## README

Feature list: **Find in folder** searches the Markdown files of the open folder for a text and lists
the matching lines by file, with the match marked. Enter or a click opens the file at the match,
highlighted as the find in the page does. It runs on a background thread, results stream in, and
Escape or a new query cancels it.

Shortcut table: Find in folder has no default key (Ctrl+Shift+F is Focus reading mode's). It is in
the "⋯" menu (greyed without a folder) and in the command palette (`>find in folder`).

## DESIGN

- The card is the command palette's card, as a literal list (`mdshared_ui::palette`, `set_literal`):
  the query box, rows grouped under the file's path (a heading row with the file's icon and its
  match count), each row one matching line with the match in the accent colour and `Line N` at
  the right, a footer ("Searching... 12 files", then "31 matches in 6 files", plus ", N files
  skipped" for files left out, "Searched N files" when nothing matched). Up and Down skip the
  headings; Enter or a click on a row opens it; Escape or a click outside closes it.
- The query is exactly what the find in the page takes (case-insensitive, the same folding, no
  options: the viewer's find has none). It is searched in the Markdown source, line by line. The
  box starts with the find box's text. Typing starts a new search at once and cancels the old.
  An empty or blank query lists nothing.
- What is searched: the files the Files pane lists (`is_markdown_path`), with the pane's rules
  (names starting with a dot, the hidden attribute on Windows, `target`, `node_modules`, links to
  folders not followed, depth 32), the folders of a folder first, each sorted by name ignoring case.
  A file over `MAX_FILE_BYTES` (16 MiB, the viewer's own limit), unreadable or holding a NUL is
  skipped and counted. UTF-16 files are read by their mark.
- Caps: 200 lines per file, 2000 in all. When either cut something the last row says "More results
  are not shown. Narrow the search." The heading of a file over its share says "200+ matches".
- Choosing a row opens the file (or switches to its tab), puts the query in the find box, runs the
  find and makes current the first match in the block that source line is in, scrolled into view.
  When that block has no match on the page (the query was in markup or a link's address) the page
  scrolls to the line and the find stays on with its matches elsewhere. In a long paragraph the
  match is the first of the paragraph, not necessarily the one on the chosen line.
- Opening a result is a jump: Back (Alt+Left) returns to where the reader was.
- Without a folder the command says "Open a folder to search it" in the status line.
- The thread exists only during a search; nothing is built or started until the command is used.

## Release note

New: Find in folder searches the Markdown files of the open folder and opens a result at its match
(menu "Find in folder..." and the command palette).

## Handoff

Built: `mdshared-app::foldersearch` (walk, matching lines, caps, the worker thread; written for
both apps and not tied to Markdown), `mdshared-ui::palette` additions (`set_literal`, `Row::Heading`
and `Row::Note` entries, `Entry::marks`, `append_entries`, `set_footer`, `label_of`; shared branch
`f/folder-search`), `mdview_core::find::line_matcher` (the page find's folding for a line), the
`FindInFolder` command, `bins/mdv-app/src/folder_search.rs`, `Workspace::find_set_at_line`,
`Wake::FolderSearch`, `--folder-search <query>` for shots (searches to the end), three shots.

Decisions:
- No default shortcut: Ctrl+Shift+F is Focus reading mode's (and a regression test pins it), and
  moving it would change existing behaviour. The slot is free for the owner to decide. The command is
  in `Menu::None` of the table (not the Edit menu, whose item count a regression test pins), in the
  "⋯" menu and in the palette. The Keyboard shortcuts dialog lists only commands with keys, so it
  does not list this one.
- The viewer's find has no options (no case or whole-word switches), so there are none here.
- Reused the palette card instead of a new panel: same look, same keys, no second list widget.
- The size limit is the viewer's open limit, not the note index's 2 MiB.

Not verified: Windows (CI only), a real window, timings on release builds (not made; the feature
adds nothing at startup, nothing on the key or scroll path, and no thread until it is used; the
"⋯" menu has one more row), a huge folder (the walk is capped by depth and the hit caps, not by
file count).

Left out: results updating while files change; a "replace" (the viewer is read-only); searching
the files the pane hides; picking the exact match inside a long paragraph; the editor adopting
`foldersearch` (the editor has its own searcher; it can switch to the shared one later, with a
matcher that takes its options, and the palette additions are additive).
