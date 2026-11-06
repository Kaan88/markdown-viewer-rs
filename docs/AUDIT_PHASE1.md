# Phase 1 audit of be52ac4

Stopped early at the user's request. The commit under review is `be52ac4`, compared with `762984d`. **None of these findings has been fixed.**

Reviewers compared against `gh api markdown -f mode=gfm -f context=cli/cli`. Confirmed means a second agent, told to try to disprove the finding, reproduced it. Paths under `/home/kaan/.cache` are Linux-only probes that did not come with the repo; the inputs quoted here are enough to reproduce.

**Totals:** 19 confirmed, 1 refuted, 11 unverified (the tests and parity lens only).

| Finding | Status | Severity | File |
|---|---|---|---|
| correctness #0 | CONFIRMED | major | `crates/core/src/parse.rs` |
| correctness #1 | CONFIRMED | minor | `crates/core/src/parse.rs` |
| correctness #2 | CONFIRMED | minor | `crates/core/src/parse.rs` |
| correctness #3 | CONFIRMED | minor | `crates/core/src/parse.rs` |
| correctness #4 | CONFIRMED | minor | `crates/core/src/parse.rs` |
| correctness #5 | CONFIRMED | minor | `crates/core/src/parse.rs` |
| correctness #6 | CONFIRMED | minor | `crates/core/src/parse.rs` |
| coverage #0 | CONFIRMED | major | `crates/core/src/html.rs:514` |
| coverage #1 | CONFIRMED | major | `docs/COVERAGE.md:55` |
| coverage #2 | CONFIRMED | minor | `crates/core/src/parse.rs alert_allowed_here; docs/` |
| coverage #3 | CONFIRMED | minor | `crates/core/src/parse.rs block_quote/finish_block/` |
| coverage #4 | CONFIRMED | minor | `crates/core/src/parse.rs in_picture / leave_pictur` |
| coverage #5 | CONFIRMED | minor | `crates/core/src/parse.rs:2253 html_source` |
| coverage #6 | CONFIRMED | minor | `crates/core/src/parse.rs parse_span / parse_length` |
| coverage #7 | REFUTED | minor | `docs/COVERAGE.md:113` |
| coverage #8 | CONFIRMED | minor | `crates/core/src/parse.rs html_input; docs/COVERAGE` |
| coverage #9 | CONFIRMED | minor | `docs/COVERAGE.md:20,107,108` |
| performance #0 | CONFIRMED | minor | `docs/COVERAGE.md` |
| performance #1 | CONFIRMED | minor | `docs/COVERAGE.md` |
| performance #2 | CONFIRMED | minor | `crates/core/src/find.rs` |
| tests-parity #0 | UNVERIFIED | major | `crates/core/tests/fuzz.rs` |
| tests-parity #1 | UNVERIFIED | minor | `crates/core/tests/fuzz.rs` |
| tests-parity #2 | UNVERIFIED | minor | `crates/core/tests/fuzz.rs` |
| tests-parity #3 | UNVERIFIED | minor | `crates/core/tests/regressions.rs` |
| tests-parity #4 | UNVERIFIED | minor | `crates/core/tests/regressions.rs` |
| tests-parity #5 | UNVERIFIED | minor | `crates/core/tests/regressions.rs` |
| tests-parity #6 | UNVERIFIED | minor | `crates/core/tests/regressions.rs` |
| tests-parity #7 | UNVERIFIED | minor | `crates/core/tests/regressions.rs` |
| tests-parity #8 | UNVERIFIED | minor | `crates/core/tests/regressions.rs` |
| tests-parity #9 | UNVERIFIED | minor | `docs/PORT_CHECKLIST.md` |
| tests-parity #10 | UNVERIFIED | minor | `crates/core/src/parse.rs` |

## correctness

### correctness #0: CONFIRMED, major

**File:** `crates/core/src/parse.rs (Event::Rule at ~545, MdTag::Heading at ~712, first_in_open_alert)`

**Problem:** The new setext check rejects valid alerts. first_in_open_alert only asks whether this is the alert's first child block. It does not ask whether the block touches the marker line. So a thematic break or setext heading after a blank line is treated as if it underlined the marker. The alert becomes a plain quote with a literal [!NOTE] paragraph. This check was added in be52ac4, so by reading the diff this is a regression. CRLF input behaves the same.

**Reproduction:** Input `> [!NOTE]\n>\n> Title\n> ===` gives ours BlockQuote[Paragraph "[!NOTE]", Heading "Title"]; gh gives markdown-alert-note containing <h1>Title</h1>. Input `> [!NOTE]\n>\n> Title\n> ---` gives the same mismatch (gh: alert with <h2>). Input `> [!NOTE]\n>\n> ---` gives ours BlockQuote[Paragraph "[!NOTE]", ThematicBreak]; gh gives an alert with <hr>. Input `> [!NOTE]\r\n>\r\n> ---\r\n` gives the same BlockQuote locally (not sent to gh).

**Proposed fix:** Reject only when the source between the end of the marker line and range.start holds no blank quote line, meaning the block starts on the line right after the marker. Add these three inputs to the regressions.

**Second check:** Reproduced with a probe on be52ac4 and compared with gh. `> [!NOTE]\n>\n> Title\n> ===` gives ours BlockQuote[P "[!NOTE]", H1 "Title"]; gh gives markdown-alert-note with <h1>Title</h1>. `...> Title\n> ---` gives the same mismatch (gh <h2>). `> [!NOTE]\n>\n> ---` gives ours BlockQuote[P "[!NOTE]", ThematicBreak]; gh gives an alert with <hr>. The CRLF form gives the same mismatch, and I did send it to gh: alert with <hr>. Also failing: `> [!NOTE]\n>\n> a\n> b\n> ---` and `> [!TIP]\n>\n> Title\n> ---\n> body` (gh: alert with heading, then body). Controls that are correct: `> [!NOTE]\n> ---` is a plain quote with H2 on both; `> [!NOTE]\n>\n> ***` and `> [!NOTE]\n>\n> # Title` stay alerts on both. Regression: `git log -S first_in_open_alert` shows only be52ac4, and 762984d has no reject_open_alert call at all. That part is from reading the code; I did not run the old commit. Valid GFM loses its alert box and shows a literal [!NOTE], and COVERAGE does not list it. Frequency is low, but it is a new regression in a top-priority area and the fix is small, so I keep major.

### correctness #1: CONFIRMED, minor

**File:** `crates/core/src/parse.rs (fit_parent, open_implied_row, open_implied_cell, Tag::Td branch in html_open_block)`

**Problem:** Implied cells, rows and items are created for stray content that has no text, and they stay in the model when empty. An <a href> wrapping a <td> adds an empty first column, and the link also lands on the cell text. Stray atoms and empty tags add phantom rows, cells and marker-less items. A browser foster-parents all of this out of the table, so the column count stays right. COVERAGE exception 191/160 and the BlockKind comment cover keeping stray text in order, not empty phantom cells that shift columns.

**Reproduction:** Input `<table><tr><a href="u"><td>a</td></a><td>b</td></tr></table>` gives ours Row[Cell(empty), Cell "a" with Link span, Cell "b"], 3 columns; gh gives `<a href="u"></a><table>...<td>a</td><td>b</td>`, 2 columns with no link on a. Input `<table><p></p><tr><td>a</td></tr></table>` gives ours 2 rows, the first holding one empty cell; gh gives 1 row. Input `<table><b></b><tr><td>a</td></tr></table>` also gives an extra empty row. Input `<table><tr><br><td>a</td></tr></table>` and `<table><tr><img src="i.png"><td>a</td></tr></table>` give an extra leading cell; gh moves the br or img before the table. Input `<ul><p></p><li>a</li></ul>` gives ours an extra empty ListItem; gh's <p></p> has no marker or text.

**Proposed fix:** Drop an implied cell, row or item when it closes with no text and no atom, as drop_last_block already does for empty paragraphs, and close any span left open at the cell boundary. Decide separately whether stray atoms should move before the table, as a browser does. If not, list that as an exception in COVERAGE.

**Second check:** All reproduced. `<table><tr><a href="u"><td>a</td></a><td>b</td></tr></table>`: ours Row[empty Cell, Cell "a" with Link, Cell "b"]; gh `<a href="u"></a><table>...<td>a</td><td>b</td>`, with no link on a. `<table><p></p><tr><td>a</td></tr></table>` and `<table><b></b><tr>...`: ours has an extra leading row; gh has 1 row. `<table><tr><br><td>a</td>` and `<table><tr><img src="i.png"><td>a</td>`: ours has an extra leading cell; gh moves the br or img before the table. `<ul><p></p><li>a</li></ul>`: ours has an extra empty ListItem. New variants: `<ul><a href="u"><li>a</li></a><li>b</li></ul>` adds an empty item, while gh keeps 2 li; a trailing `<b></b>` in a tr and `<br>` after the last row also add a phantom cell or row. Downgraded because common real-world shapes do not trigger it: `<table><!-- c -->`, `<tr><a name="x"></a><td>`, `<table><a id="x"></a><tr>`, `<ul><a id="x"></a><li>`, `<ul><!-- c -->`, colgroup/col and blank-line Markdown in cells all give the correct shape. It needs malformed HTML, no text is lost, and the column shift is visible. COVERAGE covers stray content, not the empty phantom cells.

### correctness #2: CONFIRMED, minor

**File:** `crates/core/src/parse.rs (block_quote and the setext checks)`

**Problem:** More alert mismatches with gh, none listed in COVERAGE (line 109 lists only `===`, a reference label and `[x]: /u`). Setext underlines that pulldown does not read as an underline still leave an alert, and so does an underline below a line starting with #. An alert holding only inline HTML with no text is rejected as empty, though GitHub keeps it. A footnote definition right after the marker stays an alert here, though GitHub shows a plain quote.

**Reproduction:** `> [!NOTE]\n> --` and `> [!NOTE]\n> -` give ours Alert[Paragraph "--"] and Alert[Paragraph "-"]; gh gives blockquote <h2>[!NOTE]</h2>. `> [!NOTE]\n> #x\n> ---` gives ours Alert[Heading "#x"]; gh gives blockquote <h2>[!NOTE]<br>#x</h2>. The cause is the `!= b'#'` test at parse.rs ~712. `> [!NOTE]\n> <a id="x"></a>` and `> [!NOTE]\n> <span></span>` give ours BlockQuote; gh gives an alert. `> [!NOTE]\n> [^1]: x` gives ours Alert[FootnoteDefinition]; gh gives blockquote <p>[!NOTE]</p>.

**Proposed fix:** Handle these in the same source check as the first finding: treat a next line of only `=` or `-` as an underline, and detect setext from the block's last line, not its first byte. List whatever stays different in COVERAGE.

