# Spike: Profiling a cold rookery build (wl-060f393c)

**Verdict: the 12s of `typst-compile` is entirely Typst's own 5-iteration
introspection-convergence loop — none of it is rheo's own code. Iterations 3
and 4 alone consume ~76% of the wall time (9.03s of 11.9s), and each of the
machine's 12 worker threads sits idle roughly 60% of the time inside that
window, which is why 37s of aggregate CPU turns into only ~2x wall-clock
speedup rather than the 12x the core count would allow.**

## Measurement conditions

- rheo checkout: `/home/lox/code/_fcl/rheo`, workspace version `0.6.4`
  (`Cargo.toml`), built `--release` with `CARGO_TARGET_DIR=/home/lox/.cargo-target`.
- Typst pins (`Cargo.lock`): `typst 0.15.0`, `typst-bundle 0.15.0`.
- Project measured: `/home/lox/code/waterline/rookery` — 146 `.typ` source
  files, compiling to **807 HTML pages, 26.8 MB (+2 assets)**. (The problem
  statement that motivated this bird quoted 796 pages/16.8s from an earlier
  run; the rookery has grown since, and this machine was presumably under
  different load — the numbers below are self-consistent across three runs on
  this machine today, which is what the parallelism question needs.) The
  rookery was being edited by its author while this was measured and reported
  809 pages an hour later, so treat the page count as approximate and the
  timings as a snapshot of that afternoon's content.
- Machine: `nproc` = **12**.
- Command (repeated three times into fresh build directories, `rm -rf`'d
  first):

  ```
  /home/lox/.cargo-target/release/rheo compile /home/lox/code/waterline/rookery \
    --html \
    --build-dir /tmp/rookery-perf/coldN \
    --input today=2026-09-22 \
    --timings /tmp/rookery-perf/cold-traceN.json
  ```

- The three INFO summary lines:

  | run | wall | typst-compile | packages | plugin-write |
  |---|---|---|---|---|
  | 1 | 12.6s | 12.2s | 142ms | 86ms |
  | 2 | 12.2s | 11.9s | 131ms | 71ms |
  | 3 | 12.6s | 12.2s | 134ms | 69ms |

  Spread is small (11.9–12.2s typst-compile) and everything outside the
  compile is under 0.25s combined — confirms the earlier framing that
  packages/fonts/plugin-write are noise next to the compile itself.

- **What the instrument itself costs.** Two runs of the same project with
  `--timings` omitted report typst-compile at 10.8s and 11.0s, against
  11.9–12.2s with it on: instrumentation adds roughly 10%. The trace is
  therefore a fair picture of the untimed build rather than a picture of
  itself, but no figure below should be quoted as the cost of a normal build
  to better than that margin.

- **A trace of this project is 2.4 GB** (17.7M begin/end events, `B`/`E`
  pairs rather than single `X` spans). Loading one with `json.load` needs a
  machine with the RAM for it and takes about 35s. Worth knowing before
  turning `--timings` on against anything this size, and worth remembering
  when the sibling watch bird writes one of these *per rebuild*.

## Findings

All analysis below is from run 1's trace (`cold-trace1.json`, a Chrome-tracing
JSON array of 17,712,042 begin/end events; timestamps and durations are
microseconds). Runs 2 and 3 were not re-analysed in full — their summary lines
match run 1 closely enough that the shape of the compile is not in doubt.

**Every event name in the trace originates in the upstream `typst`,
`typst-realize`, and `typst-html` crates via `typst_timing::TimingScope`
(confirmed by grepping their source for the exact event names) — not in any
`rheo::*` crate.** rheo's own instrumentation (`self.timed(phase::…)` in
`crates/core/src/build.rs`) is a *different* mechanism that produces the INFO
summary line, not this trace. So nothing found below is a rheo code path.

### Top 15 event names by inclusive duration (summed µs, one run)

| event | inclusive dur | count |
|---|---|---|
| func call | 181,140,161 | 8,099,893 |
| html block fragment | 126,819,916 | 67,709 |
| realize | 54,720,266 | 117,952 |
| html document | 51,116,107 | 2,679 |
| context | 39,591,807 | 41,477 |
| for loop | 23,841,269 | 123,827 |
| **compile** (whole run, tid 1) | 11,908,376 | 1 |
| bundle | 11,030,816 | 5 |
| bibliography | 9,665,626 | 2,699 |
| iter (3) | 5,872,676 | 1 |
| iter (4) | 3,161,822 | 1 |
| html inline fragment | 2,997,703 | 66,735 |
| eval | 1,854,082 | 1,479 |
| iter (1) | 1,345,299 | 1 |
| load bibliography | 681,936 | 23 |

(Inclusive durations sum across nested calls and across the 13 threads, so
these are not additive against the 11.9s wall clock.)

