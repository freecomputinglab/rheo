# Spike: does a changing ligament feed drive the narrowed-rebuild cost

> Read with "What this changes about the verdict above", further down: the
> `frozen` runs below produced a wrong search index, so their cost is not an
> achievable target.

**Verdict: yes, twice over. Feed churn costs a narrowed rebuild in two separate
ways, and a stable feed removes both. First, cache misses: the same single
edited vertebra compiles in 6.6–7.4s when the fed value differs from the last
compile and 2.6–2.7s when it does not, about 2.5x from memoisation alone.
Second, a cascade: with the real feed every prose edit also re-triggers a
second round of three dependents (`cycle-25-26`, `cycle-26-27`, `index`,
~11s); with a frozen feed it does not. The second effect means something the
edited page *attaches* is computed from the fed ligaments, so each feed change
feeds back into the harvest. Together they take a rebuild from ~17.5–21s to
~2.6–2.9s (~1.2–1.6s for a comment-only edit). The parked design "Feed
ligaments without invalidating the library" is worth doing, and the feedback
loop needs its own look (see "Open question").**

## Measurement conditions

- rheo binary: a `--release` build of `/home/lox/code/_fcl/rheo-9f7` (this
  checkout, branch `feat/ligaments`, with `Ligaments::to_value` already
  sorting pages by handle), copied once to `/tmp/feedspike-rheo-9f7` rather
  than invoked from the shared `$CARGO_TARGET_DIR`, which is contended on this
  machine.
- Project: a `/tmp` copy of `/home/lox/code/waterline/rookery`
  (`/tmp/feedspike-waterline-rookery`, 31 MB — its `build/` dir and large
  `.pdf`/`.epub`/`.mp4` files excluded from the copy and symlinked back in
  place instead), repointed at a `/tmp` copy of `/home/lox/code/_fcl/rookery`
  (`/tmp/feedspike-rookery-pkg`, 211 MB) via a `path =` package entry, so
  neither original repository was touched. `[watch] narrow = true` was already
  set in the site's `rheo.toml`. A cold compile produced 2,204 HTML pages.
- Machine: `nproc` = 12. `uptime` load sat at 0.5–2.6 throughout, accounted
  for by the operator's own `rheo watch` on the real project (left untouched)
  plus this spike's own `rheo watch`; `pgrep -af 'rheo|typst'` confirmed no
  other heavy compile ran concurrently at any point.
- Each series is three edits to the same vertebra,
  `nest/papers/ends-of-theory/index.typ`: appending one sentence inside the
  idea body, waited out to the `narrowed:` line each time. The frozen series
  adds a fourth edit — a `//` comment line that changes no content — to test
  whether that approaches a cached no-op. Timings are each rebuild's summed
  `phase=typst-compile ms=` (one line per narrowing round) from
  `RUST_LOG=rheo=debug`, plus wall time from the `detected file changes` line
  to the `narrowed:` line.
- The three `RHEO_SPIKE_FEED` modes, implemented as a temporary knob in
  `compile_bundle_subset` (`crates/core/src/build.rs`, removed before this
  spike landed): `frozen` stores the first ligament value it is about to feed
  and re-feeds that same value on every later call; `perturb` feeds the real
  value plus one extra attach (`"rheo-spike"`, a fresh counter int) on a
  fabricated handle outside the compiled subset, so the fed value differs
  every compile without any page reading it; unset is today's behaviour.

## Results

| mode | edit | `narrowed:` line | typst-compile ms (per round) | wall (changed → narrowed) |
|---|---|---|---|---|
| unset (baseline) | run 1 | `4 vertebra(e) in 2 round(s)` | 6587 + 10963 = 17550 | 17.98s |
| unset (baseline) | run 2 | same | 6653 + 10798 = 17451 | 17.89s |
| unset (baseline) | run 3 | same | 7405 + 10789 = 18194 | 18.65s |
| frozen | run 1 | `4 vertebra(e) in 2 round(s)` | 6532 + 7343 = 13875 | 14.29s |
| frozen | run 2 | `1 vertebra(e) in 1 round(s)` | 2705 | 3.03s |
| frozen | run 3 | same | 2587 | 2.92s |
| frozen | comment-only | same | 1237 | 1.57s |
| perturb | run 1 | `4 vertebra(e) in 2 round(s)` | 6783 + 11504 = 18287 | 18.71s |
| perturb | run 2 | same | 7634 + 12956 = 20590 | 21.05s |
| perturb | run 3 | same | 7939 + 12691 = 20630 | 21.10s |

