# Baseline: `just perf-watch` on the rookery (wl-7a9a81ce)

Recorded 2026-09-25. rheo `0.6.4` (`Cargo.toml`), `typst 0.15.0` /
`typst-bundle 0.15.0` (`Cargo.lock`). Machine: NixOS 26.11, Linux 6.18.53,
`nproc` = 12. Only the operator's two idle `rheo watch` processes
(`rookery --html --open`, `writing/blog --html --open`) were running
alongside this measurement — checked with `pgrep -af 'rheo|typst'` before
starting.

Measured against `/tmp/bench-rookery`, a copy of
`/home/lox/code/waterline/rookery` (858 pages, 145 `.typ` files), not the
live project — the operator's two watchers keep the real rookery busy, and
this harness must not compete with them or trigger their rebuilds.

## Output

```
watch-bench: project=/tmp/bench-rookery rheo=0.6.4 date=2026-09-25
watch-bench: vertebra=/tmp/bench-rookery/index.typ asset=/tmp/bench-rookery/static/kermodeStrangeNature2026.slides.pdf
cold: 23400ms —   23.398160361s  INFO 858 page(s), 29.7 MB in 23.4s (+2 asset(s)) — typst-compile 23.1s, packages 104ms, plugin-write 85ms
scenario=touch runs_ms=227,235,239 median_ms=235
scenario=vertebra-edit runs_ms=896,920,903 median_ms=903
scenario=asset-edit runs_ms=239,238,235 median_ms=238
```

A later run should be compared scenario-by-scenario against these medians,
not against the cold number — per-edit rebuild latency is what this effort
optimises, and a slower cold build is an acceptable trade for a faster
rebuild. Run-to-run noise on this machine is on the order of 10-15ms for the
cheap scenarios (`touch`, `asset-edit`) and a few tens of ms for
`vertebra-edit`; treat a change smaller than that as noise, and re-run before
crediting anything under about 5% to a code change.

## Second section: converging in 4 passes vs. 5

The rookery recently dropped from 5 introspection-convergence passes to 4 via
two call sites in `_lib/template.typ` (waterline, not this repository): a
lexical `handle:` binding on `#idea` that spares a pass reading
`state("rheo-handle")`, and folding `citations-as-ideas` down to `BIBTEX.all`
directly instead of wrapping it in a closure that recomputes `_claimed-keys`.
Reverting both on a second copy, `/tmp/bench-rookery-5pass`, and confirming
with `--iterations`:

```
5-pass copy:  5 convergence iteration(s) — iter(1) 2.8s, iter(2) 1.2s, iter(3) 21.8s, iter(4) 10.6s, iter(5) 364ms
4-pass copy:  4 convergence iteration(s) — iter(1) 3.0s, iter(2) 22.9s, iter(3) 10.8s, iter(4) 4.7s
```

`just perf-watch` on the 5-pass copy:

```
cold: 20400ms —   20.444855431s  INFO 858 page(s), 29.7 MB in 20.4s (+2 asset(s)) — typst-compile 20.1s, packages 106ms, plugin-write 77ms
scenario=touch runs_ms=251,235,233 median_ms=235
scenario=vertebra-edit runs_ms=981,965,971 median_ms=971
scenario=asset-edit runs_ms=243,246,243 median_ms=243
```

Side by side (medians, ms):

| scenario | 5-pass | 4-pass |
|---|---|---|
| touch | 235 | 235 |
| vertebra-edit | 971 | 903 |
| asset-edit | 243 | 238 |

The two cheap scenarios (`touch`, `asset-edit`) are unchanged either way,
because neither trips the `#idea`/citation code paths the two call sites
touch; `vertebra-edit`, which edits a page that does mint content through
those paths, drops by about 70ms (~7%) going from 5 passes to 4 — a real but
modest win on this project, well short of the ~430ms a full pass removed from
the middle of the loop would suggest, which says most of `vertebra-edit`'s
cost is elsewhere in the compile.
