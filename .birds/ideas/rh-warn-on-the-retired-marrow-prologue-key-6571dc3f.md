---
id: rh-warn-on-the-retired-marrow-prologue-key-6571dc3f
short-id: '6'
title: Warn on the retired marrow_prologue key
priority: 3
labels:
- fix-retired-keys
deps: []
closed: true
---
`marrow_prologue` was replaced by `dot_marrow_is_epilogue` (whose sense it
inverts: `marrow_prologue = true` is now `dot_marrow_is_epilogue = false`). A
`rheo.toml` still carrying the old key gets NO warning and a silently different
build.

MEASURED, on a scratch project whose `rheo.toml` reads
`marrow_prologue = true` and whose `content/.marrow.typ` is
`#show strong: it => [TOUCHED]`:

- Before the rename: the marrow spliced as a prologue, and `*bold text*` in the
  vertebra rendered as `TOUCHED`.
- After: no warning on the build, and the page renders `bold text` — the marrow
  moved to the epilogue, where a `#show` rule reaches no pre-existing vertebra.

So the one key whose entire purpose was to move a `#show` rule now moves it
back, without saying anything. `[spine] merge` and `[spine] vertebrae` both
warn in exactly this situation; this key cannot, and the reason is structural.

## Why it cannot warn today

`warn_on_retired_keys` (`crates/core/src/config/retired.rs:102`) matches a
`RetiredKey`'s `table` field against a `shown_table` string and looks the key up
in an `extra: toml::Table` flatten map. It is called from three places, all in
`crates/core/src/config/mod.rs` — line 447 for `[spine]`, line 450 for a plugin
section, line 452 for `[<format>.spine]`.

`marrow_prologue` was a TOP-LEVEL key, and there is no fourth call for the
top-level table. `RheoConfig` is built from `RheoConfigRaw`
(`crates/core/src/config/mod.rs:371`), and unknown top-level tables become
`PluginSection` entries rather than landing in a flatten map the warner could
read. So no existing `RETIRED_KEYS` entry can name the top level.

## Steps

1. Give `RheoConfigRaw` a flatten map for unrecognized top-level SCALAR keys,
   or otherwise capture them, so a retired top-level key is observable. Check
   first whether the existing "unknown table becomes a PluginSection" rule
   already swallows a scalar like `marrow_prologue = true` — if a bare boolean
   at the top level is currently a parse error rather than a silent drop, this
   bird is much smaller and the whole fix is step 3.
2. Add a fourth `warn_on_retired_keys` call for the top-level table. The
   `shown_table` string wants a form that reads correctly in the message
   template `` "`{}` in {} is retired and has no effect — {}" `` — the existing
   values are bracketed table names like `"[spine]"`, so pick something that
   does not read as a table (the message is prose, and "in the top level" is
   fine) and adjust `declared_table` at
   `crates/core/src/config/retired.rs:103-107`, which today rewrites anything
   ending in `.spine]` to `[spine]`.
3. Add the `RetiredKey` entry to `RETIRED_KEYS`
   (`crates/core/src/config/retired.rs:26`), with a replacement string that
   names the inversion explicitly, e.g. "replaced by `dot_marrow_is_epilogue`,
   whose sense is inverted: `marrow_prologue = true` is now
   `dot_marrow_is_epilogue = false`".
4. Check `crates/cli/src/migrate.rs` — it reports retired keys from the same
   table and may need the top-level case too. Grep it for `RETIRED_KEYS` to see
   whether it iterates the list generically (in which case it needs nothing) or
   per table.

## Honest uncertainty

Step 1 is the unverified part. Whether a leftover top-level scalar is currently
dropped silently or rejected has NOT been established — the measurement above
only shows that no warning appears, which is consistent with either. Establish
which before writing code, and if it is already a hard parse error, say so and
reduce this bird to steps 3 and 4.

## Do NOT

- Do NOT reinstate `marrow_prologue` as a working alias. It is retired; the
  point is to say so, not to keep honouring it.
- Do NOT change `dot_marrow_is_epilogue`'s default (`true`) or its sense.
- Do NOT change the marrow filename rules
  (`.marrow.prelude.typ` / `.marrow.epilogue.typ` / bare `.marrow.typ`).
- Do NOT add a retired-key warning for `[spine] prelude` — that key is new and
  current.

## VERIFY

1. `just lint` and `just test` are green in `/home/lox/code/_fcl/rheo`.
2. A unit test in `crates/core/src/config/mod.rs`'s test module parses a
   manifest containing `marrow_prologue = true` and asserts it does not affect
   `dot_marrow_is_epilogue`.
3. End to end, from `/home/lox/code/_fcl/rheo`:

   ```bash
   mkdir -p /tmp/rh-retired/content
   printf 'version = "0.6.2"\nformats = ["html"]\ncontent_dir = "content"\nmarrow_prologue = true\n' > /tmp/rh-retired/rheo.toml
   printf '= Index\n\nThis has *bold text*.\n' > /tmp/rh-retired/content/index.typ
   printf '#show strong: it => [TOUCHED]\n' > /tmp/rh-retired/content/.marrow.typ
   cargo run -- compile /tmp/rh-retired --html 2>&1 | grep -i retired
   ```

   must print a warning naming `marrow_prologue` and
   `dot_marrow_is_epilogue`.
4. `bd show <this bird's id>` reports it retired once the flight lands.