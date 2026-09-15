---
id: rh-title-a-synthesized-index-by-directory-0b69f6c4
short-id: 0b
title: Title a synthesized index by directory
priority: 3
labels:
- fix-auto-index-title
deps:
- blocked-by:rh-bind-rheo-index-per-vertebra-c7c34420
closed: false
---
Touches: crates/core/src/reticulate/spine.rs

Every synthesized directory index is titled "Index".

`auto_index` replaces a non-clickable group node with a synthesized landing page
at the notional path `<dir>/index.typ`. The title for a landing node is derived
from its file STEM — `DocumentTitle::to_readable_name(&stem)` at
`crates/core/src/reticulate/spine.rs:397` — and that stem is `index`. So a nav
built from `spine-flat` shows one entry called "Index" per directory, where the
group node it replaced was called "Chapters", "Appendices", and so on.

MEASURED: a project with `content/chapters/one.typ` and `content/_lib/x.typ`
produces `build/html/chapters.html` and `build/html/_lib.html`, both with
`<title>Index</title>`.

This is consistent with what a real, empty `content/chapters/index.typ` does
today, so it is not a regression — but for a real file the author can fix it
with `#set document(title: ...)`, and for a synthesized one there is no file to
write that in. The page rheo invents should carry the name of the thing it
stands for.

The group node already computes exactly the right string:
`SpineScan::prettify(&dirname)` at
`crates/core/src/reticulate/spine/scan.rs:227` strips a leading numeric order
prefix, turns `-`/`_` into spaces and title-cases each word, so `01-intro/`
becomes "Intro". It is `pub(super)` in `scan.rs`, whose `super` is the `spine`
module, so it is already callable from `crates/core/src/reticulate/spine.rs`
with no visibility change.

Steps:

1. `crates/core/src/reticulate/spine.rs:397` — where `title` is computed inside
   the `file_infos` map. The `is_synthesized` boolean is already in scope a few
   lines above (line 409 in the same closure computes it). When the vertebra is
   synthesized, derive the title from the PARENT DIRECTORY name instead of the
   stem: take `file.parent()`, then its `file_name()`, and pass that through
   `SpineScan::prettify`.
2. Fall back to the existing stem-derived title if the parent name cannot be
   read. A synthesized path always has a parent by construction
   (`crates/core/src/reticulate/spine/scan.rs:212` builds it as
   `dir.join(&index_name)`), so this fallback should be unreachable — do not
   `unwrap()` on it anyway.
3. Extend the comment at `crates/core/src/reticulate/spine.rs:385-395`, which
   explains that the title is purely path-derived and only a pre-compile
   placeholder. Add one sentence: for a synthesized index the path-derived title
   is the FINAL title, because there is no file in which an author could publish
   a `#set document(title: ...)` beacon for it.

NON-GOALS:

- Do NOT change the title of a REAL `index.typ`. It stays "Index", the author
  can override it with `#set document(title: ...)`, and changing it would break
  projects that rely on the current path-derived value.
- Do NOT introduce a config key for this. One correct default beats a knob.
- Do NOT touch `prettify` itself. Its output is already what the group node
  used, and matching that exactly is the point.

VERIFY, all four:

1. `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
2. A project with `content/index.typ` and `content/chapters/one.typ`, built with
   `cargo run -- compile <path> --html`, gives `build/html/chapters.html` the
   title "Chapters", not "Index".
3. A directory named `01-intro/` with one child gives its synthesized page the
   title "Intro" — the same string the group node produced with
   `auto_index = false`.
4. A project with a REAL, empty `content/chapters/index.typ` still gives
   `build/html/chapters.html` the title "Index", unchanged.