### Top by self time (dur minus children)

| event | self dur | count |
|---|---|---|
| func call | 39,748,007 | 8,099,893 |
| bibliography | 9,665,620 | 2,699 |
| bundle | 4,910,332 | 5 |
| realize | 2,251,823 | 117,952 |
| for loop | 1,559,185 | 123,827 |
| html block fragment | 1,519,329 | 67,709 |

`func call` dominates self time too — it is Typst's generic function-call
overhead, spread across every evaluated expression in the document.

### The convergence loop, in full

Typst's compile loop (`typst-0.15.0/src/lib.rs:133-161`) re-realizes the whole
document up to `MAX_ITERS = 5` times, stopping as soon as a comemo constraint
validates (i.e. nothing observable changed since the last pass) —
`typst-library-0.15.0/src/introspection/convergence.rs:16` names the phases
`"iter (1)"`..`"iter (5)"`. This build ran **all five** and converged only on
the fifth (no non-convergence warning was emitted, so it did stabilize —
right at the edge of the limit):

| iteration | duration |
|---|---|
| iter (1) | 1.345s |
| iter (2) | 0.531s |
| iter (3) | 5.873s |
| iter (4) | 3.162s |
| iter (5) | 0.161s |
| **sum** | **11.07s of 11.91s total (93%)** |

Iterations 3 and 4 alone are 9.03s — **76% of the whole compile.**

Reproduced on a second, independent trace taken later the same day against a
`/tmp` copy of the same project, reading the `iter (N)` `B`/`E` timestamps
straight out of the JSON rather than through the analysis script:

| iteration | duration (first trace) | duration (second trace) |
|---|---|---|
| iter (1) | 1.345s | 1.328s |
| iter (2) | 0.531s | 0.586s |
| iter (3) | 5.873s | 5.919s |
| iter (4) | 3.162s | 3.345s |
| iter (5) | 0.161s | 0.152s |

The shape is stable, and it is a strange shape. Iterations 1 and 2 are cheap,
3 and 4 are each a full expensive realization, and 5 is nearly free. A
monotonically cheapening sequence is what comemo caching alone would produce.
Two expensive passes in the middle instead say that iterations 1 and 2 render
something much smaller than the real document — plausibly because the
introspector is still empty or partial, so `query`-dependent and
`locate`-dependent content has not materialized yet — and that whatever
iteration 3 then produced was still not final. **Converging one iteration
sooner would be worth about 3.3s of 12.2s.** Why iteration 4 was needed is
not established here, and is the first unknown below.

### The bibliography is not the driver (a negative result)

The site-wide bibliography was the obvious suspect: `bibliography` carries the
largest named self time in the trace after the generic `func call`
(9.67s across 2,699 spans, ~540 per iteration), Typst's native bibliography
element is introspection-driven, and the rookery attaches one through
`rookery.with(bibliography: arguments(bytes(read("../references.bib"))))` in
`_lib/template.typ`.

Tested directly, on two `/tmp` copies of the rookery — one untouched, one with
that single argument deleted — rather than reasoned about:

| copy | pages | typst-compile |
|---|---|---|
| unmodified | 809 | 11.4s |
| site-wide bibliography removed | 809 | 10.1s |

**1.3s, about 11%.** Real, but nowhere near the 9.03s the convergence loop
costs, and the page count is identical, so the bibliography is a cost rather
than the thing forcing five passes. This rules out the largest named cost in
the trace as the explanation for the headline finding. Neither copy is in the
repository and neither build touched waterline's own build directory.

### Per-tid busy time (the parallelism answer)

13 distinct `tid`s, all under one `pid`. `tid=1` is the main thread and is the
*only* thread that ever holds a top-level (depth-0) span — its one top-level
span is the entire `compile` call, 11.908s, i.e. tid 1 is busy 100% of wall
time doing iteration bookkeeping (`context`, `func call`, the `bundle`/`iter`
wrapper spans) that is not itself dispatched to other threads.

The other 12 `tid`s (2–13, matching `nproc`) each hold a run of top-level
`"html document"` spans — i.e. **per-page HTML rendering is already
parallelized**, one page per worker-thread span:

| tid | busy (top-level) | pages rendered |
|---|---|---|
| 9 | 4.762s | 128 |
| 6 | 4.547s | 195 |
| 13 | 4.286s | 158 |
| 7 | 4.225s | 171 |
| 2 | 4.222s | 204 |
| 10 | 4.221s | 406 |
| 5 | 4.204s | 254 |
| 11 | 4.157s | 217 |
| 4 | 4.140s | 266 |
| 12 | 4.137s | 227 |
| 8 | 4.132s | 227 |
| 3 | 4.083s | 226 |

