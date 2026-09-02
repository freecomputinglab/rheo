# Unreleased — user-visible changes

## A package namespace can resolve straight from a directory on disk

`[packages.<ns>]` gained a third source alongside `repo` and `releases`: `path
= "../pkgs"` points a namespace at a directory holding `<name>/<version>/`
package trees, read in place. There is no git operation and no cache — the
directory the Typst compile reads IS the working tree, so an edit to the
package shows up on the very next build rather than needing a commit and a
bookmark move first.

This is for the loop of editing a package and the site that consumes it
together. Before, that loop went through `repo = "<local checkout>"`, which
works but still clones a fresh tree into `~/.cache/rheo/git/` for every commit
along the way. `path` skips all of that.

It is deliberately machine-local: a relative `path` resolves against
`rheo.toml`'s own directory, but the directory it names is still a line on
your own disk, not something committed for anyone else to build against.
rheo has no local-override config file (yet) to keep a `path` entry out of a
shared `rheo.toml` — flip it locally, and flip it back before committing.

`rheo watch` picks this up too: a `path`-backed package's directory is watched
the same as the project itself, so editing a `.typ` file inside it rebuilds
the consuming site within the usual debounce window — even when the package
declares no `[tool.rheo.*]` assets of its own, which is the common case for a
package that is pure Typst with no stylesheet or script.

# 0.6.2 — user-visible changes

## A package served from a repository ref contributes its marrow again

0.6.1 added `[packages.<ns>]`, letting a project resolve a namespace from a
repository ref. A package fetched that way lives at a path keyed by the resolved
commit, and two places still located packages by probing Typst's
`{namespace}/{name}/{version}` directory layout instead of asking the resolver —
so they found nothing.

One of them gathers the `.marrow.typ` a package ships. A package that mints
pages from its marrow therefore minted none of them, on a build that succeeded
and reported nothing: `@rookery/core` lost every per-idea page, and the only
symptom was a smaller site.

It was easy to miss locally, because a developer machine usually has the package
symlinked into the Typst cache for exactly that namespace — which makes the
probe succeed and hides the bug until the site is built somewhere clean, like a
deploy runner.

Projects consuming packages from a RELEASE were never affected: a release is
unpacked into the Typst cache, where the probe finds it.

# 0.6.0 — user-visible changes

What differs for someone using rheo. Items marked **[landed]** are already on
this branch.

GitHub release notes are auto-generated from merged PR titles (`docs/new-releases.md`).
This file is the prose version: what an upgrading project actually experiences.

## The upgrade is silent by default

Worth reading first, because it shapes how every removal below is experienced.

Unknown `rheo.toml` keys are swallowed into `#[serde(flatten)]` `extra` maps
(`crates/core/src/config/mod.rs:45`, `:90`, `:143`, `:221`). There is no
`deny_unknown_fields` anywhere in `crates/core/src/config/`, and nothing warns — the same
mechanism that already makes a retired `merge` key silent. A key that stops being read does
not become an error; it becomes a no-op.

The `rheo.toml` version check compounds it. A mismatch against the binary's version is a
*warning*, not an error (`crates/core/src/config/validation.rs:15-20`), and its text —
"rheo.toml version {} does not match rheo version {}. Consider updating your rheo.toml
version field." — does not mention `rheo migrate`. And `rheo migrate` is manual-only, dry-run
unless `--apply` (`crates/cli/src/lib.rs:197-211`).

So without further work, a 0.5.x project upgrading to 0.6.0 sees one vague version warning,
its feed silently stops being generated, and the command that would have explained this is
one nothing tells it to run.

`rheo-retired-key-warn-dou` closes that: one shared table of retired keys in
`crates/core/src/config/`, warned from at build time and read by `rheo migrate` (rather than
each keeping its own list), plus `rheo migrate` named in the version-mismatch warning.
`rheo-migrate-feed-dhe` writes the detailed per-key report, with file and line.

## Removed

Atom feed generation leaves rheo entirely. `crates/html/src/feed.rs` is deleted, not
deprecated — there is no compatibility flag and no one-release warning shim
(`rheo-drop-feed-bdo`).

| Surface | After 0.6.0 |
| --- | --- |
| `[html] feed_base_url`, `feed_author`, `feed_title` | Ignored. No `build/html/feed.xml` |
| `[[html.feed_include]]` | Ignored |
| `<link rel="alternate" type="application/atom+xml">` | No longer injected into any page's `<head>` |
| `#let rheo-feed-title` / `-updated` / `-exclude` | Ignored |
| `#let rheo-author` | Ignored **[landed]** — EPUB `dc:creator` now comes only from `#set document(author: ...)` |
| Any `#let rheo-<key>` | The whole harvested-variable convention is gone (`rheo-drop-vars-1ay`) |
| `sys.inputs.rheo-target` | Removed **[landed]** — use `target()`, or `sys.inputs.rheo-context.target` |

