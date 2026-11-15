# Render gaps (branch `f/render-gaps`)

Four small gaps closed, each its own commit. Fixtures: `fixtures/wiki-embeds/` (`Embeds.md`,
`Target.md`), `fixtures/floats-clear.md`, `fixtures/floats-table.md`; `tests/shots.sh` renders
all three at its end. The fixtures sit in new files, so no existing fixture changed.

## 1. Markup inside a wiki-link label

**Built.** `[[Note|a *b* c]]` is one link to `Note` whose children are the label's text and
its emphasis, strong or code spans. `find_wiki` meets the run `[[Note|a ` with no closing pair;
when the source after the run reaches `]]` on the same line (`wiki_label_closes`: no bracket,
backslash, `<`, line end, and a `]]` inside a code span does not count) and the application
wants the note, `open_wiki_label` takes `[[Note|` out of the text and opens a link span there
(pushed as `Origin::Auto`). `close_wiki_label` ends it in the run that holds the `]]`, takes
the two brackets out and fixes up the escape and entity offsets and the line marks.

**Decisions.** The link opens on the source lookahead, so a label that never closes stays text
exactly as before. The span has an `Auto` origin so that markup which crosses the brackets
(`*x [[N|a *b* c]] d*`) still gives a sound tree: the outer `End` pops the link first. The
note is asked once, as for a plain link; `Missing`, `Text` and `wiki_text` work as before. A
label with a bare address at its start is left text. No cost without `[[`: the same `memmem`
test as before, one integer comparison in `flush_run`.

**Run.** Unit tests in `crates/core/tests/regressions.rs` (spans, nesting, heading, missing
note, labels that never close, crossing markup); `fixtures/wiki-embeds/Embeds.md` shot.
**Not verified.** An embed with a markup label (`![[Note|a *b*]]`) stays text. A markup label
inside a table cell (the bar must be `\|`) was not tried. Not looked at by the owner.

**Release note.** A wiki-link label may now contain emphasis, strong text and code
(`[[Note|a *b* c]]`) and is still one link to the note.

## 2. `![[embed]]`

**Built.** `parse_wiki_embeds` / `parse_bytes_wiki_embeds` ask the application
`(name, embed)`; `parse_wiki` and `parse_bytes_wiki` keep their signatures and still leave embeds
as text. For an embed whose name ends in `png jpg jpeg gif webp svg` (`links::is_image_name`)
and has no `#heading`, `Link` makes the picture, the same atom `![](name)` makes, alt text
from the label (`|200` or `|200x100` is a size, not a label, and is ignored). Any other name is
asked as a note: found, it is a link to the note, like `[[Note]]`; not found, it stays text.
`Decider::answer_embed` (`workspace/notes.rs`) answers `Link` for a picture only when the file
is there, relative to the document's folder, the way `![](name)` finds it; no `/`, `\` or `:`
start. Nothing is transcluded.

**Decisions.** An embed is never drawn as a missing link. An embed left as text is remembered
in `wiki_text` as `!name` (only when the application said `Text`), so the document is parsed
again when the picture or the note appears; a `Missing` note embed is not remembered, so it
does not turn into a link until the file is read again. A picture with a heading, a `!` that
is escaped or not in the same run (`**a!**[[x]]`), code and text inside another link stay text.

**Run.** Core unit tests (picture, size, note, not found, `parse_wiki` unchanged) and an app
test (`wiki_labels_with_markup_and_embeds_resolve_beside_the_document`) that opens the fixture.
**Not verified.** Pictures found only elsewhere in the open folder (Obsidian looks anywhere in
the vault) are not shown; a picture that appears or goes while the document is open was not
watched. Size hints are ignored. Not looked at by the owner.

**Release note.** `![[picture.png]]` shows the picture and `![[Note]]` links to the note, when
they are found next to the document; anything else stays as written.

## 3. `<br clear>`

**Built.** `<br clear="left|right|all|both">` (any case, quoted or not, `/>` or not) ends the
block it is in and the next block starts below the open floats at that side
(`Document::clears`, `clear_of`; `Pager::clear_side` at the top of `block`). Mid-paragraph, the
rest of the paragraph is that next block (the machinery a block tag inside a paragraph
already uses). `<br clear>` alone on a line, or at a paragraph's end, applies to the block that
follows. A value that is empty, absent or unknown (`none`) is the plain break it was.

**Decisions.** Block granularity: the break splits the block, so the paragraph gap shows
between the two parts (a browser has none). Where the block cannot be ended (a heading, a
tight list item's own text) the break stays a break and nothing is cleared. A clear whose block
is dropped moves to the block that takes its place. Layout cost: one `is_empty` test per block
when floats are open, nothing otherwise. The float state still passes on block to block, so the
lazy paths agree with the one-pass page (tested, including a jump).

**Run.** Core tests (forms, mid-paragraph, no usable value, ordering) plus the sorted/in-range
check in the fuzz invariants; render tests in `crates/render/tests/floats.rs` (each side, with
and without a clear, mid-paragraph, refinement and a jump).
**Not verified.** `<br clear>` inside a list item, quote or table cell was parsed but its
look was not examined. Not looked at by the owner.

**Release note.** `<br clear="all">` (and `left`, `right`) now makes the content after it
start below floated images.

## 4. A table beside a float

**Built.** `Pager::table` no longer clears floats first. With floats open it takes the room
the floats leave from the table's top down (`band`, as boxes do) and stands there, in that
room and aligned within it, when its natural width (its width in a free column) is no more than
that room; otherwise it clears the floats and lays out as before. A table is never narrowed.

**Decisions.** The test is the table's width in the full column against the room at its first
row, down to the end of the page (the same conservative reading boxes use), so a table that
would run under a float lower down goes below. A stated width of 100% never fits beside.
Without floats the path is one `is_empty` test.

**Run.** Render tests (right, left, too wide, `width="100%"`, lazy equals one-pass); the
existing "cannot narrow" test now uses a wide table. `COVERAGE.md` still says a table starts
below a float; not edited here.
**Not verified.** A table beside a float inside a list item, quote or `<details>`; a table
with `colspan`/captions beside a float; right-to-left text. Not looked at by the owner.

**Release note.** A table that fits in the room a floated image leaves now stands beside it
instead of starting below.

## What ran

All with `CARGO_TARGET_DIR` set outside the worktree: `cargo fmt --all`, `cargo clippy --locked
--workspace --all-targets -- -D warnings`, `cargo test --locked --release --workspace`.
`tests/shots.sh` was run into one folder before the first change and into another after the
last one (release build, `--no-session`), the new fixtures moved aside for the second run
because the Files pane lists the fixtures folder: all 108 files were byte-identical. The three
new shots were rendered and looked at; no window was opened and nothing of the owner's was
touched.