Every worker's first span starts at ~0.84s and its last ends at ~11.80–11.83s
— i.e. each worker is *available* for essentially the whole compile window
(~11s) but only *busy* ~4.1–4.76s of it: **roughly 40% utilization**, not the
near-100% a fully parallel workload would show. The gaps are where the workers
have no page ready to render because the current convergence iteration hasn't
produced one yet, or because tid 1's bookkeeping between iterations blocks
handing out new work.

**Verdict: mostly serial, with a real but partial parallel component.** The
per-page rendering step is genuinely spread across all 12 cores, but the
convergence loop that wraps it is inherently sequential (iteration N+1 cannot
start until iteration N's introspector is final), and each worker is starved
roughly 60% of the time it exists. That combination — a serial outer loop
around a parallel inner step, with workers idle between rounds — is
consistent with the observed ~2.2x CPU/wall ratio on a 12-core machine.

### Longest single events

| dur | start | tid | name |
|---|---|---|---|
| 11.908s | 0 | 1 | compile |
| 5.873s | 2.712s | 1 | iter (3) |
| 5.870s | 2.712s | 1 | bundle |
| 4.443s | 2.712s | 1 | realize |
| 3.298s | 2.712s | 1 | context |
| 3.298s | 2.712s | 1 | func call |
| 3.162s | 8.584s | 1 | iter (4) |
| 3.146s | 8.584s | 1 | bundle |
| 2.364s | 3.619s | 1 | func call |
| 1.521s | 8.585s | 1 | realize |

All ten are on `tid 1` — the longest single spans in the whole compile are the
main thread's own iteration/bundle/realize/context/func-call chain, not
anything happening on the worker threads.

## What this does NOT tell us

- This is a **cold** build. It says nothing directly about how much of the
  11.9s recurs on a `rheo watch` incremental rebuild after a single-page edit
  — comemo may or may not re-run all 5 convergence iterations, or re-render
  all 807 pages, when only one page's source changed. That question is
  answered by the sibling bird
  `wl-measures-watch-rebuild-latency-on-db988471`.
- **Which specific introspection (a counter, a `locate()`, a cross-reference,
  the bibliography, page numbering, …) is the one still changing after 4
  iterations and forcing the 5th** is unknown from this trace alone. The
  trace records *that* iterations 3–4 are expensive and *that* five ran, not
  *why* convergence took that long — establishing that would mean bisecting
  the rookery's own content, which is out of scope here (this bird
  deliberately does not modify waterline).
- Whether reducing iteration count would also shrink `func call`/`realize`
  proportionally, or whether later iterations are cheaper per-page thanks to
  comemo caching (iteration 5 is far cheaper than 3 or 4, which is at least
  consistent with caching helping once most of the tree has stabilized) is
  unknown — not established well enough here to size a fix.
- The Typst trace schema (event names, `ph`/`ts`/`dur`) is Typst 0.15.0's; it
  is upstream and may change in a future Typst release this document does not
  describe.

## Candidate optimizations — leads, not decisions

1. **Cut the number of convergence iterations.** This is the single largest
   cost in the whole build (iter 3+4 = 9.03s, 76% of wall) and, per the
   standing priority, plausibly matters even more for incremental rebuilds —
   *every* `rheo watch` recompile pays for however many iterations that
   build's introspections need, not just the cold one. The cut itself is
   **not filed as a bird**: the trace does not locate *which* content
   construct forces the 5th iteration, and finding that means bisecting the
   rookery's own content, which is out of scope here.

   What *is* filed is the instrument that would make such a bisection cheap:
   `wl-summarises-typst-s-convergence-5acf3cf6` adds a flag that reports the
   iteration count and per-iteration durations as one INFO line, off the
   timing data rheo already collects, instead of a 2.4 GB trace and a Python
   script. Until reading this number is cheap, nobody will read it twice, and
   a change that moved five iterations to four would go unnoticed.
2. **Worker-thread idle time (~60%) between iterations.** Real, but the
   dispatch loop that starves the workers lives in `typst-realize`/`typst`
   upstream, not in any `rheo::*` crate — there is nothing in this
   repository's code to change. Not filed.
3. **rheo's own accounted phases (packages, plugin-write, fonts) are already
   negligible** — under 250ms combined against an 11.9s compile, confirmed
   again here. Not a lead.

One follow-up bird is filed from this spike,
`wl-summarises-typst-s-convergence-5acf3cf6`, for the instrument named under
lead 1. No bird is filed for a fix, because both sizeable leads either lack a
locatable fix in code this bird's scope can touch (upstream Typst) or lack a
located triggering construct to fix in the first place.

Waterline's tracker already carries three `perf-*`-labelled birds —
`perf-typst-timings` (the `--timings` flag, landed), `perf-watch-timings` (the
per-rebuild variant, landed), and `perf-cold-build-profile` (this spike). The
new bird duplicates none of them.
