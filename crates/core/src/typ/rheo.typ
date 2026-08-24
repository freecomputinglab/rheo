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
// Exactly the set the bundle source synthesizes a `rheo-handle` figure for, so
// membership answers what a `query` for that figure used to — read off
// `sys.inputs`, which needs no `#context`.
#let _rheo-handles() = {
  let c = sys.inputs.at("rheo-context", default: none)
  if c == none { return () }
  c.at("spine-flat", default: ()).map(v => v.at("handle", default: ""))
}

// Rewrites #link(<handle>) cross-vertebra links into per-format hrefs, as a
// factory applied per #document rather than one global rule — so it needs no
// #context: both handle membership and the current page's depth are answerable
// from its argument plus sys.inputs. See docs/link-rule.md, which records why,
// and what this does NOT fix (convergence warnings).

// The URL scheme a marrow-minted page is addressed by, since Typst cannot
// attach a computed label for such a page to link to. See docs/link-rule.md.
#let _RHEO_PAGE_SCHEME = "rheo-page:"

#let rheo-link-rule(handle) = it => {
  let ext = sys.inputs.at("rheo-context", default: (:)).at("ext", default: none)
  if ext == none { return it }
  // No membership test here, deliberately: the set of minted handles is not
  // knowable statically, and querying for it would reinstate the #context this
  // rule exists to avoid. A link to a page nothing mints yields a dead href.
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

// Wraps document() with the per-document init a spine vertebra gets for free,
// and installs rheo-link-rule — which it must: the bundle source applies that
// rule per #document and knows nothing about a page minted here. The #show sits
// at the top of the same markup block as #body, which is what scopes it to the
// rest of that block. See docs/link-rule.md.
#let rheo-document(path, handle: "", title: [], format: "html", body) = {
  document(path, format: format, title: title)[
    #rheo-page-init(handle)
    #show link: rheo-link-rule(handle)
    #body
  ]
}
