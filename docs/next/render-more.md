# More render gaps (branch `f/render-more`)

Eight small gaps from `docs/COVERAGE.md` and `docs/HANDOFF.md`, on top of `f/render-gaps`. Gaps
4 and 5 are one commit and so are 7 and 8, since each pair edits the same functions. A fixture,
`fixtures/render-more.md`, shows 1 to 3; `tests/shots.sh` renders it at its end (three shots).
No existing shot changed: all 111 earlier pictures are byte-identical before and after (the new
fixture was moved aside for the second run, as the Files pane lists the fixtures folder). None of
the old shots shows a ruby, a figure or an `<abbr>` inside its first screen.

## 1. `<ruby>` whose base wraps

**Built.** A ruby base is kept whole: no line break falls inside it, so the annotation always
sits over one line. A browser does the same for a simple ruby (a break opportunity lies between
ruby bases, not inside one). `build_layout` (`layout/pager.rs`) gives each base the parley style
`TextWrapMode::NoWrap`, except its last character: parley reads the mode of the character before
a break, so leaving the last one out keeps a break after the base possible. A base longer than 40
characters (`RUBY_WHOLE_MAX`) stays breakable, so it cannot run out of a narrow column.

**Decisions.** Kept together, not an annotation over each part, because a split annotation reads
as two notes. Cost: a loop over `inline.notes`, which is empty for a document with no ruby.

**Run.** Render test sweeping widths 160 to 420 (the base is one rectangle at every width, the
note is above it, the tail never passes the column, and the same words without ruby do split);
the fixture shot at 640 and 430 px, looked at.
**Not verified.** A base that is wider than the column (up to 40 characters) overflows it
instead of wrapping. Right-to-left bases. Not looked at by the owner.

**COVERAGE.md.** Row `<ruby>` with `<rt>`/`<rp>`: replace "a base that wraps has its whole
annotation centred over its first line only" with "a base is not broken across lines (up to 40
characters; a longer one may wrap, and its annotation is then over the first line)". "Drawn with
limits": the bullet "`<ruby>` annotations are half size and sit over the base's first line only"
becomes "`<ruby>` annotations are half size; a base is kept on one line up to 40 characters".

**Release note.** A ruby annotation stays over its base when the line would have broken inside
the base.

## 2. `<figure>` margin

**Built.** A figure has the 40 px side margin of a browser and GitHub: `Pager::block`'s `Figure`
arm lays its content at `left + margin` and `width - 2 * margin`; the estimate (`estimate.rs`)
takes the same width. `figure_margin` gives `min(40 px, width / 4)` so a narrow column keeps its
content. `<figcaption>` is unchanged (it is inside the figure, so it is inset too, as in a
browser). The exported page now has the same margin (`figure{margin-left:40px;margin-right:40px}`).

**Run.** Render tests (left edge 40 px in; aligned right, the caption ends 40 px before the
column's edge; a 120 px column still holds the caption; the old caption test was changed to expect
the 40 px). The fixture shot, looked at.
**Not verified.** A figure beside a float; a figure inside a table cell or list item (the margin
is taken there too, as in a browser). Not looked at by the owner.

**COVERAGE.md.** Row `<figure>`, `<figcaption>`: delete "GitHub's default 40 px side margin on a
figure is not drawn" (add "A figure has GitHub's 40 px side margin, less in a column narrower than
160 px"). "Drawn with limits": delete the bullet "`<figure>` has no 40 px side margin."

**Release note.** A `<figure>` has the same 40 px side margin as in a browser.

## 3. `<abbr>`, `<bdo>`, `<time>`

