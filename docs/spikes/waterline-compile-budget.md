# Spike: waterline's compile budget

**Verdict: neither target is close. Cold build sits at 29-31s against a 20s
target (iteration-2 of the five-pass convergence loop, not iterations 3+4 as
in the 2026-09 baseline, is now the single most expensive pass — roughly half
the instrumented total). A warm full rebuild (`narrow = false`) is 13-14s
regardless of what is edited — a page that mints no notes costs as much as one
that does, both roughly 15x the 3s target, which is the clearest sign the
per-edit cost has become almost entirely fixed rather than proportional to the
page count. A narrowed rebuild (`narrow = true`) resolves the same edit in two
rounds, ~11s and ~8s: `index` is always the round-2 dependent for a
note-minting edit because `@rookery/search`'s filter panel renders every
note's full body into `data-panel-text` on the home page, which binds
`idea-body:<id>` for every id. Emptying `@rookery/search`'s `.marrow.typ` cuts
round 1 to ~5.4s and round 2 to ~5.0s — roughly half of each round, even with
the already-landed per-compile ligament fix in place — so the search corpus
rebuild, not the registry merge, is now the largest single lead. Memory grows
~1.8-2.0 GB per edit under the shipped `max_age = 10`, identically under `4`
over the same 5 edits (both are still climbing — neither has reached the
generation count where they'd diverge), and visibly plateaus by the third/
fourth edit under `max_age = 2` (~13.2-13.3 GB). Swapping all of the project's
base64-inlined images for 1x1 placeholders cut total HTML output from 41.5 MB
to 26.3 MB but barely moved the per-edit memory growth curve (16.68 GB vs
16.88 GB after 5 edits) — output byte count is not what is driving the GB-
scale growth on this project; something scaling with the ~1,100+-note
registry/ligament/search-corpus work per rebuild is. `--timings` cannot
currently trace a narrowed rebuild's rounds separately — `Build::rebuild`'s
narrow path never calls `begin_rebuild_timings`, so every narrowed round
silently overwrites whichever trace file the last full rebuild wrote, which
blocked a function-level breakdown of round 1 for this spike and will block
the next one too.**

## Measurement conditions

- rheo binary: a `--release` build of this flight's checkout of
  `/home/lox/code/_fcl/rheo` (`rheo 0.6.4`, `Cargo.lock` pins `typst 0.15.0`/
  `typst-bundle 0.15.0`), built with `CARGO_TARGET_DIR=/home/lox/.cargo-target`
  and used by absolute path throughout — the `rheo` on `PATH` is older.
- Project: `/tmp/wl-budget`, a copy of `/home/lox/code/waterline/rookery`
  (`tar --exclude=./build`), 634 `.typ` files, 1,800 pages / 41.5 MB HTML cold.
  `[packages.rookery]` repointed at `/tmp/wl-budget-rookerypkg`, a copy of
  `/home/lox/code/_fcl/rookery`, via a `path =` entry — neither original
  repository was touched at any point. `[packages.rheodev]` was left on its
  `repo =`/`branch = "dev"` git clone of `/home/lox/code/_fcl/rheo-packages`
  (read-only clone, also untouched).
- Every run passed `--input today=2026-10-19`.
- Machine: `nproc` = 12, 60 GiB RAM. `pgrep -af 'rheo|typst'` showed nothing
  else running before every measurement window; `uptime` load sat at 0.9-5.4,
  the high end entirely this spike's own concurrent `rheo watch`/build
  processes (checked with `ps`), never a second compile running alongside one
  being measured.
- The note-minting edit: appending a one-line sentence to the body of the
  `#job("mpiwg-summer-26-27", ..)` call in `grad/jobs/cycle-26-27.typ`
  (the same site/line the bird's own problem statement used). The no-idea
  edit: appending a line to `writing/blog/devonthink-part-ii.typ`, a blog
  post that calls neither `#idea` nor `#note`.
- `cargo test --release`, `cargo fmt --check`, and `cargo clippy --release --
  -D warnings` all pass clean on this flight's checkout — 442 tests (428 +
  6 + 8) passed, 0 failed; no pre-existing failure to report.

## 1. Cold build (target <20s)

Three plain `rheo compile` runs (no `--iterations`, which itself adds
overhead — see below), fresh `--build-dir` each time:

| run | wall | typst-compile |
|---|---|---|
| 1 | 31.3s | 30.0s |
| 2 | 29.0s | 27.9s |
| 3 | 30.5s | 29.4s |

**9-11s over target.** Two `--iterations` runs (instrumentation inflates the
total — compare 24.5s/26.7s summed-iteration totals here against the 28-30s
plain `typst-compile` above) gave the per-iteration shape:

| run | iter(1) | iter(2) | iter(3) | iter(4) | iter(5) | 5 iterations? |
|---|---|---|---|---|---|---|
| 1 | 2.7s | 11.2s | 6.8s | 3.4s | 0.36s | yes |
| 2 | 3.0s | 12.2s | 7.4s | 3.6s | 0.47s | yes |

Still all **5** convergence iterations (no reduction to 4 as the 2026-09
baseline's two call-site fixes gave the old rookery). But the shape has
moved: `rookery-compile-profile.md` (2026-09-22, 807 pages) found iterations
3+4 carrying 76% of the compile; here iteration 2 ALONE carries ~45-46% of
the instrumented total, and iterations 2+3 together ~73-74%. The expensive
middle of the convergence loop shifted one iteration earlier as the site grew
from 807 to 1,800 pages. Why iteration 2 specifically got expensive is not
established here — see Lead 4.

## 2. Warm full rebuild (target <3s)

`narrow = false` (the project's current setting), no `--open`, `RUST_LOG=rheo=debug`:

| edit | run | wall | typst-compile |
|---|---|---|---|
| (a) note-minting (`cycle-26-27.typ`) | 1 | 15.1s | 13.2s |
| (a) note-minting, repeated | 2 | 14.2s | 12.6s |
| (b) no-idea (`devonthink-part-ii.typ`) | 1 | 13.7s | 12.2s |
| (b) no-idea, repeated | 2 | 13.4s | 11.9s |

**(b) is not near the old 0.9s** (`watch-bench-baseline.md`'s
`vertebra-edit` on `index.typ`, a comparable non-idea edit, 2026-09-25) — it
has regressed to roughly the same cost as the note-minting edit. The two
numbers differing by under a second, against a 13x gap in page count since
the 0.9s baseline, is the clearest evidence that a warm rebuild's cost today
is dominated by fixed work independent of which vertebra changed or whether
it mints anything, not by anything proportional to the edit's own content.

## 3. Narrowed round fixed cost

`[watch] narrow = true` added to the copy's `rheo.toml`, no `--open`. Same
note-minting edit, two repeats, both two rounds:

| run | round 1 (`typst-compile`) | round 2 (`typst-compile`) | `narrowed:` line |
|---|---|---|---|
| 1 | 11.1s | 8.2s | `3 vertebra(e) in 2 round(s) — grad:jobs:cycle-26-27, grad:jobs:cycle-25-26, index` |
| 2 | 11.4s | 8.2s | `3 vertebra(e) in 2 round(s) — grad:jobs:cycle-26-27, index, grad:jobs:cycle-25-26` |

**Ablation — `@rookery/search`'s `.marrow.typ` body emptied** (same technique
`narrowed-rebuild-profile.md` used), repointing `[packages.rookery]` at a
second `/tmp` copy of the package sources, never the real `/home/lox/code/_fcl/rookery`:

| run | round 1 | round 2 |
|---|---|---|
| 1 | 5.4s | 5.1s |
| 2 | 5.1s | 4.7s |

Search accounts for roughly **half of each round** (round 1: ~5.7-6.3s of
~11.1-11.4s; round 2: ~3.1-3.5s of ~8.2s) — even with the per-compile ligament
fix from `narrowed-rebuild-profile.md` already landed (`_LIG-RECS` is a
module-level value in `@rookery/core`'s `state.typ`; `_reg-final()` is now a
cheap spread, `(: ..live, .._LIG-RECS, ..live)`, not a per-call merge). The
registry merge that spike blamed for most of the then-13s "remaining" cost
has apparently already shrunk; what is left is dominated by search rebuilding
its whole corpus from `ideas().filter(...)` every round, exactly the cost
`narrowed-rebuild-profile.md`'s own follow-up recommended fixing (see Lead 2).

**The `_reg-final()`/no-merge ablation was NOT repeated here** — out of scope
for the time this spike had, and because the fix it was measuring the absence
of has already landed; re-deriving its current (now presumably much smaller)
share would need its own pass.

**Could not attribute round 1's cost by named function, as asked.** A
`--timings` trace was taken of a narrowed rebuild, but `Build::rebuild`'s
narrow path (`crates/core/src/build.rs`, `narrow_one_plugin` →
`compile_bundle_subset` → `compile_bundle_once`) never calls
`begin_rebuild_timings` — that is only called from `Build::run` (the full-
rebuild path, `devserver: false`) and `Build::compile_for_watch` (the
dev-server half, `devserver: true`). `current_timings_target` is therefore
whatever the session's last *full* rebuild set it to — in every session here,
the initial cold build — and `typst_timing::export_json` clears its buffer on
every export, so each narrowed round's `report_typst_instrumentation` call
silently overwrites that same file with only its own events. The file on disk
after an edit is real (it reflects *a* round's trace, not stale data), but
narrowed rounds across a whole watch session all alias onto one filename, so
there is no way to tell which round produced it, or to see round 1 once round
2 has run. See Lead 3.

## 4. Why `index` is a dependent of every prose edit

Confirmed, from source rather than a literal ligament dump (ligaments
themselves are an internal cache, not a file this spike could inspect
directly) plus the narrowed-rebuild logs above, which name `index` as a
round-2 dependent on every note-minting edit tried:

- `@rookery/search`'s `filter-panel.typ:238` calls `ideas(tagged: tag, values:
  true)` with the function's default `body: true` — every row includes its
  note's body.
- `ideas()` itself cannot emit a ligament bind (a documented limitation at
  `data.typ:283-301` — a code block can't join a `content` bind with the
  array `ideas()` returns). The bind happens instead where a row's body is
  actually realized into content: `filter-panel.typ:252`'s own comment says
  so directly — `data-panel-text below renders the row's body text, so this
  binds it too`.
- Confirmed in a built `index.html`:
  `grep -c data-panel-text` finds the attribute once per surviving note
  (`data-panel-text="donald knuth, litera…"`, `data-panel-text="edit chiara's
  paper …"`, …), and swapping images for placeholders (§5) shrank this same
  file's share of the 15 MB difference in total output, consistent with text
  this size being baked into every page that carries the panel.

**What would have to change in rookery:** pass `body: false` to the
`ideas()` call the filter panel uses to build `index`'s row data, and source
the panel's filter TEXT from somewhere that doesn't scale with every note's
own prose — the already-built search index (`@rookery/search`'s own
`.marrow.typ` already separates an "asset" mode, fetched once, from an
"inline" mode that pays the embedding cost per page) is the obvious existing
mechanism to reuse, rather than inventing a second one for the panel.

## 5. Memory

`narrow = false`, no `--open`, 5 edits (scaled down from the bird's 15 — see
limitation below), alternating a unique one-line append to
`grad/jobs/cycle-26-27.typ` each time, RSS read via `ps -o rss=` on the
watch process right after each rebuild's summary line:

| | cold | edit 1 | edit 2 | edit 3 | edit 4 | edit 5 |
|---|---|---|---|---|---|---|
| `max_age = 10` (shipped) | 6.73 GB | 9.18 GB | 11.27 GB | 13.15 GB | 15.08 GB | 16.88 GB |
| `max_age = 4` | 6.70 GB | 9.18 GB | 11.25 GB | 13.15 GB | 15.06 GB | 16.87 GB |
| `max_age = 2` | 6.69 GB | 9.18 GB | 11.26 GB | 13.17 GB | **13.24 GB** | **13.33 GB** |

(`max_age` edited locally in `evict_compile_cache`, `crates/core/src/build.rs`
— `comemo::evict(10)` → `evict(4)` → `evict(2)` → reverted to `evict(10)`
before this spike's final bank, each change rebuilt `--release` before
measuring.)

`10` and `4` are indistinguishable over only 5 edits — both are still well
short of the generation count (`evict_compile_cache`'s own doc comment:
plateaus "by the eleventh rebuild" for 10, "by the fourteenth" for 4) where
they would diverge. `2` visibly plateaus by the third/fourth edit here
(~13.2-13.3 GB), which is the behavior the doc comment predicts and is the
one difference this scaled-down run was able to demonstrate directly.

**Placeholder-images session**, same procedure, `max_age = 10` (the shipped
binary), every `.png`/`.jpg`/`.jpeg`/`.webp`/`.gif` under the project (static
AND content-colocated — `#image()` calls reference images next to their own
`.typ` file, e.g. `writing/weeknotes/26w24.typ`'s `img/desk-torino.jpeg`, not
only `static/`) replaced with a matching-format 1x1 placeholder (via
ImageMagick; a same-bytes-wrong-extension swap fails Typst's own format
sniffing and breaks the build outright — hit this once, fixed by generating
a real tiny file per format):

| | cold (output size) | cold RSS | edit 1 | edit 2 | edit 3 | edit 4 | edit 5 |
|---|---|---|---|---|---|---|---|
| real images | 41.5 MB | 6.70 GB | 9.18 GB | 11.27 GB | 13.15 GB | 15.08 GB | 16.88 GB |
| placeholders | 26.3 MB | 6.52 GB | 8.97 GB | 11.06 GB | 12.95 GB | 14.86 GB | 16.68 GB |

Cutting 15.2 MB of base64-inlined images off every page (41.5 MB → 26.3 MB,
confirming the bird's own 15.8 MB estimate) shrank the growth curve by only
~200 MB (~1.2%) after 5 edits. **This does not support `evict_compile_cache`'s
doc comment at face value** — its framing ("the growth tracks how much
DISTINCT OUTPUT a rebuild produces") reads as output byte count, but on this
project a 37% output-size cut barely moved memory growth. The comment's own
cited evidence (23.6 GB → 41 MB from fixing a per-page duplication bug) was
about output that scaled WITH PAGE COUNT, not about raw byte size — this
spike's result is consistent with that narrower reading, just not with the
sentence as written. What IS scaling with each GB-sized step here is more
likely the ~1,100+-note registry/ligament/search-corpus machinery each
rebuild re-derives, not the HTML bytes it ultimately writes.

**Limitation:** 5 edits per variant, not the bird's 15, to fit this spike's
time budget — each eviction-age variant needs its own `--release` rebuild
(~2-4 minutes of LTO link time) plus a fresh cold build and edit sequence, and
three variants plus the placeholder session was already four full cycles.
The `max_age=10` vs `4` numbers above are consequently inconclusive on their
own terms (see above); a follow-up with the full 15 edits per variant would
settle whether they ever diverge on this project the way the doc comment's
43 MB project did.

## Leads

Ranked by estimated seconds removed against the two targets (cold <20s, warm
rebuild <3s). None of these are filed — the operator files from this list.

1. **rookery — stop embedding full note bodies in the home page's filter
   panel.** `@rookery/search`'s `filter-panel.typ:238` calls `ideas(tagged:
   tag, values: true)` with the default `body: true`, baking every note's
   prose into `index.html`'s `data-panel-text` attributes (§4). This is the
   entire reason `index` is a round-2 dependent of every note edit. Passing
   `body: false` and sourcing panel filter text from the already-built search
   asset instead would likely remove round 2 outright for a note edit —
   **~8s off today's ~19s two-round narrowed rebuild**, and it is the biggest
   single number in this spike.
2. **rookery — rebuild `@rookery/search`'s corpus from per-note ligaments
   instead of `ideas().filter(...)` every round.** The no-search ablation
   (§3) shows search costs roughly half of EACH narrowed round even after the
   per-compile ligament-merge fix landed — **~5-6s off round 1, ~3-4s off
   round 2**, i.e. roughly half of the current ~19s two-round edit.
   `narrowed-rebuild-profile.md` recommended this already; it is still
   unaddressed.
3. **rheo — `--timings` does not instrument narrowed rebuilds per-round.**
   `Build::rebuild`'s narrow path never calls `begin_rebuild_timings`
   (`crates/core/src/build.rs`), so every narrowed round silently overwrites
   whichever file the last FULL rebuild (usually the cold build) wrote,
   rather than getting its own `rebuild-NNN.json` as the flag's help text
   promises. No seconds removed — this is an observability gap, not a
   latency cost — but it blocked a function-level breakdown of round 1 for
   this spike, and will block the next narrowed-path investigation the same
   way.
4. **rheo or rookery (unclear until traced) — iteration 2 is now the
   convergence loop's most expensive pass**, not iterations 3+4 as in the
   2026-09 baseline (§1). Nobody has re-profiled why the expensive middle of
   the loop moved one iteration earlier as the site roughly doubled in page
   count. Unknown savings until investigated — potentially several seconds
   of the ~30s cold build if the cause turns out to be registry/ligament
   work rather than pure Typst introspection, in which case it would also
   speed every narrowed round's typst-compile. A one-shot `rheo compile
   --timings` trace (unlike the narrowed path, correctly instrumented) is
   the right next step.
5. **rheo (docs only) — `evict_compile_cache`'s doc comment overstates what
   it measured.** Its "the growth tracks how much DISTINCT OUTPUT a rebuild
   produces" sentence reads as byte count; §5's placeholder-image result
   shows a 37% output-size cut barely moving memory growth on this project.
   The underlying evidence (a per-page duplication bug scaling with page
   count) still supports the comment's actual conclusion — only the
   phrasing invites the wrong generalization. No seconds removed; a doc
   clarification, not a code change.
