# Spike: is rheo using Typst's incremental compilation as well as it can

**Verdict: mostly yes. Of the two candidate fixes, only one does anything, and
only in a narrow case. Seeding the convergence loop with the previous
compile's introspector (H1) cuts an unchanged-bytes rebuild from 358ms to
142ms (5 iterations to 1) and a comment-only edit of a listing page's prelude
from 2.4s to 2.1s, but makes an edit to a note-minting weeknote 35% slower
(10.9s to 14.7s) and does nothing for any narrowed rebuild; it is a mixed
result and not worth landing as it stands. Giving each document its own
source file (H2) is refuted outright: its premise, that a changed document set
renumbers every document's spans, is true of Typst but already neutralised in
rheo by the incremental reparse in `parser::cache::source`, so it measures at
baseline or slightly worse everywhere (touch 370ms, prelude 3.0s, page add
26.2s against 358ms, 2.4s, 25.3s). H2 also exposed a latent cliff in that
parse cache: its two 1,024-entry tables are close to
rookery's working set, and exceeding them silently turns a 3s prelude edit
into a 40s cold build. That, and a 10s penalty on the first full rebuild after
narrowed ones that neither prototype explains, are the real findings.**

Both prototypes keep the accepted output a fixpoint, and output is
byte-identical to the unmodified binary's in every check below.

## Measurement conditions

- Baseline: `830b30b2` (`feat/performance` tip), `--release`, own
  `CARGO_TARGET_DIR`. Prototype binary: the three commits listed at the end,
  `--release`, separate target dir. `typst 0.15.0`, `nproc` = 12, machine
  otherwise idle, runs strictly sequential.
- Project: a copy of `waterline/rookery` (without `build/`), 2,513 output
  files, 841 `.typ` files, `[packages.rookery]` from the local checkout.
  Never the original. Cold build 41-46s.
- Driver: `rheo watch <copy> --html --input today=2026-11-16 --trigger stdin
  --events <file>` with `RUST_LOG=rheo=debug`, `rebuild <path>` lines on a
  fifo, one fresh copy and one fresh watcher per run. Figures are
  `phase="typst-compile"` `ms`. Not run under `--iterations` or `--timings`.
- One session runs six steps in order, each appending a never-seen-before
  comment line, so every step writes new bytes:

  | step | edit |
  |---|---|
  | b-index | `index.typ`, mints no notes |
  | c-weeknote | `nest/weeknotes/26w40.typ`, mints notes |
  | a-touch | `touch index.typ`, identical bytes |
  | d-prelude | `_lib/prelude.typ` |
  | f-add | new `nest/zz-probe.typ` with one `#todo` (spine change) |
  | g-edit-added | a further edit to that new page |

- Each cell is the median of **three sessions** (min-max in the raw files).
  `[watch] narrow = false` means every step is a full rebuild; `narrow = true`
  is rookery's setting, where b, c, a and g narrow and d and f go full.
- Iteration counts come from the prototype binary itself, in `debug` logs
  ("bundle convergence iterations=N"), because the loop is re-implemented
  there. The "baseline" iteration counts are from one session of the prototype
  with every switch off; its loop is the same as `typst::compile`'s and its
  timings matched the baseline binary's (349 / 4636 / 10763 / 2351 / 25246 /
  4854ms against 358 / 4232 / 10875 / 2372 / 25297 / 5196ms).
- Noise: the baseline's own runs vary by about 5% on the large cells and 20%
  on the occasional outlier (one narrowed d-prelude run read 15.2s against
  9.6 and 10.0s). Treat differences under 10% as noise.

## How the prototypes are switched

Environment variables, prototype only: `RHEO_WARM=1` seeds convergence
(H1), `RHEO_WARM_NARROW=1` also seeds and stores for narrowed compiles,
`RHEO_STABLE_DOCS=1` gives each document its own source (H2). With none set the
compile loop is Typst's own, re-implemented (`RheoWorld::compile_bundle_seeded`).
The re-implementation drops Typst's `analyze` on non-convergence and replaces
it with a one-line warning, and skips the experimental-feature warning
`typst::compile` raises (rheo filtered it anyway).

## H1: seed the convergence loop with the previous introspector

