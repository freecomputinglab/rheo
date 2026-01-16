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

#let lemmacount = counter("lemmas")
#let lemma(it) = block(inset: 8pt, [
  #lemmacount.step()
  #strong[Lemma #context lemmacount.display()]: #it
])

#let rheo_template(doc) = context {
  doc
}
