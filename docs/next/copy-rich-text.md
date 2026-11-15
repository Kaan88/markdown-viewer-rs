# Copy as rich text (copy-rich-text)

## README

Feature list: **Copy keeps the formatting.** Copying a selection (Ctrl+C or Copy in the context menu)
puts plain text and HTML on the clipboard together, so a paste into a word processor or a mail keeps
headings, emphasis, links, lists, tables and code, and a paste into a text field gets the plain text
as before. No new shortcut, no setting.

## DESIGN

- What a copy put on the clipboard before: only the selected slice of `Document::text`
  (`CF_UNICODETEXT` on Windows, `text/plain` through arboard elsewhere). That text is unchanged.
- Now `Workspace::selected_html` builds an HTML fragment for the same range with
  `mdview_core::rich::selection_html`, from the parsed document model (never from the source), and
  `platform::set_clipboard_html(html, text)` (mdshared-ui) writes both. If writing both fails the
  app falls back to plain text alone.
- Windows: `CF_UNICODETEXT` plus the registered format "HTML Format" (CF_HTML, UTF-8, header with
  ten-digit byte offsets, `StartFragment` and `EndFragment` comments). Linux: `text/html` (a document
  with a charset declaration) and `text/plain` through arboard, which serves both on X11 and on
  Wayland (wlr data-control, as the text copy already did).
- The HTML is clipped to the selection: a partly selected paragraph gives that part, a partly
  selected span (emphasis, link) is cut at the range, a container (list, quote, table, row) is
  written only when something inside it is selected. In a table row that has any selected cell all
  cells are written, those outside the selection empty, so columns keep their place.
- Mapping: paragraph `p`, heading `h1..h6`, quote `blockquote`, alert a quote with its label in
  bold, list `ul`/`ol` (`start` kept), items `li` with task boxes as a ballot-box character, code,
  diagram and formula blocks `pre code` with their source, rule `hr`, table with borders, `th`/`td`
  with `colspan`, `rowspan` and `text-align`, details as a div with the summary in bold, footnote
  definitions and references as `sup`, definition lists, figures. Inline: `em`, `strong`, `del`,
  `u`, `code`, `kbd`, `sub`, `sup`, `mark`, `small`, `q`, `ruby`/`rt`, a hard break as `br`, inline
  formulas as `code` with their TeX.
- No external resources: an image is written as its alt text (nothing is fetched on paste).
  A link is kept only when it is http, https or mailto; relative links, notes (wiki-links), fragment
  links and other schemes become plain text. All text and attribute values are escaped; raw HTML of
  the source never reaches the output (only the parsed model does).
- Left out: "Copy as Markdown". The document model keeps no source positions (see `tasks.rs`) and
  the parsed text is not the file's bytes (front matter and math are rewritten first), so a
  selection cannot be mapped back to source ranges reliably.

## Release note

Copying text from a page now also puts it on the clipboard as HTML, so pasting into Word, LibreOffice
or a mail keeps headings, emphasis, links, lists, tables and code. Plain-text pastes are unchanged.

## Handoff

Built: `crates/core/src/rich.rs` (HTML from the model, with unit tests for each block and span kind,
clipping, escaping, links, images, table rows), `Workspace::selected_html` and its test, `copy()`
in `app/commands.rs`. Shared: `mdshared_ui::cf_html` (CF_HTML header arithmetic and the HTML
document, unit-tested on every OS) and `platform::set_clipboard_html` (branch `f/copy-rich-text` of
markdown-shared-rs, pinned in the three Cargo.toml files that name mdshared-*).

Decisions: no new menu entry or setting (the existing Copy does it); alignment of paragraphs and
headings is kept as `text-align`; the HTML of a closed `<details>` body is included when the
selection spans it, as the plain text is; images as alt text rather than data URIs.

Not verified: a real paste into Word, LibreOffice, Thunderbird or a browser on Windows, X11 and
Wayland (only unit tests and CI; the clipboard cannot be exercised on CI runners except the
Windows round trip of the text); that arboard offers `text/html` with `text/plain` as expected by
each Wayland compositor; no budgets were measured (nothing runs at startup or on keystroke or
scroll; the HTML is built only when Copy runs, in time proportional to the selection).

Left out: Copy as Markdown (see DESIGN); no screenshot was added because no new visible state
exists (the status line text is the existing "Copied").
