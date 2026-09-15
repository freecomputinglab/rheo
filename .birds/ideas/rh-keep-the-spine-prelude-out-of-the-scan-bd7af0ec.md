---
id: rh-keep-the-spine-prelude-out-of-the-scan-bd7af0ec
short-id: b
title: Keep the spine prelude out of the scan
priority: 4
labels:
- fix-prelude-scan-exclude
deps:
- blocked-by:rh-mark-synthesized-vertebrae-in-spine-flat-607892ff
closed: true
---
Touches: crates/core/src/reticulate/spine.rs, crates/core/src/build.rs, CLAUDE.md, changelog.md

The documented `[spine] prelude` setup publishes two junk pages.

`[spine] prelude` names a path relative to `content_dir`, so the prelude file
necessarily lives INSIDE the scanned content tree. Nothing excludes it from the
spine scan, so it is compiled as a vertebra of its own — and with `auto_index`
on (the default) its directory also gets a synthesized landing page.

MEASURED, with exactly the config CLAUDE.md documents
(`prelude = "_lib/prelude.typ"`, one `content/index.typ`, one
`content/chapters/one.typ`, one `content/_lib/prelude.typ`):

    build/html/index.html
    build/html/chapters.html      <- synthesized, correct
    build/html/_lib.html          <- synthesized index for the prelude's own dir
    build/html/_lib/prelude.html  <- the prelude file itself, as a page

Both junk pages land in `spine-flat`, so they reach every nav, feed and sitemap
built on it. Marrow does not have this problem because
`SpineScan::run_with_marrow` (`crates/core/src/reticulate/spine.rs:57-73`)
pushes every reserved marrow filename onto `exclude_patterns` before building
the exclude set. The prelude needs the same treatment.

Steps:

1. `crates/core/src/reticulate/spine.rs:57` — give `run_with_marrow` a new
   parameter `prelude: Option<&str>`, after `marrow_file`. It takes the
   `[spine] prelude` value as written in `rheo.toml`, which is already relative
   to `content_dir` — exactly what the exclude globs are matched against.
2. At `crates/core/src/reticulate/spine.rs:68-72`, where `exclude_patterns` is
   assembled, push `globset::escape(prelude)` when `prelude` is `Some`. Escape
   it for the same reason the marrow filename is escaped: it is a literal path
   the user wrote, not a glob of their choosing.
3. `crates/core/src/reticulate/spine.rs:44` — `SpineScan::run` delegates to
   `run_with_marrow`; pass `None` from it, keeping `run` the no-prelude
   convenience the existing tests call.
4. `crates/core/src/build.rs:610-617` — the only non-test caller of
   `run_with_marrow`. Pass `spine.prelude.as_deref()`. The merged `Spine` value
   is already in scope there as `spine`, and `spine.prelude` is read a few lines
   below at `crates/core/src/build.rs:628`.
5. Excluding the prelude path is NOT on its own enough to remove
   `build/html/_lib.html`. That page disappears only because, once the prelude
   is excluded, `_lib/` has no remaining `.typ` children and
   `scan_subdir` drops the whole node at
   `crates/core/src/reticulate/spine/scan.rs:205` (`children.is_empty()`).
   Confirm that in VERIFY rather than assuming it. If `_lib/` holds other `.typ`
   library files, a synthesized index over them is correct behaviour and out of
   scope here.
6. Fix the example in `CLAUDE.md` (the "`[spine] prelude`" paragraph under
   "Spine configuration") while you are here. It sets
   `prelude = "_lib/prelude.typ"`, which resolves under `content_dir` to
   `content/_lib/prelude.typ`, and then shows that file importing
   `"/_lib/template.typ"`, which is root-absolute and so resolves to
   `<project root>/_lib/template.typ`. Those are two different directories.
   Make the example internally consistent: either the import becomes
   `"/content/_lib/template.typ"`, or the library file moves to the project
   root. `changelog.md`'s "A project can inject Typst into every vertebra with
   `[spine] prelude`" section repeats the same example and needs the same fix.

NON-GOALS:

- Do NOT change where `prelude` is resolved from. `content_dir`-relative is
  consistent with `exclude` and every other spine path key.
- Do NOT exclude the prelude's own imports. Only the file `[spine] prelude`
  names is excluded; a library module it imports is not a vertebra unless it
  sits in the content tree under its own name, which is the author's choice.
- Do NOT touch `auto_index`'s own behaviour. A separate bird covers the
  different bug where a directory whose landing file was EXCLUDED by
  `[spine] exclude` still gets a synthesized index at the excluded path.

VERIFY, all four:

1. `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
2. A scratch project with `content/index.typ`, `content/chapters/one.typ`,
   `content/_lib/prelude.typ` and `prelude = "_lib/prelude.typ"`, built with
   `cargo run -- compile <path> --html`, produces EXACTLY `index.html`,
   `chapters.html`, `chapters/one.html` and `rheo-default.css` under
   `build/html/` — no `_lib.html`, no `_lib/prelude.html`.
3. The prelude still takes effect: with `#let tag = rheo-context().handle` in it
   and `#tag` in `content/index.typ`, `build/html/index.html` renders `index`.
4. A unit test beside the existing scan tests in
   `crates/core/src/reticulate/spine.rs` asserts that a prelude path passed to
   `run_with_marrow` never appears in the returned `scan.files`.