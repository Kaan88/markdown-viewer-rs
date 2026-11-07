# Handoff: Phase 1 audit fixes complete

**Branch:** `phase1-audit-fixes`, based on `main` at `4127954`. The work is committed and pushed on this branch. [PLAN.md](PLAN.md), [AUDIT_PHASE1.md](AUDIT_PHASE1.md), [COVERAGE.md](COVERAGE.md) and [PORT_CHECKLIST.md](PORT_CHECKLIST.md) now agree with the code; where they disagree with this file, this file is current.

## State

- The core crate's tests pass **on Windows**: 7 core, 11 fuzz, 113 regression and 3 spec tests, release profile. The fuzz and regression counts grew on this branch because every behaviour changed by the audit now has a test that fails if it reverts; the alert, implied-block and CRLF tests were mutation-checked by reverting the fix and watching them fail.
- The spec suites still pass with 645/652 CommonMark and 665/672 GFM, the same 7 listed exceptions each.
- Every finding the Phase 1 audit confirmed is either fixed with a regression test or listed as a known gap in `COVERAGE.md`. `AUDIT_PHASE1.md`'s table has a last column saying which, per finding.
- `cargo fmt --all` is clean and the build has no warnings. There is still no CI, so run the tests yourself.

## Windows setup

`cargo test` works from a plain shell now. The test helpers ask for a directory under `%TEMP%` first and fall back to a workspace-local `target/tmp` when that is not writable. On this host the DSH file sandbox blocks writes outside the workspace even when the runtime context says `danger-full-access`, so the fallback is what runs here; PowerShell and Rust probes confirmed it is an environment limit, not a repo bug.

Setting `TMP`/`TEMP` to `target\tmp` still works but is no longer needed:

```powershell
New-Item -ItemType Directory -Force target\tmp | Out-Null
$env:TMP = (Resolve-Path target\tmp).Path
$env:TEMP = $env:TMP
cargo test --release -p mdview-core
```

A debug `cargo test -p mdview-core` also passes; the fuzz suite is simply slower (about 45 s against about 6 s in release).

Other setup notes:

- Git's schannel backend fails in this sandbox with `SEC_E_NO_CREDENTIALS`; use `gh auth token` plus `-c http.sslBackend=openssl`. The remote URL in `.git/config` is the plain HTTPS URL, so no token is stored there. `gh api markdown` works normally and was used for every GitHub comparison.
- Cargo network access and the MSVC toolchain (Rust 1.99.1) work.

## What this branch changed, by audit finding

The full per-finding list is in `AUDIT_PHASE1.md`. The user-visible decisions are these:

- **Raw HTML.** `script`, `style` and `iframe` show as literal text, as GitHub's tag filter does. `object`, `embed`, `head` and `template` are dropped as tags with their children kept, so an `<object>` fallback shows. A raw `<input>` is ignored; Markdown task boxes still draw.
- **Alerts.** A blank quote line between the marker and a heading, rule or `***` keeps the alert; `---`/`===`/`-`/`--` directly under the marker turn that line into a setext heading. A `#x` line plus `---` is setext text, not ATX. An empty body made only of inline HTML (`<i></i>`, `<a href=x></a>`, `<img>`, `<p></p>`, `<span></span>`) keeps the alert, while a comment, unknown tag, `<a>` without href or raw `<input>` does not. A `<td>`/`<th>` resets the `<div>`, list and quote ancestors; `<details>` still rejects inside a cell.
- **Tables and lists.** An implied row, cell or item whose subtree has no text and no atoms is dropped, so empty tags no longer add phantom rows, columns or marker-less items.
- **Lengths and images.** `width`/`height` follow the HTML dimension rule (`10em` is 10, `+5` and `.5` are no length, `50 %` is 50px). `colspan`/`rowspan` accept a leading `+`. Image dimensions clamp to 2000 on both axes together; table attributes keep 12000.
- **Pictures.** A `<source>` counts only as a direct child of a `<picture>`, only for its first `<img>`, and only while the picture's block is open; the first matching source wins.
- **Footnotes.** References inside a skipped duplicate definition are numbered, as on GitHub. A definition flattened at `MAX_BLOCK_DEPTH` sets `defined` but keeps `block == NONE`. The fuzz bound and its comment now say `MAX_BLOCK_DEPTH + 5`.
- **Private-use characters.** `can_hide` tests the whole `SWAP_PREFIX` range (U+E000 to U+E03F), so a document's own characters survive a rejected `$` or alert.
- **Find.** A CRLF pair in a query is one break, not two spaces. A multi-line selection from a code block or across two paragraphs still never matches; that and the real budget condition are listed in `COVERAGE.md`.
- **Dead code.** `Tag::DropContent`, the `Pending` machinery, `wait_for_inline_closer`, `Tokenizer::skip_past_close` and `find_close_tag` are gone.

## Decisions recorded

- **Footnote relocation is the Phase 2 renderer's job.** Core keeps each definition where it was written. The renderer draws the end-of-document list in order of first reference, skips `FootnoteDefinition` blocks in flow, and does not draw unreferenced ones (number 0). Until then, find and the block order see mid-file and unreferenced definitions. This is in `COVERAGE.md` under "Decided, work left for later phases".
- **Two alert gaps are documented, not fixed**: a marker that is not on the quote's first line (`> ` blank or `> <!-- c -->` before `> [!NOTE]`), and a marker that is also a reference label (`[!NOTE]: /u`). Both are in `COVERAGE.md`'s known gaps.
- **`tests-parity #10` is accepted**: no value assertion can distinguish the fewer-allocations change on the bare-URL path, and the test says so.
- The script/style/iframe question from the old handoff is closed: literal text, implemented, tested.
- No test was added for a pure constant-factor slowdown at 256 KiB; the new `scaling_holds_at_the_file_limit` test covers the 16 MiB end, not a fixed 2x.

## Still valid from the previous handoff

- Decisions are unchanged: WinUI 3 on Windows, Qt Widgets through CXX-Qt on Linux, custom renderer, no webview, Windows 11 only, no screen-reader work, private repo, no CI, all authored code in Rust.
- Coverage exclusions in `COVERAGE.md` remain accepted unless the user says otherwise.
- Text must use `Format::CustomSubpixel([0.3, 0.0, -0.3])`, not `Format::Subpixel`. The renderer still needs the regression test.
- Nothing counts as working on Windows until it has run on Windows; the core now has, the WinUI spike and shell have not.
- The user wants a window they can open real files in as soon as the renderer can draw a page. The phase-order question from the old handoff (Phase 2 then a first WinUI window, instead of the plan's Qt-first order) is **still unanswered** and is the first thing to settle next.
- `spikes/winui/W0.md` is still the first-run checklist; the spike's Linux build scripts have not been ported to Windows.

## What to do next

1. Settle the order: Phase 2 renderer (WinUI first?) against the plan's Qt-first order, and tell the user what they get first.
2. Build the Phase 2 renderer against the model in `crates/core/src/model.rs`, including the footnote end list and the `CustomSubpixel` text format.
3. Open a WinUI window on a real file. `spikes/winui/W0.md` is the checklist; nothing in the shell has run on Windows yet.
