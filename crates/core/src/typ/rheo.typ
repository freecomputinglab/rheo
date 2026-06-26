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

// Synthesized cross-vertebra handle anchors are invisible figure elements.
// Hide them and fix @ref display to show the vertebra title, not a figure counter.
#show figure.where(kind: "rheo-handle"): none
#show ref: it => {
  if it.element != none and it.element.func() == figure and it.element.kind == "rheo-handle" {
    link(it.target, it.element.body)
  } else {
    it
  }
}
