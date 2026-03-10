// @rheo:test
// @rheo:formats html,pdf,epub
// @rheo:description Verifies target() function returns correct format string

= Target Function Test

This test verifies that the `target()` function returns format-specific values.

#context {
  let format = target()
  [Current format: *#format*]
}

== Conditional Content

#context if target() == "html" {
  [HTML-specific content: This appears only in HTML output]
} else if target() == "pdf" or target() == "paged" {
  [PDF-specific content: This appears only in PDF output]
} else if target() == "epub" {
  [EPUB-specific content: This appears only in EPUB output]
} else {
  [Unknown format detected]
}