**Second check:** All reproduced against gh. `> [!NOTE]\n> --` and `> [!NOTE]\n> -`: ours Alert[P "--"] and Alert[P "-"]; gh blockquote <h2>[!NOTE]</h2>. `> [!NOTE]\n> #x\n> ---`: ours Alert[H2 "#x"]; gh blockquote <h2>[!NOTE]<br>#x</h2>. `> [!NOTE]\n> <a id="x"></a>` and `> [!NOTE]\n> <span></span>`: ours BlockQuote[P "[!NOTE] "]; gh alert. `> [!NOTE]\n> [^1]: x`: ours Alert[FootnoteDefinition]; gh blockquote <p>[!NOTE]</p>. None of these is in the COVERAGE alert line, which lists only `===`, a reference label and `[x]: /u`. Controls `> [!NOTE]\n> <br>` and `<img>` match gh as alerts. All are rare edge inputs with no text lost, so minor.

### correctness #3: CONFIRMED, minor

**File:** `crates/core/src/parse.rs (parse_with_checks can_hide) and crates/core/src/autolink.rs (restore, protect_url_delimiters)`

**Problem:** A document's own U+E000, U+E001 or U+E002 is silently rewritten to _, * or ~ once any delimiter is hidden. can_hide checks only U+E003 and U+E004, and hiding sets restore = true, and restore then maps the whole swap table. The doc comment on protect_url_delimiters says restore must not touch such documents, and COVERAGE line 102 does not mention this. The $ path already existed, but be52ac4 adds a new trigger through rejected alerts and adds the comment that this breaks.

**Reproduction:** `\u{E000} \u{E001} \u{E002}\n\n- > [!NOTE]\n  > body` gives Paragraph text "_ * ~". `\u{E000} and $x$y$` gives "_ and $x$y$". A document without a hide keeps U+E000 intact.

**Proposed fix:** Test for SWAP_PREFIX in can_hide, which falls back to the literal pass, or have restore undo only what this parse swapped.

**Second check:** Reproduced. `  \n\n- > [!NOTE]\n  > body` gives P text "_ * ~". ` and $x$y$` gives "_ and $x$y$". Control ` plain` keeps U+E000. gh keeps the character: `gh api` output for the $ case starts with bytes 356 200 200 (U+E000). So a document's own character is silently changed to a different visible character, against the doc comment on protect_url_delimiters, and COVERAGE does not mention it. Needs private-use characters in U+E000-E002 plus a rejected $ or alert, so it is rare.

### correctness #4: CONFIRMED, minor

**File:** `crates/core/src/parse.rs (MdTag::FootnoteDefinition at ~769)`

**Problem:** When a footnote definition is flattened at MAX_BLOCK_DEPTH, md_block returns without creating a block, yet the next line stores cur_block as footnotes[id].block. The footnote then points at an unrelated ancestor, and a later definition counts as the duplicate. Two different footnotes at that depth would point at the same ancestor. Only documents nested past 256 levels can reach this.

**Reproduction:** `<div>` x300 + `\n\n[^a]: first\n\n[^a]: second\n\nr[^a]` gives Footnote{number:1, block:255} and block 255 is a Div. `>` x300 + ` [^a]: first\n\nr[^a]` gives block:255, a BlockQuote. In both, the block's kind is not FootnoteDefinition.

**Proposed fix:** Set .block only when a FootnoteDefinition block was actually opened, and otherwise keep a separate 'seen' flag for first-wins. Otherwise document that block may be an ancestor.

**Second check:** Reproduced. `<div>` x300 + `\n\n[^a]: first\n\n[^a]: second\n\nr[^a]` gives FN a #1 block=255, kind Div. `>` x300 + ` [^a]: first\n\nr[^a]` gives block=255, kind BlockQuote. Two different footnotes (`[^a]` and `[^b]` under 300 `>`) both give block=255, a BlockQuote. Control with 10 divs gives kind FootnoteDefinition. This breaks the model comment that says block is "The definition", and the block's text is the ancestor's, not the note's. Only documents nested past 256 levels reach it, and the target still encloses the definition. Dropping "second" in the first case is correct first-wins, so that part of the report is not a separate harm.

### correctness #5: CONFIRMED, minor

**File:** `crates/core/src/parse.rs (skip_depth, duplicate footnote definitions)`

**Problem:** Skipping a duplicate definition also skips the footnote references inside it. On GitHub they still count: they number the referenced footnote and list it. Here that footnote gets number 0, and later numbers shift.

**Reproduction:** `[^a]: 1\n\n[^a]: 2[^b]\n\n[^b]: b\n\nx[^a]`: ours gives footnote a number 1 and footnote b number 0. gh renders x with superscript 2 and lists two footnotes, b first and then 1.

**Proposed fix:** While skipping a repeated definition, still number any Event::FootnoteReference, or list this as an exception in COVERAGE.

**Second check:** Reproduced. `[^a]: 1\n\n[^a]: 2[^b]\n\n[^b]: b\n\nx[^a]`: ours FN a#1, FN b#0. gh renders x with superscript 2 and lists two footnotes, b first (back to reference 1) and then a (back to reference 2). So ours shows a different number and leaves footnote b unnumbered. Needs a duplicate definition that holds a reference no other text makes, which is very rare. Not in COVERAGE.

### correctness #6: CONFIRMED, minor

**File:** `crates/core/src/parse.rs (flattened), crates/core/src/limits.rs (MAX_BLOCK_DEPTH), crates/core/tests/fuzz.rs:422`

**Problem:** The real maximum depth is MAX_BLOCK_DEPTH + 5, which breaks the repo's own fuzz assertion `depth <= MAX_BLOCK_DEPTH + 4`. flattened() checks block_depth before fit_parent adds the implied row and cell. The fuzz does not generate an input that reaches this. Separately, flattening a table loses the cell break made by a closing tag, so words join.

**Reproduction:** `<section>` x254 + `<table><table>x` gives depth 261. x253 gives 260, x255 gives 259, x256 gives 257. Separately, `<section>` x300 + `<table><tr><td>one</td>two</table>` gives text "onetwo\n", where the same table with no prefix gives "one\ntwo\n".

**Proposed fix:** Add the two implied levels to the check in flattened() when the parent is a Table or a TableRow, or loosen the bound in the comment and the test to +5. For the joined words, end the inline container when a </td> or </tr> arrives while flattened.

**Second check:** Reproduced. `<section>` x254 + `<table><table>x` gives depth 261, above the fuzz bound `depth[i] <= MAX_BLOCK_DEPTH + 4` (260) at crates/core/tests/fuzz.rs:422, using the same root-is-1 depth count. x253 gives 260, x255 gives 259, x256 gives 257. `<table><table><table>x` and a table/ul mix also reach 261 at x254. Joined words: `<section>` x300 + `<table><tr><td>one</td>two</table>` gives text "onetwo\n", while with no prefix it gives "one\ntwo\n". An ordinary flattened table with only proper cells stays separated ("one\ntwo\nthree\n"). The invariant the repo states is false by one level, which is harmless to layout in practice. The joined words need nesting past 256 plus stray text. Minor.

**Checked and fine:**

- No panics, overflow or hangs in about 180,000 fuzz inputs on a debug build with overflow checks (seeds 12345 and 987654321). The parts covered alerts, tables, lists, entities, $, CRLF and U+E000/3/4. Each input ran raw and under 250 to 257 <section> tags.
- Parent links held on all fuzz inputs: every block's children cover its range exactly, the shape guarantee held, block_attrs stayed sorted, colspan and rowspan stayed clamped, and no U+E000-E004 leaked into text or strings unless the source had one. Fuzz assertions on nesting under ListItem and on depth did fire: the first is GitHub-correct, because GitHub keeps an alert under a bare <ul>; the second is the +5 depth finding. The word-order comparison also fired 35 and 39 times on the two seeds; the samples I read were joined words from flattened tables, except one where deep parsing showed a literal `script` the raw parse dropped, which I did not trace. Beyond that I filtered these out of the second seed's output, so the rest were not read one by one.
- U+E004 hide-and-reparse offsets are correct: marker text is restored in paragraphs and setext headings, and other content on the route (links, bare URLs, $x$) matches gh. Rejected offsets arrive sorted, by reading push order and by test. The literal path with U+E003 or U+E004 present keeps parent links, with the marker as its own paragraph as documented.
- Flattening at MAX_BLOCK_DEPTH keeps all text in order for 400-level quotes, lists, divs and 300-level nested tables, and `after`/`tail` text returns to the root once closed. Tables, HTML lists and Markdown lists at depths 250 to 258 fit whole or leave no part behind. 200k repeats of `>`, `- `, `<div>`, `<table>`, `<td>`, `<li>` and `<ul>` each parsed in under 300 ms on a debug build.
- Alert placement matched gh in the cases run: quote, list item, footnote, <div> and <details> reject; <section>, <td>, <center>, <summary>, <p>, <b>, <a> and <dl> keep; lazy continuation, `>[!NOTE]`, trailing space, lowercase, title text on the marker line, table, fence, `***`, indented text and `2. item` all match. <h2> and <ul> differ only in where the alert sits in the tree, not in alert status.
- ReloadTracker: exhaustive check of 579,194 operation sequences up to length 5 (begin manual or automatic, timed_out, finished with each outcome, current and stale generation) found no violation. Refreshing never stays up with nothing running, a first success never announces, a sticky message is cleared by the first success, and later successes say Refreshed.
- Find: the 64-character exactness claim holds by reasoning (at most 64 comparisons per start position) and by tests on ASCII and multibyte near-matches. A 65-character query also found its real hit. A 500-character query on repeated text reports capped=true and misses a real hit, as documented. NBSP matches a space in the first position and mid-query, with correct UTF-8 ranges.
- Autolink trailing entities: the Entities base offset is right after an escape wall, a leading entity and inside strong. Stacked and paren entities match gh (`&amp;&amp;`, `&period;`, `&comma;`, `&lpar;&rpar;`, `&rpar;`, `&ast;&ast;`). By reading the code plus the fuzz, a zero-length entity range cannot hang trim_end, because a URL cannot end after a whitespace character.
- Duplicate footnotes keep the first definition across case variants and in mixed order, and match gh except for the reference case in the findings.
- parse_bytes: 16 MiB + 1 byte gives FileTooLarge. 16 MiB of invalid bytes decodes to about 50 MB and parses rather than blanking (833 ms on a debug build, in memory only).
- Fragments decode once. classify_link keeps `a%25b` encoded on all three routes, and resolve_fragment resolves `a%25b`, `a%2520b`, `user-content-` and percent-encoded UTF-8 correctly.
- parse_span and parse_length handle `2.5`, `0`, `-1`, `9999999999`, `abc` and `1e40` within the documented limits.
- Repo tests pass at be52ac4: 7, 10, 100 and 3 tests, with MDVIEW_FUZZ_SECS=3 and -j 4. git status is clean and HEAD is unchanged.

**Not checked:**

