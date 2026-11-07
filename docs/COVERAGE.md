# Coverage: what GitHub renders and what mdview does

The aim is that anything that renders on GitHub also renders here, and anything we can't render shows its source and a reason.

**See [Not implemented](#not-implemented) for the short list.**

**Status of this list.** It began as my knowledge of GitHub's "Basic writing and formatting syntax", "Working with advanced formatting" and GFM documents. The Phase 1 audit then compared the model with `gh api markdown -f mode=gfm -f context=cli/cli`, and this revision records the fixes on `phase1-audit-fixes`. Everything below has a test in `crates/core/tests/`; the comparisons with GitHub used that API, not github.com itself, so a real repository render may still differ. The WinUI shell and the renderer are still unverified.

- **Supported:** in the document model today, with a test in `crates/core/tests/`. No renderer exists until Phase 2, so nothing here has been seen on screen.
- **Planned:** the phase is named.
- **Excluded:** the user reviewed and accepted these. We don't implement them.

## Spec suites

Results of `cargo test --release -p mdview-core --test spec`, viewer configuration (all extensions and the autolink pass always on):

| Suite | Examples | Match | Listed exceptions |
|---|---|---|---|
| CommonMark 0.31.2 | 652 | 645 | 7 |
| cmark-gfm `spec.txt` | 672 | 665 | 7 |

**How the comparison works.** The model is rendered to HTML by a test-only writer. Both that and the expected HTML then go through the same reduction (`tests/common/visible.rs`, `normalize.rs`), which keeps only what the allow-list can show. It drops comments and unknown tags, treats `<b>` as `<strong>`, collapses nested strong, removes table sections, ignores URL spelling, whitespace at block starts, empty links and empty paragraphs. Without the reduction, 578 and 598 examples match. The rest are examples with raw HTML, plus nested strong, table sections, the checkbox's `disabled` attribute and the bare-URL exceptions. This comparison answers "does it look the same under our allow-list", not "is the HTML identical". The reduction shares two things with the crate: its entity table and `links::percent_decode`, so a bug in either cancels out on both sides. It also merges synonyms as the crate does (`s`/`strike` to `del`, `ins` to `u`, `var`/`cite`/`dfn` to `em`, `tt`/`samp` to `code`, `section` and similar to `div`) and keeps the children of tags outside the allow-list.

**Exceptions:**

| CommonMark | GFM | Input | Why |
|---|---|---|---|
| 602, 608, 611, 612 | 610, 616, 619, 620 | Plain text URLs and emails | Deliberate. Bare URLs stay linked as on GitHub. |
| 148 | 118 | `<pre>` with a blank line inside a `<table>` HTML block | A blank line ends that HTML block, so `<pre>` closes there. Text and order are kept; the second half is not monospace. A browser would keep it inside `<pre>`. |
| 167 | 137 | `<del>`, blank line, paragraph, `</del>` | Looks the same: the strike goes inside the paragraph instead of around it. |
| 191 | 160 | Indented code directly in a `<tr>` | The model promises that rows hold only cells (see `BlockKind`), so it gets an implied cell. The expected HTML leaves it in the `<tr>`, and a browser would move it before the table. Text and order are kept. |

Numbers 398, 426, 434-436, 473-477 (nested strong), 614 (escape in autolink) and 652 (tagfilter) now match and are not exceptions.

## Basic syntax

| Feature | Status | Notes |
|---|---|---|
| Headings, ATX and setext | Supported | |
| Heading anchors | Supported | Compared with `gh api markdown` on 44 headings in Latin, Turkish, Greek, Arabic, Persian, Indic, Myanmar, Khmer, CJK and emoji, and all agree. Slugs come from the text before whitespace collapses, keep what `\p{Word}` keeps, and use github-slugger's duplicate suffixes. GitHub gives a heading with an empty slug no anchor; here its slug is empty and nothing links to it |
| Bold, italic, bold italic | Supported | |
| Strikethrough `~~x~~`, `~x~` | Supported | |
| Subscript, superscript with `<sub>`, `<sup>` | Supported | |
| Underline with `<ins>`, `<u>` | Supported | |
| Blockquotes | Supported | |
| Inline code, fenced and indented code | Supported | |
| Syntax highlighting | Planned, Phase 4 | Blocks are recorded with their language for the engine |
| Links, reference links, relative links | Supported | Following them per the old app's rules. A link or image naming a host through `//host/x`, `\\host\x` or any mix of the two is ignored even in percent-encoded form, because on Windows it would contact that host |
| Section links, `#fragment` | Supported | Also accepts `user-content-` and percent-encoding, decoded once in `Document::resolve_fragment` |
| Images, relative and remote | Supported in model | Loading in Phase 2 |
| `<picture>` with light and dark sources | Supported in model | Picks in Phase 2. A `<source>` counts only as a direct child of a `<picture>`, only for its first `<img>`, and only while that picture's block is open; the first matching source wins. A picture that closes without an image leaves nothing behind |
| Ordered, unordered, nested lists | Supported | |
| Task lists | Supported | Read-only, drawn in Phase 2 |
| Backslash escapes, entities | Supported | |
| Hard and soft line breaks | Supported | |
| Hidden comments `<!-- -->` | Supported | |
| Footnotes | Supported in model | Numbering follows first reference, including one inside a duplicate definition, and the first definition of a label wins. The model keeps each definition where it was written, and keeps unreferenced ones with number 0; the Phase 2 renderer draws the end list and skips the definitions in flow. Until then find and the block order see that text. See the known gaps |
| Alerts: note, tip, important, warning, caution | Supported | Compared with `gh api markdown` (context `cli/cli`). Title text on the marker line, an empty alert, and one inside a quote, list item, footnote, `<div>` or `<details>` stay plain quotes showing the literal marker. A blank quote line between the marker and a heading, rule or `***` makes it an ordinary child, so the alert stays. `-` and `--` under the marker make that line a setext heading, as on GitHub. A `<td>` or `<th>` resets the `<div>`, list and quote ancestors, so an alert inside a cell is kept unless `<details>` is above it. Empty inline HTML (`<i></i>`, `<a href=x></a>`, `<img>`, `<p></p>`, `<span></span>`) counts as content, while a comment, `<foo>`, `<a>` with no href or a raw `<input>` does not |
| Tables, alignment, inline content | Supported | |
| Horizontal rules | Supported | |
| Bare URL and email autolinks | Supported | Including `_`, `*`, `~` inside a URL |

## Advanced formatting

| Feature | Status | Notes |
|---|---|---|
| Collapsed sections, `<details>` and `<summary>` | Supported | Open state is view state in Phase 2 |
| Math: `$..$`, `$$..$$`, ```` ```math ```` | Supported in model | Drawing in Phase 4. Over 4 KiB keeps source and a reason. What may touch the delimiters follows GitHub, as measured below |
| GitHub's `` $`..`$ `` math form | Planned, Phase 4 | Reads as `$`, code, `$` today. Not tested |
| Mermaid | Supported in model | Drawing in Phase 4. Over 64 KiB keeps source and a reason. Family coverage is per the plan |
| Front matter as a table | Planned, Phase 2 | Today reads as a rule and a setext heading, which looks wrong |
| Emoji shortcodes `:+1:` | Planned, Phase 2 | Needs a table outside the launch path |
| `<kbd>`, `<mark>`, `<code>`, `<tt>`, `<samp>`, `<var>`, `<cite>`, `<dfn>` | Supported | |
| `<p>`, `<div>`, `<center>`, HTML tables and lists | Supported | Inside a table cell or a paragraph as well: see the known gaps for the differences |
| `align` on blocks, `<img>`, `<table>`, `<tr>`, `<hr>` | Supported in model | Includes `justify`. On an image: left, right, top, middle, bottom. Drawing in Phase 2 |
| `dir` on HTML blocks | Supported in model | `ltr`, `rtl`, `auto`. Drawing in Phase 2. GitHub also marks every paragraph `dir="auto"`, which is for the Phase 2 renderer to match |
| `width`, `height`, `valign` on tables, rows, cells, `<hr>`; `type` on lists and items | Supported in model | Through `Document::attrs_of`. Drawing in Phase 2. A length follows the HTML rule: digits, an optional fraction, and `%` only when it comes immediately, so `10em` is 10 while `+5` and `.5` are no length. Pixels stop at 12000 |
| `colspan`, `rowspan` | Supported in model | Clamped to 1-1000 and 1-65534 as a browser does, except that `rowspan="0"` means 1 and not the rest of the section. A leading `+` is accepted |
| `<img>` with width and height | Supported in model | Clamped to 2000 on both axes together, keeping the ratio, as GitHub does. A percentage is left alone |
| `<img>` with no usable `src` | Supported in model | Keeps its alt text, as on GitHub |
| Raw `<input>` outside a task list | Supported | Ignored, as GitHub's sanitizer removes it; a Markdown task box still draws |
| `<dl>`, `<figure>`, `<section>` and similar | Supported as plain blocks | No term or caption styling yet |
| `<ruby>`, `<abbr>`, `<small>`, `<q>`, `<bdo>`, `<time>` and similar | Planned, Phase 2 | Text is kept without the styling. The corpus study decides priority |
| Inline CSS `style=` | Excluded | GitHub strips it too |
| `hspace` on images; `cellpadding`, `cellspacing` on tables; `nowrap` on cells | Excluded | GitHub keeps these and the model drops them. Spacing only. Worth adding if the Phase 2 corpus shows them |
| `<li value>` | Excluded | GitHub keeps it; numbering here ignores it |
| `align`, `valign` on `<thead>`, `<tbody>`, `<tfoot>`; `dir` on `<span>` | Excluded | GitHub keeps these, but those tags leave no node in the model to hold them. Text is kept. `<tr>` and cells do carry them |
| Script, style, iframe | Never run | Shown as literal text, as GitHub's tag filter does: `<style>.a {}</style>` reads as itself. Nothing here executes or loads either way |
| `object`, `embed`, `head`, `template` | Supported | The tag is dropped and its children are kept, as on GitHub, so an `<object>` fallback and its link still show |
| Image `width`, `height` | Supported in model | |

## Things that only make sense on GitHub

| Feature | Status | Reason |
|---|---|---|
| `@mentions`, `#123` issue refs, commit SHAs, custom autolink references | Excluded | They need a repository and network context. They read as plain text |
| Issue and PR task-list state, sub-issue lists | Excluded | Read-only viewer with no server |
| Code permalinks rendered as snippets | Excluded | Needs network and repository access. Stays a link |
| Attached video and asset URLs as players | Excluded | No media engine, which would hurt launch and memory. Stays a link |
| Colour chips for `#ffffff` in code | Excluded | GitHub shows these only in issues and PRs |
| GeoJSON and TopoJSON maps | Excluded | Needs map tiles from the network. Shown as a code block |
| ASCII STL 3D models | Excluded | Needs a 3D renderer. Shown as a code block |

## Not implemented

**Accepted exclusions** (rows marked Excluded above):

- GitHub-only features: `@mentions`, issue refs, commit SHAs, custom autolinks, issue and PR task state, code permalink snippets, video and asset players, colour chips, GeoJSON and TopoJSON maps, ASCII STL models.
- Inline CSS `style=`, `hspace`, `cellpadding`, `cellspacing`, `nowrap`, `<li value>`, `align` and `valign` on table sections, `dir` on `<span>`.

**Planned, not built yet:**

- Phase 2: the renderer, so nothing has been seen on screen. Also front matter, emoji shortcodes, and `<ruby>`, `<abbr>`, `<small>`, `<q>`, `<bdo>`, `<time>` styling.
- Phase 3: the Qt shell and file watcher.
- Phase 4: math, Mermaid and SVG drawing, syntax highlighting, and the `` $`..`$ `` math form.
- Phase 5: the WinUI shell. Only a test skeleton exists.

**Decided, work left for later phases:**

- Footnote definitions stay where they were written in the model; core does not move them. The Phase 2 renderer draws the end-of-document list in order of first reference and skips `FootnoteDefinition` blocks in flow, and does not draw unreferenced ones (number 0). Until then find and the block order see mid-file and unreferenced definitions.

**Known defects:** the fixes from [AUDIT_PHASE1.md](AUDIT_PHASE1.md) are in `phase1-audit-fixes`. What remains differs from GitHub in the known gaps below, and each gap says what GitHub does.

## Known gaps in what is claimed as supported

- Find does no Unicode normalization: a decomposed `é` does not match a composed one. The old app didn't either.
- Bare URL delimiter handling is skipped for a document that already contains a character from U+E000 to U+E03F, so such a document can truncate those URLs at `_`, `*` or `~`, and a rejected alert's marker shows as its own paragraph rather than joined to the text after it. The document's own characters are left alone either way.
- A block tag inside a Markdown heading (`# a <div>b</div>`) is ignored with its text kept, so the heading stays one outline entry. GitHub keeps the tag inside the heading.
- A block tag in a paragraph splits it as a browser does, and in a table cell it opens child blocks. Markdown formatting open at that point carries into the new blocks, but text after its end closes the anonymous block it was in, so `**a <p>b</p> c** d` puts `d` on a new line. A browser keeps it on the line of `c`.
- A `<pre>` opened inline loses Markdown formatting inside it, which GitHub would apply: the model's code blocks hold plain text.
- A `<video>` keeps its fallback text, where GitHub drops it. Script, style and iframe now show as literal text, and `object`, `embed`, `head` and `template` keep their children, both as GitHub does.
- Content where a table or list has no room for it stays in document order, so its text and order are kept but its place is not GitHub's. Stray text or inline HTML in a `<table>` or `<tr>` gets an implied cell, where GitHub moves it before the table; a stray `<br>` or `<img>` there keeps a cell of its own with the break or picture, and a link wrapped around a `<td>` stays on the cell's text. Stray content in a `<ul>` or `<ol>` gets an item with no marker (`ListMarker::None`), where GitHub leaves it bare in the list. A lone `<td>` or `<tr>` drops the tag and keeps the text as on GitHub; a lone `<li>` gets a list around it, which is the model's choice and GitHub does not wrap it.
- GitHub autolinks the raw source, so inside a bare URL it doesn't decode an entity: `http://a.b/c&amp;d` shows `&amp;` literally and links to that, and `http://a.b/c&#35;` ends the link before the `;`. Here the entity is decoded and stays in the link. A URL ending in a letter-only named entity does trim as on GitHub; `&nbsp;` and `&lt;` end a link here where GitHub keeps going, and a digit-bearing name such as `&frac12;` does not trim because GitHub's rule stops at anything but letters.
- Alerts still differ from GitHub in these cases. A marker that is not on the quote's first line, as in `> ` (blank) or `> <!-- c -->` followed by `> [!NOTE]`, stays a plain quote here and is an alert there. An alert marker that is also a reference label (`[!NOTE]: /u`) stays an alert here and becomes a plain quote with a link there. A footnote definition directly under a marker rejects the alert on both, but the definition's text stays in the model here (GitHub draws no unreferenced definition).
- Footnote definitions are not moved. GitHub collects them into one list at the end of the document in order of first reference, lifts them out of quotes, and hides unreferenced ones. Here each stays where it was written, in source order and inside its container, and an unreferenced one keeps its text with number 0. Find and the block order therefore see text a reader will not see in Phase 2. See "Decided, work left for later phases".
- Block containers nested over 256 deep add no block, so their content joins the deepest one with its text kept. Tables and lists at that depth lose their structure, keeping cell and item text as separate lines. The real bound is five levels past that for an implied row, its cell and a leaf inside. GitHub keeps going.
- Find stops at 64 character comparisons per byte of text and marks its count with `+`. A query up to 64 characters can't reach this, and a longer one reaches it when enough of the file nearly matches it: a 300-character query on a file about a quarter made of a run of its first character misses a real hit and reports `capped`. The flag is set, so the result is incomplete rather than silently wrong; the shell must show it that way. A query containing a line break matches wrapped text (a CRLF pair counts as one break), but a selection from inside a code block or across two paragraphs still never matches.
- HTML nested over 512 elements deep ignores the deeper tags and keeps their text. Repeats of one inline tag left open across blocks count once, so `<i><i>` needs one `</i>`. Past one re-opened span per 4 bytes of input, unclosed tags stop carrying over and the document shows a warning.
- Math delimiters use a rule measured with `gh api markdown` (context `cli/cli`), since GitHub doesn't document it. A span opens after whitespace, `(`, a closing code span, a closing tag, `<br>`, or the start of `**` or `~~` at a boundary. It doesn't open after a letter or digit in any script, other punctuation, a non-breaking or ideographic space, `*`, `_`, a link's `[` or an opening tag. It can't end right before an ASCII letter, digit, `_`, or a single `*`. A different `gh api` context or a later change on GitHub's side could move this. Known differences: `_a_$x$`-style spans after a closing `_` or `*` follow GitHub only when a letter or digit is right before that delimiter, and once one pair in a block is rejected, that block's remaining `$` are decided from the source text, not from parser events, so a `$` inside a link destination or raw HTML tag counts as a delimiter there. In that same fallback, a span right after `>` on a continuation line of a block quote counts as not at a boundary. A document containing U+E003 can't use the re-parse, so bad spans show literally and Markdown inside them is lost.
- `xmpp:` and `mailto:` forms are linked as in the GFM spec; I did not confirm that github.com still does this.

## Known limits

- Hostile input has a large memory peak, even inside the 16 MiB limit. The worst case found is 16 MiB of `#\n`: about 1.85 GiB peak RSS, about 540 MiB retained, 8.4 M blocks, in about 2.7 s on the audit machine (Linux, measured with a probe outside this repo). 16 MiB of `>` peaks at about 1.1 GiB with the block depth limit in place (about 1.6 GiB without it) and retains nothing; 16 MiB of `[` peaks at about 1.05 GiB. Pulldown-cmark builds its tree before the first event, so most of the `>` peak is not this crate's; for `#\n` it is, because about four fifths of that peak is this crate's own blocks and anchors. Retained memory therefore grows with block count, not only with nesting: `- a\n` retains about 35 MiB at 2 MiB of input. Lowering the file limit is the one certain lever, and real documents stay far below it.
- The 16 MiB limit is on the bytes on disk. A UTF-16 or invalid UTF-8 file can decode to as much as 48 MiB of text, and that text is what gets parsed.
