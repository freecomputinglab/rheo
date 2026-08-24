# The cross-vertebra link rule

`typ/rheo.typ` defines `rheo-link-rule`, which rewrites `#link(<handle>)`
cross-vertebra links into per-format hrefs. The bundle source applies it per
`#document` (`reticulate/bundle_source.rs`), closed over that page's own handle.
This document records why it has that shape, since the code cannot say so
without becoming unreadable.

## Why a per-document factory rather than one global show rule

The rule replaced a single bundle-global `#show link: it => context { … }`.

Both questions the old rule needed `#context` for are answerable statically:

- **Does the dest name a vertebra?** `_rheo-handles()` reads `spine-flat`
  straight off `sys.inputs`, which needs no context.
- **How deep is the current page?** Its handle, which the bundle source already
  writes into the document (`#rheo-page-init("<handle>")`), so the rule can take
  it as an argument.

So the rule is a plain function: no `#context`, and no `query()` per link.

**Determinism is the real reason.** Deciding handle membership by querying for
the synthesized anchor made precedence depend on **document order**:
`query(label)` returns matches in bundle order and the old rule inspected only
`.first()`, so a project that attached the same label to an element of its own
shadowed the handle whenever its vertebra happened to come first. That is a coin
flip, and it silently emitted broken links — including in rheo's own
`cases/bundle_ref_cross_directory` fixture, where a line of doc prose reading
`(e.g. <intro>)` claimed the `intro` handle and three cross-references landed on
that bullet instead of the page, one of them resolving to the linking page
itself. Reading `spine-flat` makes a handle win every time.

## What the change does not buy: convergence

MEASURED (rheo 0.6.0 against this change, on maths.ohrg.org and four reductions
of it): identical `did not converge` / `did not stabilize` counts either way,
and identical output. The old rule's `#context` cost no relayout iteration that
something else was not already spending.

Typst caps its fixpoint at 5 (`MAX_ITERS`,
`typst-library/src/introspection/convergence.rs`); a project that goes over is
spending those rounds elsewhere. On the site above it is a package-level
bundle-wide `query` over `link` elements whose own result feeds the pages it
queries (`@rheo/rookery`'s `_page-links`) — which is why replacing one `link(…)`
with an otherwise identical `html.elem("a", …)`, invisible to that query,
converged.

Do not reach for this rule when chasing a convergence warning; find which
`query` is being fed.

## The `rheo-page:` URL scheme

A page minted from a `.marrow.typ` is addressed by handle through a reserved URL
scheme rather than by label.

Typst 0.15 cannot attach a **computed** label: `<name>` is syntax, and there is
no anchor form taking a `label(..)` value, so only rheo — which synthesizes
bundle source text in Rust — can mint a labelled anchor. A package minting its
own pages from a marrow therefore cannot make `#link(label(..))` to them work at
all: the label does not exist, and Typst raises "label `<x>` does not exist in
the document" before any show rule runs. A string dest is the only channel such
a package has.

Rewriting it in the per-`#document` rule is what makes it correct on every page
the link lands on — including pages it was never written on. A `#context` in the
linking content cannot do this. MEASURED: a `context` reading
`state("rheo-handle")` inside a note body that is later replayed resolves
**once**, and every copy carries that single answer — which is exactly
`@rheo/rookery`'s 72-dead-link bug, where one author link came out
`../ideas/x.html` on four pages at two different depths. A show rule installed by
the enclosing `#document` applies afresh at each realization instead, so one
stored body comes out right at depth 0, at depth 1, and on a minted page.
Verified with a single note transcluded onto three pages.

**No membership test, deliberately.** The set of minted handles is not knowable
statically in the rule — marrow runs after every `#document` is emitted, so a
spine vertebra's rule cannot close over it — and querying for it would put back
the `#context` this rule exists in order not to have. The scheme *is* the
assertion. The tradeoff: a link to a page nothing mints yields a dead href
silently, rather than the "label does not exist" error the label form would have
given.

## `rheo-document`

A marrow-minted page calls Typst's own `document()`, so it skips
`rheo-page-init` and inherits whatever `state("rheo-handle")` and footnote count
the last spine document left behind. `rheo-document` wraps both together, and
also installs `rheo-link-rule` — which it must, since the bundle source knows
nothing about a minted page and would otherwise leave it with no rule at all,
emitting every cross-vertebra `#link(<handle>)` as a bare Typst label link.

`format` defaults to `"html"`: both per-page plugins (html and epub) compile
HTML-shaped documents, since `FormatPlugin::typst_format` defaults to
`TypstFormat::Html` and neither overrides it. A future per-page plugin that is
not HTML-shaped would need that default revisited.

`state("rheo-handle")` is still published by `rheo-page-init` even though the
link rule no longer reads it: packages read it (`@rheo/rookery`'s `urls.typ`,
`outline.typ`, `idea.typ`), so it is part of the contract.
