# Spike: what one edit costs under `rheo watch` (wl-db988471)

**Verdict: an edit to a page that mints no notes costs 635ms of rebuild and
0.85s from saving the file to the summary line appearing — down from 858ms and
1.08s, after two commits that stopped a rebuild redoing the ~800 parses it had
already done. What is left is almost all inside `typst::compile`: 565ms of the
635ms, of which 480ms is Typst re-rendering all 812 pages through five
convergence iterations. The expensive case is an edit to a page that *does*
mint notes — 4.7s — and that cost is the note registry's blast radius in
`@rookery/core`, not in rheo.**

A sibling spike, [`rookery-compile-profile.md`](rookery-compile-profile.md),
profiles the *cold* build. This one is the warm case, and they are not the same
question: incremental rebuild latency is what is being optimised here, and a
slower cold build would be an acceptable price for a faster rebuild. In the
event nothing here cost the cold build anything — it got faster too.

## Measurement conditions

- rheo checkout: `/home/lox/code/_fcl/rheo`, workspace version `0.6.4`, built
  `--release` with `CARGO_TARGET_DIR=/home/lox/.cargo-target`. Typst pins
  (`Cargo.lock`): `typst 0.15.0`, `typst-bundle 0.15.0`.
- Machine: `nproc` = **12**.
- Project: a `/tmp` copy of `/home/lox/code/waterline/rookery`, **812 pages,
  26.9 MB (+2 assets)**, 145 `.typ` files on disk. A copy, so that nothing
  measured here edits the real project or its build directory.
- Two binaries, from the same checkout: **before** = the commit
  `Records where a cold rookery compile's 12 seconds go`, **after** = two
  commits later, with both parse-caching commits in. Same project, same
  session shape, same afternoon.
- Command, per session:

  ```
  rheo watch /tmp/rookery-perf/src --html \
    --build-dir /tmp/rookery-perf/bench-bld \
    --input today=2026-09-22
  ```

- Each scenario is **three runs**, waiting for one rebuild's summary line
  before making the next edit. An edit is one appended comment line; the
  numbers below are the three runs in order, not an average.
- Method note, because it changed an answer: the first pass at this was
  measured in a session that was also writing a `--timings` trace per
  rebuild — 4.4 GB of them — and every number came out roughly double
  (a cheap rebuild read 1.8s rather than 858ms). **Do not measure latency in
  a session that is writing traces.** Every figure below is from an
  uninstrumented session, and the traces analysed further down were taken
  separately.

## The five scenarios

Rebuild wall is the `in Ns` figure from rheo's own INFO summary line.
End-to-end is wall-clock from the `write()` that saves the file to that line
appearing, so it includes the debounce.

| what changed | rebuild wall, before | rebuild wall, after | end-to-end, before | end-to-end, after |
|---|---|---|---|---|
| nothing (`touch`, identical bytes) | 418 / 424 / 427ms | **221 / 208 / 204ms** | 0.64-0.65s | **0.37-0.43s** |
| one file under `static/` (asset only) | 419 / 431 / 441ms | **209 / 217 / 213ms** | 0.65-0.71s | **0.41-0.42s** |
| `index.typ`, a listing page that mints no notes | 891 / 858 / 833ms | **658 / 635 / 626ms** | 1.05-1.10s | **0.81-0.87s** |
| one weeknote, which mints notes | 5.4 / 5.0 / 5.1s | **4.9 / 4.7 / 4.7s** | 5.20-5.60s | **4.84-5.05s** |
| `_lib/prelude.typ`, which every vertebra imports | 9.7 / 9.4 / 9.6s | **9.6 / 9.3 / 9.3s** | 9.67-9.93s | **9.48-9.77s** |
| the session's own cold compile, for reference | 13.2s | 11.7s | — | — |

The `packages` phase, reported on every one of those lines, went from
**158-180ms to 43-52ms**. It is the same figure whatever changed, because it
never depended on what changed.

Two of these scenarios answer questions the tracker had open:

- **An asset-only change is not the expensive waste it was assumed to be.** It
  does still trigger a full `typst::compile` — but that compile costs 67ms,
  because comemo serves all of it. Skipping the recompile for an asset change
  would save 67ms and would need a correctness guard (Typst reads image files
  through the world, and this project's own convention puts images beside the
  `.typ` files that name them), so the guard would cost more than the saving.
  **Not worth filing.** This is a negative result and it retires a lead.
- **The floor is now 204ms, not 418ms.** A rebuild where literally nothing
  differs is what every other rebuild pays on top of, and halving it helps
  every case.

## Where the time went, and where it goes now

Both traces below are of the same scenario — an appended comment line in
`index.typ`, the cheap case — taken with `--timings <DIR>`, which writes one
Chrome-tracing JSON per rebuild. A *warm* rebuild's trace is ~9.5 MB and 70k
events, which is small enough to analyse directly; the cold build's is 2.5 GB
and 17.7M events, which is the problem the sibling spike describes.

| event | before: count | before: incl | after: count | after: incl |
|---|---|---|---|---|
| `create source` | 1,293 | 0.60s | **0** | **—** |
| `parse` | 1,808 | 0.62s | **0** | **—** |
| `eval` | 147 | 0.35s | 147 | **0.05s** |
| `html document` | 2,694 | 2.02s | 2,694 | 1.50s |
| `compile` (the whole `typst::compile`) | 1 | 1.15s | 1 | **0.57s** |
| whole trace | 74,014 events | | 68,688 events | |

`create source` and `parse` are gone from the trace outright, which is the
mechanism rather than a faster number: nothing re-parses.

