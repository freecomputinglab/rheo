# Spike: why a narrowed rebuild costs as much as a full warm rebuild

**Verdict: a narrowed rebuild of waterline's rookery (1,767 pages) is 20.8–26.5s
once `@rookery/search`'s and `@rookery/core`'s read-side ligament work is in
play — against 0.66s when no package emits ligaments yet. Emptying
`@rookery/search`'s `.marrow.typ` body cuts it to ~13.2s. Making `_reg-final()`
return the live registry alone — skipping the ligament merge entirely — cuts it
to 1.8–4.1s, on its own, with search still running — but that ablation also
shrinks everything downstream that walks the registry, search included, so it
bounds the registry-sized work as a whole rather than pricing the merge alone.
The MathML `<style>` block is
present on a narrowed page that contains math, confirming it is omitted only
from pages that compile no math, which is harmless.**

## Measurement conditions

- rheo binary: a `--release` build of `/home/lox/code/_fcl/rheo` at its current
  head, copied once rather than rebuilt (the shared `$CARGO_TARGET_DIR` is
  contended on this machine).
- Project: a `/tmp` copy of `/home/lox/code/waterline/rookery`, repointed at a
  `/tmp` copy of `/home/lox/code/_fcl/rookery` via a `path =` package entry so
  neither original repository was touched, with `[watch] narrow = true` added.
  1,767 pages, 41.0 MB, 616 `.typ` files.
- Machine: `nproc` = 12. Another process (the operator's own `rheo watch` on
  the real project, left untouched throughout) held ~12% CPU the whole time.
  `uptime` load sat at 0.9–1.7 during the clean baseline runs; it rose to
  3.3–5.3 during the ablation runs, entirely accounted for by this spike's own
  `rheo watch` process at 200%+ CPU (checked with `ps`) — no second full
  compile was ever running concurrently.
- Each variant below is edited the same way: append one sentence inside the
  body of the idea in `writing/papers/ends-of-theory/index.typ`, wait for the
  `narrowed:` line, repeat twice more. The MathML check instead edits
  `writing/dissertation/chapters/4.rookery.typ`, which contains inline math.
- One session was restarted with `--timings` to trace a single narrowed
  rebuild. That trace inflates wall time by roughly 5x (65s against ~21–27s
  uninstrumented) from the instrumentation overhead of recording ~15M events —
  its durations are not comparable to the table below, only the relative
  weight of its spans is used.

## The variants

| variant | edit | `narrowed:` line | iterations | wall (files-changed → narrowed) |
|---|---|---|---|---|
| cold build (reference) | — | — | 5 iter: 2.6/13.2 or 12.8 or 15.7/9.1–9.4/4.1–4.2/0.3s | 37.3–43.1s compile, ~41s wall |
| baseline, run 1 | prose in `ends-of-theory` | `narrowed: 1 vertebra(e) in 1 round(s)` | 4: 8.7/4.0/1.6/0.012s | **26.5s** |
| baseline, run 2 | same, repeated | same | 4: 7.6/2.0/1.6/0.012s | **21.3s** |
| baseline, run 3 | same, repeated | same | 4: 7.1/1.9/1.6/0.02s | **20.8s** |
| no-search, run 1 | same, `search/0.1.1/.marrow.typ` body emptied | same | 4: 3.9/1.7/1.6/0.012s | **13.2s** |
| no-search, run 2 | same, repeated | same | 4: 3.7/2.0/1.5/0.013s | **13.2s** |
| no-merge, run 1 | same, `_reg-final()` returns `_registry.final()` only | same | 4: 1.2/0.38/0.153/0.013s | **4.1s** |
| no-merge, run 2 | same, repeated | same | 4: 0.129/0.243/0.102/0.008s | **1.8s** |

The known reference points from prior measurement (not re-derived): 0.66s when
no package emits ligaments at all, and ~13s for an edit to `index.typ` outside
any idea.

Every `narrowed:` line reports the same shape regardless of variant — `1
vertebra(e) in 1 round(s) — writing:papers:ends-of-theory` — so none of these
ablations changed *what* rheo decided to rebuild, only how expensive that
decision was to carry out. The no-merge pages are of course wrong (notes other
than the one live vertebra vanish from listings) — this ablation measures cost
only, as the harness says.

## What the trace shows

Ten largest spans by self time, from one `--timings` capture of a baseline-
shaped rebuild (search and merge both live):

