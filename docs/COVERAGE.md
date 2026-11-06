# Coverage: what GitHub renders and what mdview does

The aim is that anything that renders on GitHub also renders here, and anything we can't render shows its source and a reason.

**Status of this list.** It comes from my knowledge of GitHub's "Basic writing and formatting syntax", "Working with advanced formatting" and GFM documents. I could not check it against github.com in this phase, and no document was compared with a real GitHub rendering.

- **Supported:** in the document model today, with a test in `crates/core/tests/`. No renderer exists until Phase 2, so nothing here has been seen on screen.
- **Planned:** the phase is named.
- **Proposed exclusion:** a proposal for the user to approve or reject. Nothing is final until reviewed.

## Spec suites

Results of `cargo test --release -p mdview-core --test spec`, viewer configuration (all extensions and the autolink pass always on):

| Suite | Examples | Match | Listed exceptions |
|---|---|---|---|
| CommonMark 0.31.2 | 652 | 645 | 7 |
| cmark-gfm `spec.txt` | 672 | 665 | 7 |

**How the comparison works.** The model is rendered to HTML by a test-only writer. Both that and the expected HTML then go through the same reduction (`tests/common/visible.rs`, `normalize.rs`), which keeps only what the allow-list can show. It drops comments and unknown tags, treats `<b>` as `<strong>`, collapses nested strong, removes table sections, ignores URL spelling, whitespace at block starts, empty links and empty paragraphs. Without the reduction, 578 and 598 examples match. The rest are examples with raw HTML, plus nested strong, table sections, the checkbox's `disabled` attribute and the bare-URL exceptions. This comparison answers "does it look the same under our allow-list", not "is the HTML identical". The reduction shares one thing with the crate: its entity table.

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
| `<picture>` with light and dark sources | Supported in model | Picks in Phase 2. A `<source>` counts only inside a `<picture>` and only for its first `<img>` |
| Ordered, unordered, nested lists | Supported | |
| Task lists | Supported | Read-only, drawn in Phase 2 |
| Backslash escapes, entities | Supported | |
| Hard and soft line breaks | Supported | |
| Hidden comments `<!-- -->` | Supported | |
| Footnotes | Supported | When a label is defined twice the first definition wins and the later one is dropped, as on GitHub |
| Alerts: note, tip, important, warning, caution | Supported | Compared with `gh api markdown` (context `cli/cli`). Title text on the marker line, an empty alert, and one inside a quote, list item, footnote, `<div>` or `<details>` stay plain quotes showing the literal marker. In `<td>`, `<section>` and `<center>` they are alerts, as on GitHub |
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
| `width`, `height`, `valign` on tables, rows, cells, `<hr>`; `type` on lists and items | Supported in model | Through `Document::attrs_of`. Drawing in Phase 2. Lengths must be finite, and pixels stop at 12000 |
| `colspan`, `rowspan` | Supported in model | Clamped to 1-1000 and 1-65534 as a browser does, except that `rowspan="0"` means 1 and not the rest of the section |
| `<img>` with no usable `src` | Supported in model | Keeps its alt text, as on GitHub |
| `<dl>`, `<figure>`, `<section>` and similar | Supported as plain blocks | No term or caption styling yet |
| `<ruby>`, `<abbr>`, `<small>`, `<q>`, `<bdo>`, `<time>` and similar | Planned, Phase 2 | Text is kept without the styling. The corpus study decides priority |
| Inline CSS `style=` | Proposed exclusion | GitHub strips it too |
| `hspace` on images; `cellpadding`, `cellspacing` on tables; `nowrap` on cells | Proposed exclusion | GitHub keeps these and the model drops them. Spacing only. Worth adding if the Phase 2 corpus shows them |
| `<li value>` | Proposed exclusion | GitHub keeps it; numbering here ignores it |
| `align`, `valign` on `<thead>`, `<tbody>`, `<tfoot>`; `dir` on `<span>` | Proposed exclusion | GitHub keeps these, but those tags leave no node in the model to hold them. Text is kept. `<tr>` and cells do carry them |
| Image `width`, `height` | Supported in model | |
| Script, style, iframe, object | Never run | Removed when paired, which differs from GitHub: see the known gaps. Unpaired tagfilter names show as text |

## Things that only make sense on GitHub

| Feature | Status | Reason |
|---|---|---|
| `@mentions`, `#123` issue refs, commit SHAs, custom autolink references | Proposed exclusion | They need a repository and network context. They read as plain text |
| Issue and PR task-list state, sub-issue lists | Proposed exclusion | Read-only viewer with no server |
| Code permalinks rendered as snippets | Proposed exclusion | Needs network and repository access. Stays a link |
| Attached video and asset URLs as players | Proposed exclusion | No media engine, which would hurt launch and memory. Stays a link |
| Colour chips for `#ffffff` in code | Proposed exclusion | GitHub shows these only in issues and PRs |
| GeoJSON and TopoJSON maps | Proposed exclusion | Needs map tiles from the network. Shown as a code block |
| ASCII STL 3D models | Proposed exclusion | Needs a 3D renderer. Shown as a code block |

