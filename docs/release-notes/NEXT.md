Adds Copy as Markdown, a slide list and links in presentations, wiki-link and HTML rendering gaps, Vue and Svelte highlighting, menu access keys and the snap-layouts flyout, and returns windows to their place after a side-by-side preview.

## What is new

- After mdedit's "Preview side by side", the window returns to the place and size it had before
  once the preview ends, the previewed tab is closed, or mdedit gives the arrangement up. A
  window you have moved or resized in the meantime stays where you put it. Needs the mdedit of
  the same release.
- Copy as Markdown: right-click a selection and choose "Copy as Markdown" (also in the command
  palette) to copy the Markdown source of the selected blocks exactly as it is in the file.
- Presentation mode: links on a slide can be clicked (a link to a heading goes to the slide that
  holds it), text on a slide can be selected with the mouse and copied with Ctrl+C, and G lists
  the slides by number and title; Enter goes to one and Escape closes the list.
- The Outline pane can be used from the keyboard: choose "Focus outline" in the command palette
  or click a row, then move with the arrow keys; Enter goes to the heading and Escape returns to
  the page.
- Vue and Svelte files, and `vue` and `svelte` code blocks, are now syntax highlighted.
- Menus show underlined access keys. Hold or press Alt with a menu open to see them, then press the
  letter to pick the item. This is in the "..." menu and the tab, page and find box menus.
- On Windows 11, pointing at the maximize button shows the snap-layouts flyout.
- On KDE Plasma (Wayland), Settings > Window backdrop blurs the desktop behind the title row, bar
  and side panes. It was written without a Linux machine to run it on and has not been seen working.
- Wiki-links: a label may contain emphasis, strong text and code (`[[Note|a *b* c]]`);
  `![[picture.png]]` shows the picture and `![[Note]]` links to the note, when they are found next
  to the document; `[[Note#^block]]` opens the note at that block; `[[#Heading]]` links within the
  document even when no folder is open.
- A table that fits in the room a floated image leaves now stands beside it instead of starting
  below, and `<br clear="all">` (also `left` and `right`) makes the content after it start below
  floated images.
- An `<abbr title>` is underlined with dots and shows its title when the pointer rests on it. A
  `<figure>` has the same 40 px side margin as in a browser. A ruby annotation stays over its base
  when the line would have broken inside the base.
- An exported HTML file follows the reader's light or dark browser setting, and tables keep the
  widths and placement their HTML gave them.
- Find in folder follows the folder: results update when files change, and opening a result goes
  to its own match inside a long paragraph.

## Fixed

- When mdedit's caret moves up through a long paragraph, the page now follows before the caret's
  line leaves the top of the window.
- A window that is completely covered by another no longer keeps animating GIFs and WebP images
  on Windows. On X11 and macOS, where the system reports a covered window, animation also stops
  and nothing is drawn until the window is visible again.
- A note or a folder created beside the open document or below the open folder is picked up at
  once, with the Files pane open or not.