| span | self time | count | total (incl. children) |
|---|---|---|---|
| `func call` | 8.76s | 7,006,865 | 64.0s |
| `for loop` | 0.90s | 14,730 | 2.91s |
| `while loop` | 0.55s | 465,545 | 1.05s |
| `export html` | 0.39s | 1,767 | 0.40s |
| `bundle` | 0.29s | 4 | 9.55s |
| `realize` | 0.05s | 4,567 | 9.73s |
| `bibliography` | 0.04s | 16 | 0.04s |
| `export bundle` | 0.04s | 1 | 0.04s |
| `cite-group` | 0.04s | 294 | 0.04s |
| `eval` | 0.03s | 267 | 1.99s |

`bundle`'s total (9.55s across its 4 nested calls) matches the four convergence
iterations' summed duration almost exactly, which is the expected sanity check
that the trace is measuring what the log line reports.

The dominant line is 7 million `func call` events — Typst's generic
instrumentation point for every user-defined function invocation, undifferentiated
by which function. That volume is consistent with a per-pass loop over
~1,100 registry records (the merge) and then again over however many of those
`ideas()` returns feed the search corpus — but the trace cannot itself say
which of those two loops, or how much of either, because neither is named
distinctly from any other Typst function call. That is what the ablations in
steps 3–4 are for; the trace establishes only that the cost is many small
calls, not one big blocking operation.

## Why the two costs are not independent

Ablating search alone (merge still live) saves ~14s off the baseline (26.5s →
13.2s). Ablating merge alone (search still live) saves ~23–25s (26.5s →
1.8–4.1s) — almost the whole cost, on its own. If the two were independent
costs sitting side by side, removing search should leave roughly the
merge-only cost, and removing merge should leave roughly the search-only cost;
instead removing merge alone collapses nearly everything, including most of
what looks like search's share.

The reason is `@rookery/search`'s `.marrow.typ` builds its corpus from
`ideas().filter(e => e.page != none)` — which reads the *merged* registry.
With `_reg-final()` short-circuited to the live registry alone, `ideas()`
sees only the one vertebra's own records rather than the full ~1,100-record
merge, so search's own corpus-building loop shrinks to almost nothing as a
side effect. The registry's size is upstream of search's cost, so the no-merge
row cannot be read as the merge's own price.

## The MathML question

Checked by making a narrowed-rebuild edit to
`writing/dissertation/chapters/4.rookery.typ`, which contains inline math
(`$O(n^2 dot 2^n)$`). The rebuilt page's `<head>` carries the
`<style>/* Alignment */ …</style>` block that `typst-html`
(`typst-html-0.15.1/src/mathml.rs`) emits. This confirms the suspicion: the
block is only emitted when the compile contains math, and a page with math
brings its own math into that compile regardless of narrowing, so its absence
from a narrowed page with no math in it is the harmless case, not a defect.

## Conclusion

The no-merge ablation removes more than the merge. With `_reg-final()` returning
the live registry alone, every reader of the registry (search's corpus pass,
`@rookery/core`'s marrow, every listing page and every `hyperlink`) sees one
vertebra's notes instead of ~1,100, so 1.8–4.1s is the floor once ALL
registry-sized work is gone, not the cost the merge itself leaves behind. What
the table does separate:

- search's corpus pass costs ~8–13s of a 21–26s rebuild (baseline against
  no-search), so it is a meaningful share and worth rebuilding from ligaments;
- the remaining ~13s is the merge plus everything downstream that walks the
  merged registry, in some split this spike does not measure.

The merge is the obvious suspect for much of that 13s. `_reg-final()` is called
from about thirty sites in `@rookery/core`, `hyperlink` among them, and each
call runs `_reg-merge(_ligaments())`: either a comemo hit that has to hash the
whole ligament input to find, or a Typst-level loop rebuilding ~1,100 records
through `_ligament-rec`. Either way it repeats per call and per pass, and 7M
`func call` events are consistent with that.

## Recommended follow-ups

- Build the ligament-sourced records once per compile rather than per
  `_reg-final()` call: a module-level value computed from `sys.inputs` when the
  module is first evaluated, merged with the live registry by a native dict
  spread. Measure it against the baseline above; it is the change that would
  confirm or refute the merge as the cost.
- Rebuild search rows from per-note ligaments instead of recomputing the whole
  corpus from `ideas()` every build, which the no-search row prices at ~8–13s.
- No further investigation needed on the MathML style block — confirmed
  harmless.
