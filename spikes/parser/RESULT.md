# G4: parser comparison, pulldown-cmark 0.13.6 vs comrak 0.59.0

**Gate:** parse 1 MB with both; run the spec suites. Pass if numbers are recorded and pulldown misses at most 2% of GFM after the autolink pass. Otherwise comrak behind `parse()`.

**Result: PASS. pulldown-cmark stays.** 662/672 GFM examples, 10 misses, 1.49% (limit: 13 misses, since 2% of 672 is 13.44). Comrak misses the same 10.

**This PASS depends on two choices, so check them:**

| Reading of the rule | pulldown + autolink | comrak | Within 2%? |
|---|---|---|---|
| Used for the verdict: official per-example extensions, writer-neutral comparison | 662/672, 10 miss, 1.49% | 662/672, 10 miss, 1.49% | both yes |
| Extensions always on, writer-neutral | 657/672, 15 miss, 2.23% | 658/672, 14 miss, 2.08% | **both no** |
| Official per-example extensions, strict port of `normalize.py` | 639/672, 33 miss, 4.91% | 662/672, 10 miss, 1.49% | **pulldown no** |
| Extensions always on, strict port of `normalize.py` | 634/672, 38 miss, 5.65% | 658/672, 14 miss, 2.08% | **both no** |

Why I used the first row:

- **Per-example extensions** are what the official `spec_tests.py` does. 4 of the always-on misses (610, 616, 619, 620) are plain CommonMark examples that expect no bare-URL link, so they can't pass with autolink on. Comrak fails them too, so switching parsers wouldn't help.
- **Writer-neutral comparison:** the rule is about the parser, and the viewer uses events, not pulldown's HTML writer. The 23 extra strict misses are writer output only: literal `"` instead of `&quot;`, `style="text-align"` instead of `align`, and an empty `<tbody></tbody>` (a test confirms a header-only table has no row events). These are not hand-picked: the same rewrite on CommonMark takes pulldown from 631 to 652/652, matching the 652 spec tests it ships. The rewrite applies to both sides of every comparison.

If you read the rule strictly, the third row would point to comrak. I don't recommend that, given the speed and memory numbers below.

## Spec results (measured, deterministic)

Each cell shows passes, with misses in brackets. "Exact" is a byte comparison, "official-norm" is the strict port, "semantic" adds the writer-neutral rewrite. `results/spec_summary.txt` has the full output.

| Suite and configuration | Exact | Official-norm | Semantic |
|---|---|---|---|
| CommonMark 0.31.2, pulldown (/652) | 630 | 631 (21) | 652 (0) |
| CommonMark 0.31.2, comrak (/652) | 652 | 652 (0) | 652 (0) |
| GFM per-example, pulldown, no autolink pass (/672) | 620 | 628 (44) | 651 (21) |
| GFM per-example, pulldown + autolink pass (/672) | 631 | 639 (33) | **662 (10)** |
| GFM per-example, comrak (/672) | 660 | 662 (10) | 662 (10) |
| GFM always on, pulldown, no autolink pass (/672) | 620 | 628 (44) | 651 (21) |
| GFM always on, pulldown + autolink pass (/672) | 626 | 634 (38) | 657 (15) |
| GFM always on, comrak (/672) | 656 | 658 (14) | 658 (14) |
| Viewer flags (+footnotes, math), pulldown + autolink (/672) | 626 | 634 (38) | 657 (15) |
| Viewer flags (+footnotes, math), comrak (/672) | 656 | 658 (14) | 658 (14) |

The autolink pass adds 11 per-example passes, which is all 11 examples tagged `autolink`.

**The 10 misses, shared by both parsers:**

- **9 are nested strong (398, 426, 434, 435, 436, 473, 474, 475, 477).** This spec expects `<strong><strong>` collapsed to one. CommonMark 0.31.2 expects the nesting that both parsers produce. It should draw as plain bold (expected, not rendered here).
- **1 is tagfilter (652).** Neither parser supports it, and comrak's changelog says it removed the option. The plan's HTML allow-list has to cover this.

**Always-on adds:**

- **610, 616, 619, 620:** these expect no autolink, as above. Both parsers miss them.
- **614, pulldown only:** `<foo\+@bar.example.com>`. Text merging hides the backslash escape, so the pass links it. This is a real but rare limitation of the post-pass.

## 1 MiB benchmark (measured, 20 runs, noisy)