- Command table semantics (TOOLBAR, Menu::Theme, titles, separator moves) against the old app: I read the diff only and did not compare to the original UI.
- The parse_bench example changes and the scaling tests' statistics. I did not look for allocation savings on the bare-URL path, and did no timing beyond ratios seen in passing.
- Math with CRLF on a rejected literal span, beyond the cases run. CRLF in math was compared with expected output locally only, not with gh, because CRLF cases were left out of the gh runs.
- Whether a browser resolves a literal `%20` id through a raw fragment first: this can't be observed with gh api markdown, so it is not reported.
- Mid-URL entities and `http&colon;//a.b`, `www&period;a.b` link here but not on gh. Mid-URL decoding is documented in COVERAGE line 108, and the `&colon;`/`&period;` form is not part of this commit, so neither is reported as a defect.
- Behaviour on Windows, including the cfg!(windows) label path.
- Probes are at /home/kaan/.cache/mdview-audit/correctness (297 MB including target dirs, which are safe to delete). No input file over 10 MiB was written.

## coverage

### coverage #0: CONFIRMED, major

**File:** `crates/core/src/html.rs:514 (also parse.rs:353 DROP_NAMES, tests/common/visible.rs:22 DROP_PAIRED_SILENT, tests/regressions.rs:319,1767)`

**Problem:** Paired <object>, <embed>, <head> and <template> are dropped with their content, with no warning. GitHub strips only the tag and keeps the children. This is separate from the open script/style/iframe decision. It breaks the plan's rule of 'never a blank or a silent drop', and a document can come out blank. COVERAGE line 85 lists object and points to the known gaps, but the gap at line 106 names only script, style and iframe and says GitHub shows them as literal text, which is wrong for object. head, embed and template appear nowhere in COVERAGE. visible.rs removes all four from both sides, so the spec comparison can't reveal it, and the 'How the comparison works' paragraph doesn't disclose it. regressions.rs pins the behaviour as intended ('gone').

**Reproduction:** Input `<object data="x.svg"><img src="f.png" alt="fb"></object>`: gh gives `<p><a href="f.png"><img src="f.png" alt="fb"></a></p>`, ours gives an empty document (HTML output empty, no warnings). `<object><p>para</p></object>\n\nz`: gh keeps `para` and z, ours only z. `<template>x</template>y`, `<head>x</head>y`, `<object>x</object>y`, `<embed>x</embed>y`: gh `xy`, ours `y`.

**Proposed fix:** Treat these four as unknown tags (drop the tag, keep the children), which is safe because there is no HTML engine. Remove them from DROP_NAMES and DROP_PAIRED_SILENT, and change the two regression assertions. If removal is kept on purpose, list all four as a gap with GitHub's real behaviour and add them to the user's open decision.

**Second check:** Reproduced with a probe on be52ac4 and with gh api. `<object data="x.svg"><img src="f.png" alt="fb"></object>`: gh gives `<p><a href="f.png"><img src="f.png" alt="fb"></a></p>`, ours gives zero blocks and no warnings, so the document is blank. `<object><p>para</p></object>\n\nz`: gh keeps para and z, ours only `P "z"`. `<template>x</template>y`, `<head>x</head>y`, `<object>x</object>y`, `<embed>x</embed>y`: gh `xy`, ours `y`. A more realistic case: `<object data="x.pdf">Your browser cannot show this <a href="x.pdf">download</a></object>\n\nnext`: gh keeps the fallback text and link, ours only next. `- <object>gone</object> after`: gh `gone after`, ours `after`. Silent drop: the plan's Coverage section says 'never a blank or a silent drop'. Docs and tests also check out: html.rs:514 classifies the four as DropContent with literal_if_unclosed false, visible.rs:22 DROP_PAIRED_SILENT removes them from both sides, COVERAGE:85 lists object and points to the known gaps, and the gap at line 106 names only script, style and iframe. head, embed and template appear nowhere in COVERAGE. regressions.rs asserts `gone` is absent. This is outside the open script/style/iframe decision. Fallback content in `<object>` is the normal authoring pattern, which is why I keep it major.

### coverage #1: CONFIRMED, major

**File:** `docs/COVERAGE.md:55 (Footnotes row; model in crates/core/src/parse.rs FootnoteDefinition)`

**Problem:** Footnotes are marked Supported, and the new note mentions only duplicate labels. GitHub collects definitions into one list at the end of the document, in order of first reference, lifts them out of quotes, and omits unreferenced ones. The model leaves each definition where it was written, in source order, inside its container, and keeps unreferenced ones (number 0). Neither COVERAGE, PORT_CHECKLIST nor the plan mentions this, and the plan requires every gap to be listed. Find and the block order also see the footnotes in these positions.

