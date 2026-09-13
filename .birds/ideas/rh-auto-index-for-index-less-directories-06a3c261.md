---
id: rh-auto-index-for-index-less-directories-06a3c261
short-id: '0'
title: Auto index for index-less directories
priority: 3
labels:
- feat-auto-index
deps: []
closed: false
---
A content directory with no landing file gets NO PAGE today. `scan_subdir`
(`crates/core/src/reticulate/spine/scan.rs:152`) looks for `index.typ` and then
`<dirname>.typ`; finding neither, the `match landing_idx` at
`scan.rs:183-195` makes a `SpineNode::group` — "no landing file; a
non-clickable directory/section" (`crates/core/src/reticulate/spine/tree.rs:16`)
— or drops the node entirely when the directory has no children either.

So a project that writes `content/writing/dissertation/chapters/knuth.typ` and
no `chapters/index.typ` gets `knuth.html` and nothing to reach it from: the
directory is a dead entry in the nav, and the only route to the page is typing
its URL. Every directory needs a hand-written `index.typ` whose whole job is to
list what is already in the directory.

This bird mints that page instead, ON BY DEFAULT, with a body the project can
replace.

Touches: crates/core/src/reticulate/spine/scan.rs, crates/core/src/reticulate/spine.rs, crates/core/src/config/mod.rs, crates/core/src/typ/rheo.typ, docs/contract.md

## The design, already decided

1. **The engine synthesizes an empty vertebra, it does not render a listing in
   Rust.** The synthesized source is one call, `#rheo-index()`.
2. **`rheo-index()` is injected per vertebra, exactly as `rheo-context()` is**,
   and ships a default implementation: the current directory's children as a
   list of links.
3. **A project overrides it by binding its own `rheo-index` in its
   `[spine] prelude`.** That works because of an order rheo already documents:
   the injected prelude carries the `rheo-context()` binding and the project's
   `[spine] prelude` is prepended AFTER it (`crates/core/src/config/mod.rs:99-106`:
   "prepended inside every vertebra, after its `rheo-context()` binding"), and
   the vertebra's own body comes last (`crates/core/src/synth/source_injector.rs:81`
   — `format!("{head}{prelude}{body}{epilogue}")`). A later `#let` wins, so a
   project's definition shadows rheo's for every page, and the synthesized body
   calls whichever is in scope.

   That is the whole customization story: a site styles its directory indexes
   once, in its prelude, and writes no `index.typ` files at all.
4. **On by default.** `[spine] auto_index = true`. This is a deliberate
   behaviour change: sites with index-less directories gain pages they did not
   have, and nav entries that were inert become links. Setting the key to
   `false` restores today's behaviour exactly.

## Steps

1. `crates/core/src/config/mod.rs` — add `auto_index: bool` to `struct Spine`
   (parsed, lines 75-104) and to `struct SpineRaw` (serde input, lines 108-121),
   defaulting to `true`. `[spine]` accepts `title`, `exclude`, `section` XOR
   `include`, and `prelude` today; this is one more key and needs no
   interaction with the `section`/`include` mutual exclusion at lines 118-122.

2. `crates/core/src/reticulate/spine/scan.rs` — in the `match landing_idx` at
   lines 183-195, add an arm between the two that exist: when there is no
   landing file, the directory HAS children, and `auto_index` is on, push a
   SYNTHESIZED landing instead of building a group.

   - The notional path to push into `files` is `<dir>/index.typ`. It does not
     exist on disk, and that is the point: every downstream derivation —
     the handle, the output path, the segment, the title — is path-derived and
     therefore comes out identical to what a real, empty `index.typ` would have
     produced. Nothing else has to learn about synthesis.
   - Record the index it was pushed at, so the source reader (step 3) can tell
     a synthesized entry from a real one. A `HashSet<usize>` carried on
     `SpineScan` beside `files` and `tree` is the shape that fits; thread it
     through `SpineScan`'s construction and its `apply_sections` /
     `apply_include` rebuilds, which currently rebuild `files` by re-indexing.
   - `scan_subdir` needs the `auto_index` flag to reach it. It already takes
     `exclude` down the recursion; take the flag the same way rather than
     reading config from inside the scan.
   - The `None if children.is_empty() => None` arm STAYS. A directory that is
     empty after exclusion is still dropped whole — an index page listing
     nothing is worse than no page.