Feeds still exist, as the `@rheo/feeds` Typst package driven from a `.marrow.typ`. This is
a workflow change rather than a config rename: publication-level facts — feed title, author,
site base URL, which pages to include — stop being `rheo.toml` keys and become Typst. The
removal is gated on that package existing and reaching parity; shipping the deletion before
it would leave a release with no feed capability anywhere.

One quiet relaxation comes with dropping the variable convention: a top-level
`#let rheo-anything = (1, 2)` is currently a compile error (the RHS had to be a string or
boolean literal). Afterwards it is an ordinary Typst binding that rheo does not look at.

## Added

**Flat spine ordering** (`rheo-spine-include-df5`). A new `include` key on `[spine]` /
`[<format>.spine]` takes an ordered list of glob patterns and uses it as that spine's
definitive order — without the path prefix that `[[spine.section]]` forces on every child.
A file matched by no pattern is dropped, so `include` also removes the need for a parallel
`exclude`. Setting both `include` and `section` on one table is a validation error, and a
pattern matching nothing is an error. Reordering *within* a nested content directory is out
of scope for the first version.

```toml
[html.spine]
include = ["index.typ", "install.typ", "ideas.typ", "flights.typ"]
# /install.html stays flat; only the order changes
```

**Version negotiation for packages.** `rheo-context().rheo-version` (and
`sys.inputs.rheo-context.rheo-version`) carries the compiling binary's own semver, so a
package can enforce a floor and fail with its own message; on an older rheo the key is simply
absent, which is itself the signal (`rheo-rheo-version-key-n5k`). From the other direction, a
package declaring `[tool.rheo] min_version` in its `typst.toml` gets a clean build error
listing every offending package at once, instead of silently minting nothing
(`rheo-pkg-min-version-1fn`).

**Project-supplied `sys.inputs`** **[landed]** (`rheo-cli-input-flag-q12`,
`rheo-toml-inputs-table-rih`). A project can now seed arbitrary keys onto `sys.inputs`,
so a build script can parameterise a compile — Typst has no environment access, so this
was previously impossible under rheo, which forwarded only its own `rheo-context`. Two
sources: a `[inputs]` table in `rheo.toml` as the declared baseline, and a repeatable
`--input KEY=VALUE` on `compile`/`watch` that overrides it per key. Values are strings in
both, with no coercion. `rheo-context` is reserved and rejected from both.

```toml
[inputs]
rookery-exclude = "private"
```

```sh
rheo compile . --input rookery-include=private   # the dev build, same source tree
```

The point is what a package can now do with it: `sys.inputs` reads need no `#context`, so a
package can branch on a key *structurally* rather than rendering and hiding.
`@rheo/rookery` 0.6.0's `exclude-tags` is the first consumer — one rookery, a public build
with the `protected` notes genuinely absent (no page minted, no search index entry, no feed
item) and a dev build that keeps them. Pairs with `--config rheo.public.toml` for
per-variant input sets with no flags at all. Documented in `docs/contract.md`.

