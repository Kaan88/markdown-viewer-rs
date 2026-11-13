# Port checklist: Kaan88/markdown-viewer-cpp to Rust

All six files were read in full, plus selected parts of `src/document.cpp`. Nothing was run, so shortcomings come from reading the code and the Qt docs.

Tags:
- **[BUG]**: a defect visible in the code; don't port it.
- **[INFER]**: my reasoning from the code or Qt behaviour, not observed. Treat it as UNVERIFIED.

The saved `window.cpp` dump, with 4 stderr lines before the source, is at `/home/kaan/.claude/projects/-home-kaan-markdown-viewer/354efa0d-cde2-4ce7-8746-f07ca6433a9c/tool-results/bpcy5jj1w.txt`.

## 1. window.cpp behaviours

### Window and tabs
- **Window (373-381):**
  - Default size is 1220x820, minimum 720x480.
  - Accepts drops; dock nesting is off.
  - Title is `<file> — Markdown Viewer C++`, or `Markdown Viewer C++` with no tab (721-730).
- **Tabs (378-386):** document mode, closable, movable, and the tab bar auto-hides with one tab. There is no middle-click close. Closing the last tab shows the empty state, not an app exit.
- **Empty state (387-407):**
  - Object name `emptyState`, in a stacked widget.
  - Title "Markdown Viewer" at 1.6x size, DemiBold.
  - Hint "Open a Markdown file or drag one into this window."
  - Button "Open Markdown file…" with an icon.
- **Status bar:**
  - Shows the current path with no timeout, plus a path tooltip.
  - Startup with no tab: "Open a README or drop a Markdown file here." (672).
  - After closing the last tab: "Open a Markdown file or drag one into this window." (689).
- **Tab context menu (411-433):** only over a tab.
  - Close tab.
  - Close other tabs (reverse order).
  - Copy file path.
  - Show in File Explorer (`explorer.exe /select,`). This is Windows-only, so it needs a Linux equivalent.
- **Tab labels (691-720):**
  - The default label is the filename, with the full path as tooltip.
  - Filenames that match case-insensitively become `name — parentDirName`.
  - If a duplicate also has the same parent directory (case-insensitive), the label is `name — full native path`.
  - **[BUG]** `a/docs/README.md` and `b/docs/README.md` get identical labels.
- **Tab cycling (636-645):** Ctrl+Tab and Ctrl+Shift+Tab wrap, and only act with more than one tab.

### Menus and shortcuts

| Menu | Item | Shortcut | Notes |
|---|---|---|---|
| File | Open files… | Ctrl+O | Multi-select; filter `Markdown (*.md *.markdown *.mdown *.mkd);;All files (*)`; starts in `folder` |
| File | Open folder… | Ctrl+Shift+O | |
| File | Refresh | F5 | Status "Refreshing document…" has no timeout and nothing clears it **[BUG]** |
| File | Open recent (submenu) | | Object name `recentMenu` |
| File | Manage recent files… | | |
| File | Clear recent files | | Persists at once; 2.5 s message "Recent files cleared" |
| File | Close tab | Ctrl+W, Ctrl+F4 | |
| File | Close all tabs | Ctrl+Shift+W | Also clears the search box |
| File | Exit | QKeySequence::Quit | |
| Edit | Find | Ctrl+F | Focuses search and selects all |
| Edit | Find next / previous | F3 / Shift+F3 | |
| Edit | Clear search | | |
| View | Theme: System / Light / Dark | | Exclusive radio group |
| View | Zoom in / out | ZoomIn / ZoomOut | Step 10 |
| View | Reset zoom to 100% | Ctrl+0 | |
| View | FILES / ON THIS PAGE toggles | Ctrl+Shift+1 / 2 | Dock toggle actions |
| View | Focus reading mode | Ctrl+Shift+F | Checkable |
| Help | Keyboard shortcuts | | Message box listing the shortcuts (623-630) |
| Help | About Markdown Viewer C++ | | Version plus "A native, read-only Markdown reader for Windows." |

**Escape (646-655):**
- In focus mode, it exits focus mode.
- Otherwise, if search has focus, it clears search and focuses the reader.

**Toolbar (441-522):**
- Object name `toolbar`, title "Reader", not movable, text beside icons.
- Items: Open files…, Open folder…, Refresh, separator.
- Search `QLineEdit`: placeholder and accessible name "Find in document", clear button, max width 260.
- Match label: object name `searchResultCount`, min width 66, centred.
- Previous and Next, with tooltips naming Shift+F3 and F3.

**Docks:**
- FILES on the left and ON THIS PAGE on the right, initial widths 215 and 200 (656).
- Object names are `files` and `outline`. They keep Qt's default closable, movable and floatable behaviour.

### Search (329-367, 499-508, 731-744)
- Plain substring and case-insensitive (default flags). No regex, whole-word or case options.
- Runs on every keystroke with no debounce. Typing highlights but does not scroll; Enter or F3 moves to a match.
- All matches get background `#facc15` and foreground `#172033`. The active match is the normal text selection.
- Highlights are capped at 10,000, so the count also stops there.
- Next and previous wrap around.
- It re-runs on tab change and after a re-render of the current page.
- Label text: empty, `No matches`, `N matches`, or `A of N`. Tooltip: `N matches in this document` or `No matches in this document`.
- **Quirk:** with no selection, the caret position can equal a match start, which shows `1 of N` instead of `N matches` (362).
- **[INFER]** `setTheme` calls `setHtml`, and `applyTheme` does not re-run find, so theme or zoom changes probably lose highlights while the count stays.

