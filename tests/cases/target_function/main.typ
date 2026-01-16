// @rheo:test
// @rheo:formats html,pdf,epub
// @rheo:description Verifies sys.inputs.rheo-target returns correct format string

// Helper to get the rheo output format, with fallback to Typst's target()
#let rheo-target() = {
  if "rheo-target" in sys.inputs {
    sys.inputs.rheo-target
  } else {
    target()
  }
}

= Target Function Test

This test verifies that `sys.inputs.rheo-target` returns format-specific values.

#context {
  let format = rheo-target()
  [Current format: *#format*]
}

== Conditional Content

#context if rheo-target() == "html" {
  [HTML-specific content: This appears only in HTML output]
} else if rheo-target() == "pdf" or rheo-target() == "paged" {
  [PDF-specific content: This appears only in PDF output]
} else if rheo-target() == "epub" {
  [EPUB-specific content: This appears only in EPUB output]
} else {
  [Unknown format detected]
}
