// The rheo output format is surfaced through Typst's own `target()`: rheo
// injects a `target()` polyfill (into every file) that returns
// "epub"/"html"/"paged" from `sys.inputs.rheo-context.target`, falling back to
// `std.target()` under vanilla Typst. Detect the format with `target() == "epub"`
// etc. There is no `rheo-target()` helper — use `target()` directly.

#let rheo_template(doc) = context {
  doc
}

// Libertinus Serif is embedded in Typst, so we can rely on it always being available. Any subsequent font declarations will override this.
#set text(font: "Libertinus Serif")

// Synthesized cross-vertebra handle anchors use labeled #figure elements.
// #metadata and bare document labels are not referenceable in Typst 0.15 —
// #figure is the only element that supports cross-document @handle resolution.
// Hide them so they render nothing, and fix @ref display to the vertebra title.
#show figure.where(kind: "rheo-handle"): none
#show ref: it => {
  if it.element != none and it.element.func() == figure and it.element.kind == "rheo-handle" {
    link(it.target, it.element.body)
  } else {
    it
  }
}

// The vertebrae this bundle is building, by handle, for the link rule below.
//
// `spine-flat` is the same list `#rheo-metadata-all` maps and a package's own
// "is this a vertebra" test reads (`@rheo/rookery`'s `_is-vertebra`). It is
// exactly the set of vertebrae the bundle source synthesizes a `rheo-handle`
// figure for — one `BundleAnchor` per vertebra, plus `<handle.typ>` escape
// aliases carrying the same handle — so membership here answers the same
// question a `query(it.dest)` for such a figure used to, without introspection.
//
// `sys.inputs` needs no `#context`, which is the whole point: see the rule.
#let _rheo-handles() = {
  let c = sys.inputs.at("rheo-context", default: none)
  if c == none { return () }
  c.at("spine-flat", default: ()).map(v => v.at("handle", default: ""))
}

// Rewrite #link(<handle>)[text] cross-vertebra links into per-format hrefs.
// Only links whose target names a vertebra are touched; authored labels pass
// through. The href is depth-relative to the current page, whose handle arrives
// as this factory's argument, and the target handle's `:` separators become `/`,
// prefixed with one `../` per level the current page is nested. The output
// extension comes from `sys.inputs.rheo-context.ext`; when it is absent (PDF)
// the rule is a no-op and native link handling applies. The redundant `#handle`
// fragment is dropped — the anchor sits at the top of the target page.
//
// A FACTORY APPLIED PER #DOCUMENT, not one global `#show link:` rule wrapped in
// `#context` — which is what this replaced. Both questions the old rule asked
// introspection are answerable statically: whether the dest names a vertebra
// (`_rheo-handles` above, read from `sys.inputs`), and how deep the current page
// is (`handle`, which the bundle source WRITES — it already emits
// `#rheo-page-init("<handle>")` at the top of every #document, and now emits
// `#show link: rheo-link-rule("<handle>")` beside it). So the rule is a plain
// function: no `#context`, and no `query()` per link in the project.
//
// WHY IT MATTERS. Deciding membership by querying for the synthesized anchor made
// precedence depend on DOCUMENT ORDER: `query(label)` returns matches in bundle
// order and the old rule inspected only `.first()`, so a project that attached
// the same label to an element of its own shadowed the handle whenever its
// vertebra happened to come first. That is a coin flip, and it was silently
// emitting broken links — including in rheo's own
// `cases/bundle_ref_cross_directory` fixture, where a line of doc prose reading
// `(e.g. <intro>)` claimed the `intro` handle and three cross-references landed
// on that bullet instead of the page, one of them resolving to the linking page
// itself. Reading `spine-flat` makes a handle win every time, deterministically.
//
// WHAT IT DOES NOT BUY: convergence. MEASURED (rheo 0.6.0 against this change, on
// maths.ohrg.org and four reductions of it): identical `did not converge` /
// `did not stabilize` counts either way, and identical output. The old rule's
// `#context` cost no relayout iteration that something else was not already
// spending. Typst caps its fixpoint at 5 (`MAX_ITERS`,
// typst-library/src/introspection/convergence.rs); a project that goes over is
// spending those rounds elsewhere. On the site above it is a PACKAGE-level
// bundle-wide `query` over `link` elements whose own result feeds the pages it
// queries (`@rheo/rookery`'s `_page-links`) — which is why replacing one
// `link(…)` with an otherwise identical `html.elem("a", …)`, invisible to that
// query, converged. Do not reach for this rule when chasing a convergence
// warning; find which `query` is being fed.
//
// `state("rheo-handle")` is still published by `rheo-page-init` below: packages
// read it (`@rheo/rookery`'s `urls.typ`, `outline.typ`, `idea.typ`), so it is
// part of the contract even though this rule no longer needs it.
// The URL scheme a marrow-minted page is addressed by. See the rule below.
#let _RHEO_PAGE_SCHEME = "rheo-page:"

