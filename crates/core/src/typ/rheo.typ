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

// Rewrite #link(<handle>)[text] cross-vertebra links into per-format hrefs.
// Only links whose target is a rheo-synthesized `rheo-handle` figure are
// touched; authored labels pass through. The href is depth-relative to the
// current page: the current page's handle is read from `state("rheo-handle")`
// (published per-#document by the bundle source, since a show rule here in the
// shared main-file template cannot see a vertebra's local `rheo-context`), and
// the target handle's `:` separators become `/`, prefixed with one `../` per
// level the current page is nested. The output extension comes from
// `sys.inputs.rheo-context.ext`; when it is absent (PDF) the rule is a no-op and
// native link handling applies. The redundant `#handle` fragment is dropped —
// the anchor sits at the top of the target page.
#show link: it => context {
  let ext = sys.inputs.rheo-context.at("ext", default: none)
  if ext == none { return it }
  if type(it.dest) == label {
    let matches = query(it.dest)
    if matches.len() > 0 {
      let elem = matches.first()
      if elem.func() == figure and elem.kind == "rheo-handle" {
        let target-handle = repr(it.dest).slice(1, -1)
        // The `<handle.typ>` escape alias resolves to the same vertebra output
        // as the canonical `<handle>`, so drop a trailing `.typ` before building
        // the href — otherwise it would point at a nonexistent `…/x.typ.html`.
        if target-handle.ends-with(".typ") {
          target-handle = target-handle.slice(0, -4)
        }
        let here-handle = state("rheo-handle").get()
        let depth = here-handle.split(":").len() - 1
        let prefix = if depth == 0 { "./" } else { range(depth).map(x => "../").join() }
        return link(prefix + target-handle.replace(":", "/") + "." + ext, it.body)
      }
    }
  }
  it
}

// Per-document init hook, called once at the top of each #document block by the
// bundle source (crates/core/src/reticulate/bundle_source.rs). Publishes this
// page's handle to state("rheo-handle") for the cross-vertebra link rule above,
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
#let rheo-document(path, handle: "", title: [], format: "html", body) = {
  document(path, format: format, title: title)[
    #rheo-page-init(handle)
    #body
  ]
}