Typst's `compile_impl` starts iteration 1 from `EmptyIntrospector` on every
compile. The prototype starts it from the previous successful compile's
`Bundle.introspector` instead, kept per plugin on the long-lived `Build`, and
accepts an iteration under exactly the same test (the constraint validates
against the introspector it produced), so the output is still a fixpoint.

Medians, ms (`narrow = false`, every step a full rebuild), iterations in
brackets:

| step | baseline | H1 | H2 | H1 + H2 |
|---|---|---|---|---|
| a-touch | 358 [5] | **142 [1]** | 370 [5] | 145 [1] |
| b-index | 4,232 [5] | 5,012 [3] | 5,288 [5] | 4,828 [3] |
| c-weeknote | 10,875 [5] | 14,738 [5] | 11,733 [5] | 14,744 [5] |
| d-prelude | 2,372 [5] | **2,053 [1]** | 3,036 [5] | 2,693 [1] |
| f-add | 25,297 [5] | **22,705 [4]** | 26,176 [5] | 22,031 [4] |
| g-edit-added | 5,196 [5] | 5,187 [3] | 5,990 [5] | 5,419 [3] |

`narrow = true` (rookery's setting; b, c, a and g are narrowed compiles, d and
f full), medians, ms:

| step | baseline | H1 | H1, narrowed too | H2 | H1 + H2 |
|---|---|---|---|---|---|
| b-index (narrowed) | 2,532 [3] | 2,848 | 3,385 | 2,652 | 2,669 |
| c-weeknote (narrowed) | 1,417 [4] | 1,557 | 1,518 | 1,427 | 1,451 |
| a-touch (narrowed) | 909 [3] | 931 | 1,465 | 921 | 912 |
| d-prelude (full) | 9,991 [5] | 13,096 | 18,394 | 11,331 | 14,295 |
| f-add (full) | 22,284 [5] | 21,333 | 21,834 | 26,551 | 23,108 |
| g-edit-added (narrowed) | 1,721 [4] | 1,848 | 3,858 | 1,853 | 1,832 |

The "H1" column of the second table seeds full compiles only. "H1, narrowed
too" also seeds a narrowed compile with the last stored introspector, which is
the question of whether a full compile's introspector may seed a narrowed one.
It may, in the sense that output is identical (see correctness), but it is
slower on every step, 2x on the cheap ones, because the stored introspector
describes 2,400 pages and the narrowed compile has three. It should be
skipped, and the prototype's default skips it. The H1 columns come from the
first binary (1,024-entry parse caches, which only matter once H2 is on); the
H2 and H1 + H2 columns come from the final binary.

What the counters say. A temporary patch (not committed) added one
`eprintln!` to the bodies of `typst_eval::eval`, `typst_bundle::bundle_impl` and
`typst_html::html_document_for_bundle_impl`, which run only on a memo miss.
Misses per step, `narrow = false`:

| step | baseline htmldoc / eval / bundle | H1 htmldoc / eval / bundle |
|---|---|---|
| a-touch | 0 / 0 / 0 | 0 / 0 / 0 |
| b-index | 4 / 810 / 5 | 2 / 810 / 3 |
| c-weeknote | 1,630 / 810 / 5 | **3,142** / 810 / 5 |
| d-prelude | 0 / 803 / 0 | 0 / 803 / 0 |
| f-add | 8,012 / 811 / 5 | 7,193 / 811 / 4 |

Mechanism, confirmed for the cases it helps. When the edit changes nothing
Typst can see (touch, a prelude comment) the fixpoint is already known:
iteration 1 seeded with it validates, the loop stops at 1, and the five-iteration
cost of re-validating a hit (about 55ms each) falls to one. The `bundle`
misses column is why it cannot help further: after a touch even the baseline
misses nothing, so there is nothing left to save but those four re-validations.

Mechanism for the loss on `c-weeknote`. Baseline re-renders 1,630 documents,
once each, across its five iterations. Seeded, it re-renders 3,142, about twice
as many. Reading it as a consequence rather than as proven: without seeding,
iteration k of this rebuild is run against the same kind of introspector as
iteration k of the last rebuild (empty, then its own successors), so pages
whose reads are unaffected by the edit hit the previous rebuild's iteration-k
entries. Seeding replaces that fixed ladder with one that starts from the
previous fixpoint; the pages that read the edited notes miss in a different
pattern, and the entries left by the previous rebuild no longer line up with
it. The experiment supports the counts but not that explanation, which would
need the constraint contents of the missing entries.

