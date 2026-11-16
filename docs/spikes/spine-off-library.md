# Spike: moving the spine off Typst's library

**Verdict: worth doing, and it is the whole fix for "adding a page costs a cold
build". With `spine` and `spine-flat` behind a constant-hash store, a page add
whose spine was never compiled before drops from 39.6 s to 13.5 s of
typst-compile (2.9x), and the first add of a session from 39.0 s to 22.5 s
(1.7x). Deleting a page, and adding one whose spine was compiled before, do not
move: they were already cache hits (0.3 s and 10.3 s). The prototype's output
is byte-identical to the unmodified binary's on a full build, and its narrowed
rebuilds differ from a full build in exactly the files the unmodified binary's
do. The cost is a breaking contract change: `spine`/`spine-flat` leave
`sys.inputs.rheo-context`, so every released `@rheo` package that reads them
off `sys.inputs` needs a release.**

## Measurement conditions

- rheo: `--release` build of this branch (`feat/performance`, tip has
  `LigamentStore`) for the baseline; the same tree plus the prototype below for
  "after". Both run by absolute path, built with separate `CARGO_TARGET_DIR`s.
- Project: `/tmp` copy of `waterline/rookery` (2,395 pages cold, 841 `.typ`
  files), `[watch] narrow = true`. `@rookery` and `@rheo` both resolve from
  `path =` copies in `/tmp`, because the prototype has to patch their spine
  readers. The baseline copy points at pristine copies of the same two trees, so
  before and after differ only in the prototype. `@rheo` therefore comes from
  rheo-packages' working tree, not the released tarballs.
- Command: `rheo watch rookery --input today=2026-11-15 --html --trigger stdin
  --events events.jsonl`, `RUST_LOG=rheo=debug`; `rebuild <path>` lines on a
  fifo. Figures are `phase="typst-compile"` `ms`. Machine idle, `nproc` = 12,
  before and after run sequentially, a fresh `build/` and a fresh watcher per
  run.
- Every step writes bytes never seen before in that run. A step-(c) that only
  retitles an idea does not change the spine (spine titles are path-derived, see
  `docs/limitations.md`), so the spec's literal (c) is a content-only rebuild; step (e)
  below is the page add with a never-compiled spine.

