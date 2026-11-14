# export-print

## README

Feature list: **Export as HTML** saves the open document as one self-contained `.html` file
(the page's light look, your page font and width, code colours, tables, task boxes, footnotes,
formulas and diagrams as pictures, local images embedded). **Print (opens in browser)** opens a
printable copy in the default browser, which shows its own print dialog.

Both are in the main menu (File items) and the command palette (`>export`, `>print`); no shortcuts
(Ctrl+P is the palette).

## DESIGN

- Commands `ExportHtml` ("Export as HTML…") and `Print` ("Print (opens in browser)"); enabled with a
  document open. Icons: DOWNLOAD and OPEN_WITH from the shared set.
- Export: Save dialog (`pick_save_async`, helper thread, suggests `<name>.html` in the document's
  folder; `.html` is added when missing). The page is made on that thread; the window shows
  "Exporting…", then "Exported to <file>" or "Could not save the HTML file". One export at a time.
- Print: writes `<temp>/mdview-print/<name>-<ms>.html` (files older than a day are removed at the
  next print) and opens it with `platform::open_file` (html/htm only). The page opens every
  `<details>` and calls `window.print()` after load; without scripts Ctrl+P works.
- The page is always the light theme with the reader's page font, code font and page width
  (Narrow 700, Wide 1000, Full no limit). Window size and zoom are not used.
- Images: local files are embedded as data URIs (png, jpeg, gif, webp, svg) up to 4 MiB each and
  24 MiB in all; larger ones link by `file://` (this machine only). Remote images are linked only
  when the reader loaded them (tab allowed or "always"), else their alt text. Nothing is downloaded.
- Formulas and diagrams: drawn by the engine process at 2x and embedded as PNG (source as alt text).
  If the engine is missing or rejects them, the source stays in a labelled code box.
- Links keep only http, https, mailto and relative addresses; wiki-links are plain text.
  Headings and explicit anchors keep ids. Footnotes are listed at the end. Print style sheet: white
  page, no break inside code/tables/images, external link addresses shown after links.
- Writer: `crates/render/src/export.rs` (`export_html`), from the document model.

## Release note

Export as HTML saves a document as one self-contained page; Print opens a printable copy in your browser.

## Handoff

Built: render `export` module and tests (unit and `tests/export.rs`, which runs the engine when
`MDV_ENGINE` or target/release has it; `MDV_EXPORT_DIR` dumps the pages), app `app/export.rs`,
commands, menu/palette entries, Workspace `export_theme` and `remote_images_allowed`, shots.
The pin moved to shared fc1c559 (`platform::open_file`, branch f/export-print there).
Decisions: light theme only; no setting; print via browser. The copy-as-rich-text writer
(`rich.rs`, branch f/copy-rich-text) was not on next, so this is a separate full-document writer;
the two could be unified later.
Checked by eye: exported pages rendered in headless Chrome on Linux. NOT verified: Windows
(Save dialog thread, ShellExecute, temp path), a real browser print dialog, budgets (release builds
not made; nothing runs at startup or on scroll/keys), no shot of the status toast (no flag shows it).
Left out: table widths from HTML attributes, dark export, PDF, native printing.