Recommendation on H1: **do not land as it is.** It wins on the two cases that
were already cheap (140ms from 358, 2.05s from 2.4s) and loses 35% on the case
that matters most, an edit to a note-minting page. A version that seeds only
when the compile's inputs hash equal to the last compile's (a touch, a
comment-only prelude edit) would keep the wins and avoid the loss, but those
two cases are already under 2.5s; an equally effective and simpler change is
to skip the compile when every input hash is unchanged. That is not tested here.

## H2: one source file per document

### The premise is true

Typst numbers a source's spans with a stride of `space / (2 * descendants)`,
so one extra node moves every span, and a document's `Locator` is keyed by its
`#document` element's span (`locator.next(&elem.span())` in `typst-bundle`).
A test in `reticulate/bundle_source.rs` renders a main of documents `a b c`
and one of `a c` and asserts that the unchanged first document's `FuncCall`
span differs. It does. So a narrowed compile, a page add, or any change to the
document set renumbers a from-scratch parse of the main.

### And already handled

`parser::cache::source` (commit `5e842823`, "Keeps spans stable when an edited
file is re-parsed") never parses an edit from scratch: on a miss it takes the
last source served for that `FileId` and calls `Source::replace`, which
reparses only the changed region and leaves every other node's span alone.
Typst's content hash includes spans (`RawContent::hash` hashes `self.span`),
which is why that matters, and the counters show it working: with nothing but a
prelude comment changed, baseline misses 0 of 8,012 document renders while
re-evaluating 803 vertebrae. The main is served through the same path, so a
document inserted or dropped re-numbers the changed region only.

### Measured

H2 in `narrow = false`, medians: touch 370ms (baseline 358), b-index 5,288
(4,232), c-weeknote 11,733 (10,875), d-prelude 3,036 (2,372), f-add 26,176
(25,297), g-edit-added 5,990 (5,196). Narrowed, the miss counters are
identical to baseline's: 5, 3 and 21 document misses for the first three
narrowed steps in both. H2 is never faster and costs 800 more `eval`s per
rebuild (1,605 against 803) for the include of each document's file, which is
why the prelude step rises by 0.6s (touch is unaffected at 370ms).

### An aborted first measurement, kept because it found something

The first H2 build measured d-prelude at 40.8s with 8,005 document misses
(every document, every iteration), a total loss of the memo. The cause was not
H2 but the parse cache. `SOURCES` and `LAST_SOURCES` in
`crates/core/src/parser/cache.rs` each hold **1,024** entries and shed their
coldest half when full. Rookery's working set is about 840 vertebrae plus the
package files a compile reaches (about 150) plus the main and rheo's own
modules; H2 adds one source per document and takes it past the bound. Once
`LAST_SOURCES` loses a file id, the next edit parses that file from scratch,
every span in it changes, every content hash changes, and a 3s rebuild becomes
a 40s one. Raising both tables to 8,192 restored H2 to the
numbers above. See "Third leads".

Recommendation on H2: **do not land.** The mechanism it targets is real in
Typst and already defused in rheo. Narrowed rebuilds were never losing to span
renumbering.

## Third leads

1. **The parse cache sits close to a cliff on rookery.** `SOURCES` and
   `LAST_SOURCES` hold 1,024 entries each; rookery's baseline evaluates about
   810 vertebrae and parses roughly 150 package files on top of that
   (exact working set not counted). Nothing warns when a project crosses the line: it just starts
   doing from-scratch parses on every edit and loses span stability, which
   turns a ~2s comment edit into a ~40s one (observed under H2, 40.8s from
   2.4s, 17x). The cost of raising the bound is memory for parsed trees, which
   was not measured; 8,192 was used without a memory reading. Fixed on its own,
   independent of anything above: both tables are now 8,192.
2. **The first full rebuild after narrowed rebuilds is 4x what the same edit
   costs after a full one, and neither prototype explains it.** A prelude
   comment edit costs 2.2s right after the cold build, 2.4s after three full
   rebuilds, and 9.6-10.9s after three narrowed ones, with 1,631 document
   misses (one render of each document) where the others have 0. Ruled out:
   comemo eviction (`[watch] cache_size = 60` gives the same 1,631 misses and
   10.7s), span renumbering of the main (H2 keeps the same 1,631), seeding
   (H1 narrowed-too is slower, 16-18s). Not ruled out and the likeliest
   suspect: something a narrowed round feeds the library or `sys.inputs`
   (`rheo-ligaments` is seeded only there) that a full compile does not, so the
   next full compile's `library` hash no longer matches the entries the earlier
   full compile left. This is the largest remaining incremental-reuse loss
   found: it is the d-prelude row of the `narrow = true` table (~10s against
   2.3s), and it applies to any edit that sends a narrowed session to a full
   rebuild. Not investigated further here.
3. **Typst 0.15.1 changes nothing relevant.** Diffing 0.15.0 against 0.15.1:
   `typst`, `typst-eval` and `typst-realize` are identical; `typst-bundle` makes
   one error a delayed error (an image-format document with more than one page);
   `typst-html` re-indents cached SVG frames after generation (so the frame is
   cached regardless of nesting) and fixes a MathML flag; `typst-library` and
   `typst-layout` carry math-class and list-marker fixes. Nothing touches
   memoization keys, the convergence loop or the bundle introspector.
4. **The five-iteration hit cost is about 55ms per iteration.** A touch costs
   358ms with five iterations and 142ms with one, so re-validating the whole
   bundle against a changed introspector is a fixed per-iteration tax even when
   everything hits. Cheap rebuilds are dominated by it; the only way to avoid
   it is not to run the iterations, which is what H1 does for the touch case.
5. **`sys.inputs` hashing and per-document `PageInit`/state** were considered
   and not measured. Both are already stable across rebuilds in these runs:
   an unchanged-bytes rebuild misses 0 of 0 at every level, so neither is
   defeating the memo.

## Correctness

- `rheo compile` on an unchanged copy, baseline against H1 + H2 (the final
  binary, all switches on): `diff -rq` of the two output trees (2,513 files) is
  empty. No `<style>` stripping was needed. A cold compile seeds nothing, so
  this checks H2's synthesised sources and the re-implemented loop, not H1's
  warm path.
- After each watch session (run 1 of each configuration), the watched
  `build/html` was diffed against one fresh baseline `rheo compile` of the same
  final project state (after the page add and the edit to the added page).
  Baseline, H1, H2, H1 + H2, H1 narrowed-too and H1 + H2 narrowed-too, in both
  `narrow` modes: **zero differing files in all ten**. That includes baseline
  under `narrow = true`, so the known narrowed-versus-full differences of
  `narrowed-vs-full.md` did not appear in this particular final state, which
  ends on a full rebuild and a one-comment edit. This is a weak check for H1,
  since every edit before the page add was comment-only and so changed no
  output; only the page add is a real test of a seeded fixpoint.
- `cargo test -p rheo-core` on the final tree: 481 passed, 0 failed, with no
  switches set. With `RHEO_STABLE_DOCS=1` seven tests that assert the inline
  shape of the synthesised main fail, as expected; no test exercises H1's
  warm path or the combined-PDF layout under H2.

## Not done

- No memory measurement for the larger parse caches, or for holding a second
  introspector alive between rebuilds (H1).
- Only the HTML plugin. PDF and EPUB, and the combined-PDF layout under H2,
  were not run.
- No attribution of the 1,631-miss penalty after narrowed rebuilds (third lead
  2); the ligaments-in-the-library suspect is untested.
- The explanation for H1's loss on `c-weeknote` rests on miss counts alone.
- `--iterations`/`--timings` were not used for timings, by design.
- Narrowed-too columns for d-prelude, and the first H2 numbers, were taken on a
  binary with 1,024-entry parse caches and are marked as such above. Each
  cell is three sessions; no cell is a larger sample.
- A touch that changes nothing could skip the compile outright (358ms to
  roughly zero) with a content-hash guard on every input; not prototyped.

## Prototypes

Not landed. The prototypes (env-switched, as above) lived in a scratch jj
workspace on top of `830b30b2`: "Seeds convergence with the previous
introspector" (H1) and "Splits each bundle document into its own source
file" (H2, with the span-shift unit test). Only the parse-cache bound landed.