**Reproduction:** `[^b] [^a]\n\n[^a]: A\n\nmid\n\n[^b]: B\n\nend\n`: gh gives p, mid, end, then a footnotes section with B then A. Ours gives P, FnDef(#2,a), P mid, FnDef(#1,b), P end. `[^unused]: a\n[^unused]: b\n\nq`: gh `<p>q</p>`, ours FnDef(#0) with text `a` before q. `> [^1]\n\n> [^1]: in quote\n\n[^1]: top`: gh gives an empty quote plus an end list, ours nests FnDef in the quote. `[^1]\n\n[^1]: a [^2]\n[^1]: b [^3]\n\n[^2]: two\n[^3]: three`: gh numbers three as 3, ours FnDef(#0), because the reference in the dropped duplicate doesn't count.

**Proposed fix:** Decide where relocation lives (model or Phase 2 renderer) and record it in COVERAGE as Planned or as a known gap. Either way, say that unreferenced definitions stay in the model and need hiding, including from find.

**Second check:** Reproduced. `[^b] [^a]\n\n[^a]: A\n\nmid\n\n[^b]: B\n\nend\n`: gh gives p, mid, end, then a footnotes section with B then A. Ours gives P, FootnoteDefinition(A), P mid, FootnoteDefinition(B), P end, numbered b#1 a#2. `[^unused]: a\n[^unused]: b\n\nq`: gh `<p>q</p>`, ours FootnoteDefinition with `a` before q and FN unused#0. `> [^1]\n\n> [^1]: in quote\n\n[^1]: top`: gh gives an empty quote plus an end section, ours nests the definition in the quote. `[^1]\n\n[^1]: a [^2]\n[^1]: b [^3]\n\n[^2]: two\n[^3]: three`: gh lists three, ours FN 3#0. My own run: find on `q\n\n[^unused]: hiddenword\n` returns a hit at 2..12, because the text is in Document::text, and hidden_ranges with everything closed returns nothing for it, so find_visible has no way to skip it. Text order is source order (Azz before Bzz). COVERAGE:55 says Supported and mentions only duplicate labels. Grep finds no mention of this in PORT_CHECKLIST or the plan, and the plan requires every gap to be listed. It is latent until the Phase 2 renderer exists, but the model and find both depend on where this text sits, and it affects any document with mid-file or unused definitions.

### coverage #2: CONFIRMED, minor

**File:** `crates/core/src/parse.rs alert_allowed_here; docs/COVERAGE.md:56`

**Problem:** The new alert rule is wrong when a table cell sits inside a rejecting container. On GitHub a <td> or <th> resets <div>, <li> and quote ancestors, and only <details> still rejects. Ours rejects if any such ancestor is on the stack, so the literal `[!TIP]` shows. COVERAGE's 'in <td> they are alerts, as on GitHub' doesn't hold for these nestings. The method's comment also says 'Bottom-up', but `.iter().any` walks from the outermost entry.

**Reproduction:** `<div align="center">\n<table><tr><td>\n\n> [!TIP]\n> x\n\n</td></tr></table>\n</div>`: gh gives an alert in the cell, ours Quote with text `[!TIP] x`. Same mismatch for `<div>...<th>`, `<li><table><tr><td>`, `- <table><tr><td>`, `<blockquote><table><tr><td>` and `> <table><tr><td>` + nested quote. Agreeing cases: `<details><table><tr><td>` plain on both, `<td><div>` plain on both, `<ul>` alone an alert on both.

**Proposed fix:** Walk the stack from the innermost entry and stop at the first Td or Th. Keep a separate check for any Details ancestor. Add the gh-measured cases above as regressions.

**Second check:** Reproduced. `<div align="center">\n<table><tr><td>\n\n> [!TIP]\n> x\n\n</td></tr></table>\n</div>`: gh gives a markdown-alert-tip in the cell, ours BlockQuote > P `[!TIP] x`. The same mismatch appears for `<div>..<th>`, `<li><table><tr><td>`, `- <table><tr><td>`, `<blockquote><table><tr><td>` and `> <table><tr><td>` with a nested quote. Agreeing cases: `<details><table><tr><td>` plain on both, `<td><div>` plain on both, `<ul>` alone an alert on both, a plain `<td>` an alert on both. So COVERAGE:56's 'in <td> they are alerts' does not hold under these ancestors. The code confirms the cause: alert_allowed_here uses `.iter().any` over the whole stack from the outermost entry with no cell reset, and its comment says 'Bottom-up'.

### coverage #3: CONFIRMED, minor

**File:** `crates/core/src/parse.rs block_quote/finish_block/Event::Rule; docs/COVERAGE.md:109`

**Problem:** Alert cases beyond the three listed in COVERAGE differ from GitHub, and the listed reference-label gap is misdescribed. Line 109 says the label case happens 'when the quote stays plain', but with a defined `[!NOTE]` label and a body GitHub gives a plain quote with a link, while ours gives an alert and the marker disappears.

**Reproduction:** (a) `> \n> [!NOTE]\n> body` and `> <!-- c -->\n> [!NOTE]\n> body`: gh alert, ours Quote showing `[!NOTE] body`. (b) `> [!NOTE]\n> <i></i>`, `> <a href="x"></a>`, `> <img>`, `> <p></p>`: gh alert, ours Quote with the literal marker, because no child block exists. (c) `> [!NOTE]\n> --` and `> -`: gh plain quote with `<h2>[!NOTE]</h2>`, ours alert holding `--` / `-`; only `---` and `===` were considered. (d) `> [!NOTE]\n> [^1]: f`: gh plain quote with `[!NOTE]`, ours alert holding FnDef(#0). (e) `> [!NOTE]\n> body\n\n[!NOTE]: /u`: gh `<blockquote><p><a href="/u">!NOTE</a><br>body</p>`, ours ALERT(Note) with text `body`.

**Proposed fix:** Fix (a) if cheap by detecting the marker after leading blank or comment lines; it is the most plausible case in real files. Treat (b)'s empty inline tags as content. Extend the setext check to `-` and `--`. Otherwise add these to line 109 and correct its first sentence.

**Second check:** All parts reproduced. (a) `> \n> [!NOTE]\n> body` and `> <!-- c -->\n> [!NOTE]\n> body`: gh alert, ours BlockQuote with P `[!NOTE] body`. (b) `> [!NOTE]\n> <i></i>`, `<a href="x"></a>`, `<img>`, `<p></p>`: gh alert, ours BlockQuote with the literal marker. (c) `> [!NOTE]\n> --` and `> -`: gh `<blockquote><h2>[!NOTE]</h2>`, ours Alert with P `--` / `-`. `---` matches gh here, and `===` is the listed gap. (d) `> [!NOTE]\n> [^1]: f`: gh plain quote with `[!NOTE]`, ours Alert holding FootnoteDefinition(#0). (e) `> [!NOTE]\n> body\n\n[!NOTE]: /u`: gh `<blockquote><p><a href="/u">!NOTE</a><br>body</p>`, ours Alert(Note) with `body`, so COVERAGE:109's 'stays text here' is wrong for this form. Only (a) is plausible in real files; the rest are edge cases, so minor.

### coverage #4: CONFIRMED, minor

**File:** `crates/core/src/parse.rs in_picture / leave_picture; docs/COVERAGE.md:49`

**Problem:** The new claim 'A <source> counts only inside a <picture>' is false when the picture was implicitly closed. `in_picture` is reset only by `</picture>` or an `<img>`, so a source in a closed block or element carries over to the next image anywhere later, and that image gets a wrong dark variant.

**Reproduction:** `<picture><source media="(prefers-color-scheme: dark)" srcset="d.png">\n\n<img src="b.png">`: gh closes the picture in its own `<p>` and b.png is plain, ours gives b.png dark="d.png". Same result for `<div><picture><source ...></div>\n\n<img src="b.png">`, `> <picture><source ...>\n\n<img src="b.png">`, `- <picture><source ...>\n- <img src="b.png">`, `<p><picture><source ...></p>\n<p><img src="b.png"></p>`, and `<picture><div><source ...></div><img src="a.png"></picture>`. Chromium DOMParser on the `<div>` form placed the img under BODY, outside the picture.

**Proposed fix:** Clear picture state when the enclosing block or Markdown block ends (for example, record the block where the picture opened and call leave_picture when it closes), and ignore a source whose parent isn't the picture.

**Second check:** Reproduced. `<picture><source media="(prefers-color-scheme: dark)" srcset="d.png">\n\n<img src="b.png">`: gh closes the picture in its own `<p>` and b.png is plain, ours gives IMG src=b.png dark=d.png. The same dark=d.png carry-over appears with `<div><picture><source ...></div>\n\n<img>`, `> <picture><source ...>\n\n<img>`, `- <picture><source ...>\n- <img>`, and `<p><picture><source ...></p>\n<p><img></p>`. `<picture><div><source ...></div><img src="a.png"></picture>`: ours dark=d.png. gh moves the img out of the picture. In my headless Chrome test on a page the img stayed under PICTURE, but the source is not a direct child there, so it still would not apply. Explicitly closed `<picture>...</picture>` then `<img>` is correct on ours (dark empty). COVERAGE:49's 'counts only inside a <picture>' is false for these. It needs an unclosed picture, so it is rare, but the result is the wrong image in dark theme.

### coverage #5: CONFIRMED, minor

**File:** `crates/core/src/parse.rs:2253 html_source`

**Problem:** When two sources match the same scheme, the last one wins here. A browser uses the first matching source, and GitHub passes both through, so a viewer shows a different image from GitHub.

**Reproduction:** `<picture><source media="(prefers-color-scheme: dark)" srcset="d1.png"><source media="(prefers-color-scheme: dark)" srcset="d2.png"><img src="a.png"></picture>`: ours gives dark="d2.png". gh keeps both sources. Chromium, with two `media="all"` sources, gave currentSrc = the first (d1).

**Proposed fix:** Assign picture_dark and picture_light only while they are still empty.

**Second check:** Reproduced. Two dark sources d1.png and d2.png with `<img src="a.png">`: ours IMG dark=d2.png, and the light form gives l2.png. gh keeps both sources in order. A headless Chrome page with two `media="all"` sources gave currentSrc ending `#d1`, the first. html_source assigns picture_dark and picture_light unconditionally, so the last one wins.

### coverage #6: CONFIRMED, minor

**File:** `crates/core/src/parse.rs parse_span / parse_length; docs/COVERAGE.md:75-76`

**Problem:** COVERAGE says the numeric attributes are handled 'as a browser does', but parse_span and parse_length differ from the browser on values GitHub passes through unchanged. Separately, GitHub clamps image width and height to 2000 while ours stops at 12000, and COVERAGE doesn't say so.

**Reproduction:** Ours vs Chromium (values read back from the DOM): colspan="+2" 1 vs 2. img width="10em" None vs 10. "100abc" None vs 100. "1e3" Px(1000) vs 1. "+5" Px(5) vs invalid. ".5" Px(0) vs invalid. "50 %" Percent(50) vs 50px. gh echoes all of these verbatim. `<img width="5000px">` and `<img height="99999">`: gh `width="2000"` and `height="2000"`, ours Px(5000) and Px(12000). `<table width="99999">` is unchanged on gh, so the clamp applies to images only.

**Proposed fix:** Parse lengths with the HTML dimension rule (optional whitespace, digits, optional fraction, `%` immediately after, ignore the rest) rather than f32::parse. Accept a leading `+` in parse_span. Clamp image dimensions to 2000, or list the difference.

**Second check:** Reproduced, with one correction: COVERAGE:76's 'as a browser does' covers only the span clamps, and line 75 makes no browser claim about lengths. The false statement is parse_span's doc comment 'under the HTML rules'. Ours vs headless Chrome: colspan `+2` 1 vs 2; `10em` None vs 10; `100abc` None vs 100; `1e3` Px(1000) vs 1; `+5` Px(5) vs invalid (img.width 0 on a test image that did not decode); `.5` Px(0) vs invalid (same); `50 %` Percent(50) vs 50px. Forms the reporter did not list, and more plausible in real READMEs: `300px;` None vs 300, `300;` None vs 300, `100%;` None vs 100%. gh echoes all of these verbatim. Clamp: `<img width="5000px">` gh width=2000, ours Px(5000); `<img height="99999">` gh 2000, ours Px(12000); `width=3000 height=100` gh 2000x67 (ratio kept), ours 3000x100; `<table width="99999">` unchanged on gh. COVERAGE does not list any of this.

### coverage #7: REFUTED, minor

**File:** `docs/COVERAGE.md:113 (math rule); crates/core/src/parse.rs math`

**Problem:** Inline `$...$` spanning a line break is math here and literal in the project's reference mode. The measured rule in COVERAGE doesn't mention it. The dollars disappear and the text becomes a math span. CRLF normalization now makes these spans clean, but it doesn't question whether they should exist. Caveat: gh mode=gfm inserts `<br>` at soft breaks, which may be why it refuses. mode=markdown renders no math at all (`$x$ y` stays literal), so README behaviour can't be measured through this API.

**Reproduction:** `from $a\nto b$ end`: gh `<p>from $a<br>\nto b$ end</p>`, ours P text `from a\nto b end` with Math span `a\nto b`. Same for `$a\nb$`, `$a\r\nb$`, `$a\rb$` and `- $a\r\n  b$`. `$$a\nb$$` is display math on both, so only inline differs.

**Proposed fix:** Check one rendered repo file on github.com to settle the rule. Then either reject inline spans containing a soft break or add the difference to the listed math gaps.

**Second check:** Refuted for rendered repository files. The gfm-mode difference does reproduce: `from $a\nto b$ end` gh gfm `from $a<br>\nto b$ end`, ours Math. But I read real repo files through the repository contents API with `Accept: application/vnd.github.html`, and GitHub renders inline math across a line break there. narodnik/abstract-algebra-pinter-solutions ch33.md gives `<math-renderer class="js-inline-math">$\langle a, b \rangle = { e, \alpha = a,\n\beta = b, ...}$</math-renderer>`. iambrj/notes eit/2.md gives `<math-renderer class="js-inline-math">$(a,\nb)$</math-renderer>`. iambrj/notes imc/2.md gives `$\exists\nk\in\mathcal{K}$` in a math-renderer. So the literal output is an artifact of gfm mode's `<br>` at soft breaks, and ours matches what a repo README shows. Not a defect. One useful note for the docs: gfm comment mode and file rendering differ here, so the 'measured with gh api' math rule is measured in the mode that does not match this case.

### coverage #8: CONFIRMED, minor

**File:** `crates/core/src/parse.rs html_input; docs/COVERAGE.md`

**Problem:** Raw `<input type="checkbox">` becomes a drawn checkbox here. GitHub's sanitizer removes it outside task lists, and COVERAGE doesn't list the difference. It also makes an alert out of a quote GitHub leaves plain.

**Reproduction:** `<input type="checkbox" checked>y`: gh `<p>y</p>`, ours Checkbox{checked:true} then y. `> [!NOTE]\n> <input type=checkbox>`: gh plain quote with `[!NOTE]`, ours ALERT(Note) holding a checkbox.

**Proposed fix:** Ignore raw input tags, keeping only Markdown task-list checkboxes, or list this as a deliberate difference.

**Second check:** Reproduced. `<input type="checkbox" checked>y`: gh `<p>y</p>`, ours P with span Checkbox{checked:true}. `<input type="checkbox">` alone: gh empty, ours a Checkbox. `<ul><li><input type="checkbox" checked disabled> done</li></ul>`: gh `<li> done</li>`, ours a checkbox. `> [!NOTE]\n> <input type=checkbox>`: gh plain quote with `[!NOTE]`, ours Alert(Note) holding a checkbox. `- [x] task` matches on both. Grep finds no difference listed in COVERAGE for raw input, and regressions.rs has no `<input` case. Ours shows something GitHub removes, and it is unlisted, which the plan requires to be listed.

### coverage #9: CONFIRMED, minor

**File:** `docs/COVERAGE.md:20,107,108`

**Problem:** Three statements are inaccurate. (1) 'The reduction shares one thing with the crate: its entity table': visible.rs:234 also calls `mdview_core::links::percent_decode`, so a bug in that function would cancel out on both sides. (2) Line 20's list of what the reduction does omits the synonym merges (s/strike to del, ins to u, var/cite/dfn to em, tt/samp to code, section and others to div) and the paired object/embed/head/template removal. (3) Line 107 says a lone `<li>` 'gets a list around it ... as on GitHub', but gh leaves it bare with no list. Also, line 108 names `&nbsp;` as the entity that ends a link here, but `&lt;` does too.

**Reproduction:** Read visible.rs:10,22,25-60,234. `<li>a</li>`: gh `<li>a</li>`, ours List > Li. `http://a.b/&lt;x`: gh links `http://a.b/&amp;lt;x` whole, ours links `http://a.b/` then text `<x`.

**Proposed fix:** Correct these sentences. Say that the lone `<li>` wrapper is a model choice GitHub doesn't make, and add `&lt;` to line 108.

**Second check:** All are documentation inaccuracies I verified. (1) visible.rs:234 canonical_url calls `mdview_core::links::percent_decode` as well as `html::decode_entities`, so COVERAGE:20's 'shares one thing with the crate: its entity table' is false. (2) Line 20 mentions only `<b>` to `<strong>`, but visible.rs canonical() also merges s/strike to del, ins to u, var/cite/dfn to em, tt/samp to code, section and others to div, and DROP_PAIRED_SILENT removes object/embed/head/template. (3) `<li>a</li>`: gh `<li>a</li>` with no list, ours List > ListItem, so line 107's 'both as on GitHub' is wrong for `<li>`. The `<td>` and `<tr>` half is correct: gh `a`, ours Plain `a`. (4) `http://a.b/&lt;x`: gh links `http://a.b/&amp;lt;x` whole, ours LINK `http://a.b/` then text `<x`, so `&lt;` ends a link just as line 108 says `&nbsp;` does. `&gt;` and `&amp;` stay inside the link on ours.

**Checked and fine:**

- Spec exception 191/160 verdict: the listing is honest and should stay for now, but the shape guarantee doesn't force it; the implied-cell strategy does. Exact example run on gh gives `<pre>...</pre><table><tbody><tr></tr></tbody></table>`, which confirms COVERAGE's 'a browser would move it before the table'. Ours gives Table > Tr > Td > Code. The guarantee could hold with no exception by creating the Table and Tr blocks only at the first cell, so stray content before it lands before the table as on GitHub. The spec's expected string keeps the code in the `<tr>`, so the reduction would also need to apply the same move to both sides. That route is reasoned, not implemented or run. A writer flag that hides implied cells would remove the exception too, but would hide a real difference from GitHub, so I advise against it.
- Spec numbers: ran the suite with -j 4; CommonMark 652/645/7 and GFM 672/665/7, with 578 and 598 matching without the reduction, all as COVERAGE states. No unlisted misses or stale exceptions.
- tests/common is unchanged in be52ac4 (diffstat), so the commit adds no new reduction. The drop from 579/599 to 578/598 is consistent with the one new miss in each suite. The writer prints Alert as blockquote and ignores BlockAttrs, ListMarker and picture sources, so the spec suite can't see those features at all. I relied on gh comparisons for them, not the suite.
- Alert rules matching gh: empty, blank-only and comment-only quotes plain; title text on the marker plain; lowercase marker; trailing spaces; blank line after marker; lazy continuation; nested quote, list item, footnote, `<div>`, `<details>`, `<blockquote>`, `<li>` plain; `<td>`, `<th>`, `<section>`, `<center>`, `<article>`, `<aside>`, `<figure>`, `<dd>`, `<ul>`, `<table>` alerts; `---`, `abc\n---`, `abc\n===` rejected as setext; `***` and `- - -` stay alerts; alert plus math in one document. The documented `===` and `[x]: /u` gaps reproduce as written.
- Duplicate footnotes match gh on which definition wins: first wins across tight, spaced, multi-paragraph and case-differing labels, and when a reference falls between the definitions. A dropped duplicate holding an alert, open `<div>`, open comment, heading, anchor or `</b>` leaves no trace, its heading doesn't bump slugs, and a reference definition inside it still resolves, as on gh.
- Stray table and list content is as COVERAGE describes: implied cell where gh moves it before the table, ListMarker::None where gh leaves it bare, lone `<td>`/`<tr>`/`<th>` tag dropped with text kept (also inside a paragraph and `<div>`), `<table><td>` gets an implied row. Text and order are kept in all 40 or so inputs.
- colspan and rowspan: 0, 2x, 2000, 70000, ' 3 ', -3 and rowspan=0 behave as COVERAGE says, against Chromium's readback.
- Autolink trailing entities match gh on: &amp; &quot; &copy; &gt; &nbsp; &bogus; at the end, doubled &amp;&amp;, &amp;; then ., ;&amp;, *c&amp;*, inside **, email x@a.bc&amp;. Documented decoded-in-link differences (&amp;x, &copy;d, &#35;, &#x26;, &nbsp;d) reproduce as written.
- CRLF math: no CR reaches the text in `$$`, ```math, quoted `$$` or lone-CR forms, and the content matches gh's apart from a trailing newline on ```math.
- Picture: first `<img>` only, source after img ignored, source outside picture ignored, `</picture>` clears state, multi-candidate srcset takes the first (gh also rewrites to the first), media without a space, blank lines inside an HTML-block picture, Markdown image never takes a source.
- Literal tagfilter names (title, textarea, xmp, noembed, noframes, plaintext) and text-keeping for audio, svg, math, select, button, form, canvas, map, applet, label, fieldset all match gh. Only video differs: gh drops its fallback text, ours keeps it.
- The repo is untouched: git status clean at be52ac4. Probes were in /home/kaan/.cache/mdview-audit/gh-lens, and build output has been removed.

**Not checked:**

- Real github.com rendering of a repo file. Everything was measured through `gh api markdown` mode=gfm context=cli/cli, which adds `<br>` at soft breaks. mode=markdown produced no alerts or math at all, so it can't stand in for README rendering.
- Browser comparisons used only the Chromium in the desktop browser pane, via DOM readback. No other engine was tried.
- How any of this looks on screen: no renderer exists, and ListMarker::None numbering in an `<ol>` can't be judged until Phase 2. Implied cells carry no flag in the model, so a renderer can't tell them from real ones; I didn't assess whether that matters for layout.
- Claims outside this lens: find budget and 64-character exactness, non-breaking space in find, details_keys, ReloadTracker, command table, fragment decoding, allocations, MAX_BLOCK_DEPTH behaviour, the scaling test, the 16 MiB path.
- The regression and fuzz suites were not run; only tests/spec.rs was.
- The foster-parenting route for 191/160 was not implemented. It is a design reasoned from the observed output.

## performance

### performance #0: CONFIRMED, minor

**File:** `docs/COVERAGE.md`

**Problem:** The 'Known limits' memory paragraph is wrong for at least one input. It says retained memory is bounded by the block depth limit and that most of the peak is pulldown-cmark's, but that only holds for `>`. Empty headings retain 543 MiB and peak about 70% above the stated `>` figure. The `>` and `[` figures are also somewhat high compared with what I measured. This behaviour is not new: 762984d gives identical numbers for `#\n`, so only the doc claim is wrong.

**Reproduction:** Probe calling mdview_core::parse with a counting allocator, reading VmHWM from /proc/self/status (probe source at /home/kaan/.cache/mdview-audit/launchmem/probe-new/src/scale.rs). 16 MiB of `#\n`: be52ac4 gives 1893 MiB RSS, 542.9 MiB retained, 8388608 blocks, 2749 ms; 762984d gives 1893 MiB, 542.9 MiB, 2657 ms. Same probe at 16 MiB, `>`: 1106 MiB RSS with 0.0 MiB retained (doc says about 1.2 GiB); 762984d 1682 MiB, or 1.64 GiB (doc says about 1.9 GiB without the limit). `[`: 1058 MiB (doc says about 1.1 GiB). Retained also scales with ordinary structure: `- a\n` retains 35.3 MiB at 2 MiB of input, and `a|b` table rows retain 53.4 MiB at 2 MiB (not run at 16 MiB).

**Proposed fix:** Replace the sentence with measured figures: worst case found is 16 MiB of `#\n` at about 1.85 GiB peak and about 540 MiB retained. Limit the depth-bound claim to nesting, since retained memory otherwise grows with block count (about 34 bytes per byte of input for headings). Update `>` to about 1.1 GiB and the no-limit figure to about 1.6 GiB. Re-check the 'Lowering the file limit is the only remaining lever' sentence, because the `#\n` retained memory is this crate's own: blocks and anchors.

**Second check:** Reproduced with my own probe (/home/kaan/.cache/mdview-audit/refute, mem.rs, calling mdview_core::parse at be52ac4, one run at a time). 16 MiB of `#\n`: hwm=1894 MiB, retained=542.9 MiB, blocks=8388608, 2650 ms. `>`: hwm=1106 MiB, retained=0.0 MiB, blocks=256 (doc says about 1.2 GiB). `[`: hwm=1058 MiB, retained=16.0 MiB (doc says about 1.1 GiB, which is about right at 1.03 GiB). At 2 MiB: `- a\n` retained 37.0 MiB (reviewer 35.3), `# h\n` retained 35.4 MiB, so retained memory grows with block count and COVERAGE.md line 118's 'Retained memory is bounded by the block depth limit' is false outside nesting. A stronger result than the reviewer gave: pulldown-cmark alone (Parser::new_ext with Options::all, events only, pc.rs) peaks at 402 MiB on 16 MiB of `#\n` against 1894 MiB through parse, so about 80% of that peak is not pulldown's tree. 'Most of the peak is pulldown-cmark's own' and 'Lowering the file limit is the only remaining lever' are therefore also wrong for this input. For `>` the same probe gives 914 of 1106 MiB, so the claim holds there. Not rerun: the 762984d figures (I did not check out the old commit) and the table-row figure (my `| a | b |` input had no delimiter row, so it was not a table: 1 block, 2.0 MiB). Severity is minor because this is a wrong doc claim about hostile input, not new behaviour, and the reviewer's report that it predates be52ac4 fits this. The doc does matter given memory is a top priority, since it wrongly closes off work inside this crate.

### performance #1: CONFIRMED, minor

**File:** `docs/COVERAGE.md`

**Problem:** The find budget can miss a real match for queries over 64 characters on text that does not 'nearly match it everywhere'. Search stops early with zero hits and capped=true, which is a count of `0+` although a match exists later in the file. COVERAGE.md and the FIND_WORK_PER_BYTE doc comment in crates/core/src/limits.rs understate when this happens. Their example is a 5000-character query on a file of one letter, but the real condition is roughly query length times the share of the file that nearly matches being over 64. The flag is set correctly, so the result is marked as incomplete, not silently wrong.

**Reproduction:** Parsed document of 970302 text bytes: a fenced block holding 250000 `=`, then 16000 lines of `The quick brown fox jumps over the lazy dog.`, then a final paragraph of 299 `=` plus `x`. Searching for that 300-character final line returns hits=0, capped=true, in 43.6 ms. Via find_in_text, about 1 MB with a share of it a run: 10% run gives hits=1, 25% run gives hits=0 capped=true. A 65-character query (64 `a` plus `b`): a 4000-byte `a` run gives hits=1; a 4200-byte run gives hits=0 capped=true. Repeated 41-byte line separated by spaces, 25000 times, with a query of K copies plus `END` and a real hit at the end: K=64 (2627 chars) hits=1; K=70 (2873 chars) hits=0 capped=true.

**Proposed fix:** Reword the doc comment and COVERAGE.md with the real condition and these thresholds, for example a 300-character query where about a fifth of the file is a run of its first character. Require the Phase 2 shell to show the capped state as 'stopped searching', not as a count of `0+`. If that is not enough, spend the remaining search on an exact-bytes memmem pass after the budget runs out, so a verbatim pasted match is still found.

**Second check:** Reproduced (fnd.rs, be52ac4). Parsed document, text=970302 bytes, 250000 `=` in a fence, 16000 fox lines, final paragraph of 299 `=` plus `x`: the 300-character query gives hits=0 capped=true in 81.6 ms, though the match exists. A 64-character query on the same document gives hits=1 capped=false, which supports the exactness claim; the code agrees, since match_at charges at most query-length comparisons per byte position. find_in_text with a 300-character query on about 1 MB: run share of 5/10/15/20% gives hits=1, 25/30/50% gives hits=0 capped=true. Repeated 39-byte line 25000 times, query of K copies plus END: K=64 (2499 chars) hits=1, K=70 (2733 chars) hits=0 capped=true. These thresholds fit the reviewer's rule of query length times nearly-matching share over 64. One detail differs: the reviewer's 65-character threshold of about 4200 bytes depends on layout. With `a` run + space + query I got hits=1 at runs of 4200 and 5000 and hits=0 capped=true at 100000. With a 4200 run inside 1 MB of prose, hits=1. The wording at COVERAGE.md line 111, limits.rs line 23 and PORT_CHECKLIST line 438 ('nearly matches it everywhere') is too strong, because a 25% run is enough to lose a verbatim match. Keeping it minor: capped is set correctly, and it takes a query that is itself highly repetitive or a long paste from a highly repetitive file. Ordinary prose was unaffected: a 445-character paragraph on 1 MB of near-identical paragraphs gives hits=1 capped=false in under 2 ms. The reviewer's `0+` display is what the docs describe (the count marked with `+`), but no Phase 2 shell exists yet, so I did not observe it.

### performance #2: CONFIRMED, minor

**File:** `crates/core/src/find.rs`

**Problem:** A query containing a line break can return zero hits for text that is on screen. Line 40 maps `\r` and `\n` to one space each, so a CRLF pair becomes two spaces and never matches a soft-wrapped paragraph. Clipboard text from many Windows apps uses CRLF, and Win11 is the only target. Separately, a break copied from a code block becomes a space, while the code block's text keeps `\n`, so a multi-line selection from a code block can never match. Neither is in COVERAGE.md's known gaps. This predates be52ac4: the same mapping is at line 39 of 762984d.

**Reproduction:** parse("para one\nsame para\n") via find: `one\nsame` gives hits=1, `one same` gives hits=1, `one\r\nsame` gives hits=0. parse("```\nfoo\nbar\n```\n"): text is "foo\nbar\n", and `foo\nbar`, `foo\r\nbar` and `foo bar` all give hits=0, while `foo` gives hits=1. capped=false in the code-block queries, so nothing tells the user the search was incomplete; the `one...` queries did not record capped.

**Proposed fix:** Collapse `\r\n` to a single break while building the query. Then either let a query break match a `\n` inside a code block's own text, keeping the block-separator rule, or list multi-line code selections as a known gap in COVERAGE.md.

**Second check:** Reproduced (fnd.rs, be52ac4). parse of a fence with foo/bar, then 'para one\nsame para', then 'next para' gives text="foo\nbar\npara one same para\nnext para\n". `one\nsame` hits=1, `one same` hits=1, `one\r\nsame` hits=0 capped=false. `foo` hits=1; `foo\nbar`, `foo\r\nbar` and `foo bar` all hits=0 capped=false. The reviewer missed a third case with the same cause: a selection across two paragraphs also never matches, since `para\nnext`, `para next`, `para\r\nnext`, `para\n\nnext` and `para\r\n\r\nnext` all give hits=0 capped=false, because block separators are `\n` and every query break becomes a space. The mapping is at find.rs line 40, and at line 39 of 762984d, so it predates this commit. The comment there states the intent that a pasted line break should match wrapped text, and CRLF defeats that intent. COVERAGE.md lists none of this as a known gap (the find entries are lines 101 and 111). Caveat on reachability: no shell exists yet, and the find bar is a single-line WinUI TextBox or Qt QLineEdit, which may truncate or alter pasted breaks; I did not test either control. So this is a confirmed core behaviour, but how often users hit it depends on Phase 2. The failure is a false 'no matches', and it costs nothing to fix the CRLF part, but it only affects multi-line pasted queries, hence minor.

**Checked and fine:**

- parse_bench on generated_1mib.md, 40 runs, two rounds each. 762984d: median 5.75 and 5.90 ms, peak heap 10.91 MiB, retained 2.08 MiB, 3415 allocations. be52ac4: median 5.98 and 6.09 ms, peak 10.91 MiB, retained 2.08 MiB, 3016 allocations. That is about +3-4% time, close to noise on this machine, with identical memory and identical block, span, link, image and heading counts.
- be52ac4's new bare-URL figure: 6.37-6.41 ms, peak 11.91 MiB, retained 2.08 MiB, 3021 allocations.
- Nothing newly super-linear found. About 110 hostile patterns were run at 2 MiB (about 60 of them also at 500 KB), and those with both sizes grew about 4x in RSS and at most about 4x in time, allowing for noise on the smallest timings. Covered: implied rows, cells and items (`<table>a</tr>`, `<ul>a</li>`, `</tr><hr>`, `<li></ul>`, stray `<p>`), block_attrs (`<td width=2>`, `<td colspan=2>`, 2 MB of attributes on one tag), and the alert reject routes (nested, in list, empty, setext, with U+E004 present, mixed with rejected `$`).
- Also covered at 2 MiB: 600 `<section>` before alerts (alert_allowed_here is capped by the HTML depth limit), duplicate footnotes, deep `>` plus lazy `<li>`, `</div>` and `<ul>` lines, deep `<ul>` plus `>`, entity and escape mixes before bare URLs, `.&amp;` trims, math with CRLF, `<picture>` and `<source>`.
- `#\n` scaled linearly from 250 KB to 4 MB: 28.9, 60.6, 122.5, 268.2, 575.6 ms. The Slugger stays linear there.
- block_attrs stayed strictly sorted by block in every pattern run (asserted in the probe), so attrs_of's binary search is valid on those inputs.
- New memory cost is small and bounded. `<table>a</tr>` now retains 35.0 MiB at 2 MiB of input (0.3 MiB before) and `<ul>a</li>` 31.2 MiB (0.3 MiB before); both are within the block rate of ordinary lists.
- MAX_BLOCK_DEPTH works: 16 MiB of `>` goes from 576 MiB retained and 16777215 blocks to 0.0 MiB and 256 blocks, and RSS from 1682 MiB to 1106 MiB.
- Reparse route: appending one rejected alert (`- > [!NOTE]`) to the 1 MiB file gives 12.1 ms versus 5.87 ms in 762984d, or 2.06x, with peak 11.91 versus 10.91 MiB and the same retained memory. A valid alert costs nothing extra (5.86 ms). Worst hostile reject pattern was 79 ms at 2 MiB versus 14.6 ms before, and stays linear; total passes are capped by MATH_PASSES.
- The find claim holds: a query up to 64 characters is exact. Work is at most 64 comparisons per attempt, with at most one attempt per byte, so it cannot exceed the budget. Confirmed on the worst cases at 2 MB: `a`, `A`, `s` and NBSP runs and an `é` run with a 64-character near-match, all capped=false; a real hit at the end of a 2 MB run was found (hits=1, capped=false).
- Differential test of find_in_text against a naive reference with no budget and no memchr shortcut: 200000 random trials, queries of 1-64 chars, alphabet including I/İ/ı, K and the Kelvin sign, s/ſ, ς/σ/Σ, NBSP, newline, the Angstrom sign and emoji. 0 mismatches, 0 capped. It confirms the budget and needle shortcuts change nothing; it reuses fold's rules, so it does not test the folding rules themselves.
- The budget bounds long-query time: 5000 characters on a 2 MB run takes 85.8 ms for ASCII and 1138 ms for `é`. Non-ASCII compares cost about 13x more because of to_lowercase, so a 16 MiB `é` file would take about 9 s (extrapolated, not run). Only hostile input reaches this, but Phase 2 should not run find on the UI thread without allowing for it.
- details_keys is linear: 50000 details in 0.5 ms, 200000 in 2.0 ms. find_visible over closed details: 1.2 ms and 5.3 ms.
- Repo untouched: HEAD still be52ac4, git status clean. The worktree is removed and pruned, build output deleted, no input over 10 MiB left on disk. Only 20K of probe source remains in /home/kaan/.cache/mdview-audit/launchmem.

**Not checked:**

- find at real 16 MiB or 48 MiB text sizes: only up to 2 MB was run, and larger figures are extrapolated.
- A hostile input that forces all MATH_PASSES plus the literal pass: I did not construct one. Observed reject patterns were linear, and the 6-parse ceiling comes from reading the code.
- Old-commit allocation count on the bare-URL path, so the 'fewer allocations' claim is confirmed only for the plain path (3415 to 3016). 762984d's parse_bench has no bare-URL section and I did not run that file separately.
- 16 MiB runs of the `- a\n` and table-row retained figures, which are 2 MiB measurements only. The 16 MiB `a|b` plus `a\n` table run gave 862 MiB RSS and 43.0 MiB retained, because pulldown-cmark stops the table at 786441 blocks, so that pattern does not scale linearly past that point.
- Whether `#\n` is the true worst case for peak memory: it is the worst of about 110 patterns tried, not a proven maximum.
- Comparison with GitHub: `gh api markdown` was not used, since nothing in this lens depends on GitHub's rendering.
- Absolute timings: the machine was shared (load 3-7) and the scale probes used thin LTO, while parse_bench used the workspace's fat LTO, so only ratios are meaningful.
- Launch path beyond parse, since no shell exists yet, and Windows behaviour: everything ran on Linux.

## tests-parity

**Second check did not run**: it was stopped. Treat these as unverified.

### tests-parity #0: UNVERIFIED, major

**File:** `crates/core/tests/fuzz.rs`

**Problem:** assert_scales only compares 256 KiB with 1 MiB, and the commit removed the old absolute limits (5 s scan, 20 s parse at 2 MiB). Code that is quadratic with a small constant passes: it fails only if the quadratic term at 256 KiB is at least half the linear term, and by 16 MiB (the limit) that term can be about 32x the linear one. Nothing now catches a slowdown at the size the limit allows, or a pure constant-factor regression.

**Reproduction:** I added zz_hidden_quadratic to an instrumented copy of fuzz.rs (probe/crates/core/tests/fuzz_ratio.rs under /home/kaan/.cache/mdview-audit/tests-parity/). It feeds assert_scales a closure doing 2 linear passes plus len^2/524288 byte reads. Output: RATIO 6.62 (3.17 ms to 20.98 ms), assert passes, then 'HIDDEN passed assert_scales; 256KiB 3.284857ms, 16MiB 2.849880511s, ratio 867.6 (linear would be 64)'. That is 13.5x slower than linear at the limit, and the test stays green.

**Proposed fix:** Add a third, larger size to the comparison, such as 256 KiB against 4 MiB, kept to a few cases if run time matters. Or add a generous absolute limit in release builds at the 16 MiB limit.

### tests-parity #1: UNVERIFIED, minor

**File:** `crates/core/tests/fuzz.rs`

**Problem:** The find arm of details_keys_and_find_stay_linear cannot detect the fix it appears to guard. With a fixed 5001-character query, the code without the budget is still linear in text size (5001 comparisons per position), so the ratio stays near 4. If the budget is reverted, this test does not fail; it just runs for a very long time. Only find_work_is_bounded_on_a_long_query_against_repetitive_text in regressions.rs catches the revert, and it takes about 110 s to do so.

**Reproduction:** Mutant `if work > budget` -> `if false`. Release probe with the 5001-char query: 'find 16K 92.238892ms capped=false ; 64K 406.882087ms capped=false ratio 4.41', below the 8x threshold. HEAD gives ratio 3.67 with capped=true. `cargo test --test fuzz details_keys_and_find_stay_linear` on the same mutant did not finish in 400 s (debug). The mutant run of regressions, core_tests and spec failed in 110-114 s.

**Proposed fix:** Remove the find arm, or make the query grow with the text (for example query length = text length / 16), so that the unbudgeted code really is quadratic.

### tests-parity #2: UNVERIFIED, minor

**File:** `crates/core/tests/fuzz.rs`

**Problem:** The scaling test passed every loaded run, so I saw no false alarm, but its margin nearly disappears under memory contention. Cache pressure hits the 1 MiB input harder than the 256 KiB one on every run, so taking the minimum over rounds does not remove it. One linear case went from 4.05x idle to 7.80x against a threshold of 8x.

**Reproduction:** Baseline on pinned idle CPUs 2,3 ('www under' parsing): RATIO 4.05, 403 ms to 1.636 s. Load: 4 pointer-chasing processes over 128 MiB each on CPUs 4-7, bursty spinners on CPUs 2,3, test binary on CPUs 2,3 with RUST_TEST_THREADS=2. 3 of 3 runs passed. Highest ratios: 'www under / parsing' 7.80 (355 ms to 2.77 s), 'colons / parsing' 7.67, 'entities / scanning' 7.56, 'backslash pairs / scanning' 7.51. Release parse of the same input is linear: x1.95-2.00 per doubling from 256 KiB to 8 MiB, 0.778 s at 8 MiB. So 7.80 is contention, not a parser defect.

**Proposed fix:** Use a larger gap between the sizes (16x with a 64x threshold, say), or on a miss re-measure the failing case alone after a pause before panicking.

### tests-parity #3: UNVERIFIED, minor

**File:** `crates/core/tests/regressions.rs`

**Problem:** The test picture_sources_only_count_inside_a_picture_until_its_image does not cover a <picture> that closes without an image. Removing leave_picture() from the </picture> branch survives all four test files, so a later unrelated <img> could pick up the closed picture's dark source. HEAD is correct; only the guard is missing.

**Reproduction:** Mutant P4 (remove self.leave_picture() in the Tag::Picture closing branch): regressions, core_tests, spec and fuzz all rc=0. Input `<picture><source media="(prefers-color-scheme: dark)" srcset=d.png></picture><img src=b>` gives dark=[""] at HEAD. gh api also leaves the img outside the picture: `<picture><source ...></picture></themed-picture><a ...><img src="b"`.

**Proposed fix:** Add that input to the test with `assert_eq!(d.images[0].dark_src, StrRef::default())`.

### tests-parity #4: UNVERIFIED, minor

**File:** `crates/core/tests/regressions.rs`

**Problem:** ReloadTracker::timed_out sets sticky_showing, but no test covers it. If that line is removed, a first load that times out and then succeeds on retry returns None, and 'Rendering exceeded 15 seconds' stays on screen. That is the stuck-status bug this tracker exists to prevent.

**Reproduction:** Mutant R3 (remove `self.sticky_showing = true;` from timed_out): regressions, core_tests, spec and fuzz all rc=0. The other three status mutants (R1, R2, R4) are caught.

**Proposed fix:** In first_load_is_not_announced_as_a_refresh, add: begin(false), timed_out(g), begin(false), finished(ok), then assert the result is Some with empty text.

### tests-parity #5: UNVERIFIED, minor

**File:** `crates/core/tests/regressions.rs`

**Problem:** Parts of the command table the commit claims to have added or kept have no test. The File menu separators and the strings ABOUT_TITLE, SHORTCUTS_TITLE and MATCH_COUNT_NAME are unchecked, as is the Help item label. command_table_matches_the_old_toolbar_and_view_menu checks only the View separators and two of the titles. The current values do match the old app; the risk is drift only.

**Reproduction:** Each mutant left regressions, core_tests, spec and fuzz at rc=0: K2 (no separator before Exit), K3 (no separator before Open recent), K5 ABOUT_TITLE="About", K6 SHORTCUTS_TITLE="Shortcuts", K7 MATCH_COUNT_NAME="Count", K8 Help label="Keys".

**Proposed fix:** Assert the full (label, separator_before) list for File, Edit and Help, as the test already does for View, and assert the remaining title constants.

### tests-parity #6: UNVERIFIED, minor

**File:** `crates/core/tests/regressions.rs`

**Problem:** bare_url_ending_in_an_entity_trims_as_on_github has gaps. Its `&x1;` case claims to cover a digit-bearing form, but `&x1;` is not an entity, so is_named_entity never sees it. Real digit-bearing entities are not tested, and COVERAGE's sentence 'A URL ending in a named entity does trim as on GitHub' is wrong for them. Also, every case starts its run at offset 0, except the second URL in one case. The offset-shift mutants escape this test, and E5, E6 and E6b are caught only by the time-budgeted fuzz tests.

**Reproduction:** Mutant E3 (is_named_entity accepts digits): all suites rc=0. Input `http://a.b/c&frac12; x` at HEAD: text `http://a.b/c½ x`, link `http://a.b/c½`. gh api: `<a href="http://a.b/c&amp;frac12">http://a.b/c&amp;frac12</a>; x`. So the URL does not trim there and the `;` stays outside the link. Mutants E5 (www no shift), E6 and E6b (base 0) pass regressions and fail only in fuzz (markdown_autolink_through_parser_fuzz, plus markdown_structure_fuzz for E5). HEAD is right on `para\n\nhttp://a.b/c&amp; x` and `y www.a.b/c&amp; x`, link `http://a.b/c` and `http://www.a.b/c`.

**Proposed fix:** Add `&frac12;`, plus the `para` and `y www` inputs, to the regression table. In COVERAGE, say the trim applies only to letter-only entity names and that `&frac12;` and `&sup2;` fall under the decoded-entity gap.

### tests-parity #7: UNVERIFIED, minor

**File:** `crates/core/tests/regressions.rs`

**Problem:** Several new shape and depth guards can be removed without any test noticing. Direct `<td>` attributes in a `<table>` with no row depend on the implied-row branch, which no test exercises with an attribute. The alert check for a flattened quote is also never exercised. I did not prove the other three removable guards are equivalent; they look redundant with flattened().

**Reproduction:** All suites rc=0 for: H5 (remove open_implied_row in the Td branch); A8 (`(Origin::Md, Node::Nothing) => false`); H4 (Tr parent guard); H6 (Td parent guard); H7 (md cell to Plain). Input `<table><td colspan=2>a</td></table>` gives colspan=[2] at HEAD, and no test asserts it. Under H5 the td tag returns early and the implied cell would be colspan 1 (by reading the code, not run under the mutant).

**Proposed fix:** Add the colspan input to table_content_always_has_a_row_and_a_cell. Add a test with an alert below 256 nested divs or sections. Remove H4 and H6 if they are confirmed redundant.

### tests-parity #8: UNVERIFIED, minor

**File:** `crates/core/tests/regressions.rs`

**Problem:** orphan_cells_and_rows_keep_their_text pins a result GitHub does not produce, and the difference is not listed. COVERAGE says a lone <td> or <tr> drops the tag and keeps the text 'as on GitHub', but inside a GFM table cell GitHub moves the text out and splits the row. The known gaps do not mention this case.

**Reproduction:** Input `| h |\n|---|\n| a <tr>b<td>c |\n`. The test asserts the last block's text is `a bc`, and it passes at HEAD. gh api: `b<markdown-accessiblity-table><table>...<tr><td>a </td></tr><tr><td>c</td></tr>...`, so `b` sits before the table and `a` and `c` are in separate rows.

**Proposed fix:** Add this to COVERAGE's known gaps, and say in the test that this expected value differs from GitHub.

### tests-parity #9: UNVERIFIED, minor

**File:** `docs/PORT_CHECKLIST.md`

**Problem:** Line 426 overclaims: 'every menu item, shortcut and status string above, in commands.rs ... and the toolbar and dialog titles'. Missing from commands.rs: the status 'Rendering…' (listed at line 165), and the worker errors 'Cannot start native renderer: ' and 'Could not render document: ' (lines 228-229). The old file dialog titles 'Open Markdown' and 'Open folder' are missing too, and they are not even in the inventory. The Keyboard shortcuts dialog also changed: 10 grouped lines became 18 per-command lines, and it is not listed under 'Deliberately different'.

**Reproduction:** Grep of crates/core/src for 'Rendering', 'native renderer' and 'Could not render' finds only RENDER_TIMEOUT. Grep of crates/core/src and docs/PORT_CHECKLIST.md for '"Open Markdown"' and '"Open folder"' finds nothing; old window.cpp has them at lines 844 and about 851 (getExistingDirectory). commands::shortcut_help() printed 18 lines such as `Ctrl+Shift+1\tFILES` and `Ctrl+Q\tExit`. The old text (window.cpp 624-630) has `Show Files / Outline\tCtrl+Shift+1 / Ctrl+Shift+2` and `Focus reading mode\tCtrl+Shift+F (Escape to exit)`, with no Exit line.

**Proposed fix:** Add the two dialog titles to strings and to the inventory. State that 'Rendering…' and the worker errors are dropped with the worker process, or carry them. List the generated shortcuts dialog as deliberately different.

### tests-parity #10: UNVERIFIED, minor

**File:** `crates/core/src/parse.rs`

**Problem:** The fix for fewer allocations on the bare-URL path has no regression test. It preserves behaviour, so protected_urls_give_the_same_strings_as_unprotected_ones would still pass if it were reverted. parse_bench reports heap use, but nothing asserts on it.

**Reproduction:** Not run as a mutant; reasoned from the diff. `autolink::restore(&t).into_owned()` and pushing the Cow directly produce identical strings, so no value assertion can tell them apart.

**Proposed fix:** Accept this and say so, or add a counting global allocator test that compares a protected and an unprotected parse.

**Checked and fine:**

- Baseline at be52ac4 in the probe crate: core_tests 7, fuzz 10, regressions 100 and spec 3 all pass (debug, -j 4, RUST_TEST_THREADS=4).
- I ran 74 single-change mutants against the source. New regression tests fail when reverted for these fixes: alerts (empty, nested, list, footnote, div, details, blockquote, setext both paths, literal fallback, reparse with and without $), duplicate footnotes, fit_parent, orphan tr/td, implied list and its end, ListMarker::None, span and length clamps (zero, overflow, inf, px, percent), CRLF and bare CR in math, entity trim, nbsp in fold and in the needle shortcut, the 16 MiB limit in parse and parse_bytes, details_keys, source outside picture, image ending the picture, ReloadTracker first load and manual sticky, toolbar order, Edit menu title, all three fragment decode sites, MAX_BLOCK_DEPTH.
- Find budget: mutants with budget off, budget at half, budget at 63 per byte, and work counted as 1 per attempt all fail find_work_is_bounded_on_a_long_query_against_repetitive_text.
- The claim that find is exact up to 64 characters holds. By reading the code, work is at most (characters in text) x (query length), which is at most bytes x 64, and the comparison is strict. The 63-per-byte mutant fails the 64-character test case, so that test sits exactly on the boundary.
- details_keys revert: fails the timed test (3.99 s in a plain release build against the 3 s limit) and the scaling test (34.29 ms to 568.26 ms, 16.6x). HEAD's timed test took 140-281 ms even under load, so it has plenty of margin.
- Scaling test under pinned CPU and memory load: 3 of 3 runs passed with no false alarm (see the margin finding).
- Command table against old window.cpp: toolbar order, labels and tooltips match (Open files…, Open folder…, Refresh, separator, find box, count, Previous with 'Previous match (Shift+F3)', Next with 'Next match (F3)'). Theme submenu (System, Light, Dark, radio, first in View), all File and View separators, Edit and Help items, tab context menu, shortcuts, menu titles, toolbar title 'Reader' and 'Manage recent files' all match. 'Show in File Explorer' is correct under cfg(windows).
- 'Document refreshed' only after an earlier load matches the old app: refreshingExisting = !renderedHtml.isEmpty(), and a failed first load does not count as loaded.
- Spec results match COVERAGE.md: CommonMark 652 examples, 645 match, 7 exceptions, 578 without the reduction. GFM 672, 665, 7, 598. Exceptions are checked in both directions, so a stale one fails.
- Spec exception CommonMark 191 / GFM 160 is described accurately: gh api puts the code before an empty table (`<pre>...&lt;td&gt;...</pre>...<table><tbody><tr></tr></tbody></table>`), while mdview uses an implied cell. Removing fit_parent makes both suites fail. It is a visible difference from GitHub, listed in COVERAGE, and the user should see it when reviewing exclusions.
- gh api agrees with the regression tests' expected results on about 50 snippets: alert placement, setext, duplicate footnotes including label case, entity-ending URLs, lone td/tr/li. The exception is the GFM-cell case reported above.
- The checklist's reference file bpcy5jj1w.txt exists, and its other Phase 1 statements on access keys, the 10000+ counter and the bytes-on-disk limit match the code.
- Repo left untouched (git status clean). Probes are in /home/kaan/.cache/mdview-audit/tests-parity/ (265 MB, no input file over 10 MiB on disk). One orphaned test binary of my own, left by a harness timeout, was stopped by its exact PID.

**Not checked:**

- Windows: the cfg(windows) label and canonical path tests were not run; the 'Show in File Explorer' check is from reading the code only.
- How Qt draws toolbar text and default tooltips for the labels (for example whether iconText strips the ellipsis). No shell exists yet, and shell-side details such as icons are not in the table.
- The real release profile (fat LTO): timings used a plain release build in the probe workspace.
- Whether fuzz-only catches (E5, E6, E6b) repeat on every run. Those tests are seeded but time-budgeted, so a slower machine runs fewer inputs. Each mutant was run once.
- The H4, H6 and H7 mutants were not proven equivalent. They survived all suites, and redundancy with flattened() is from reading the code.
- The fewer-allocations change was not mutated or measured, only reasoned about. Scaling false alarms under real swap pressure were not tried; the load was CPU and cache contention only.
- The plan's Phase 1 exit items other than those named in this audit's scope, and the user's question about when the apps can be tried, which is outside this audit step.

## Mutation log (tests and parity lens)

Each line is one fix deliberately broken. `rc=101` means a test caught it; `rc=0` means nothing did. Plain release build, not fat LTO; the run was stopped before the last mutants.

```
A1 alert_allowed always: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker', 'rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (6s)
A2 empty alert keep: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker', 'rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
A3 Rule setext off: rc=101 ['a_setext_underline_takes_the_marker_line_as_on_github', 'rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
A4 heading setext off: rc=101 ['a_setext_underline_takes_the_marker_line_as_on_github'] (3s)
A5 allow-list minus Div: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker'] (3s)
A5 allow-list minus Blockquote: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker'] (3s)
A5 allow-list minus Li: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker'] (3s)
A5 allow-list minus Details: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker'] (3s)
A6 allow minus footnote: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker'] (3s)
A7 allow minus listitem: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker', 'rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
A7b allow minus Alert parent: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker'] (3s)
A8 Nothing=>false: rc=0 [] | fuzz: rc=0 [] (51s)
A9 literal blockquote path off: rc=101 ['rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
A9b literal reject path off: rc=101 ['rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
A10 can_hide ignores bracket: rc=101 ['rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
A11 literal returns Err: rc=101 ['rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
A13 no reparse without $: rc=101 ['a_setext_underline_takes_the_marker_line_as_on_github', 'misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker', 'rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
A14 $ path ignores alerts: rc=101 ['misplaced_or_empty_alerts_stay_plain_quotes_with_their_marker'] (3s)
F1 footnote last/dup kept: rc=101 ['first_footnote_definition_wins_and_the_later_subtree_goes'] (3s)
H1 orphan tr/td kept: rc=101 ['orphan_cells_and_rows_keep_their_text'] (3s)
H2 fit_parent off: rc=101 ['commonmark_suite', 'gfm_suite', 'list_content_always_has_an_item', 'table_content_always_has_a_row_and_a_cell'] (3s)
H3 no auto list: rc=101 ['list_content_always_has_an_item'] (3s)
H4 Tr guard off: rc=0 [] | fuzz: rc=0 [] (51s)
H5 Td no implied row: rc=0 [] | fuzz: rc=0 [] (47s)
H6 Td guard off: rc=0 [] | fuzz: rc=0 [] (48s)
H7 md cell not plain: rc=0 [] | fuzz: rc=0 [] (48s)
H8 auto list does not end: rc=101 ['list_content_always_has_an_item'] (3s)
H9 implied item keeps marker: rc=101 ['list_content_always_has_an_item'] (3s)
H10 span no upper clamp: rc=101 ['html_spans_and_lengths_are_clamped'] (3s)
H10b span zero stays: rc=101 ['html_spans_and_lengths_are_clamped'] (3s)
H11 span overflow =>1: rc=101 ['html_spans_and_lengths_are_clamped'] (3s)
H12 inf allowed: rc=101 ['html_spans_and_lengths_are_clamped'] (3s)
H12b px no clamp: rc=101 ['html_spans_and_lengths_are_clamped'] (3s)
H12c pct no clamp: rc=101 ['html_spans_and_lengths_are_clamped'] (3s)
C1 math CR kept: rc=101 ['display_math_has_no_carriage_returns'] (3s)
C2 bare CR kept: rc=101 ['display_math_has_no_carriage_returns'] (3s)
E1 entity trim off: rc=101 ['bare_url_ending_in_an_entity_trims_as_on_github'] (3s)
E2 alnum entity: rc=101 ['bare_url_ending_in_an_entity_trims_as_on_github'] (3s)
E3 named alnum: rc=0 [] | fuzz: rc=0 [] (47s)
E5 www no shift: rc=0 [] | fuzz: rc=101 ['markdown_autolink_through_parser_fuzz', 'markdown_structure_fuzz'] (47s)
E5b url no shift: rc=101 ['bare_url_ending_in_an_entity_trims_as_on_github'] (3s)
E6 base 0 (last run): rc=0 [] | fuzz: rc=101 ['markdown_autolink_through_parser_fuzz'] (47s)
E6b base 0 (before escape): rc=0 [] | fuzz: rc=101 ['markdown_autolink_through_parser_fuzz'] (48s)
E7 entities not cleared: rc=0 [] | fuzz: rc=0 [] (48s)
SW swap check only 3 chars: rc=101 ['rejected_alert_is_still_kept_in_a_document_that_cannot_hide_it'] (3s)
N1 nbsp fold: rc=101 ['find_matches_a_typed_space_against_a_non_breaking_one'] (3s)
N2 nbsp needles: rc=101 ['find_matches_a_typed_space_against_a_non_breaking_one'] (3s)
B1 budget off: rc=101 ['find_work_is_bounded_on_a_long_query_against_repetitive_text'] (114s)
B2 budget half: rc=101 ['find_work_is_bounded_on_a_long_query_against_repetitive_text'] (2s)
B2b budget -1: rc=101 ['find_work_is_bounded_on_a_long_query_against_repetitive_text'] (2s)
B3 work counts 1: rc=101 ['find_work_is_bounded_on_a_long_query_against_repetitive_text'] (110s)
L2 parse_bytes limit off: rc=101 ['file_limit_is_enforced'] (3s)
L1 parse limit = 16MiB: rc=101 ['files_under_the_limit_never_load_blank'] (3s)
D1 details Vec: rc=101 ['details_keys_count_repeats_and_stay_fast_on_many_summaries'] (32s)
P1 source anywhere: rc=101 ['picture_sources_only_count_inside_a_picture_until_its_image'] (4s)
P2 image no leave: rc=101 ['picture_sources_only_count_inside_a_picture_until_its_image'] (4s)
P4 close no leave: rc=0 [] | fuzz: rc=0 [] (48s)
R1 first load announced: rc=101 ['first_load_is_not_announced_as_a_refresh'] (3s)
R1b loaded also on any outcome: rc=101 ['first_load_is_not_announced_as_a_refresh'] (3s)
R2 sticky manual off: rc=101 ['first_load_is_not_announced_as_a_refresh', 'refreshing_status_never_sticks'] (3s)
R3 sticky timeout off: rc=0 [] | fuzz: rc=0 [] (51s)
R4 sticky not updated: rc=101 ['first_load_is_not_announced_as_a_refresh'] (3s)
K1 toolbar swap: rc=101 ['command_table_matches_the_inventory', 'command_table_matches_the_old_toolbar_and_view_menu'] (3s)
K2 file sep Exit off: rc=0 [] | fuzz: rc=0 [] (48s)
K3 file sep Recent off: rc=0 [] | fuzz: rc=0 [] (48s)
K4 Title Menu Edit: rc=101 ['command_table_matches_the_old_toolbar_and_view_menu'] (3s)
K5 ABOUT_TITLE: rc=0 [] | fuzz: rc=0 [] (51s)
K6 SHORTCUTS_TITLE: rc=0 [] | fuzz: rc=0 [] (48s)
K7 MATCH_COUNT_NAME: rc=0 [] | fuzz: rc=0 [] (48s)
K8 Help label: rc=0 [] | fuzz: rc=0 [] (48s)
Fr1 fragment decoded: rc=101 ['a_fragment_is_decoded_once'] (3s)
Fr2 relative decoded: rc=101 ['a_fragment_is_decoded_once'] (3s)
Fr3 file url decoded: rc=101 ['a_fragment_is_decoded_once'] (3s)
Dp1 no flatten: rc=101 ['nesting_stops_at_a_fixed_depth_with_the_text_kept'] (3s)
```