**Author-facing primitives** **[landed]**: `<rheo-content page="..." select="..."/>`
transclusion of compiled page HTML into bundle-emitted assets, `<rheo-head>` hoisting of a
wrapper's children into that page's `<head>`, and the reserved `.rheo/` control-asset prefix
(`.rheo/head.html` appends to every page's `<head>`). These are what make a feed, sitemap or
search index expressible in Typst instead of in a plugin crate.

## Silence becomes signal

- A malformed `<rheo-content .../>` placeholder is now a build error naming the asset,
  instead of surviving into your feed or sitemap as literal text. Two triggers: a missing
  `page` attribute, and a `>` character inside an attribute value
  (`rheo-transclude-strict-ofv`).
- `rheo watch` previews now match what `rheo compile` writes for pages using `<rheo-head>`.
  The dev server's in-memory path never hoisted, so the preview silently differed
  (`rheo-head-hoist-watch-mhp`).
- `<template>` elements keep their children through an HTML build. They are currently emptied
  with no error, which forced `@rheo/rookery-search` to ship `hidden` divs instead
  (`rheo-onp`).
- An authored label beginning `rheo-meta:` is a hard error naming the file and label
  **[landed]** — that prefix is reserved for metadata beacons.

## Document metadata now resolves through Typst [landed]

Per-vertebra `#set document(...)` values are read off the compiled bundle rather than scanned
out of source text before compilation. Consequences for authors:

- A title set via an imported `#show:` template, inside a code block, from a non-literal
  expression, or by several `#set document(...)` rules now resolves correctly. The old scan
  missed all of those.
- `#set document(date: datetime.today())` used to be rejected outright. It is now accepted
  and resolves to a real date — which means it **changes on every build**. Use a literal
  `datetime(year:, month:, day:)` for anything syndicated.
- Reading another vertebra's real title is possible: `(rheo-context().metadata-of)(handle)`,
  or `rheo-metadata(handle)` / `rheo-metadata-all()` from a bundle-root `.marrow.typ`. These
  require `#context` (they are backed by `query`), unlike the rest of `rheo-context()`.
- `rheo-context().spine` / `.spine-flat` titles remain path-derived only — they do not reflect
  an authored `#set document(title: ...)`.
- Combined PDF has no per-vertebra metadata by design; `rheo-metadata(handle)` returns `(:)`
  there.
- `rheo-context` is a function: call it as `rheo-context()`. `rheo migrate` rewrites the old
  bare binding.

See `docs/limitations.md` for the full account, including which values come back as Typst
content rather than strings.

## A marrow-minted page can be linked to by handle [landed]

A package that mints its own pages from a `.marrow.typ` could not be linked to by handle at
all. Typst attaches labels syntactically, so a computed `<ideas:my-note>` does not exist and
`#link(label("ideas:" + slug))` fails with *label `<ideas:my-note>` does not exist in the
document* before any show rule runs — only rheo, which synthesizes bundle source in Rust, can
mint a labelled anchor. Packages worked around it by computing a depth-relative href
themselves, which cannot be right in content that is replayed onto more than one page.

The per-`#document` link rule now also rewrites a link whose dest is the reserved string
`rheo-page:<handle>`, applying the same depth arithmetic it applies to a handle label
(`rheo-link-rule`, `crates/core/src/typ/rheo.typ`). Because the rule belongs to the enclosing
`#document`, it resolves afresh wherever the content is realized — so one stored body linking
to a minted page comes out correct on a root page, on a nested page, and on a minted page,
which no `#context` inside that body can achieve. `@rheo/rookery` 0.4.1 uses this, and the 72
dead author links it was shipping are gone.

There is no membership check: marrow runs after every `#document` is emitted, so a vertebra's
rule cannot close over the set of minted handles, and querying for it would reintroduce the
`#context` the rule exists without. The scheme is the assertion, and a `rheo-page:` link to a
page nothing mints goes dead silently rather than erroring. **A package using it must set
`[tool.rheo] min_version = "0.6.0"`** — an older rheo passes the dest through and the page
ships a literal `href="rheo-page:…"`.

## A vertebra handle now always wins a label collision [landed]

The `#show link:` rule that rewrites `#link(<handle>)` into a depth-relative href used to be
one bundle-global rule wrapped in `#context`, deciding whether a label named a vertebra by
running `query(it.dest)` and inspecting the first match. It is now applied per `#document`,
closed over that page's handle, and decides from `sys.inputs.rheo-context.spine-flat` — so it
runs no `query` and needs no `#context` at all (`rheo-link-rule`,
`crates/core/src/typ/rheo.typ`; `rheo-link-rule-static-mvl`).

`state("rheo-handle")` is still published per page — packages read it and it stays part of
the contract — the link rule simply no longer needs it.

**This changes output for one case, and fixes it.** `query(label)` returns matches in bundle
document order, so a project that attached a vertebra's handle to an element of its own
shadowed the handle whenever its vertebra happened to come first — coin-flip precedence that
silently produced broken links. In rheo's own `bundle_ref_cross_directory` fixture, a line of
doc prose reading `(e.g. <intro>)` claimed the `intro` handle, and three cross-references
landed on that line instead of the page — one of them resolving to the linking page itself.
All three are correct now. A label naming a vertebra always resolves to that vertebra's page;
if a project wants a label of its own with that name, rename the label.

It is worth saying what this does **not** do, because the shape of the old rule invites the
inference. It does not help a document that fails to converge. MEASURED across a rookery site
and four reductions of it: identical `did not converge` counts and identical output before and
after. Typst's five-iteration fixpoint cap is spent by whatever `query` a project or its
packages feed, and on that site it is a package-level bundle-wide query over `link` elements
whose result feeds the very pages it queries.

## No user-visible change

Also in this cut, with no effect on how rheo behaves: moving injected Typst out of Rust
`format!` strings into real `.typ` files (`rheo-typ-extraction-wna`), a key reference for
package authors (`rheo-package-contract-pki`), and documentation catch-up
(`rheo-epub-author-docs-5i3`, `rheo-claude-md-zb3`).

`rheo-ap3` — allowing a package's marrow to be spliced *before* the documents, so it can
theme a whole site — remains a design proposal. Nothing changes unless it is built, and then
only opt-in.