Document: seed 20261106, 1,048,757 bytes, 18,477 lines, sha256 `33706ebb...d249564`. It holds 271 headings, 1,945 paragraphs, 3,476 list items, 292 tables, 411 code blocks, 287 HTML blocks, 1,586 links, 6,117 emphasis spans, 542 tasks, 38 alerts, 35 footnotes and 60 math spans.

All cases use viewer options, including autolink. The one exception is `pulldown_walk`, which has no autolink pass.

Load average was 6.43 on 28 threads, with parallel builds running. Release profile follows the plan: fat LTO, 1 codegen unit, strip, unwind. Peak heap is deterministic across the 20 runs.

| Case | Time, median ms (min-max) | Peak heap, conservative | Peak heap, in-place realloc | Allocations |
|---|---|---|---|---|
| pulldown, streaming walk | 3.158 (3.098-4.236) | 9.48 MB | 6.58 MB | 1,917 |
| pulldown + autolink, streaming walk | 5.175 (5.092-6.183) | 9.48 MB | 6.58 MB | 3,227 |
| pulldown + autolink, all events retained | 6.287 (6.149-7.337) | 22.32 MB | 17.09 MB | 3,243 |
| comrak, parse + AST walk | 10.856 (10.303-13.178) | 18.59 MB | 18.55 MB | 95,997 |

- **Heap columns:** "conservative" counts old and new blocks during every realloc. "In-place" counts neither twice, so it is a lower bound. The true peak is between them. The document text is excluded, and so is malloc overhead.
- **Retained memory:** pulldown is not smaller if every event is kept. The memory advantage only holds if `parse()` streams events into a compact doc model. That model's size is not measured here.
- **First parse in a fresh process (20 processes each):** pulldown + autolink median 6.568 ms (6.060-9.103), comrak 12.376 ms (11.806-13.408).
- **Stripped minimal binary, one parse:** pulldown 690,432 bytes, comrak 1,000,504 bytes. Both include std, and the comrak binary enables only tables.
- **Package count, including the crate itself:** pulldown 5, comrak 18 (`cargo tree -e normal`, unique lines).
- **Estimated:** nothing in this file is estimated, apart from the "should draw as plain bold" expectation above.

## Agreement on the generated document (measured)

I compared normalized HTML from both parsers with GFM options, as a multiset of block chunks: 11,453 for pulldown, 11,491 for comrak. Every difference is accounted for:

- **38 alerts:** different writer markup (`<blockquote class>` vs `<div>` with a title paragraph).
- **171 loose task items:** the checkbox sits inside `<p>` in pulldown and before it in comrak. The renderer must handle the marker being inside the paragraph.

There were no other text or structure differences. Footnotes and math were off for this comparison, so those constructs were not compared here.

## Feature tests (measured, `cargo test --release`: 11 passed, 0 failed)

Each of these is confirmed for both parsers, except where noted:

- **GitHub alerts:** `BlockQuote(Some(kind))`. All 5 kinds tested on pulldown, WARNING on comrak. The marker doesn't leak into text. `[!FOO]` stays a plain quote (pulldown).
- **Footnotes:** `FootnoteReference` and `FootnoteDefinition`.
- **Task lists:** `TaskListMarker(bool)`, including `[X]`.
- **Strikethrough:** `~~x~~` and `~x~`.
- **Math:** `InlineMath` and `DisplayMath`. `$5 and $10` does not become math (pulldown).
- **` ```math ` fences:** an ordinary code block on pulldown. The viewer must route these by info string.
- **`fixtures/showcase.md`:** contains all of the above with `ENABLE_MATH`, `ENABLE_FOOTNOTES` and `ENABLE_GFM` on (pulldown).

Required pulldown flags: `TABLES | STRIKETHROUGH | TASKLISTS | GFM | FOOTNOTES | MATH`, plus the autolink pass (`autolink()` and `find_links()` in `src/lib.rs`).

## Not verified

- **Windows 11:** only `x86_64-unknown-linux-gnu` is installed here, so nothing was built or timed for Windows.
- **Normalizer port:** I ported `normalize.py` to Rust and did not run the original. Evidence that the port is sound: its docstring examples pass as tests, and comrak gets 652/652. Its entity table is limited to `lt`, `gt`, `amp`, `quot` and numeric references.
- **Autolink pass:**
  - It runs after inline parsing, so a URL containing emphasis delimiters such as `http://a.b/_c_` would probably split. Not tested.
  - It doesn't handle `xmpp:` or `mailto:` forms, which this spec.txt doesn't test.
  - It hasn't been fuzzed. The plan schedules that for later.
