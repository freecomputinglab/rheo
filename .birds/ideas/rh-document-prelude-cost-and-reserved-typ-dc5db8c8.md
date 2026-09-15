---
id: rh-document-prelude-cost-and-reserved-typ-dc5db8c8
short-id: dc
title: Document prelude cost and reserved typ
priority: 2
labels:
- doc-prelude-cost-typ-prefix
deps:
- blocked-by:rh-keep-the-spine-prelude-out-of-the-scan-bd7af0ec
- blocked-by:rh-bind-rheo-index-per-vertebra-c7c34420
closed: true
---
Touches: CLAUDE.md, docs/contract.md

Two things the prelude/auto_index work made true and nobody wrote down.

(1) THE PRELUDE'S COST SCALES WITH PAGE COUNT. `[spine] prelude` is inlined
verbatim into every vertebra's injected source — `vertebra_injections` at
`crates/core/src/reticulate/spine.rs:522-540` formats it into the prelude string
once per vertebra. That is unavoidable and correct: lexical scope demands the
text be IN the file, which is the whole reason the feature exists. But it means
a 10 KB prelude on a 5000-page site is 50 MB of synthesized source, parsed 5000
times, where marrow is parsed once.

The documented example already does the right thing — it imports a module and
binds two names — but only by accident of being short. Nobody reading it is told
that its brevity is the point. A reader who puts a template's worth of code in
the prelude will pay for it on every page and will not know why.

(2) `typ/` IS NOW A RESERVED PROJECT-ROOT PREFIX. `RheoWorld` serves
`typ/metadata.typ` and, since the `auto_index` work, `typ/rheo.typ` from memory
before falling back to disk — `crates/core/src/world.rs:471-490`. A project that
has its own `typ/rheo.typ` or `typ/metadata.typ` at its root cannot import it:
rheo's copy wins, silently. The two constants are
`METADATA_MODULE_PATH` and `RHEO_TEMPLATE_MODULE_PATH`
(`crates/core/src/util/constants.rs`). `docs/contract.md` documents the reserved
`.rheo/` OUTPUT prefix but says nothing about the reserved `typ/` INPUT prefix.

Steps:

1. `CLAUDE.md`, in the "`[spine] prelude`" paragraph under "Spine configuration"
   — add two sentences: the prelude's text is spliced into every vertebra, so
   its cost is page count times its own size; keep it to a few `#let` bindings
   that import from a root-absolute module, which Typst evaluates once, rather
   than putting the code itself there. Say it as guidance, not as a warning
   banner.
2. `docs/contract.md` — add `typ/` to the reserved-names material, beside the
   existing `.rheo/` control-asset prefix. State: paths under `typ/` at the
   PROJECT ROOT are served by rheo from memory, not from disk; a project file at
   such a path is shadowed and unreachable; the two paths in use today are
   `typ/rheo.typ` and `typ/metadata.typ`, and more may be added. Mark its
   stability with the same vocabulary the rest of that file uses (see its
   "Stability" section).
3. Check whether any other document claims `.rheo/` is the only reserved prefix,
   and fix it if so. `CLAUDE.md`'s "Control assets" paragraph under
   "Marrow-authored derived artifacts" is the likely one — confirm rather than
   assume.

NON-GOALS:

- Do NOT change any code. This bird is documentation only; if the `typ/`
  shadowing turns out to deserve a warning at build time, that is a separate
  bird and a separate argument.
- Do NOT add a `rheo.toml` key to relocate either reserved path.
- Do NOT rewrite the `[spine] prelude` reference text wholesale. Two sentences
  added to what is already there.

VERIFY, all three:

1. `CLAUDE.md`'s prelude paragraph says the splice is per vertebra and advises
   keeping the file to imports and bindings.
2. `docs/contract.md` names `typ/` as reserved, names both paths in use, and
   says a project file at such a path is shadowed.
3. No document in `docs/` or `CLAUDE.md` still implies `.rheo/` is the only
   reserved prefix.