The reference cold build (no narrowing, every page emitted) was 2,204 pages in
34.8s wall, 31.6–32.7s typst-compile.

## Reading the shape

`unset` and `perturb` never settle below ~17.5s: every edit re-triggers a
2-round rebuild (the edited page, then its three dependents —
`nest:jobs:cycle-25-26`, `nest:jobs:cycle-26-27`, `index` — once the diff
against the previous harvest is non-empty), and `perturb`'s synthetic,
page-irrelevant attach is sufficient on its own to keep that diff non-empty
and the cost at or slightly above baseline (18.3–21.1s vs 17.5–18.65s) even
though nothing real changed. That the feed difference alone — not the prose
edit — sustains the cost is the hypothesis's central prediction, and it held.

`frozen` tells the other half of the story. Its first edit still pays the full
2-round cost (14s): the snapshot was taken during the cold build, so that
round still compiles against a value it has not seen. From the second edit on,
two things change at once. Round 1 gets cheaper (2.6–2.7s against 6.6–7.4s for
the same single vertebra under `unset`), which is the cache-miss effect. And
round 2 disappears (`1 vertebra(e) in 1 round(s)`). That second change cannot
come from the feed directly: rheo picks dependents by diffing the edited
page's *harvested* attaches against its previous harvest (`diff_attach_keys`
in `narrow_one_plugin`), not the feed. So under `unset`, a prose edit changes
some attached value other than the body, and under `frozen` it does not,
which can only happen if that value is derived from the fed ligaments. The
comment-only edit, which changes no content at all, comes in cheaper again
(1.2–1.6s), approaching a fully cached no-op.

## Answer: what changes between rebuilds

Instrumented `narrow_one_plugin` and `rebuild` (`crates/core/src/build.rs`) to
log every `(handle, key)` whose cached attach value moved, both within a
rebuild's own fixpoint rounds and across one rebuild to the next, and ran the
same three-edit series against `/tmp/feedspike-waterline-rookery` with a
fresh `--release` binary. The instrumentation was temporary and is gone from
the tree; this section reports what it showed.

**Exactly two keys change on a prose-only edit, and they are always the
same two**, for whichever paper is edited:

- `idea-body:idea:<id>` — the note's `raw` body, attached separately from
  everything else (`_idea-ligaments`, `@rookery/core`'s `state.typ`, the
  `_attach(_LIG-IDEA-BODY + id, rec.at("raw", ...), ...)` call). This is
  deliberate: it is the one attach a prose edit is SUPPOSED to move. `idea:<id>`
  and `ideas` — the note's other fields — did not change in any run: `_idea-ligaments`
  removes `raw` and `body` from the record before attaching it under those keys,
  and nothing else about the note moved.
- `search-row:idea:<id>` — NOT a core attach. It belongs to `@rookery/search`'s
  own bundle-root marrow (`search/0.1.1/.marrow.typ`), which attaches
  `(body: row.body, tokens: toks.at(i))` under this key for every note, to
  skip re-tokenizing a note whose body comes out byte-identical to what was
  cached (`_cached-tokens`, read back via `_lig-get`). `row.body` is a
  plain-text rendering of the note's flattened body — it contains the same
  "Word count: N." line the paper's own template prints, and N changed with
  every edit (one appended sentence moved it from 6190 to 6194, 6194 to 6198,
  and so on across the three runs on the same paper). So this key, too, moves
  on every prose edit, for a reason entirely outside `@rookery/core`: the
  search index cache keys itself on the note's rendered body text, not a
  separate "did the prose change" signal.

