# Spike: does a changing ligament feed drive the narrowed-rebuild cost

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

## Open question: what feeds back into the harvest

Which of `@rookery/core`'s attaches (`idea:<id>`, `ideas`, `tag:*`,
`backlinks:*` in `state.typ`'s `_idea-ligaments`) carries a value computed
from `sys.inputs.rheo-ligaments`, and which pages bind it so that
`cycle-25-26`, `cycle-26-27` and `index` recompile after a prose edit? If an
attach is derived from the merged registry rather than the page's own notes,
narrowed rebuilds cascade on every edit whatever the delivery channel, so this
is worth answering before or alongside the delivery redesign.

## Conclusion

The competing explanation from `docs/spikes/narrowed-rebuild-profile.md` —
that the cost is unavoidable per-compile, registry-sized rookery work
(`_reg-final()`'s merge, `@rookery/search`'s corpus pass) — does not hold on
its own here: the exact same registry, the exact same rookery packages, and
the exact same narrowed subset cost ~17.5–21s when the fed ligament value
differs between compiles and ~2.6–2.9s when it does not. The two spikes are
not in conflict — that prior spike never varied feed stability, so it
correctly priced the registry-sized work as the cost *given* that the feed
always differs, which is what happens today. This spike adds the other axis:
hold the feed still, and most of that cost disappears, because the fed value
sits inside `sys.inputs`, which comemo hashes as one unit — any difference,
however irrelevant to what a given page reads, invalidates every memoised
evaluation that touches it, rebuilding the registry merge and the search
corpus from scratch regardless of whether anything they'd actually read
changed.

This makes the parked design bird — feeding ligaments without invalidating the
whole library, e.g. by making `_reg-final()`/the search corpus read a
narrower, independently-memoized slice of `sys.inputs` rather than the whole
value, or building the merge once per compile from a stable handle rather than
re-deriving it from `sys.inputs` on every `_reg-final()` call — the change
most likely to pay off: feed churn, not registry size, is the lever a cache
fix can actually pull.
