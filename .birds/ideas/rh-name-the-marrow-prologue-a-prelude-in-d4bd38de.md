---
id: rh-name-the-marrow-prologue-a-prelude-in-d4bd38de
short-id: d
title: Name the marrow prologue a prelude in code
priority: 1
labels:
- chore-naming
deps: []
closed: false
---
The marrow position that splices BEFORE every document is now spelled
`prelude` in every name a user sees, and `prologue` in every name the code
uses. One concept, two words.

User-facing (all `prelude`):
- the reserved filename `.marrow.prelude.typ` (`MARROW_PRELUDE_FILE`,
  `crates/core/src/util/constants.rs`)
- `docs/contract.md`'s marrow table
- `CLAUDE.md`'s "Marrow filenames" paragraph
- `changelog.md`'s entry for the rename

Internal (all `prologue`):
- `VirtualSpine::marrow_prologue` (`crates/core/src/reticulate/spine.rs:238`)
  and `VirtualSpine::with_marrow_prologue` (line 267)
- `BundleSource::marrow_prologue` (`crates/core/src/reticulate/bundle_source.rs:50`,
  read at line 62)
- `MarrowContext::marrow_prologue` (`crates/core/src/build.rs:149`), the local
  `marrow_prologue` in `resolve_marrow` (line 673), and the parameter on
  `build_virtual_spine` (line 727)
- `PackageIndex::marrow_prologue` (`crates/core/src/packages/manifest.rs`)
- the test names `package_prologue_marrow_is_read_from_its_own_filename` and
  `detect_package_marrow_prologue_in_dirs_collects_in_import_order`

A reader who learns the filename then greps for `prelude` in the source finds
the unrelated `[spine] prelude` and none of this.

## Decision already made — do not re-derive

Rename the internal vocabulary to `prelude`, matching the filenames, rather
than renaming the filenames to `prologue`. The filenames are the published
contract and were chosen deliberately; the field names are not.

## Steps

1. Rename, in `crates/core/`:
   - `VirtualSpine::marrow_prologue` -> `marrow_prelude`, and
     `with_marrow_prologue` -> `with_marrow_prelude`
     (`crates/core/src/reticulate/spine.rs:238,267`, plus the call in
     `build_virtual_spine` and the ~6 struct literals in test modules).
   - `BundleSource::marrow_prologue` -> `marrow_prelude`
     (`crates/core/src/reticulate/bundle_source.rs:50,62`).
   - `MarrowContext::marrow_prologue` -> `marrow_prelude`
     (`crates/core/src/build.rs:149`) and the locals and parameters that feed
     it.
   - `PackageIndex::marrow_prologue` -> `marrow_prelude`
     (`crates/core/src/packages/manifest.rs`), and its callers.
2. Rename the two test functions named above, and any assertion message string
   containing the word "prologue", to match.
3. Sweep the doc comments in those files for "prologue" and update the prose.
   `crates/core/src/reticulate/spine.rs:234-238`'s field doc and
   `crates/core/src/build.rs`'s `resolve_marrow` banner both use it in prose.
4. Leave `docs/contract.md`, `CLAUDE.md` and `changelog.md` alone — they
   already say `prelude`.

## Do NOT

- Do NOT rename any FILENAME constant or its value.
  `.marrow.prelude.typ`, `.marrow.epilogue.typ` and `.marrow.typ` are the
  published contract and must not move.
- Do NOT rename `[spine] prelude`, `Spine::prelude`,
  `VirtualSpine::vertebra_prelude` or `with_vertebra_prelude`. Those are a
  DIFFERENT feature — Typst prepended inside each vertebra, not at the bundle
  root — and already read correctly.
- Do NOT change any behaviour. This bird is a pure rename; every test must pass
  without its assertions being weakened.
- Do NOT rename `dot_marrow_is_epilogue`.

## Note for the operator, not a step

`prelude` now names two different things in rheo: a marrow position
(`.marrow.prelude.typ`, at the bundle root, before every document) and a
per-vertebra injection (`[spine] prelude`, inside each page). This bird makes
the marrow half internally consistent; it does not resolve that collision,
which is a naming call for the operator rather than a defect.

## VERIFY

1. `just lint` and `just test` are green in `/home/lox/code/_fcl/rheo`, with
   the same test count as before (361 unit tests).
2. From `/home/lox/code/_fcl/rheo-tests`:
   `RHEO_MANIFEST=../rheo/Cargo.toml cargo test --test harness` is green, 135
   tests.
3. No stale vocabulary left in the crates:

   ```bash
   rg -n "marrow_prologue|marrow-prologue" /home/lox/code/_fcl/rheo/crates/
   ```

   prints nothing.
4. The filenames are untouched:

   ```bash
   rg -n 'MARROW_PRELUDE_FILE|MARROW_EPILOGUE_FILE' /home/lox/code/_fcl/rheo/crates/core/src/util/constants.rs
   ```

   still shows `.marrow.prelude.typ` and `.marrow.epilogue.typ`.