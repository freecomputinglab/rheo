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
        let here-handle = state("rheo-handle").get()
        let depth = here-handle.split(":").len() - 1
        let prefix = if depth == 0 { "./" } else { range(depth).map(x => "../").join() }
        return link(prefix + target-handle.replace(":", "/") + "." + ext, it.body)
      }
    }
  }
  it
}
