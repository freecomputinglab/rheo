---
id: rh-mark-synthesized-vertebrae-in-spine-flat-607892ff
short-id: '60'
title: Mark synthesized vertebrae in spine-flat
priority: 3
labels:
- fix-spine-flat-synthesized
deps: []
closed: true
---
A template that reads spine paths off disk cannot survive `auto_index`.

`spine-flat` lists one dict per clickable vertebra — `handle`, `path`, `title`
(`crates/core/src/reticulate/spine/serialize.rs:120-133`). A vertebra
synthesized by `auto_index` (feat-auto-index, retired) appears there like any
other, carrying the notional path `<dir>/index.typ` for a file that does not
exist on disk.

A package that does anything with those paths other than link to them breaks.
Real case, waterline/rookery `_lib/template.typ:953-975`: it builds
`_SPINE-PATHS` from `spine-flat`, then `read("/" + p)` on each to scan for
citation keys and cut a sub-bibliography. `clusters/` has no `index.typ`, so
rheo mints one, and the read fails:

    error: file not found (searched at .../rookery/clusters/index.typ)

Because the binding is top-level, the failure lands at IMPORT time and takes
down every page in the site, not just the clusters index. The template has no
way to tell a phantom path from a real one and Typst has no `read`-with-default
and no error recovery, so there is no fix on the package side — the field has
to come from rheo.

`Vertebra` already carries the bit (`crates/core/src/reticulate/spine.rs:216-220`,
set at `:427` and copied at `:464`). It is only missing from the serialization.

Non-goals: changing `auto_index` behaviour, making synthesized sources readable
through the world (a separate, larger question — the overlay is a Mould-stage
concern), touching rookery.

## The test comes first

`rheo-tests-covers-synthesized-spine-entries-before-0858b140`, in the sibling
`/home/lox/code/_fcl/rheo-tests` repo, adds the integration case that this bird
turns green: a project with an index-less directory asserting `synthesized` on
every `spine-flat` entry and every `spine` node. Work that bird BEFORE this one
and watch it fail, then come back here.

It also carries a fact that bites this bird: `cases/rheo_context_sys_inputs`
pins both key sets exactly (`("handle", "path", "title")` and
`("children", "handle", "path", "title")`), so rheo's CI — which clones
rheo-tests — goes red on this change until that paired branch lands.
rheo-tests branch name, per its README: `rheo/feat/vertebra-prelude`.

## Steps

1. `crates/core/src/reticulate/spine/serialize.rs:120-133` — in `spine_flat()`,
   add a fourth key to each dict:
   `("synthesized".to_string(), TypstLiteral::bool(v.synthesized))`.

2. Same file, `node_literal()` at `:86-114` — the tree nodes carry
   `title`/`handle`/`path`/`children`, and a group node has `handle: none`,
   `path: none`. Add `synthesized` there too, in the `Some(v)` arm from
   `v.synthesized` and `false` in the `None` (group) arm, so a package walking
   `spine` rather than `spine-flat` gets the same guarantee.

3. `docs/contract.md:41` — the `spine-flat` row reads
   `array` (dicts: `handle`/`path`/`title`). Add `synthesized` to the key list
   and note in the description column: `synthesized: true` means `path` is
   notional — `auto_index` minted the page and there is no such file on disk,
   so never `read()` it. Update the `spine` row above it the same way.

4. Tests, `crates/core/src/reticulate/spine/serialize.rs` tests module — the
   existing `spine_tree_nests_group_nodes_with_none_handle_and_path` builds a
   scan with `auto_index` off. Add one that runs `SpineScan::run(&content, &[], true)`
   over a content dir with an index-less subdirectory holding one page (the
   shape `synthesized_index_source_and_output_path_match_a_real_index_typ`
   at `spine.rs:822` already sets up) and asserts the serialized `spine-flat`
   carries `synthesized: true` for the minted `<dir>/index.typ` entry and
   `synthesized: false` for the real page.

VERIFY: `cargo test -p rheo-core spine` passes, `cargo fmt && cargo clippy -- -D warnings`
clean, and with this rheo built, `rheo compile rookery --html --input today=2026-09-14`
in `/home/lox/code/waterline` succeeds — `_SPINE-PATHS` filters the phantom
`clusters/index.typ` on the new field and the bibliography reads.