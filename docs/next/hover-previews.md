# hover-previews

## README

Feature list: **Hover previews.** Rest the pointer on a footnote mark and a small card shows the
footnote's text; on a link to a heading of the same page, the heading and the first lines under
it; on a link to another local Markdown file, its title and first lines. Web addresses keep their
hint at the bottom left. Settings, Content: "Preview footnotes and links" (on by default).

No new shortcut (a pointer feature). The card goes away on any key, press, wheel or when the
pointer leaves the link.

## DESIGN

- Trigger: the pointer rests (the tooltips' 500 ms; moving on the mark before the card shows
  starts the wait again) on a footnote reference, or on a link whose target is `#heading` of the
  open page (a heading only; a bare anchor has no card), or a local Markdown file (with or
  without `#heading`). Never for `http(s)`/`mailto`, wiki-links, missing targets, other file types.
- Card: title (semibold: "Footnote N", the heading, or the file's first heading) and up to 6
  wrapped lines of text, at most 380 px wide, last line ends in an ellipsis when cut. The body is
  at most 360 characters (`mdview_core::preview`), whitespace made single spaces, code, rules,
  math and pictures skipped, a heading ends it (a footnote has no such end). Placed 12/20 px right
  and below the pointer where it came to rest, above it when there is no room, kept inside the
  window. Same frame, shadow and palette as the tooltips; text in the UI font (the shared
  Painter has no page font).
- A footnote without a definition has no card. A file link with a fragment shows that heading and
  its lines; an unknown fragment shows the file's start. A file that cannot be read has no card.
- Dismissal: the pointer leaving the mark/link (small moves on it keep a shown card), any key,
  any press, the wheel, the window losing focus or the pointer leaving it, history travel, a
  setting turned off.
- Files: read at the moment the card is due, on one short-lived `preview` thread per file, at most
  128 KiB of it, parsed with the page's own parser; the answer is woken into the event loop
  (`Wake::Preview`). 12 answers are kept (newest last); a kept answer is used only while the file's
  modification time is unchanged. A second rest on the same link is answered from the cache.
- Setting `previews` in the session file (default on, written as `previews=1|0`); row
  "Preview footnotes and links" under Content in Settings; id 30 in `settings.rs`.

## Release note

Resting the pointer on a footnote, or on a link to a heading or to another local Markdown file,
shows a small card with its text; a setting turns it off.

## Handoff

Built: `crates/core/src/preview.rs` (card text from a parsed document: footnote, heading, start of
a document; unit tests), `bins/mdv-app/src/preview.rs` (Target, Card, file loading and cache, card
layout and drawing; unit tests), `bins/mdv-app/src/app/cards.rs` (wanting, timing and resolving
the card), hooks in `workspace.rs` (`Hover::preview`, `previews()`, `preview_text`), `ui.rs`
(`Ui::card`, `Ui::previews`), `app.rs` (`Wake::Preview`, drawing, overlay signature, dismissal),
`app/input.rs`, `app/commands.rs`, settings row, `fixtures/previews*.md`, eight shots in
`tests/shots.sh`, app-level tests at the end of `app/tests.rs`.

Decisions: the card is drawn in mdview (not a new `Bubble` kind in the shared crate) because it is
multi-line and nothing in the editor needs it yet; move it to `mdshared-ui::overlay` when the
editor wants one. Text is the UI font, not the page's fonts/styles: the Painter only has the UI
faces and the page renderer cannot draw into an overlay without a layout pass; the card follows
the tooltips' look instead. The renderer does support footnotes, so footnote cards are included.
Settings id 30 is far from the existing ones to avoid clashes with other branches.

Not verified: Windows (CI only), release-build budgets (startup, memory, scroll; nothing runs at
launch or on the scroll path: `Hover::preview` is computed only for a pointer on a link or footnote
mark and costs a link lookup; the thread starts only on a rest over a local Markdown link),
behaviour in a real window (not run on this machine), HiDPI beyond the pinned shot scale.

Left out: cards for wiki-links and for links to headings of other files shown as "file#heading"
beyond what is above; images or formatting inside the card (plain text only); a delay setting.