**Where those parses had been is the part the earlier reconnaissance got
wrong.** Bucketing their start timestamps by 250ms shows ~800 of the 1,293
landing *before* `typst::compile` is entered at all — in the `packages` phase
and the spine scan — and only ~300 inside it. An earlier note on the tracker
put `create source` at 23,671 spans and 6.27s and called it constant across
rebuilds; that figure came from a trace that still held a cold build's events.
The real shape was ~1,300 parses, two thirds of them in rheo's own pre-compile
phases, and the fresh `RheoWorld` was the smaller half of the problem.

Three things were re-parsing every file on every rebuild:

1. **The package-import scan** read and parsed every project `.typ` *and*
   every `.typ` of every imported package — ~650 files here, since
   `@rookery/core` resolves from a local checkout — to find which `@` imports
   exist. That is the `packages` phase's 158-180ms.
2. **The spine scan** parsed every vertebra again to harvest its labels.
3. **`RheoWorld`** parsed every vertebra a third time, because a world is
   built fresh inside each compile and its source slots therefore start empty
   on every rebuild.

And underneath all three, `parser::syntax_site::parse_source` threw away the
tree the `Source` it was handed had *already* parsed and re-parsed the same
text, so each of those was two parses rather than one, despite the module's
own doc comment promising one.

## What the two commits changed

Everything now goes through one content-addressed cache
(`crates/core/src/parser/cache.rs`). The key is a 128-bit hash of the bytes,
so a hit is indistinguishable from redoing the work and changed bytes simply
miss. **It cannot serve a stale result, only an identical one** — which is why
this needed no changed-path set threaded through the build, no invalidation
logic to get right, and no reuse of the world itself. `RheoWorld::reset` is
still dead code, and is still not called.

The cost is memory: parsed trees and harvested label sites held between
rebuilds, bounded per cache (1,024 sources, 4,096 harvests) with the coldest
half shed when a cache fills, so a long session that edits one file thousands
of times does not grow without bound.

Correctness, as checked rather than argued: a cold `rheo compile --html` of
this project emits a tree `diff -r` reports **identical** to one built before
either commit; under `watch`, an edit whose rendered output changes reaches
the built HTML, and a deleted vertebra's pages disappear.

## The end-to-end budget

For the cheap case, after:

| part | cost |
|---|---|
| debounce and filesystem notice | ~0.2s |
| rheo's non-compile phases (`packages`, spine, marrow, mould, export, write) | ~0.07s |
| `typst::compile` | 0.565s |
| **save to summary line** | **0.85s** |

The debounce is a rebuild fires after 150ms of filesystem quiet, or 750ms
after the first event of a batch, whichever comes first. It is an additive
floor on every rebuild and it is now the second-largest term in the cheap
case, which it was not before.

With `--open`, the dev server runs a second compile of its own per rebuild.
That was measured at **0.14s** — not the doubling it looks like on paper, but
real, and `wl-serves-the-dev-server-from-the-rebuild-a72d6ad7` covers removing
it.

## Inside the compile: what a rebuild still re-runs

A warm rebuild runs **all five convergence iterations**, every time, however
little changed. What varies is what each costs:

| iteration | cold | warm rebuild, before | warm rebuild, after |
|---|---|---|---|
| iter (1) | 1.33s | 0.026s | 0.018s |
| iter (2) | 0.59s | 0.043s | 0.031s |
| iter (3) | 5.92s | 0.305s | 0.180s |
| iter (4) | 3.35s | 0.274s | 0.168s |
| iter (5) | 0.15s | 0.205s | 0.135s |

So **iteration count is a cold-build problem and blast radius is the
incremental one.** Comemo is doing a great deal of work here — 68,688 spans on
a warm rebuild against 17.7M cold — but it re-runs `html document` for all
2,694 page-iterations regardless, and that is 1.5s of CPU across 12 threads,
or the bulk of the 565ms wall.

## Candidate optimisations — leads, ranked by per-edit latency removed

1. **Narrow a note edit's blast radius.** The weeknote case is 4.7s against
   the listing page's 635ms, and the difference is that editing a page which
   mints notes invalidates the note registry every other page reads. That is
   the single largest per-edit cost left, it is roughly 4s, and it is in
   `@rookery/core`, not in rheo. Already filed as
   `wl-narrows-a-note-edit-s-blast-radius-d654058b`.
2. **The dev server's second compile**, 0.14s per rebuild with `--open`.
   Already filed.
3. **The debounce**, ~0.2s of the 0.85s. Cheap to change and risky to change
   for the right reason: a shorter quiet window means rebuilding on a
   half-written file. Not filed; this is a knob, not a defect.
4. **Skipping the recompile for an asset-only change.** Sized at 67ms and
   needing a correctness guard. Explicitly **not** filed — see above.
5. **The convergence loop.** Not a lead for the incremental case at all: the
   five iterations cost 0.53s of 565ms warm, and cutting one would save
   ~0.15s, against ~3.3s cold. The instrument for reading iteration count
   cheaply is filed as `wl-summarises-typst-s-convergence-5acf3cf6`.

## What this does not tell us

- **Why `html document` re-runs 2,694 times on a rebuild where one page
  changed.** 812 pages over five iterations would be 4,060, so comemo is
  serving about a third of them; which third, and what would let it serve
  more, is not established here.
- **Whether the parse caches help or hurt a very long session.** They were
  measured over sessions of a dozen rebuilds. The shed-the-coldest-half bound
  is reasoned about, not measured under pressure.
- **Nothing here is measured on any project but this one.** A project with no
  local-path package dependency has far fewer files in the package scan and
  would see much less of the `packages` phase win.

None of this reaches any published waterline site until rheo is released and
each site's pinned `version` in its `site.toml` is moved: `scripts/build-site.sh`
downloads the pinned release rather than building this checkout.