- **Spec version:** `spec.txt` came from cmark-gfm `master` and is built on an older CommonMark than 0.31.2. Its strong-collapse behaviour may not match github.com. No comparison against github.com was made.
- **Alert titles:** custom alert titles (`> [!NOTE] Title`) were not tested.
- **Real documents:** the benchmark document is synthetic, so real files will have a different mix.

## Reproduce

Run from `spikes/parser`. Spec files were downloaded on 2026-11-06 into `data/`:

- `spec.json` from `https://spec.commonmark.org/0.31.2/spec.json`, sha256 `d431b29d97b6f73e...`
- `spec.txt` from `https://raw.githubusercontent.com/github/cmark-gfm/master/test/spec.txt`, sha256 `7d8e5814befec287...`
- `normalize.py` and `spec_tests.py` from the same repo, read for reference and never executed

```sh
/usr/bin/cargo build --release -j 6
R=target/release
$R/gen 20261106 1048576 data/generated_1mib.md
$R/spec data/spec.json data/spec.txt results data/generated_1mib.md
$R/bench_time data/generated_1mib.md 20
$R/bench_heap data/generated_1mib.md 20
for n in pulldown_autolink_walk comrak_parse_walk; do
  for i in $(seq 20); do $R/bench_time data/generated_1mib.md --cold $n; done
done
/usr/bin/cargo test --release -j 6
```

Raw timings are in `results/bench_time.txt` and `results/cold_*.txt`. Per-example failures are in `results/fail_*.txt`.

## Verification

Independent audit, 2026-11-06. Load average was about 2, but the machine was not idle: a Windows VM and an unrelated Wine rustc were running. Raw output is in `results/verify/`.

**G4 verdict: PASS, but only under a comparator the plan does not mention.** With the official normalizer it misses 4.91%, not 1.49%. The choice is disclosed in the report, and I agree with it, because the 23 extra misses are not parser differences. You can still overrule it.

| Check | Report | Re-run | Result |
|---|---|---|---|
| Per-example, writer-neutral | 662/672, 10 misses | identical, same 10 examples | Confirmed |
| Per-example, strict | 33 misses | 33 | Confirmed |
| Always on, writer-neutral | 15 misses, 2.23% | 15 | Confirmed |
| pulldown + autolink, median | 5.175 ms | 5.188 ms | Confirmed |
| comrak, median | 10.856 ms | 9.857 ms | Confirmed, the old figure was noise |
| Peak heap, all 4 cases | as tabled | identical | Confirmed |
| `cargo test --release` | 11 passed | 4 + 7 passed | Confirmed |

**New evidence the report lacked:**

- **Official `normalize.py`, run for the first time** (Python 3, `-I`, a copy outside the spike), over pulldown's real output. It gives the same 33 misses as the Rust port. A Python re-implementation of the three `semantic()` rewrites clears exactly 23 of them, leaving the same 10. So the port is sound and the rewrite hides nothing else in this suite.
- **The autolink limitation is now confirmed, not "probably".** Comrak also stops at the delimiter, but keeps most of the URL:

| Input | pulldown + autolink | comrak |
|---|---|---|
| `http://a.b/_c_` | link `http://a.b/`, then emphasised `c` | link `http://a.b/_c`, then `_` |
| `https://example.com/*x*` | link `https://example.com/`, then emphasised `x` | link `https://example.com/*x`, then `*` |
| `https://x.y/~a~ z` | link `https://x.y/`, then struck `a` | link `https://x.y/~a`, then `~ z` |

Pulldown's link points to the wrong page in these cases. They are rare, because `foo__bar__baz`, `a_b_c` and Turkish paths all linked correctly. Phase 1 should fix this or list it as a known limit.

**Flags:**

- **Weakened criterion:** yes, as above, and it is disclosed.
- **The viewer's real configuration misses 2.23%.** The viewer keeps extensions always on, which is over 2% even with the neutral comparator. Four of those 15 examples only exist because the examples expect autolink off. Example 614 is a real pulldown-only miss.
- **Estimated presented as measured:** none found.
- **Benchmark validity:** it does walk real events and nodes, and uses `black_box`. It does not build a doc model, so retained memory for the viewer is still unknown.
- **Windows 11:** type-checks for `x86_64-pc-windows-msvc` with the private toolchain (`cargo check`). It was not linked or run there, so all numbers remain Linux only.