**Built.** `<abbr title="...">` is `SpanKind::Abbr(StrRef)` (the title is in `Document::strings`;
an `<abbr>` with no title, or an empty one, is plain text as in a browser). It is drawn with a
dotted underline (the dashed one of a missing wiki-link had one drawn by hand; a second marker
size, `DOTTED_UNDERLINE`, picks dots), and resting the pointer on it shows its title in the hover
card (`Target::Abbr`, `preview::abbr`; it follows the "hover cards" setting like a footnote card,
and a link inside the abbreviation keeps the link's own hover). Clipboard HTML and the exported
page keep `<abbr title>`; the exported style has `abbr[title]{text-decoration:underline dotted}`.
`<time>` was already kept as plain inline text (a test now says so). `<bdo dir>` was **not**
built: parley has no bidi override per range, and putting U+202D/U+202E in the text would move
every offset of the document text; the text of a `<bdo>` stays as written.

**Decisions.** `Abbr` carries the title, so a span carried over a block boundary keeps it
(the parser's ambient spans copy the kind). The card has no title line, only the text.
**Run.** Core tests (title decoded, none, empty, `<time>` and `<bdo>` text kept), an app test
(`resting_on_an_abbreviation_shows_its_title...`, found by scanning the page for the hover), an
export test; the fixture shots (the underline, and the card at the pointer), looked at.
**Not verified.** The underline on a wrapped abbreviation at the line break; an abbreviation in a
heading or a table cell; a real window with the real pointer. Not looked at by the owner.

**COVERAGE.md.** Row `<abbr>`, `<bdo>`, `<time>` and similar: becomes "`<abbr title>` is
underlined with dots and shows its title in the hover card; `<time>` is plain text; `<bdo>` keeps
its text with no direction override". "Drawn with limits": the last bullet becomes "`<bdo>` keeps its
text without a direction override."

**Release note.** An `<abbr title>` is underlined with dots and shows its title when the pointer
rests on it.

## 4 and 5. Block references and `[[#Heading]]` with no folder

**Built.** `Document::resolve_block_id(id)` finds the block that carries an Obsidian block id: a
paragraph, a tight list item's text or a heading whose text ends in ` ^id` (letters, digits and
hyphens), and, for an id on a line of its own after a blank line, the block before it. It is
looked up when asked (a scan of the blocks), nothing is indexed. `resolve_fragment("^id")` falls
back to it, so `Reveal::Anchor` and `jump_fragment` work for blocks unchanged. `fragment_of`
(`workspace/notes.rs`) turns `[[Note#^id]]` into `^id` (a heading still becomes its slug), so a
link to another note opens it and scrolls to the block, and `[[#^id]]` scrolls in this document.
An id that is not found leaves the other note at its top, and is refused with a message in this
one.

`[[#Heading]]` (an empty note name) is now always a link: it needs only this document, so
`Decider::answer("")` no longer depends on a folder. Where the heading or block is not on the
page, the link is marked missing (muted, dashed) after the parse (`dangling`, in `parse_document`
and `check_wiki_links`), for a folder open or not.

**Decisions.** Missing rather than text for a heading that is not there: the parser decides before
the headings are known, and with a folder open this was already a link whatever the heading. A
side effect: a link whose label ends in `^id` counts as carrying that id. The link is not what
`[[nodiscard]]` is: only a note name that is empty is always a link.
**Run.** Core test (`resolve_block_id`: end of a paragraph, a two-line paragraph, a list item, a
quote named from the next paragraph, a heading, non-ids, an unknown id) and three app tests (another
note at the block, this note, no folder for a heading, a missing one refused, `[[nodiscard]]`
still text).
**Not verified.** A block id inside a table cell, quote or footnote (only paragraph, list item
text and heading); the look of the jump in a real window. Not looked at by the owner.

**COVERAGE.md.** Row for wiki-links: "`[[Note#^block]]` opens the note at the block that carries
`^block`, and `[[#^block]]` and `[[#Heading]]` work in the open document with no folder open; one
whose target is not on the page is drawn as a missing link". Remove any "`[[#Heading]]` is text
with no folder" and "opens the note at its top" statements. (HANDOFF "Not done" in the notes
section says the same: the first, third and last sentences of that list are no longer true.)

**Release note.** `[[Note#^block]]` opens the note at that block, and `[[#Heading]]` links within
the document even when no folder is open.

## 6. New folders and new notes wake the window

**Cause.** The OS watcher is told two things: the open files (and, up to 5,000, the indexed notes)
and the Files pane's folders. An event for a path is passed on when the path is a followed file,
or when it is created, removed or renamed directly in a *listed* folder. A new note or folder is
not a followed file, and a folder that holds the open files or the notes is only watched, not
listed. So only the Files pane (when shown, for the root and the expanded folders) heard of new
entries; with the pane closed, or for a folder the pane does not list, nothing woke until an
unrelated event did. Reproduced in a test with the real `FileWatcher`: without the fix neither
wake arrives in 10 s.

