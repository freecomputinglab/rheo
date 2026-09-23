# Observability, and how to build without it

rheo carries three different ways of answering "why was that slow", at three
different costs. Only the third is optional at compile time.

## Always on: the per-phase summary

Every build prints one line at INFO:

```
812 page(s), 26.9 MB in 635ms (+2 asset(s)) — typst-compile 565ms, plugin-write 64ms, packages 47ms
```

Page count and total bytes come first because they are what identifies a
project emitting per-page data it should emit once. The three slowest phases
follow. `RUST_LOG=rheo=debug` adds a `debug!` line per phase carrying `phase`
and `ms` fields, so a build's phases can be grepped and summed.

This is rheo's own clock around its own seams — a handful of `Instant::now()`
calls per build — and it is not optional. See
`crates/core/src/diagnostics/timing.rs`.

## Opt-in: Typst's own instrumentation

Typst instruments itself through `typst-timing`, which is off until something
calls `typst_timing::enable()`. Two flags do, on both `compile` and `watch`:

- `--timings <PATH>` writes the whole trace as Chrome-tracing JSON — a file
  for `compile`, one numbered file per rebuild for `watch`. Complete, and
  enormous: a cold build of an 800-page project is **2.5 GB and 17.7M
  events**. Reach for it when you need to know which Typst sub-step inside a
  slow compile is the expensive one, and read `docs/spikes/` first for what
  previous readings found.
- `--iterations` reports Typst's introspection-convergence iterations as one
  INFO line, off the same data, without materialising the trace:

  ```
  5 convergence iteration(s) — iter(1) 1.8s, iter(2) 698ms, iter(3) 6.4s, iter(4) 3.6s, iter(5) 178ms
  ```

  It reads the trace through a `Write` that keeps the `iter (N)` events and
  discards every other byte, because `typst-timing` has no in-memory reader —
  only `export_json`, which also clears the buffer. That is why giving both
  flags at once still produces exactly one export: `--iterations` writes the
  `--timings` file itself.

Neither is on by default. What each costs, on an 800-page project, measured
rather than estimated:

| | cold compile | one warm rebuild |
|---|---|---|
| neither flag | 13.6s | 626-658ms |
| `--iterations` | 24.0s | 634-692ms |
| `--timings` | 56.8s, 2.58 GB written | — |

Enabling the instrumentation at all costs the compile itself about 4%. The
rest is the pass over the events afterwards: 17.7M of them cold, which is
~10s to scan and ~43s to write out as JSON, against ~70k on a warm rebuild,
where scanning is ~30ms. **`--iterations` is therefore cheap exactly where it
matters and slow on a cold build** — which is the right way round, since the
iteration count is what a watch session wants to watch.

## Compiling it out: `--no-default-features`

The flags above and the `typst-timing` dependency behind them sit behind a
cargo feature named `timings`, on by default:

```
cargo build --release                        # with --timings and --iterations
cargo build --release --no-default-features  # without either
```

A binary built without the feature has no `--timings` and no `--iterations` in
its `--help` and rejects them as unknown arguments:

```
error: unexpected argument '--timings' found
```

Nothing can call `typst_timing::enable()`, so Typst's instrumentation stays
off for the life of the process, and none of the trace-reading code is
compiled. The per-phase summary above is unaffected.

**What it does not remove.** `typst-timing` itself is still linked, because
`typst` depends on it — the `TimingScope` calls are in Typst's own crates, so
that is not rheo's to drop. What goes is rheo's *direct* dependency on it, the
two flags, and every line that reads a trace.

**Why a feature rather than trusting the runtime guard.** The guard is real —
nothing is enabled until a flag asks — so this is not about the cost of a build
that does not use it. It is so that a binary built to publish a site cannot be
asked to write a 2.5 GB trace into a deploy, and so that the flags are absent
rather than present-but-inadvisable.

The feature is forwarded, not redefined, by each crate: `rheo-core` owns it,
`rheo-html`/`rheo-pdf`/`rheo-epub` and the `rheo` binary each declare a
`timings` feature that enables `rheo-core/timings`, and every internal
dependency is pulled with `default-features = false`. That last part is the
one that matters and the easy one to get wrong: without it, one plugin crate
pulling `rheo-core` with its defaults would quietly re-enable the feature
across the whole build, and `--no-default-features` on the binary would be a
no-op. `default-features = false` therefore lives on the `[workspace.dependencies]`
entries, because cargo refuses to let an inheriting crate override it.

To check it stayed true — and this is the check to use, since the dependency
tree cannot answer it:

```
cargo build --release --no-default-features
./target/release/rheo compile --help | grep -c timings   # must print 0
```
