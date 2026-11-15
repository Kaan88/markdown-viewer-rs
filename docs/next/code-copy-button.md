# code-copy-button

## README

Feature list: **Copy button on code blocks.** Point at a code block and a small copy button appears at
its top right; one click copies the block's code exactly (no line numbers, without the newline that
ends its last line) and the button shows a check mark for a moment. "Copy code block" in the command
palette copies the block the selection starts in, else the first code block in view.

Shortcut table: none (the command has no key; it is in the command palette, `Ctrl+P` then `>`).

## DESIGN

- The button is drawn over the page, never part of the layout: nothing shifts, and it is not drawn
  when the pointer is elsewhere. 24 x 24 logical px, 6 px in from the block's top and right edges,
  filled with the code background, a hairline edge, the shared COPY glyph (muted; text colour when
  hovered; pressed fill when pressed). The tooltip is "Copy code" in the usual style.
- It shows for the code box under the pointer, also when the pointer is beside the text in the box,
  and stays while the pointer is on the button. Not while a menu, dialog, the palette or a selection
  drag is open or going, and not for boxes shorter than 36 px or narrower than 72 px (the first line
  would be covered).
- The block's language label sits in the same corner: it gives way while the button shows (a patch of
  the code background is drawn under the button, from the label's left edge).
- When the top of a tall block has scrolled out of the viewport the button stays at the top of the
  viewport (inside the block) so it remains in reach; otherwise it scrolls with the page.
- A click copies the block's source text from the document: exact, tabs kept, trailing newline of the
  last line dropped (a block ending in blank lines keeps the inner ones). The check mark (accent
  colour) shows 1.5 s (`codebtn::COPIED_FOR`), then the page region is redrawn by the timer.
- "Copy code block" (CommandId::CopyCodeBlock, Menu::None, no key): the code block the selection starts
  in, else the first code block in view with text; the toast says "Code copied", or "No code block
  in view". Diagram and math blocks have no button (their source is offered by the page context menu).
- Cost: nothing unless the pointer is over the page. Per page frame with the pointer there it is
  one hit test (the renderer already records each code box beside the lines).

## Release note

Code blocks have a copy button: point at one and click to copy its code, or use "Copy code block" in
the command palette.

## Handoff

Built: `Code` (box rect, text range, label width) recorded by the pager in `crates/render` beside the
lines, reported in `Hit::code`; `Workspace::code_spot`, `code_text`, `code_in_reach`; the new module
`bins/mdv-app/src/codebtn.rs` (geometry, drawing, the spot under the pointer); `Hit::CopyCode`;
`Ui::copied`; the page region's signature includes the spot and the check; shot flag `--copied`
(check mark for the block under `--hover`); `--hover` over the page now composes once and moves
again, so a control that shows only under the pointer is hovered in shots. Tests: renderer hit test,
`codebtn` geometry, app-level (button appears only on code, stays on the button, hidden with a menu
open, palette entry, command text), 4 shots in tests/shots.sh.

Decisions: no shared-crate change (glyph::COPY and glyph::CHECK exist). Sticky-at-top-of-viewport for
tall blocks. The label gives way instead of the button moving beside it. No setting.

Not verified: Windows and release-build budgets (not measured: nothing runs at startup or on the
scroll path unless the pointer is over the page, where one hit test per page frame is added); the
clipboard write itself in a real window (the click path calls `platform::set_clipboard_text` as
the other Copy commands do; tests do not write the clipboard); the 1.5 s timer in a real window.

Left out: Mermaid / math blocks (their drawn boxes carry the context-menu "Copy source"; a button
over a picture was not natural); a "Copied" tooltip text while the check shows.