**Built.** `Workspace::note_folders(max)`: the folders of the open files, the open folder, and
every folder below it that holds an indexed note (at most `WATCHED_NOTE_FOLDERS` = 1,000, else only
the open folder and the files' folders). `watch_tree` hands the watcher these together with the
pane's folders (`set_folders`), recomputed only when the open files or the index change (the same
signal as `set_files`), so an idle window does no more than before. Nothing polls.

**Run.** Two app tests with a real `FileWatcher`: a new folder below the open one wakes it with the
pane closed, then a note in a folder below the root wakes it, and the new folder is listed once the
index has a note in it; a note made beside a document with no folder open wakes it and its
`[[name]]` becomes a link. They fail (10 s timeout) when the folders are not given to the watcher.
**Not verified.** A real window; Linux inotify limits with 1,000 folders. A new, still empty folder
is followed only once the index holds a note in it: a note written into it after the index walk
and before the next wake is not heard of (the wake of the folder's creation walks the index
once, before the note exists in that case).

**COVERAGE.md / HANDOFF.** None in COVERAGE. HANDOFF "Not done" (notes section): drop "A note
created beside a document while no folder is open ... until the document is read again" and "A new
folder below the open one is seen at the next wake".

**Release note.** A note or a folder created beside the open document or below the open folder
is picked up at once, with the Files pane open or not.

## 7 and 8. Export as HTML

**7, built.** A table is placed by its own `align`, else by the `align` of the nearest `<div>`,
`<center>` or `<figure>` around it (the viewer's `inherited_align`): `margin-left/right:auto`
for centre, `margin-left:auto` for right. (`text-align`, which did nothing for a table, is no
longer written on a table.) A cell that states no `align` or `valign` takes its row's, as on the
page, and a cell's `width` (`px` or `%`) is written as `width:` on the cell. The table's own `width`
was already written. Seen in headless Edge on `tables.html`: the centred, captioned table and the
60% table placed as in the viewer.
**Decisions.** Heights (`height` on cells and rows) and `<col width>` are still not written (the
parser does not keep `<col>` at all).

**8, built.** One file in both schemes: the style sheet's `:root` holds the viewer's light palette
and `@media screen and (prefers-color-scheme:dark)` the dark one (page colours and the five alert
colours; `color-scheme` is set too), whichever theme the viewer showed when it exported; fonts and
widths still come from the viewer's settings. Print keeps the light palette (the dark block is
`screen` only). What was coloured for one theme is now written for both:
code (each coloured run also has `--d:#rrggbb`, the dark colour, which the dark block puts to
use when the two highlights cover the same ranges); formulas and diagrams (drawn twice by the
engine, light and dark, written as two `<img>` with classes `lt` and `dk`, one hidden by CSS);
`<picture>` images with a light and a dark source (two `<img>` likewise). No new command.
**Decisions.** Two `<img>` with a class rather than `<picture>`, so one works inside links and
floats and with the size and alignment attributes unchanged. The engine is asked twice per
formula or diagram (once per scheme); an export is rare and runs on its own thread. The file is
larger by the second picture of each formula and diagram.
**Run.** Export tests (placement, row and cell attributes, `<center>`, aligned `<div>`, both
palettes whichever theme made the page, both colours in code, themed pictures), and headless Edge
(`msedge --headless=new`) screenshots of the exported `math`, `tables` and `everything` fixtures
in a dark and a light browser: the page background, text, table borders and both formula
pictures follow the scheme.
**Not verified.** Print in a dark browser (the print palette was reasoned, not printed); a
diagram drawn for the dark scheme (the engine's dark Mermaid was not looked at in the page);
Safari and Firefox.

**COVERAGE.md / HANDOFF.** COVERAGE has no export row to change. HANDOFF "Export as HTML":
"always the light theme" and "Left out: table widths and centring from HTML attributes, dark
export" are no longer true; the page follows `prefers-color-scheme`, tables take widths and
placement from the HTML attributes.

**Release note.** An exported HTML file follows the reader's light or dark browser setting, and
tables keep the widths and placement their HTML gave them.
