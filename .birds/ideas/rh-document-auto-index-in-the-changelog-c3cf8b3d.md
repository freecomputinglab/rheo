---
id: rh-document-auto-index-in-the-changelog-c3cf8b3d
short-id: c3
title: Document auto_index in the changelog
priority: 3
labels:
- doc-auto-index-changelog
deps:
- blocked-by:rh-keep-the-spine-prelude-out-of-the-scan-bd7af0ec
closed: false
---
Touches: changelog.md

`auto_index` has no changelog entry, and it is the only default-ON change on the
branch.

`changelog.md`'s "Unreleased — user-visible changes" section documents the two
OPT-IN features at length — "Marrow position is a filename" and "A project can
inject Typst into every vertebra with `[spine] prelude`" — and says nothing at
all about `auto_index`, which is `true` by default and changes what every
existing project builds.

What it changes, on upgrade, with no config edit:

- A directory with children and no `index.typ`/`<dirname>.typ` used to become a
  non-clickable group node with a prettified title. It now gets a synthesized
  landing page instead.
- That page is a real vertebra: a new file in `build/`, a new entry in
  `spine-flat`, and therefore a new row in any feed, sitemap or nav a project
  or package derives from `spine-flat`.
- A directory with NO children after exclusion is now dropped entirely, in both
  modes.

Steps:

1. Add a section to `changelog.md` under "Unreleased — user-visible changes",
   in the voice and shape of the two sections already there. Cover:
   - the default (`auto_index = true` under `[spine]`) and what it replaces;
   - that the synthesized page's whole body is a call to `rheo-index()`, the
     default directory-index renderer;
   - that a project restyles every directory index at once by binding its own
     `#let rheo-index() = ...` in `[spine] prelude`, which is spliced after
     rheo's own and so shadows it — rather than hand-writing an `index.typ`
     per directory;
   - that `auto_index = false` restores the previous behaviour exactly;
   - that it falls back field-by-field like every other spine key, so
     `[pdf.spine] auto_index = false` turns it off for the combined PDF alone.
2. State plainly, in its own sentence, that this one is default-on and so
   changes existing output. The other two sections do not need such a sentence
   and this one does; that asymmetry is the point of the entry.
3. Put the section BEFORE the two existing ones. The list reads newest-first and
   `auto_index` is the most recent of the three.

NON-GOALS:

- Do NOT change `auto_index`'s default, or any behaviour at all. This bird is
  documentation only.
- Do NOT restate the whole `[spine] auto_index` reference text from `CLAUDE.md`.
  The changelog says what CHANGED for an existing project; `CLAUDE.md` is the
  reference.
- Do NOT document the `rheo-index()` helper's signature here. That belongs in
  `docs/contract.md`, which already carries it under "Directory-index helper".

VERIFY, all three:

1. The new section names `auto_index`, `[spine]`, `rheo-index()`,
   `[spine] prelude` and `auto_index = false`, and says in its own sentence
   that the default is on.
2. `changelog.md` renders as well-formed markdown, with the new section at the
   same heading level as "A project can inject Typst into every vertebra with
   `[spine] prelude`".
3. `cargo test` still passes — nothing in the tree reads `changelog.md`, so this
   is a check that the bird touched nothing else.