### Zoom and theme
- **Zoom (565-584, 301-302):**
  - Range 60-250, step 10, default 100, clamped on load.
  - It only sets the default font to `11pt * z/100`. Fixed-size images and diagrams don't scale.
  - No percentage is shown anywhere.
  - Every open tab re-runs `setHtml`, and the raw scrollbar value is restored, so position drifts.
  - **[INFER]** `QTextEdit`'s built-in Ctrl+wheel zoom may also apply, separately from the app's zoom state.
- **Theme options (548-564, 958-1007):** setting values are 0 System, 1 Light, 2 Dark.
- **How the theme is applied:**
  1. `styleHints()->setColorScheme(Light / Dark / Unknown)` with signals blocked.
  2. A check for Windows high contrast, which uses `standardPalette()`.
  3. Otherwise, a hard-coded `QPalette` set application-wide. "System" only picks light or dark, so accent colours are not the OS's.
  4. `colorSchemeChanged` re-applies the theme only in System mode.

| Role | Dark | Light |
|---|---|---|
| Window | #202a36 | #f5f6f8 |
| WindowText, Text, ButtonText | #e6edf5 | #263548 |
| Base | #151d28 | #ffffff |
| AlternateBase | #263342 | #f0f2f5 |
| Button | #293648 | #f5f6f8 |
| Highlight | #176b91 | #176b91 |
| HighlightedText | white | white |
| Link | #8bd5f5 | #075f89 |
| Mid | #465668 | #c7cdd5 |
| Disabled Text | #a6b4c7 | #667588 |

### Focus mode (745-774)
- Hides the menu bar, toolbar, status bar and both docks, remembering each one's prior visibility. The tab bar stays and the window does not go fullscreen.
- Exiting restores visibility and focuses the reader.
- Closing the window exits focus mode first, so saved state is not the hidden layout (1069).
- **[INFER]** Ctrl+F focuses a hidden search box, and menu shortcuts may stop working while the menu bar is hidden.

### Recent files (794-841, 899-903, 1034-1057)
- Max 15, most recent first, de-duplicated.
- Every `open()` adds an entry, including link-followed files and tabs restored at startup. Restoring therefore reorders the list.
- **Menu:**
  - Shows the filename, plus ` (missing)` if absent.
  - The path tooltip is set, but `toolTipsVisible` is not, so it likely doesn't show **[INFER]**.
  - Clicking a missing entry removes it, with the message "Removed a missing file from Recent".
  - An empty list shows a disabled "No recent files".
- **Manage dialog (620x360):**
  - List of native paths; missing entries get ` (missing)` and disabled colour.
  - Buttons: Remove selected, Clear list, Done.
  - Double-clicking an existing file opens it and closes the dialog.
- `open()` does not persist the list. It is only written on close, clear or remove, so a crash loses additions.

### Folder tree (587-602, 853-856)
- `QFileSystemModel` updates live.
- Directories plus `*.md *.markdown *.mdown *.mkd`; other files are hidden, not greyed out.
- Hidden files and `.`/`..` are excluded, and empty directories still show.
- Only the name column shows, with no header.
- Only double-click opens a file, so Enter does not **[INFER]**. No context menu, and the current file is not highlighted.
- The root is set by Open folder, by `open(<dir>)`, by the first opened file's parent if no folder is set, or from settings.

### Outline (907-957)
- Rebuilt on tab change and after a re-render, then fully expanded.
- Nesting is a level stack, so H1 then H3 makes the H3 a child of the H1.
- Single click scrolls to the anchor. Keyboard activation is not wired.
- **Sync:**
  - Runs on each scrollbar change of the current page.
  - Takes the document position at viewport point (2,2).
  - Selects the last heading at or above it (ties go to the later item in breadth-first order).
  - If none qualifies, the selection stays as is.

### File watching and reload (172-178, 188-273)
- There is no OS watcher. Each page has an 800 ms timer comparing `mtime_ms:size`, or `missing`. Background tabs poll too.
- A tick is skipped while a worker runs. That and the poll interval are the only debounce.
- The stamp is captured before the render, so a change during rendering is caught on the next tick.
- **Deleted file:**
  - Keeps the last content, with status "File was deleted or is unavailable. Showing the last loaded content."
  - Never-rendered file: "File not found. Choose another file or remove this entry from Recent."
- **Replaced file:** the stamp changes and it reloads automatically.
- **After an auto or manual refresh:**
  - "Document refreshed from disk" shows for 2.5 s, only if there were no errors or warnings.
  - "Rendering…" shows on every reload, which shifts the layout **[INFER]**.

### Scroll preservation (207-216, 289-295)
- Priority is pending link anchor, then the nearest anchor at or above the viewport top, then the raw scrollbar value.
- So any document with headings snaps back to the heading on reload rather than keeping its exact position **[BUG]**. Port it as a true position restore.