| step | spine after the step | before (ms) | after (ms) |
|---|---|---|---|
| (a) add `nest/zz-probe.typ`, one `#idea` | new, first rebuild of the session | 38,966 | 22,458 |
| (b) delete it | the cold spine, still cached | 292 | 298 |
| (c) recreate `zz-probe.typ`, different idea title (spec's c) | equal to (a)'s, cached | 10,283 | 10,283 |
| (d) delete it again | the cold spine, cached | 305 | 300 |
| (e) add `nest/zz-probe-e.typ`, a new path | never compiled | 39,586 | 13,526 |

Repeats: (a) before 38,453 / 38,747 / 38,966 (three runs), after 22,535 / 22,458
(two); (c) before 10,655 / 10,422 / 10,283, after 10,118 / 10,283. Run to run
noise is about 1 s on the large rows. (e) was measured once per side. Whole
`build_finished` `ms` is within 1 s of typst-compile + 0.5 s on every row.

Reading the table:

- (b) and (d) are 0.3 s because the spine after a delete equals the spine the
  session started with, so comemo serves the whole compile. This is the "delete
  was cheaper" row from the filing, and it is the same cache effect, not a
  property of deletion. It does not need the prototype.
- (c) at 10.3 s is the cost of a content change with an unchanged spine. It is
  the floor for a page add: the ~12.8 s "one idea removed" figure in the filing
  was this plus noise.
- (e) is the case that mattered. 39.6 s is a cold build; 13.5 s is the floor
  plus ~3 s.
- (a) after is 9 s above (e) after. The first rebuild of a session pays
  something besides the spine that (e) does not; this spike did not isolate it
  (one candidate: first compile of the registry/search state against a changed
  page set). It is still 16 s down on (a) before.

## What the prototype is

About 130 lines, scratch only:

- `crates/core/src/spine_store.rs`: a copy of `LigamentStore` with two keys
  (`spine`, `spine-flat`), no inactive mode, token prefix `__rheo-spine/`,
  registered at `sys.inputs.rheo-spine`. `get(key)` reads a token file through
  `engine.world.file` (the tracked dependency), then answers from the store on
  the CURRENT `engine.library`, never from `self`: a closure memoised in an
  earlier world can carry an older store whose hash is identical (constant), so
  answering from `self` returns a stale spine. The existing
  `ligament_store_get_sees_a_changed_key_*` tests are the template for the same
  guard here.
- `world.rs`: `RheoWorld::assemble` lifts `spine`/`spine-flat` out of
  `WorldSpec::global_context` into the store; `RheoWorld::file` serves its tokens.
  `build_inputs` takes the store as a fourth argument (28 changed lines).
- The generated per-vertebra binding became `#let rheo-context() = (handle: ..,
  metadata-of: rheo-metadata, ..sys.inputs.rheo-context, spine:
  sys.inputs.rheo-spine.get("spine"), spine-flat:
  sys.inputs.rheo-spine.get("spine-flat"))`. Anything that takes `ctx:
  rheo-context()` (sitemap, waterline's `vertebra(ctx:)`) keeps working.
- rheo's own readers: `typ/rheo.typ` `_rheo-handles` and `rheo-index-at`,
  `typ/metadata.typ` `rheo-metadata-all`, now `sys.inputs.rheo-spine.get(..)`
  (they are imported modules and cannot see a vertebra's `rheo-context()`).

Correctness. The closures that call `target()` or `rheo-context()` capture `sys`
(at the time the injected polyfill was `#let target() = if "rheo-context" in sys.inputs ...`,
since baked to a literal; `rheo-context()` spreads `sys.inputs.rheo-context`), so each of them hashes
all of `sys.inputs`, spine included. With the spine off `sys.inputs` those
closures stop depending on it too; that is part of the measured gain, not a
separate effect. `rheo-context()` reads both spine keys eagerly, so a closure
calling it still re-runs on a spine change. That is what a closure that wants
the spine should do; a closure that only wants `.handle` pays for it. The fix is
to expose the spine only through the store and not spread it into
`rheo-context()`; not measured here.

Checks, all against the 2,397-page state after step (e):

- Full `rheo compile` of the prototype vs the unmodified binary on the
  unchanged copy: `diff -rq` empty (2,395 pages). So moving the spine changes no
  output.
- Watched build dir vs a full `rheo compile --build-dir <fresh>` of the same
  copy, after steps (a)-(e): empty. Every step here is a full rebuild (a spine
  change forces one), so this proves little by itself.
- The narrowed path, which is where a stale spine would show: append a sentence
  to `nest/weeknotes/26w41.typ`, `rebuild` it (narrowed, 1 vertebra, 1 round,
  4.5 s typst-compile on both binaries), diff against a fresh full compile. Both
  binaries differ from the full build in the same 10 files
  (`ideas/26w41*.html`, `ideas/index.html`, `nest/weeknotes/26w41.html`); after
  stripping `<style>` blocks (MathML gap) all but `ideas/26w41-rheo.html` are
  equal, and that one differs identically with the unmodified binary. Nothing
  new in the prototype. The 8 known pre-existing differences did not appear in
  this single-edit probe.

## What still dominates after

`--iterations` on step (a), instrumented (about 1.7x slower, so only the shape
is meaningful):

| | iter(1) | iter(2) | iter(3) | iter(4) | iter(5) | typst-compile |
|---|---|---|---|---|---|---|
| before | 4.9 s | 33.2 s | 15.6 s | 4.9 s | 0.6 s | 68.0 s |
| after | 0.9 s | 11.2 s | 11.9 s | 4.9 s | 0.6 s | 33.6 s |

Iterations 1 and 2 are where the spine entered every memo key, so they shrink
3-5x. Iterations 3 and 4 do not: they are the convergence passes over content
that actually changed (the new idea's registry entry, the ideas index, search
corpus), and no longer scale with the spine. What is left is the 10 s content
floor of row (c) plus a few seconds, which is the territory of the existing
"search corpus per note" lead in `ligament-feed-cache.md`. Below it is not
reachable by this change.

## Recommendation

1. Do it. Land `SpineStore` as a sibling of `LigamentStore` (they should share
   one generic store: the token scheme, `of_library`, `serve` are identical, only
   the keys and the inactive mode differ), under a new reserved `sys.inputs`
   key (`rheo-spine`), and reject it in `[inputs]`/`--input` the way
   `rheo-ligaments` is.
2. Do not spread the spine into `rheo-context()`; give it `rheo-spine()` (a
   function returning the store's value), or keep a lazy `spine:` accessor, so
   a closure that reads `.handle` or `.target` does not depend on the spine.
   Neither was measured.
3. Keep the "spine change forces a full rebuild" rule. After this the full
   rebuild a page add costs is ~13.5 s, within 3 s of the 10.3 s floor.
4. Treat it as a breaking contract change, since `spine` and `spine-flat` are
   `Stable` in `docs/contract.md`. Minor-version bump of rheo, an entry in
   `changelog.md`, and update the `rheo-context()` table. For one release, an
   opt-in `[compat] spine_in_context = true` would keep unmigrated packages
   working at the old cost (not prototyped).

Migration per reader (the prototype patched every one of these that waterline
loads, and the first group needs no change):

| reader | change |
|---|---|
| anything using `ctx: rheo-context()` (`@rheo/sitemap` views, waterline `vertebra(ctx:)`, `_ideas-binds`) | none while `rheo-context()` still carries `spine`/`spine-flat` (as prototyped); a `rheo-spine()` accessor per 2 would need `ctx.spine` readers moved |
| rheo `typ/rheo.typ` (`_rheo-handles`, `rheo-index-at`), `typ/metadata.typ` (`rheo-metadata-all`) | read the store (done) |
| rheo `synth/typst_source.rs` `ContextBinding` | add `spine:`/`spine-flat:` through the store (done) |
| rookery `core/0.1.1/src/base.typ` (`_handle-title`, `_is-vertebra`), `outline.typ:348`, `search/0.1.1/src/corpus.typ:81` | a `_rheo-spine-flat()` helper over `sys.inputs.at("rheo-spine", default: none)`; `_rheo-ctx()` stays for `target`/`ext` (done); 0.1.0 untouched |
| waterline `_lib/template.typ` (`_SPINE-BY-HANDLE`, `_SPINE-PATHS`, `_ROOT-PAGES`) | same helper (done) |
| `@rheo/sitemap` 0.1.0 `src/core.typ` (`context-of`), `@rheo/feeds` 0.1.0 `src/lib.typ` (`_rheo-ctx`, lines 637, 746) | `context-of`/`_rheo-ctx` merge the store's keys over `sys.inputs.rheo-context` (done in the scratch copy) |
| `@rheo/sidebar` 0.1.1 `src/lib.typ:28,101` (reads `spine`), `@rheo/feeds` 0.1.1, `@rheo/blogfeed` 0.1.1 (`rheo-context().spine-flat`) | same merge; not loaded by waterline, so patched in neither copy and unmeasured |

Each of those packages needs a patch release. A shim that falls back to
`sys.inputs.rheo-context` when `rheo-spine` is absent keeps them working on old
rheo, and `_rheo-too-old`-style guards need the same treatment.

## Not done

- Not landed: no change outside this document. The prototype lived in
  `/tmp/b6-rheo`, the package and project copies in `/tmp/b6-*`.
- One measurement of (e) per side; the others have 2-3.
- Output of a project with `[html.spine] prelude` readers outside waterline's
  was not checked.