3. `crates/core/src/reticulate/spine.rs` — at line 334, `VirtualSpine::build`
   reads every spine file with `fs::read_to_string(file)`, under a comment
   stating that the scan already proved the path exists. For a synthesized
   index, use the synthesized source instead of reading: the string
   `"#rheo-index()\n"`. Update that comment — the invariant is now "exists on
   disk, or is one of the scan's synthesized indexes".

   Everything else in that loop (handle, escape, `output_path`, `rel_path`,
   title, label extraction) runs unchanged on the notional path.

4. `crates/core/src/typ/rheo.typ` — add `rheo-index()`, rendering the current
   page's children as a list of links.

   - It finds its own node in `spine`, the recursive tree of
     `title`/`handle`/`path`/`children` that `rheo-context` already carries and
     which INCLUDES group nodes (`docs/contract.md:38`). `spine-flat` is the
     wrong input here: it excludes groups (`docs/contract.md:41`).
   - The current page's own handle comes from the same place the rest of the
     per-vertebra API gets it. `rheo-page-init(handle)` (`typ/rheo.typ:85`)
     already publishes it per page.
   - Link each child through the existing href machinery (`_rheo-href`,
     `typ/rheo.typ:49`), not by hand-building a relative path.
   - A child with no title falls back to its path-derived one, which the spine
     node already carries.
   - Keep it plain: a `<ul>` of links and nothing else. Anything more opinionated
     is what step 5's override is for.

5. Bind `rheo-index` INTO THE PER-VERTEBRA INJECTION, not only at bundle root.
   `crates/core/src/synth/source_injector.rs:53-63` splices `typ/rheo.typ` into
   the BUNDLE MAIN, and a vertebra is a separate Typst module — which is exactly
   why `rheo-context()` is injected per vertebra rather than defined once at
   root. `rheo-index` needs the same treatment: add it to the injected prelude
   (the `prelude` field at `crates/core/src/reticulate/spine.rs:210`, assembled
   in `VirtualSpine::build`) so it is in scope in the vertebra's own body.

   Verify by the test at `source_injector.rs:132`
   (`vertebra_wraps_its_own_prelude_and_epilogue`), which shows the shape of
   that prelude string.

6. `docs/contract.md` — document `rheo-index()` beside the `spine` and
   `spine-flat` entries at lines 38-41: what it renders, that a synthesized
   directory page's whole body is a call to it, and that a project overrides it
   by binding its own in `[spine] prelude`. Document `[spine] auto_index`
   wherever the other `[spine]` keys are documented, including that it defaults
   to `true` and what `false` restores.

7. Unit tests, in the files they test:
   - `scan.rs` — a directory with children and no landing file yields a landing
     node, not a group, with the handle a real `index.typ` would have given it;
     the same directory with `auto_index` off still yields a group; an empty
     directory still yields nothing either way.
   - `spine.rs` — a synthesized index's source is `#rheo-index()` and its
     output path matches a hand-written `index.typ`'s.

## Non-goals

- Do NOT add an integration case in `../rheo-tests`. That is a separate repo and
  gets its own bird; this one's VERIFY stays inside this crate.
- Do NOT change the `<dirname>.typ` landing convention at `scan.rs:152`. A
  directory with `foo/foo.typ` already has a landing page and is untouched here.
- Do NOT render the listing in Rust. The engine's contribution is an empty page
  whose body calls `rheo-index()`; everything visible is Typst.
- Do NOT touch marrow. `.marrow.typ` is spliced at bundle root before and after
  every document (`crates/core/src/reticulate/bundle_source.rs:56-71`) and is
  not a vertebra; it is not the mechanism for this and must not become one.
- Do NOT make a `Group` node clickable when `auto_index` is off — with the key
  off, the scan must produce exactly what it produces today.
- Do NOT add a per-directory opt-out (a dotfile, a front-matter key). The
  project-wide key plus a hand-written `index.typ` already cover both ends.

## VERIFY

1. `cargo test` passes, including the new scan and spine tests from step 7.
2. `cargo fmt && cargo clippy -- -D warnings` is clean — this project's "when
   done" step, per `CLAUDE.md:33-36`.
3. By hand, on a scratch project: `content/a/b.typ` with NO `content/a/index.typ`
   compiles to both `b.html` and an `a.html` whose body links to `b`. Adding
   `auto_index = false` under `[spine]` and recompiling produces `b.html` and no
   `a.html` at all.
4. On that same scratch project, a `[spine] prelude` file containing
   `#let rheo-index() = [my own listing]` puts that text on `a.html` — the
   override path of the design.
5. A project whose directories all have their own `index.typ` compiles to the
   same set of pages as before this change: no page appears or disappears.