### Link following (1008-1033)
1. `details:N` toggles N in `expanded` and does a full reload.
2. A relative URL with an empty path scrolls to the fragment.
3. Anything else is resolved against the document's directory.
4. Local `md`, `markdown`, `mdown`, `mkd` (case-insensitive) opens in a tab or switches to an existing one, then goes to the anchor.
5. Other local files show "Only Markdown file links open inside the viewer."
6. http, https and mailto go to the system handler.
7. Every other scheme is silently ignored.

There is no back or forward history.

### Details (document.cpp:271-291)
- `details` is rewritten to `<p><a href="details:ID">▶/▼ summary</a></p>`, with the body included only when open.
- The default summary is "Details".
- IDs are ordinal, so editing the document can shift which block is open.
- Each toggle spawns a full subprocess render.

### Opening and drag/drop (857-906, 1058-1067)
- A directory sets the folder.
- Paths are canonicalised, falling back to the absolute path if the file is missing.
- An already-open path switches tab rather than opening a duplicate.
- Drop accepts any URL and opens every local one, with no suffix filter.

### Images (52-78, 105-154)
- **Size limits:** 12 MiB and 24 MP.
- **SVG:**
  - Detected by the substring `<svg`.
  - Limited to 8000x12000.
  - Rasterised at a fixed 2x.
- **Schemes:**
  - `asset:` reads base64 from the worker's JSON.
  - `data:` accepts base64 only.
  - Local files are unrestricted.
  - Anything else returns nothing.
- **Remote:**
  - Fetched automatically with no prompt.
  - 10 s timeout, `PreferCache`, 64 MiB disk cache in `CacheLocation/images`.
  - The placeholder is 160x28 with text "Image loading / unavailable" and fixed colours `#e2e8f0` / `#475569`.
- **[BUG]** `pending` is never cleared, so failed URLs never retry during a page's life.
- **[INFER]** Layout probably jumps when an image arrives.

### Settings and geometry (369-372, 657-673, 775-793, 1068-1084)
- INI file at `AppLocalDataLocation/settings.ini`.
- Keys: `tabs`, `active`, `recent`, `folder`, `theme`, `zoom`, `geometry`, `layout`.
- All are saved on close. `recent` is also saved on clear or remove.
- `geometry` and `layout` are opaque Qt blobs, so old settings can't be migrated as they are.
- On startup, `ensureWindowVisible` clamps the window to the available area of its screen.
- Tabs are restored even if files are missing, and those show the not-found status.

### Worker process (188-273, 181-187)
- Each reload re-executes the app as `<exe> --render <path>`. `MDVIEWER_RENDERER` overrides the executable (tests use this).
- The child environment forces `QT_QPA_PLATFORM=windows`.
- **Input:** stdin JSON `{"expanded":[ints]}`, then the channel is closed.
- **Output:** stdout JSON `{html, headings:[{level,title,slug}], assets:{uri:base64}, warnings:[], error?}`.
- A 15 s deadline kills the worker.
- A generation counter discards stale results. Superseded workers are disconnected and killed. The destructor kills and waits 300 ms.
- **Error text:**
  - `Cannot start native renderer: …`
  - `Could not render document: <first 600 chars of stderr>`
  - the `error` field from the JSON
  - warnings joined by newlines
- **[BUG]** After a timeout kill, the finished handler overwrites "Rendering exceeded 15 seconds. Try a smaller document." with "Could not render document: ".
- Nothing limits concurrency across tabs, so session restore starts one worker per tab.

## 2. Stylesheet and QTextBrowser limits

**Exact values (94, 301-322):**
- Document margin 28, no frame, `QTextBrowser { border:0; }`.
- Font "Segoe UI" 11pt * zoom, which needs a Linux replacement.

```css
body {color:%fg;background:%bg;} a{color:%link;}
pre,code{font-family:Consolas;background:%AlternateBase;} pre{padding:12px;}
table{border-collapse:collapse;} th,td{border:1px solid %Mid;padding:7px;}
blockquote{margin-left:12px;color:%DisabledText;} h1,h2{margin-top:24px;}
img{vertical-align:middle;}
```

**What Qt's rich-text subset allows** ([Qt docs](https://doc.qt.io/qt-6/richtext-html-subset.html), read through a summariser):
- Not listed: `border-radius`, `box-shadow`, flexbox, `overflow`, `position`, `max-width`, `<details>`, `<summary>`, `<svg>`.
- `padding` is described for table cells, so `pre{padding:12px}` likely does nothing **[INFER]**.
- Border support on general block elements is not confirmed.

**Workarounds that reveal limits:**
- `details:` URLs plus a subprocess re-render stand in for a details widget.
- SVG is rasterised at 2x, so it is not crisp at higher DPI or zoom.
- `loadResource` is synchronous, which forces placeholders and a later `markContentsDirty`.
- Heading positions are found by walking fragment `anchorNames`.
- Search highlights use `ExtraSelection`.
- Theme and zoom changes need a full `setHtml`.
- Task-list inputs become plain `☐` and `☑` glyphs (document.cpp:306-310).
- Alerts only replace `[!NOTE]` with `<b>NOTE</b><br/>` inside a blockquote (document.cpp:337), with no colour, icon or border.

