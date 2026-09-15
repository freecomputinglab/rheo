---
id: rh-rename-dot-marrow-is-epilogue-9b3bbd43
short-id: '9'
title: Rename dot_marrow_is_epilogue
priority: 4
labels:
- rename-marrow-table
deps:
- blocked-by:rh-title-a-synthesized-index-by-directory-0b69f6c4
- blocked-by:rh-document-auto-index-in-the-changelog-c3cf8b3d
- blocked-by:rh-document-prelude-cost-and-reserved-typ-dc5db8c8
closed: true
---
Touches: crates/core/src/util/constants.rs, crates/core/src/config/mod.rs, crates/core/src/config/retired.rs, crates/core/src/reticulate/spine.rs, crates/core/src/reticulate/bundle_source.rs, crates/core/src/packages/manifest.rs, crates/core/src/build.rs, crates/cli/src/migrate.rs, CLAUDE.md, changelog.md, docs/contract.md

Rename `dot_marrow_is_epilogue`, and stop "prelude" naming two different
mechanisms. Both must land BEFORE 0.6.3 ships, or they cost users a migration
instead of costing us an afternoon. The 0.6.3 release bird (title "Cut rheo
0.6.3", short id f6) should not be flown until this one has landed.

This is a naming bird. Nothing about what the code DOES may change.

Two problems.

(1) `dot_marrow_is_epilogue` (`crates/core/src/config/mod.rs:308`) leaks a
filename's leading dot into a config key, expresses a POSITION as a boolean, and
reads as an implementation note rather than a setting. It needs a clause of
prose everywhere it appears to say which way `true` points — see `CLAUDE.md`'s
"Marrow filenames" paragraph and `changelog.md`'s marrow section, both of which
spend one. It also sits at the top level beside `marrow` (the filename
override), so one subsystem has two unrelated-looking top-level keys.

(2) "prelude" now names two different mechanisms in the same `rheo.toml`:
`.marrow.prelude.typ` is a POSITION AT BUNDLE ROOT, while `[spine] prelude` is a
FILE SPLICED INTO EVERY VERTEBRA. The rename that introduced it moved toward
this collision rather than away from it, and `VertebraInjection.prelude` already
meant the per-vertebra sense internally.

The resolution to implement:

- Keep `prelude` for the per-vertebra concept. That is where the word carries
  real meaning, and it matches `VertebraInjection.prelude`.
- Return the marrow positions to `prologue`/`epilogue`: the filenames become
  `.marrow.prologue.typ` and `.marrow.epilogue.typ`. Those names were
  unambiguous, and nothing in the package ecosystem ships either yet.
- Replace `dot_marrow_is_epilogue` and the top-level `marrow` key with a
  `[marrow]` table:

      [marrow]
      file = "bundle-root.typ"   # was the top-level `marrow` key
      position = "epilogue"      # was `dot_marrow_is_epilogue = true`

  `position` takes `"epilogue"` (the default) or `"prologue"`, and governs only
  where a BARE `.marrow.typ` lands. An explicit filename still outranks it.

Steps:

1. `crates/core/src/util/constants.rs:13` — rename `MARROW_PRELUDE_FILE` to
   `MARROW_PROLOGUE_FILE` and its value to `.marrow.prologue.typ`. Update
   `MARROW_RESERVED_FILES` at line 20 and every reference;
   `rg MARROW_PRELUDE` finds them all.
2. `crates/core/src/config/mod.rs` — add the `[marrow]` table with `file` and
   `position`. Retire BOTH the top-level `marrow` key and
   `dot_marrow_is_epilogue`, adding an entry for each to `RETIRED_KEYS`
   (`crates/core/src/config/retired.rs:25`) in the same shape as the
   `marrow_prologue` entry already there. The top-level scalar capture that
   feeds those warnings is `RheoConfig::extra`
   (`crates/core/src/config/mod.rs:336`) — do not remove it, it is what makes an
   unknown top-level scalar warnable at all.
3. Rename the internal marrow vocabulary back from `prelude` to `prologue`:
   `VirtualSpine::marrow_prelude` and `with_marrow_prelude`
   (`crates/core/src/reticulate/spine.rs:265-299`),
   `BundleSource.marrow_prelude`
   (`crates/core/src/reticulate/bundle_source.rs:50`),
   `PackageIndex::marrow_prelude` (`crates/core/src/packages/manifest.rs:461`),
   and `MarrowContext.marrow_prelude` (`crates/core/src/build.rs:149`). Leave
   `VertebraInjection.prelude` and `[spine] prelude` alone — those keep the word.
4. `crates/cli/src/migrate.rs` — teach `rheo migrate` to rewrite the old keys.
   `marrow_prologue = true` and `dot_marrow_is_epilogue = false` both become
   `[marrow] position = "prologue"`; a top-level `marrow = "x"` becomes
   `[marrow] file = "x"`. Follow the shape of the existing `vertebrae` to
   `exclude` conversion already in that file.
5. Update every document that names these: `CLAUDE.md` (the "Marrow filenames"
   paragraph and the `rheo.toml` block at the top), `changelog.md` (rewrite the
   marrow section in place — do not append a correction to it), and
   `docs/contract.md` (the marrow position table, roughly line 215).

NON-GOALS:

- Do NOT change any BEHAVIOUR. The outranking rule (either explicit name beats a
  bare `.marrow.typ`, which is then not read at all), the default position
  (epilogue), and the rule that a project's setting never moves a package's bare
  marrow all stay exactly as they are.
- Do NOT rename `[spine] prelude`, `[spine] auto_index`, or `rheo-index()`.
- Do NOT add any `[marrow]` key beyond `file` and `position`.

VERIFY, all five:

1. `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
2. `rg -n "dot_marrow_is_epilogue|MARROW_PRELUDE|marrow_prelude" crates/ docs/
   CLAUDE.md changelog.md` returns nothing.
3. A project with `[marrow] position = "prologue"` and a bare `.marrow.typ`
   containing `#show strong: it => it` has that rule reach a pre-existing
   vertebra's `*bold*`; with `position = "epilogue"` or with the key absent, it
   does not. The end-to-end pair built on `build_show_rule_project`
   (`crates/core/src/build.rs`, roughly line 1994) already pins exactly this —
   port those two tests rather than writing new ones.
4. A `rheo.toml` still carrying `dot_marrow_is_epilogue = false` builds and
   prints a retired-key warning naming `[marrow] position` as its replacement.
5. `rheo migrate` on that same file rewrites it to `[marrow] position =
   "prologue"`, and the migrated project builds to byte-identical output.