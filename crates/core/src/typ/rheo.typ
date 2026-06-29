// Get the rheo output format, with fallback to Typst's target()
// Returns: "epub", "html", "pdf" when compiled with rheo
//          "html" or "paged" when compiled with vanilla Typst
#let rheo-target() = {
  if "rheo-target" in sys.inputs {
    sys.inputs.rheo-target
  } else {
    target()
  }
}

// Check if we're compiling for a specific rheo format
// Works in vanilla Typst (returns false when rheo-target not set)
#let is-rheo-epub() = "rheo-target" in sys.inputs and sys.inputs.rheo-target == "epub"
#let is-rheo-html() = "rheo-target" in sys.inputs and sys.inputs.rheo-target == "html"
#let is-rheo-pdf() = "rheo-target" in sys.inputs and sys.inputs.rheo-target == "pdf"

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

// For #link(<handle>)[text] cross-vertebra links, strip the redundant fragment
// (#handle) from the URL — the handle anchor is always at the top of the page,
// so "llm.html" and "llm.html#llm" navigate to the same place. PDF is left
// alone because all vertebrae share one file (internal anchors stay intact).
#show link: it => context {
  if type(it.dest) == label {
    let matches = query(it.dest)
    if matches.len() > 0 {
      let elem = matches.first()
      if elem.func() == figure and elem.kind == "rheo-handle" {
        let fmt = rheo-target()
        let ext = if fmt == "epub" { ".xhtml" } else if fmt == "html" { ".html" } else { none }
        if ext != none {
          let handle = repr(it.dest).slice(1, -1)
          return link(handle + ext, it.body)
        }
      }
    }
  }
  it
}