## Known gaps in what is claimed as supported

- Find does no Unicode normalization: a decomposed `é` does not match a composed one. The old app didn't either.
- Bare URL delimiter handling is skipped for a document that already contains a character from U+E000 to U+E03F, so such a document can truncate those URLs at `_`, `*` or `~`. A document containing U+E003 or U+E004 also shows a rejected alert's marker as its own paragraph, not joined to the text after it.
- A block tag inside a Markdown heading (`# a <div>b</div>`) is ignored with its text kept, so the heading stays one outline entry. GitHub keeps the tag inside the heading.
- A block tag in a paragraph splits it as a browser does, and in a table cell it opens child blocks. Markdown formatting open at that point carries into the new blocks, but text after its end closes the anonymous block it was in, so `**a <p>b</p> c** d` puts `d` on a new line. A browser keeps it on the line of `c`.
- A `<pre>` opened inline loses Markdown formatting inside it, which GitHub would apply: the model's code blocks hold plain text.
- A paired `<script>`, `<style>` or `<iframe>` is removed with its content. GitHub shows them as literal text instead: `<style>.a {}</style>` reads `<style>.a {}</style>` there and shows nothing here. The spec comparison can't reveal this, because `tests/common/visible.rs` removes them from both sides. Pending the user's decision.
- Content where a table or list has no room for it stays in document order, so its text and order are kept but its place is not GitHub's. Stray content in a `<table>` or `<tr>` gets an implied cell, where GitHub moves it before the table. Stray content in a `<ul>` or `<ol>` gets an item with no marker (`ListMarker::None`), where GitHub leaves it bare in the list. A lone `<li>` gets a list around it, and a lone `<td>` or `<tr>` drops the tag and keeps the text, both as on GitHub.
- GitHub autolinks the raw source, so inside a bare URL it doesn't decode an entity: `http://a.b/c&amp;d` shows `&amp;` literally and links to that, and `http://a.b/c&#35;` ends the link before the `;`. Here the entity is decoded and stays in the link. A URL ending in a named entity does trim as on GitHub, and `&nbsp;` ends a link here while GitHub keeps going.
- Three alert edge cases differ from GitHub. An alert marker that's also a reference label (`[!NOTE]: /u`) becomes a link there when the quote stays plain, and stays text here. `> [!NOTE]` followed by `> ===` is a plain quote holding a heading there, and an alert here. `> [!NOTE]` followed by `> [x]: /u` is an alert holding that text there, and a plain quote here, since the parser takes the line as a reference definition.
- Block containers nested over 256 deep add no block, so their content joins the deepest one with its text kept. Tables and lists at that depth lose their structure, keeping cell and item text as separate lines. GitHub keeps going.
- Find stops at 64 character comparisons per byte of text and marks its count with `+`. A query up to 64 characters can't reach this, and a longer one only on text that nearly matches it everywhere, such as a 5000-character pasted query on a file of the same letter.
- HTML nested over 512 elements deep ignores the deeper tags and keeps their text. Repeats of one inline tag left open across blocks count once, so `<i><i>` needs one `</i>`. Past one re-opened span per 4 bytes of input, unclosed tags stop carrying over and the document shows a warning.
- Math delimiters use a rule measured with `gh api markdown` (context `cli/cli`), since GitHub doesn't document it. A span opens after whitespace, `(`, a closing code span, a closing tag, `<br>`, or the start of `**` or `~~` at a boundary. It doesn't open after a letter or digit in any script, other punctuation, a non-breaking or ideographic space, `*`, `_`, a link's `[` or an opening tag. It can't end right before an ASCII letter, digit, `_`, or a single `*`. A different `gh api` context or a later change on GitHub's side could move this. Known differences: `_a_$x$`-style spans after a closing `_` or `*` follow GitHub only when a letter or digit is right before that delimiter, and once one pair in a block is rejected, that block's remaining `$` are decided from the source text, not from parser events, so a `$` inside a link destination or raw HTML tag counts as a delimiter there. In that same fallback, a span right after `>` on a continuation line of a block quote counts as not at a boundary. A document containing U+E003 can't use the re-parse, so bad spans show literally and Markdown inside them is lost.
- `xmpp:` and `mailto:` forms are linked as in the GFM spec; I did not confirm that github.com still does this.

## Known limits

- Hostile input has a large transient memory peak, even inside the 16 MiB limit. 16 MiB of `>` peaks at about 1.2 GiB RSS, measured with the block depth limit in place (about 1.9 GiB without it), and 16 MiB of `[` at about 1.1 GiB. Retained memory is bounded by the block depth limit. Most of the peak is pulldown-cmark's own, which this crate can't change: it builds its tree before the first event arrives, so nothing here can cap it. Lowering the file limit is the only remaining lever, and real documents don't come close.
- The 16 MiB limit is on the bytes on disk. A UTF-16 or invalid UTF-8 file can decode to as much as 48 MiB of text, and that text is what gets parsed.
