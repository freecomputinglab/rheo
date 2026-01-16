// @rheo:test
// @rheo:formats html,epub
// @rheo:description Tests sys.inputs.rheo-target vs packages using std.target()
//
// This test demonstrates the sys.inputs.rheo-target mechanism for format detection:
//
// - User code using sys.inputs.rheo-target correctly sees "epub" for EPUB output
// - Universe packages that call std.target() see "html" (the underlying compile target)
//
// Why packages see "html":
// - EPUB compilation uses Typst's HTML export internally
// - Packages like bullseye explicitly call std.target() to get the "real" target
// - This is expected behavior - std.target() returns the underlying format
//
// Solution for packages:
// - Packages can adopt sys.inputs.rheo-target to detect rheo output format
// - The pattern: `if "rheo-target" in sys.inputs { sys.inputs.rheo-target } else { target() }`
// - This provides graceful degradation when compiled outside rheo

// Helper to get the rheo output format, with fallback to Typst's target()
#let rheo-target() = {
  if "rheo-target" in sys.inputs {
    sys.inputs.rheo-target
  } else {
    target()
  }
}

#import "@preview/bullseye:0.1.0": on-target

= Target Function in Package

== Using bullseye package

// Expected: "html" in both HTML and EPUB modes (bullseye calls std.target())
#context on-target(
  html: [Package sees: *html*],
  paged: [Package sees: *paged*],
)

== Using sys.inputs.rheo-target

// Expected: "html" for HTML, "epub" for EPUB (uses sys.inputs)
Main file rheo-target: #context [*#rheo-target()*]