**Rendering and design shortcomings to fix:**
- No rounded or bordered code blocks, copy button, or line numbers.
- Long code and wide tables have no per-block horizontal scrolling **[INFER]**.
- Blockquotes have no left bar, only indent and muted colour.
- Headings have no rules, and there is no real type scale or line-height control.
- No maximum reading width: text fills the viewport minus the 28 margin.
- Tables have no header fill, zebra rows or rounded edges.
- Alerts have no styled callouts, and details have no animation or widget.
- Highlighter colours are fixed (`#6a9955`, `#b65e3c`, `#7754b5`, `#247a86`) in both themes, so contrast on dark code backgrounds is questionable.
- Diagrams always have a white background, which glares in dark mode.
- Diagram text can't be selected or searched, and diagrams don't follow zoom.
- Sanitising removes all document CSS except the highlighter's own `color:#xxxxxx` spans (document.cpp:327-334). Tags outside the allow-list are dropped, keeping their children.

## 3. diagram.cpp (479 lines)

**Shared rules:**
- Source limit 64 KiB and 500 lines.
- Family is the first word of the first line, so front matter or `%%{init}` before it fails **[INFER]**.
- Failures return an error string. The document then shows a note and the raw source (document.cpp:216-218).
- Hand-drawn canvases are capped at 8000x12000, with a white background, "Segoe UI" 11, `#334155` pens and `#e0f2fe` rounded boxes.
- Helpers are about 48 lines and the dispatcher about 26.
- Unsupported families return `Unsupported Mermaid family: X` (e.g. gitGraph, journey, timeline).

| Family | Lines | Layout |
|---|---|---|
| flowchart, graph, classDiagram, stateDiagram(-v2), erDiagram | 67-230, ~164 shared | Graphviz `dot`, SVG output |
| sequenceDiagram | 231-299, ~69 | Fixed grid |
| pie | 300-338, ~39 | Fixed |
| mindmap | 339-381, ~43 | One row per node |
| gantt | 382-452, ~71 | Fixed |

**Graph families, common:**
- Direction comes from `LR|RL|BT|TB|TD` on the first line.
- Limits are 150 nodes and 300 edges.
- Nodes are emitted from a `QMap`, so sorted by ID rather than source order.
- `%%` comment lines are skipped.

**Flowchart and graph:**
- `;` splits statements, including inside labels.
- Connectors are `-->`, `---`, `-.->`, `==>`. The `.` is an unescaped regex wildcard.
- All connectors draw the same solid arrow, even `---`.
- Labels only as `-->|text|`. `A -- text --> B` throws.
- Chains (`A-->B-->C`) work.
- Node IDs are `[\w.-]+`, probably ASCII-only **[INFER]**.
- Shapes: `[...]` box, `(...)` ellipse, `{...}` diamond. Nested brackets are stripped, so `((x))` is just an ellipse.
- `<br>` becomes a newline.
- `subgraph`, `style`, `classDef`, `click`, `&` and similar throw **[INFER: except where a line happens to match a bare ID]**.

**Class, state and ER:**
- **Relation regex:** `id ["card"] op ["card"] id [: label]`.
- **Edge labels:** the cardinalities and label are merged into one centred label, not placed at the edge ends.
- **Line style:** `..` makes the edge dashed.
- **Class:** `<|` and `|>` give hollow arrowheads. `*--` and `o--` get no composition or aggregation markers.
- **ER:** `{`, `}` and `o` map to crow, odot and tee arrowheads, with `dir=both`.
- **State:** `[*]` is a start point as source and an end point as target.
- **Declarations:** `class X`, `state X`, bare `X`, optionally with `{`.
- **Blocks:** lines inside `{ }` and `id : text` are appended to the node label. There are no class compartments, ER attribute tables, or nested state rendering.
- **Errors:** an unclosed block throws. `direction`, notes and `state "x" as y` throw **[INFER]**.

**Sequence:**
- `participant` and `actor` with `\w+` and optional `as Alias`; actors draw as boxes, and quotes in aliases are kept.
- Exact-line `autonumber`.
- Arrows match `--?>>?`, `--?x`, `--?\)`. A `--` prefix means dashed, and x and ) look the same as normal arrows.
- `+` and `-` before the target are consumed and ignored, so no activation bars.
- Colon text is required.
- `loop`, `alt`, `opt`, `par`, `note`, `activate` and similar throw.
- Limits are 1-24 participants and 150 messages.
- **Layout:**
  - Columns 170 apart starting at x=100.
  - Boxes 130x55 at the top only.
  - Rows 65 apart starting at y=115.
  - Self-messages draw a 55x24 loop.
  - Width does not grow with text.

**Pie:**
- `title ` line, and `"label" : number` entries that must be positive.
- `pie title X` on the first line is ignored, because parsing starts at line 2.
- 1-100 entries.
- Canvas is 800 x max(430, 80+29n), with a 330 px pie.
- Colours are `HSV((i*71)%360,115,215)`, starting at 3 o'clock and running counter-clockwise, so it differs from Mermaid **[INFER]**.
- Legend reads `label (x.x%)`.

**Mindmap:**
- Depth comes from indent character count, so a tab counts as 1.
- Exactly one root, 1-150 nodes.
- Only whole-text wrappers and `root(...)` are stripped, so `id[Label]` shows literally. `::icon` becomes a node **[INFER]**.
- **Layout:**
  - Left-to-right, one row per node.
  - Boxes 170x50.
  - Columns 210 apart, rows 65 apart.
  - Straight arrows, not radial.