#let rheo-link-rule(handle) = it => {
  let ext = sys.inputs.at("rheo-context", default: (:)).at("ext", default: none)
  if ext == none { return it }
  // A MARROW-MINTED page, addressed by handle through a reserved URL scheme
  // rather than by label.
  //
  // Typst 0.15 cannot attach a COMPUTED label: `<name>` is syntax, and there is
  // no anchor form taking `label(..)` as a value, so only rheo — which synthesizes
  // bundle source text in Rust — can mint a labelled anchor. A package minting its
  // own pages from a marrow therefore cannot make `#link(label(..))` to them work
  // at all: the label does not exist, and Typst raises "label `<x>` does not exist
  // in the document" before any show rule runs. A string dest is the only channel
  // such a package has.
  //
  // Rewriting it HERE, in the per-#document rule, is what makes it correct on every
  // page the link lands on — including pages it was never written on. A `#context`
  // in the linking content cannot do this. MEASURED: a `context` reading
  // `state("rheo-handle")` inside a note body that is later replayed resolves ONCE,
  // and every copy carries that single answer — which is exactly `@rheo/rookery`'s
  // 72-dead-link bug, where one author link came out `../ideas/x.html` on four
  // pages at two different depths. A show rule installed by the enclosing
  // #document applies afresh at each realization instead, so one stored body comes
  // out right at depth 0, at depth 1, and on a minted page. Verified with a single
  // note transcluded onto three pages.
  //
  // NO MEMBERSHIP TEST, deliberately. The set of minted handles is not knowable
  // statically here — marrow runs after every #document is emitted, so a spine
  // vertebra's rule cannot close over it — and querying for it would put back the
  // `#context` this rule exists in order not to have. The scheme IS the assertion.
  // The tradeoff: a link to a page nothing mints yields a dead href silently rather
  // than the "label does not exist" error the label form would have given.
  if type(it.dest) == str and it.dest.starts-with(_RHEO_PAGE_SCHEME) {
    let target = it.dest.slice(_RHEO_PAGE_SCHEME.len())
    let depth = handle.split(":").len() - 1
    let prefix = if depth == 0 { "./" } else { range(depth).map(x => "../").join() }
    return link(prefix + target.replace(":", "/") + "." + ext, it.body)
  }
  if type(it.dest) != label { return it }
  let target-handle = repr(it.dest).slice(1, -1)
  // The `<handle.typ>` escape alias resolves to the same vertebra output as the
  // canonical `<handle>`, so drop a trailing `.typ` before building the href —
  // otherwise it would point at a nonexistent `…/x.typ.html`.
  if target-handle.ends-with(".typ") {
    target-handle = target-handle.slice(0, -4)
  }
  if target-handle not in _rheo-handles() { return it }
  let depth = handle.split(":").len() - 1
  let prefix = if depth == 0 { "./" } else { range(depth).map(x => "../").join() }
  link(prefix + target-handle.replace(":", "/") + "." + ext, it.body)
}

// Per-document init hook, called once at the top of each #document block by the
// bundle source (crates/core/src/reticulate/bundle_source.rs). Publishes this
// page's handle to state("rheo-handle") for any PACKAGE that needs it (the link
// rule above takes the handle as an argument instead, and reads no state),
// and — for per-page output (html/epub, where rheo-context carries an `ext`) —
// resets the footnote counter so each page numbers its footnotes from 1 —
// unless the format's `reset_footnotes` toggle is set false. The combined PDF
// has no `ext`, so its footnotes stay continuous across the book regardless.
#let rheo-page-init(handle) = {
  state("rheo-handle").update(handle)
  let per-page = sys.inputs.rheo-context.at("ext", default: none) != none
  let reset = sys.inputs.rheo-context.at("reset-footnotes", default: true)
  if per-page and reset {
    counter(footnote).update(0)
  }
}

// A page minted at the bundle root (via a `.marrow.typ` contribution) is built
// by calling Typst's own `document()` directly, so it skips `rheo-page-init`
// and inherits whatever `state("rheo-handle")` and footnote count the last
// spine document left behind. `rheo-document` wraps `document()` and
// `rheo-page-init` together so a contributed page gets the same per-document
// init a spine vertebra gets for free. Bare `document()` keeps working — this
// is a convenience, not a requirement — but a marrow contribution SHOULD use
// `rheo-document` and pass a handle unique across the project. `format`
// defaults to "html": both per-page plugins (html and epub) compile
// HTML-shaped documents (`FormatPlugin::typst_format` defaults to
// `TypstFormat::Html` and neither plugin overrides it, crates/core/src/plugins/mod.rs:297).
// A future per-page plugin that is not HTML-shaped would need this default revisited.
//
// It installs `rheo-link-rule` too, and must: the rule is applied per #document
// by the bundle source, so a page minted here — which the bundle source knows
// nothing about — would otherwise have no rule at all and emit every
// cross-vertebra `#link(<handle>)` as a bare Typst label link. The `#show` sits
// at the top of the same markup block as `#body`, which is what scopes it to the
// rest of that block. `handle` is this page's own, so the depth is right even
// though a minted page is usually a directory down.
#let rheo-document(path, handle: "", title: [], format: "html", body) = {
  document(path, format: format, title: title)[
    #rheo-page-init(handle)
    #show link: rheo-link-rule(handle)
    #body
  ]
}