**Both pages that attach them change every edit, so both costs are paid every
edit.** Measured identically across three edits to `ends-of-theory` and once
to a different paper (`cybernetic-conjecture`): the edited handle is the only
one whose `idea-body:`/`search-row:` pair moves in round 1, and the SAME three
pages always recompile in round 2 — `index`, `nest:jobs:cycle-25-26`,
`nest:jobs:cycle-26-27` — regardless of which paper was touched. That
repetition across two different edited papers is itself the finding: these
three pages bind something true of every paper (most plausibly a tag-selector
window or transclusion over the whole `papers` tag, binding `idea-body:<id>`
for every paper id up front), not something specific to `ends-of-theory`.
Neither `search-row:*` key is ever found in a `binds` list — nothing reads it
back through a Typst-visible API, so it causes no cascade on its own — but
`@rookery/search`'s own corpus-wide weighting pass (`_compress-ranked` in
`search/0.1.1/src/corpus.typ`, called from the `.marrow.typ` shown above) runs
over every note's tokens on every round that recompiles ANY note, because that
file is bundle-root marrow and rheo splices marrow in full on every narrowed
round (`compile_bundle_subset`'s own doc comment: "marrow still spliced in
full"). One row's token list differing is enough to change `_compress-ranked`'s
memo key, so this pass reruns from scratch — THIS is why round 1 never
compiles warm today: it is not that round 1 is unavoidably registry-sized
work, it is that the search corpus pass's memo key depends on literally every
note's cached tokens, and the edited note's `search-row:` always misses.

This also explains the earlier `frozen`/`perturb` results directly. Freezing
the fed value stops `_lig-get(_LIG-SEARCH-ROW + row.id)` from ever returning a
new value for the edited note either (since the feed, not the live
recompile, is what the OTHER rows see, and the one row that recompiles live
answers for itself) — `toks` ends up identical to the previous call, comemo's
memo hits, and `_compress-ranked` is skipped. `perturb`'s synthetic attach on
an uncompiled handle does not touch `search-row:*` or `idea-body:*` at all,
yet still sustains the full cost, because `sys.inputs` is hashed as one
value: ANY difference anywhere in the fed `Ligaments`, however irrelevant to
what a page reads, invalidates every memoised Typst call that touches
`sys.inputs`, not only the ones that would actually produce a different
result.

## What this changes about the verdict above

The verdict at the top of this document read `frozen`'s 2.6–2.7s round 1 as the
price of a cache hit. It is not one. Under `frozen`, `@rookery/search` reads the
edited note's `search-row` back from the stale feed and reuses last build's
tokens and body, so that round produced a WRONG search index, and its cost is a
lower bound for a build nobody wants rather than a target. What the trace shows
instead:

- **For repeated edits to one page, today's feed is already stable.** Only the
  edited page's own attaches move, and rheo already drops those from the feed it
  hands that page's round. Round 1's 6.6–7.4s is real recomputation:
  `@rookery/search`'s corpus-wide `_compress-ranked` pass reruns because one
  note's tokens really changed. A stable-feed redesign would help only when
  successive edits land on different pages.
- **Round 2 is partly an over-bind in the site, not in rheo or rookery.** `index`
  binds every note's body legitimately: its search `filter-panel` renders and
  searches each row's body text. But `cycle-25-26` and `cycle-26-27` reach every
  body through waterline's own `#cfps` (`rookery/_lib/template.typ`), whose outer
  `ideas-view(all => ...)` is unfiltered and leaves `body:` at its default
  `true`. The cfp tables never read a body. `body: false` on both of its
  `ideas-view` calls is the switch `@rookery/core`'s `data.typ` provides for
  exactly this ("a listing page (titles, tags, dates — never prose)").

## Recommendation

1. Pass `body: false` to both `ideas-view` calls in waterline's `#cfps`, so a
   prose edit stops recompiling the two cycle pages.
2. Price `@rookery/search`'s corpus pass on a one-note change. If each row's
   compressed terms can be cached per note (they already ride on `search-row`)
   and only the corpus-wide weighting recomputed, round 1 shrinks to the edited
   page plus that weighting. That needs its own spike.
3. Leave the stable-feed redesign parked until edits that alternate between
   pages are measured to matter.