**Gantt:**
- Accepts `title `, `section `, and the exact line `dateFormat YYYY-MM-DD`. Other directives (`axisFormat`, `excludes`, `todayMarker`) throw.
- Task format is `label : [done|active|crit|milestone,] [id,] start|after id, Nd|date`.
- Tags must be leading and are discarded with no visual effect.
- Only `Nd` durations are accepted, and an implicit start throws.
- Limits are 150 tasks and a 10-year span.
- **Layout:**
  - 1100 wide, 45 per row.
  - Label `section / label`, with a stray leading slash when there is no section.
  - Only the earliest and latest dates are shown, with no grid.
  - All bars look the same and have no text.

## 4. ui_tests.cpp

**Setup:**
- `QStandardPaths` test mode and separate org and app names.
- `MDVIEWER_RENDERER` points at the built app.
- Fixtures use a non-ASCII filename, `Türkçe test.md`, plus `linked.md`.
- Render waits time out at 15 s.

| Test | Asserts |
|---|---|
| `navigationSearchDetailsRefresh` (45-74) | Renders "Searchable content"; 1 heading; `find("content")` creates highlights; expanding details 0 and reloading reveals "Secret"; link `linked.md#second-page` makes 2 tabs and shows "Linked document"; external write reloads the original tab, even in the background; after deletion and 1200 ms the old content stays; recreating the file renders "fresh content"; `setTheme(true,150)` leaves reader Base equal to the app palette Base |
| `themeRoundTrip` (75-91) | Actions "Dark", "Light", "System" exist; Dark gives Base `#151d28`; Light gives white; System triggers without an assertion |
| `closeAllTabs` (92-109) | 2 tabs; "Close all tabs" gives 0 and the stack shows `emptyState` |
| `duplicateTabsSearchAndFocusMode` (110-155) | Two `README.md` files get different labels; title starts with `README.md`; "needle" shows `2 matches`; Find next shows `1 of 2`; "not present" shows `No matches`; focus mode hides the menu bar; Escape restores it |
| `manageMissingRecentFile` (156-189) | With a deleted recent file, the dialog list has 1 item containing "missing"; "Remove selected" empties it; the first action in `recentMenu` is "No recent files" |
| `scaledRendering` (190-212) | Opens `fixtures/showcase.md` at 1220x820 and waits for "Native Markdown Reader"; saves `cpp-100/150/200.png` to `artifacts/qa`; asserts only that saving works, with no image comparison |

The first and fourth tests rely on the render timing noted in the search quirk above. `manageMissingRecentFile`'s count of 1 also depends on `open()` not persisting recents.

## 5. Installer and packaging

**Install (`installer/viewer.iss`):**
- Per-user with no admin (`PrivilegesRequired=lowest`), to `{localappdata}\Programs\MarkdownViewerCpp`.
- x64 only. Publisher `Kaan88`. App name "Markdown Viewer C++". Default version 0.1.4.
- AppId `{7F65011E-D5AB-4A89-9583-B165724465F5}`, with a separate AppId and no uninstall entry for smoke tests.
- LZMA2 solid compression, modern wizard, licence page, `ChangesAssociations=yes`.
- Start Menu shortcut always. Desktop shortcut is an optional task, unchecked by default.

**File association (optional task, unchecked by default):**
- Only registers under `HKCU\Software\Classes\Applications\markdown-viewer-cpp.exe`.
- Sets `FriendlyAppName`, `shell\open\command` (`"{app}\exe" "%1"`), and `SupportedTypes` for `.md` and `.markdown` only.
- `.mdown` and `.mkd` are not registered, although the app opens them.
- It never sets itself as the default handler, and there is no ProgID.

**Artifacts (`scripts/package.ps1`):**
- Stage directory `dist/markdown-viewer-cpp-<v>-windows-x64/`.
- `artifacts/markdown-viewer-cpp-<v>-windows-x64-portable.zip`.
- `artifacts/markdown-viewer-cpp-<v>-windows-x64-setup.exe`.
- `artifacts/SHA256SUMS.txt`: lowercase hash, two spaces, filename.
- The version comes from `CMakeLists.txt` and must match `vcpkg.json`.

**Payload:**
- The exe, README, LICENSE, LICENSES, docs, fixtures and `build/Release/res`.
- `cmark-gfm`, `cmark-gfm-extensions` and `tinyxml2` DLLs.
- Graphviz 13.1.2 DLLs (`gvc`, `cgraph`, `cdt`, `pathplan`, `xdot`, `zlib1`, `libexpat`, `gvplugin_core`, `gvplugin_dot_layout`).
- Qt via `windeployqt`, plus `copy-runtime.ps1` (not read).

**Linux:** neither script covers it. `diagram.cpp`'s `__declspec(dllimport)`, the `explorer.exe` call, `QT_QPA_PLATFORM=windows`, and the Segoe UI and Consolas fonts are also Windows-specific.

**`fixtures/linked.md`:** the heading `# Second page`, then "A local linked document." and a `[Return](showcase.md)` link.
## Phase 1 status (core only)

Everything above is the old app's inventory, copied unchanged; nothing is checked off until a shell uses it. Core already carries these parts, with tests in `crates/core/tests/`:

- Find: plain case-insensitive search, cap 10,000, label and tooltip text, Turkish İ/ı, and a typed space finds a non-breaking one as Qt's did. Text in a closed `<details>` body has no hits, the same count as the old app: `find_visible` takes the shell's open state, and `find` uses the state as written. Highlighting, Enter/F3 and re-running on tab change or when a details block is toggled are shell work.
- Zoom range and step, theme setting values, recents (15, order, missing marker), session file. The old `settings.ini` is not read; `geometry` and `layout` are opaque to core.
- Link following rules and image source rules. The system handler call and fetching are shell work.
- Limits: 16 MiB on the bytes on disk, 64 KiB diagram, 4 KiB equation, and the image limits as constants. Diagram line limits per family wait for Phase 4.
- Command table: every menu item and shortcut in the inventory above, in `commands.rs`, plus the menu titles (`Menu::title`), the Theme submenu (`Menu::Theme`), the toolbar in its old order with Previous before Next and its tooltips (`TOOLBAR`), and the titles `TOOLBAR_TITLE`, `MANAGE_RECENT_TITLE`, `ABOUT_TITLE`, `SHORTCUTS_TITLE` and `MATCH_COUNT_NAME`. The status and hint strings carried are `STATUS_STARTUP`, `STATUS_ALL_CLOSED`, `EMPTY_HINT`, `EMPTY_BUTTON`, `REFRESHING`, `REFRESHED`, `RENDER_TIMEOUT`, `FILE_DELETED`, `FILE_NOT_FOUND`, `ONLY_MARKDOWN_LINKS`, `FIND_PLACEHOLDER`, `FIND_NO_MATCHES`, `DETAILS_DEFAULT_SUMMARY`, `IMAGE_PLACEHOLDER`, `FILES_DOCK`, `OUTLINE_DOCK`, `MARKDOWN_FILTER` and the recents strings. Not carried, because the worker process is gone: the old "Rendering…" status and its worker errors "Cannot start native renderer: " and "Could not render document: ". The old file dialog titles "Open Markdown" and "Open folder" are not strings in core either; each shell supplies its own caption with `MARKDOWN_FILTER`. "Show in File Explorer" on Windows is "Show in file manager" elsewhere. Access keys on individual menu items (`Open &files…`, `E&xit`) are not carried yet, only those on the menu titles. The Help text and window title say "Markdown Viewer" instead of "Markdown Viewer C++", and About no longer says "for Windows".
- Tab labels, including the fix for duplicate parents.
- "Document refreshed from disk" only after an earlier successful load, as in the old app. A tab's first load says nothing, or clears what was showing with empty text.
- Fixed state bugs: stuck "Refreshing…", timeout overwrite, failed images never retrying, snap to heading on reload (`remap_scroll`, which compares the two versions from both ends and never searches for matching text). Only the state logic exists; shells must call it.
- Paths: `canonical_or_absolute` gives ordinary `C:\x` and `\\server\share` spellings on Windows, keeping `\\?\` only for a path that needs it. The logic is tested here; the real `canonicalize` call has a `cfg(windows)` test that needs a W checkpoint to run.

**Deliberately different from the old app:**

- `details` open state is view state and uses a key from the summary text and its occurrence (`details_keys`), instead of ordinal IDs and a reload.
- A paired `<script>`, `<style>` or `<iframe>` now shows as literal text, as GitHub's tag filter does and as the plan decides; the old app dropped it silently with its content. `object`, `embed`, `head` and `template` keep their children, as on GitHub; the old app dropped those too.
- The Keyboard shortcuts dialog is generated from `commands::shortcut_help()`, one line per bound command (18 lines), where the old dialog grouped commands into 10 lines such as `Show Files / Outline  Ctrl+Shift+1 / Ctrl+Shift+2`. It cannot drift from the real bindings, and it includes the Exit line the old text lacked.
- The find counter shows `10000+` when capped, where the old one stopped at 10000 and looked exact.
- Invalid UTF-8 now loads with replacement characters and a warning, after the same retries. The old app refused the file. UTF-16 files with a BOM now load. Either can decode to more than 16 MiB of text, and only the bytes on disk count against the limit.
- A long query can stop early when enough of the file nearly matches it, showing a `+`, where the old find would block. A query up to 64 characters is exact; the real condition is in COVERAGE's known gaps.

**Not in core yet:** the file watcher (Phase 3). The old `math` and `families` tests belong to engine integration (Phase 4).

## WinUI shell status (2026-11-11, physical Windows PC)

Section 1 checked row by row against `bins/mdv-winui` after layout A (HANDOFF sections 26-27).
"Run" means exercised in the running app; "check" means a scripted `--*-test`; "code" means read
from the code only. Deliberate differences follow PLAN or the user's design choices.

| Behaviour (section 1) | WinUI | How known |
|---|---|---|
| Window: default 1220x820, min 720x480 | **differs**: 1100x900, no minimum | code |
| Window title `<file> — …` | `mdview - <file>` (taskbar only; the title bar shows tabs) | run |
| Accepts drops | done: files open in tabs, folders in the Files pane | **not run** (synthetic drag was refused by Windows) |
| Tabs: closable, movable, document mode | done | check (`--tabs-test`), run |
| Tab bar auto-hides with one tab | **differs**: always shown, it is the title bar | run |
| Closing the last tab shows the empty state | done (built-in welcome page; no Open button widget) | check |
| Status bar: path, startup and all-closed texts | **not done**: no status bar (counts sit by the find box) | run |
| Tab context menu: close, close others, copy path, show in Explorer | done | run |
| Tab labels incl. duplicate names | done (core `tab_labels`) | check |
| Ctrl+Tab / Ctrl+Shift+Tab | done | check |
| Menus | **differs** (layout A): command bar plus the ⋯ menu; every item reachable | run |
| Open files… Ctrl+O (multi-select, filter) | done | run |
| Open folder… Ctrl+Shift+O | done | code |
| Refresh F5 | done | run |
| Open recent / Clear recent / missing entries | done | run |
| Manage recent files… | done (dialog: Open, Remove, Clear list, Done) | run |
| Close tab Ctrl+W, Ctrl+F4; Close all Ctrl+Shift+W | done | check |
| Exit | done (⋯ menu); Ctrl+Q not bound | code |
| Find Ctrl+F, F3 / Shift+F3, Enter, Escape clears | done (find box, count beside it) | check, run |
| Theme System / Light / Dark | done (cycling button; System follows Windows at the click) | run |
| Zoom in/out, 60-250 step 10, Ctrl+0 | done; the percentage shows in the bar | check |
| Ctrl+Shift+1 / 2 (Files / Outline) | done | code |
| Focus reading mode Ctrl+Shift+F, Escape leaves | done (hides the bar and both panes; tabs stay) | run |
| Keyboard shortcuts, About | done (WinUI dialogs, logo in About) | run |
| Toolbar: Open, search box with clear button, match label, Previous/Next | done | run |
| Docks: Files left, outline right | done (panes; not floatable or closable as docks) | run |
| Search: substring, case-insensitive, cap, wrap, re-run on tab change | done (core `find_visible`) | check |
| Recent: max 15, order, missing marker, persisted on change | done (core; saved on every change) | check |
| Folder tree: live updates, Markdown filter | lazy `TreeView`, Markdown filter; **not live** | code |
| Outline: nesting, click scrolls | done (flat indented list, click scrolls); no scroll sync | check |
| File watching and reload, deleted-file handling | done: OS watcher (`mdview_core::watch`), deleted file keeps its text | run |
| Background tabs reload on change | reloaded when shown, not while hidden | code |
| Scroll preservation on reload | core `remap_scroll` | check (core tests) |
| Link following (fragments, local Markdown, http/mailto, others ignored) | done | check (`--outline-test`, core tests) |
| Details toggle | done (view state, no reload) | check (`--details-test`) |
| Images: limits, schemes | done (renderer + engine) | core and render tests |
| Settings: tabs, active, recent, folder, theme, zoom | done; `geometry`/`layout` not saved | check |
| Single instance | done; handoff before the Windows App SDK loads, about 20 ms | run (timed) |

**Open items from this table:** status bar texts, window size and minimum, live folder tree,
outline scroll sync, Ctrl+Q, window geometry in the session, and a hand-run drop.

The WinUI shell was retired on 2026-11-11 (branch `winui-archive`); the table above is history.

## New shell status (`bins/mdv-app`, 2026-11-12; differing rows gone through 2026-11-13)

Section 1 checked row by row against `bins/mdv-app/src` (the winit + softbuffer shell, binary
`mdview`), same rows as the WinUI table. "Test" means a unit test in `workspace.rs` or a headless
`App` test in `app/tests.rs`; "code" means the code was found and read, nothing more. **No row has
been checked by hand yet** (PLAN A3): every "done" below is built, not hand-checked, until the
user has used it and this table says "run".

On 2026-11-13 the rows marked "differs" or not done were gone through one by one. Two were
fixed or found already fixed (the Files tree, pane widths); the rest are differences kept on
purpose, each with its reason in the row. The user used the app on 2026-11-11 and 2026-11-12 and
approved it, but no row was recorded as a hand check then.

| Behaviour (section 1) | mdview (winit) | How known |
|---|---|---|
| Window: default 1220x820, min 720x480 | **differs, kept**: 1280x800, minimum 520x360; size, position and maximized state saved. The smaller minimum is possible because the command bar has no menu bar to fit | code |
| Window title `<file> — …` | done (core `strings::window_title`; the title row shows tabs) | code |
| Accepts drops | done: a folder sets the Files pane, a Markdown file opens, anything else says "Only Markdown file links…" | code; built, not hand-checked |
| Tabs: closable, movable, document mode | done (close button, middle click, drag to reorder) | test (`tab_close_button_and_middle_click_close_tabs`, `dragging_a_tab_reorders_tabs`) |
| Tab bar auto-hides with one tab | **differs, kept**: always shown, it is the title row (layout A, approved 2026-11-11); hiding it would leave the caption buttons alone in an empty row | code |
| Closing the last tab shows the empty state | done (empty state with an Open button and the recent files) | code |
| Status bar: path, startup and all-closed texts | **differs, kept**: no status bar; the texts show as toasts, and the file's full path is the tab's tooltip | test (`STATUS_ALL_CLOSED`); tooltip: code |
| Tab context menu: close, close others, copy path, show in Explorer | done (plus Close all tabs) | code; built, not hand-checked |
| Tab labels incl. duplicate names | done (core `tab_labels`) | code |
| Ctrl+Tab / Ctrl+Shift+Tab | done (`NextTab`/`PreviousTab` from core's command table) | code |
| Menus | **differs, kept** (layout A, approved 2026-11-11): command bar plus the ⋯ main menu; every item reachable | code, test (`menus_take_the_keyboard`) |
| Open files… Ctrl+O (multi-select, filter) | done (rfd, Markdown and All files filters) | code; built, not hand-checked |
| Open folder… Ctrl+Shift+O | done (rfd folder picker) | code; built, not hand-checked |
| Refresh F5 | done; keeps the reading position, also refreshes the Files tree | test (`reload_keeps_the_reading_position`) |
| Open recent / Clear recent / missing entries | done (Recent menu, missing entries marked) | test (`recents_remove_missing_and_clear`) |
| Manage recent files… | done (dialog with Open, Remove per row) | code; built, not hand-checked |
| Close tab Ctrl+W, Ctrl+F4; Close all Ctrl+Shift+W | done | code |
| Exit | done (⋯ menu, Ctrl+Q) | code |
| Find Ctrl+F, F3 / Shift+F3, Enter, Escape clears | done (find box, count beside it) | test (`find_box_types_searches_and_escape_clears`, `find_counts_and_wraps`) |
| Theme System / Light / Dark | done (cycling button; System follows the OS theme change event) | test (`theme_choice_resolves`) |
| Zoom in/out, 60-250 step 10, Ctrl+0 | done; the percentage shows in the bar; Ctrl+wheel zooms | test (`zoom_is_clamped_and_stepped`, `ctrl_wheel_zooms_and_plain_wheel_scrolls`) |
| Ctrl+Shift+1 / 2 (Files / Outline) | done | code, test (`toolbar_toggles_panes_and_the_layout_follows`) |
| Focus reading mode Ctrl+Shift+F, Escape leaves | done (hides the bar and both panes; tabs stay) | test (`focus_mode_hides_chrome_and_escape_leaves`) |
| Keyboard shortcuts, About | done (hand-drawn dialogs) | test (`shortcuts_open_and_close_dialogs`) |
| Toolbar: Open, search box with clear button, match label, Previous/Next | done | code |
| Docks: Files left, outline right | done (panes with splitters; not floatable) | test (`splitter_drag_resizes_within_limits`) |
| Search: substring, case-insensitive, cap, wrap, re-run on tab change | done (core `find_visible`); also re-runs after a reload | test |
| Recent: max 15, order, missing marker, persisted on change | done (core) | test |
| Folder tree: live updates, Markdown filter | done (2026-11-13): lazy tree, Markdown filter, hidden files skipped; the watcher follows the root and every expanded folder while the pane is open, so an entry created, removed or renamed there shows at once. Also refreshes on window focus, F5 and when the pane opens. Collapsed folders are read when expanded | run with `MDVIEW_TRACE=1` in a real window: one wake each for a new file, a rename, a delete and a new folder, none for a file in a collapsed folder or a write to a file that is not open; an expanded subfolder was not tried; the user has not looked at it |
| Outline: nesting, click scrolls | done; the current heading is highlighted as the page scrolls | code |
| File watching and reload, deleted-file handling | done: OS watcher (`mdview_core::watch`); a deleted file keeps its text and reloads when it returns | test (`a_deleted_file_keeps_its_text_and_reloads_when_it_returns`); live watching built, not hand-checked |
| Background tabs reload on change | done for loaded tabs (`check_changed`); unloaded tabs read the file when shown | code |
| Scroll preservation on reload | done, by content (core `place::Remap`): edits above and below, an edited or removed block, a page at its end follows the end; selection, find and open details kept | test (the `live_reload_*` tests in `workspace.rs`); not hand-checked |
| Reading position per file (not in the original) | done: the session's `place` lines (core `place::ReadingPlace`, shared `mdshared_app::places`), 300 files | test (`a_file_opens_where_it_was_left`, `places_are_saved_with_the_session_and_survive_another_window_and_an_edit`); not hand-checked |
| Link following (fragments, local Markdown, http/mailto, others ignored) | done | test (`links_scroll_open_tabs_hand_out_urls_and_refuse`); opening URLs built, not hand-checked |
| Details toggle | done (view state, no reload, reading position kept) | test (`a_details_toggle_above_keeps_the_reading_position`) |
| Images: limits, schemes | done (renderer + engine); **differs, kept** (decided 2026-11-12): remote images load only after the reader allows them (per-document bar, "Always load remote images" setting) | test (`remote_images_wait_for_the_reader`); bar buttons built, not hand-checked |
| Settings: tabs, active, recent, folder, theme, zoom | done, plus window size and position, open panes and their widths, `remote_images`, `page_width`, `edit_tasks` and the `place` lines | test (`session_round_trip`, core `position_and_pane_widths_round_trip_beside_the_older_keys`) |
| Single instance | done (core `single::forward` before the window, `listen_waking`) | code; built, not hand-checked |

**Open items from this table:** every hand check (PLAN A3) and a run on Linux (nothing above has
been built there).
