---
id: rh-bind-rheo-index-per-vertebra-c7c34420
short-id: c7
title: Bind rheo-index per vertebra
priority: 4
labels:
- fix-rheo-index-handle
deps:
- blocked-by:rh-keep-the-spine-prelude-out-of-the-scan-bd7af0ec
closed: true
---
Touches: crates/core/src/typ/rheo.typ, crates/core/src/synth/typst_source.rs, crates/core/src/reticulate/spine.rs, docs/contract.md

`rheo-index()` renders an EMPTY page in the combined PDF, and pays for
introspection it does not need in HTML/EPUB. Two problems, one fix.

MEASURED: a project with `content/index.typ`, `content/chapters/one.typ` and
`content/chapters/two.typ`, built with `cargo run -- compile <path> --pdf`,
produces a PDF whose synthesized `chapters` index page contributes nothing at
all — extracting the PDF's text yields the vertebra headings and no trace of the
index page's link list.

Cause: `rheo-index()` (`crates/core/src/typ/rheo.typ:128-141`) reads the current
page's handle from `state("rheo-handle")`. That state is published by
`rheo-page-init(handle)`, called once per `#document` block. A `SingleCombined`
(PDF) layout wraps every vertebra in ONE document and emits
`#rheo-page-init("")` — see the assertion at
`crates/core/src/reticulate/spine.rs:1101`. So the handle is the empty string,
`_rheo-index-find` returns `none`, `children` is `()`, and `list()` renders
nothing. Silently, on a green build.

Second, smaller problem: reading `state` forces Typst introspection, so
`rheo-index` must be a `context` block. That costs extra layout passes on every
synthesized page and is the class of thing that produces convergence warnings.
It buys nothing — rheo already bakes each vertebra's own handle into
`TypstStmt::ContextBinding`, so the handle is a compile-time constant at the
call site.

The fix: stop looking the handle up at runtime; pass it in.

Steps:

1. `crates/core/src/typ/rheo.typ:128` — rename `rheo-index` to
   `rheo-index-at(handle)`, taking the handle as its argument. Delete the
   `context` wrapper and the `state("rheo-handle").get()` line. Everything else
   it reads (`sys.inputs.rheo-context`) needs no `#context`. Keep
   `_rheo-index-find` exactly as it is.
2. Same function — fix the no-`ext` branch, which is the combined PDF. Today a
   child falls to the plain-text arm at `crates/core/src/typ/rheo.typ:138`, so
   even with the right handle a PDF would get unlinked titles. Use Typst's own
   label link instead, which is the mechanism rheo already relies on for
   cross-vertebra links in paged output:
   `link(label(child.handle), child.title)`. VERIFIED that internal links
   resolve in rheo's combined PDF: a vertebra containing
   `#link(<chapters:one>)[Go to One]` produces `/Link` annotations in the
   exported PDF. Keep the existing `child.handle == none` guard — a group node
   has no handle and must stay plain text.
3. `crates/core/src/synth/typst_source.rs:67-79` — `TypstStmt::IndexHelper`
   currently renders `#import "/typ/rheo.typ": rheo-index`. Change it to import
   `rheo-index-at`, and add a second variant beside it, e.g.
   `TypstStmt::IndexBinding`, rendering exactly:

       #let rheo-index() = rheo-index-at(rheo-context().handle)

   Update the unit test `index_helper_imports_rheo_index_from_rheo_typ`
   (`crates/core/src/synth/typst_source.rs:289`) and add one for the new
   variant.
4. `crates/core/src/reticulate/spine.rs:522-540` — in `vertebra_injections`, put
   `TypstStmt::IndexBinding` into the `TypstBlock` immediately after
   `TypstStmt::IndexHelper`, which is itself after `TypstStmt::ContextBinding`.
   The order matters twice: the binding calls `rheo-context()`, which must
   already exist above it, and the project's `[spine] prelude` is spliced after
   the whole block so a `#let rheo-index() = ...` there still shadows this
   default. Update the doc comment above the function (lines 514-520), which
   describes the current import-and-state arrangement.
5. The existing test `vertebra_injection_prelude_is_composed_function_with_own_handle`
   (`crates/core/src/reticulate/spine.rs`, roughly line 907) asserts on the
   exact injected text, including `#import "/typ/rheo.typ": rheo-index` and a
   `p.ends_with("rheo-index\n\n")`. Both break. Update them to the new text
   rather than deleting the assertions — they are what pins the ordering.
6. `docs/contract.md` — the "Directory-index helper" table entry says the helper
   reads the page's handle from `state("rheo-handle")`. Rewrite it: the handle
   comes from the per-vertebra `rheo-context()` binding, no `#context` is
   involved, and the helper works in every format including the combined PDF,
   where it emits label links rather than relative hrefs.

NON-GOALS:

- Do NOT make the `IndexHelper` import conditional on `auto_index` or on the
  vertebra being synthesized. Importing into every vertebra is deliberate: a
  hand-written `index.typ` that wants a heading plus the generated list calls
  `#rheo-index()` too, and the `[spine] prelude` override only works because the
  name is in scope everywhere.
- Do NOT change the synthesized vertebra's body. It stays exactly
  `#rheo-index()\n` (`crates/core/src/reticulate/spine.rs:411`).
- Do NOT bake the children list into the synthesized source. It would be faster
  still, but it would stop a custom `rheo-index` from walking the wider spine.
- Do NOT change `rheo-page-init` or remove `state("rheo-handle")`. That state
  exists for packages and is documented as such; this bird only stops rheo's own
  helper from depending on it.

VERIFY, all five:

1. `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
2. HTML is unchanged: a project with `content/index.typ`,
   `content/chapters/one.typ` and `content/chapters/two.typ` still renders
   `build/html/chapters.html` as a list of two links whose hrefs are
   `./chapters/one.html` and `./chapters/two.html`.
3. The same project built with `--pdf` now contains the child titles "One" and
   "Two" on the synthesized index page, and the exported PDF's `/Link`
   annotation count is higher than before the fix.
4. `rheo-index-at` is no longer a `context` block: reading
   `crates/core/src/typ/rheo.typ` shows no `context` keyword inside it.
5. The override still works: `[spine] prelude` containing
   `#let rheo-index() = [CUSTOM #rheo-context().handle]` makes
   `build/html/chapters.html` render `CUSTOM chapters`.