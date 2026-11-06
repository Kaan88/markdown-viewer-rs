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
| CommonMark 0.31.2 | 652 | 646 | 6 |
| cmark-gfm `spec.txt` | 672 | 666 | 6 |

**How the comparison works.** The model is rendered to HTML by a test-only writer. Both that and the expected HTML then go through the same reduction (`tests/common/visible.rs`, `normalize.rs`), which keeps only what the allow-list can show. It drops comments and unknown tags, treats `<b>` as `<strong>`, collapses nested strong, removes table sections, ignores URL spelling, whitespace at block starts, empty links and empty paragraphs. Without the reduction, 579 and 599 examples match. The rest are examples with raw HTML, plus nested strong, table sections, the checkbox's `disabled` attribute and the bare-URL exceptions. This comparison answers "does it look the same under our allow-list", not "is the HTML identical". The reduction shares one thing with the crate: its entity table.

**Exceptions:**

| CommonMark | GFM | Input | Why |
|---|---|---|---|
| 602, 608, 611, 612 | 610, 616, 619, 620 | Plain text URLs and emails | Deliberate. Bare URLs stay linked as on GitHub. |
| 148 | 118 | `<pre>` with a blank line inside a `<table>` HTML block | A blank line ends that HTML block, so `<pre>` closes there. Text and order are kept; the second half is not monospace. A browser would keep it inside `<pre>`. |
| 167 | 137 | `<del>`, blank line, paragraph, `</del>` | Looks the same: the strike goes inside the paragraph instead of around it. |

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
| Section links, `#fragment` | Supported | Also accepts `user-content-` and percent-encoding |
| Images, relative and remote | Supported in model | Loading in Phase 2 |
| `<picture>` with light and dark sources | Supported in model | Picks in Phase 2 |
| Ordered, unordered, nested lists | Supported | |
| Task lists | Supported | Read-only, drawn in Phase 2 |
| Backslash escapes, entities | Supported | |
| Hard and soft line breaks | Supported | |
| Hidden comments `<!-- -->` | Supported | |
| Footnotes | Supported | |
| Alerts: note, tip, important, warning, caution | Supported | Title text on the marker line stays a plain quote; I believe GitHub does the same, unverified |
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
| `width`, `height`, `valign` on tables, rows, cells, `<hr>`; `type` on lists and items | Supported in model | Through `Document::attrs_of`. Drawing in Phase 2 |
| `<img>` with no usable `src` | Supported in model | Keeps its alt text, as on GitHub |
| `<dl>`, `<figure>`, `<section>` and similar | Supported as plain blocks | No term or caption styling yet |
| `<ruby>`, `<abbr>`, `<small>`, `<q>`, `<bdo>`, `<time>` and similar | Planned, Phase 2 | Text is kept without the styling. The corpus study decides priority |
| Inline CSS `style=` | Proposed exclusion | GitHub strips it too |
| `hspace` on images; `cellpadding`, `cellspacing` on tables; `nowrap` on cells | Proposed exclusion | GitHub keeps these and the model drops them. Spacing only. Worth adding if the Phase 2 corpus shows them |
| `<li value>` | Proposed exclusion | GitHub keeps it; numbering here ignores it |
| `align`, `valign` on `<thead>`, `<tbody>`, `<tfoot>`; `dir` on `<span>` | Proposed exclusion | GitHub keeps these, but those tags leave no node in the model to hold them. Text is kept. `<tr>` and cells do carry them |
| Image `width`, `height` | Supported in model | |
| Script, style, iframe, object | Never run | Removed when paired. Unpaired tagfilter names show as text |

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
- Bare URL delimiter handling is skipped for a document that already contains U+E000 to U+E002, so such a document can truncate those URLs at `_`, `*` or `~`.
- A block tag inside a Markdown heading (`# a <div>b</div>`) is ignored with its text kept, so the heading stays one outline entry. GitHub keeps the tag inside the heading.
- A block tag in a paragraph splits it as a browser does, and in a table cell it opens child blocks. Markdown formatting open at that point carries into the new blocks, but text after its end closes the anonymous block it was in, so `**a <p>b</p> c** d` puts `d` on a new line. A browser keeps it on the line of `c`.
- A `<pre>` opened inline loses Markdown formatting inside it, which GitHub would apply: the model's code blocks hold plain text.
- HTML nested over 512 elements deep ignores the deeper tags and keeps their text. Repeats of one inline tag left open across blocks count once, so `<i><i>` needs one `</i>`. Past one re-opened span per 4 bytes of input, unclosed tags stop carrying over and the document shows a warning.
- Math delimiters use a rule measured with `gh api markdown` (context `cli/cli`), since GitHub doesn't document it. A span opens after whitespace, `(`, a closing code span, a closing tag, `<br>`, or the start of `**` or `~~` at a boundary. It doesn't open after a letter or digit in any script, other punctuation, a non-breaking or ideographic space, `*`, `_`, a link's `[` or an opening tag. It can't end right before an ASCII letter, digit, `_`, or a single `*`. A different `gh api` context or a later change on GitHub's side could move this. Known differences: `_a_$x$`-style spans after a closing `_` or `*` follow GitHub only when a letter or digit is right before that delimiter, and once one pair in a block is rejected, that block's remaining `$` are decided from the source text, not from parser events, so a `$` inside a link destination or raw HTML tag counts as a delimiter there. In that same fallback, a span right after `>` on a continuation line of a block quote counts as not at a boundary. A document containing U+E003 can't use the re-parse, so bad spans show literally and Markdown inside them is lost.
- `xmpp:` and `mailto:` forms are linked as in the GFM spec; I did not confirm that github.com still does this.
