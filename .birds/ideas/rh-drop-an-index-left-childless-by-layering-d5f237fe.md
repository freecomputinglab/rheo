---
id: rh-drop-an-index-left-childless-by-layering-d5f237fe
short-id: d5
title: Drop an index left childless by layering
priority: 3
labels:
- fix-auto-index-stranded
deps: []
closed: true
---
Touches: crates/core/src/reticulate/spine/section.rs

Layering can strand a synthesized index as an empty page.

`auto_index` synthesizes a landing page for a directory that has children and no
landing file of its own. `[[spine.section]]` and `[spine] include` then rearrange
the tree — and a section can claim every one of that directory's children,
leaving the synthesized page behind with nothing to list.

MEASURED, with `content/index.typ` and `content/chapters/one.typ`:

    [[spine.section]]
    name = "grouped"
    include = ["chapters/*.typ"]

`cargo run -- compile <path> --html` produces `build/html/grouped/one.html` as
expected, and also `build/html/chapters.html` containing exactly
`<body><ul></ul></body>`, titled "Index". That page is in `spine-flat`, so it
reaches every nav, feed and sitemap built on it.

The scan already enforces the right rule — `crates/core/src/reticulate/spine/scan.rs:202-205`
drops a directory node whose subtree is empty, with the comment "a page listing
nothing is worse than no page". Layering does not enforce it. The reasoning
recorded at `crates/core/src/reticulate/spine/section.rs:100-108` is correct as
far as it goes (a synthesized node always has children at scan time, so it is
never a movable leaf a section can claim) but it does not follow that the node
is still USEFUL once its children have been moved out from under it.

Steps:

1. `crates/core/src/reticulate/spine/section.rs` — after the rebuilt tree is
   produced and before the `files`/`tree` are returned, in BOTH
   `apply_sections` (line 90, returning at roughly line 140) and `apply_include`
   (line 249, returning at roughly line 290), prune any node that is (a) a
   landing node whose file index is in the synthesized set, and (b) now has no
   children. Prune recursively — removing one such node can leave its parent in
   the same state.
2. Do the prune on the tree BEFORE `reindex` assigns fresh indices, or express
   it in terms of the `synthesized_paths` set the two functions already build
   (`crates/core/src/reticulate/spine/section.rs:104-108` and `254-258`). Either
   is fine; say in a comment which one you chose and why. Pruning after reindex
   means recomputing indices a second time, which is the trap here.
3. A pruned node's file must also leave `files`, or `VirtualSpine::build` will
   still synthesize a vertebra for it. The existing `reindex_synthesized` helper
   (`crates/core/src/reticulate/spine/section.rs:337`) recomputes the
   synthesized set by path identity against the rebuilt `files`, so as long as
   the path is gone from `files` the set comes out right — confirm this rather
   than assuming it.
4. If pruning empties the spine entirely, the existing "spine is empty after
   applying sections" / "after applying include" errors
   (`crates/core/src/reticulate/spine/section.rs:136` and `285`) must still
   fire. Check the prune runs before that emptiness check, not after.

NON-GOALS:

- Do NOT prune a landing node that came from a REAL file on disk. An author who
  wrote `chapters/index.typ` and then moved its children into a section still
  gets their page — they wrote it, so it is theirs to keep or delete. Only a
  page rheo invented is rheo's to withdraw.
- Do NOT prune a group node. A childless group cannot arise from layering the
  way a childless synthesized landing can, and group handling is not what this
  bird is about.
- Do NOT change `scan_subdir`. The scan-time rule is already correct.

VERIFY, all four:

1. `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
2. The measured case above produces `build/html/index.html` and
   `build/html/grouped/one.html`, and NO `build/html/chapters.html`.
3. A fixture where the section claims only SOME of a directory's children —
   `content/chapters/one.typ` and `content/chapters/two.typ` with
   `include = ["chapters/one.typ"]` — still produces `build/html/chapters.html`,
   now listing only `two`.
4. A unit test in `crates/core/src/reticulate/spine/section.rs` beside
   `apply_sections_groups_flat_files` (line 365): a scan run with
   `auto_index = true` over a directory with no landing file, whose every child
   a section then claims, returns a `SpineScan` whose `files` no longer contains
   that directory's notional `index.typ` and whose `synthesized` set is empty.