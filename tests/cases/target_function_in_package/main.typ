// @rheo:test
// @rheo:formats html,epub
// @rheo:description Tests target() polyfill vs packages using std.target()
//
// This test demonstrates rheo's target() polyfill:
//
// - User code using target() sees "epub" for EPUB output (via polyfill)
// - Universe packages that call std.target() see "html" (the underlying compile target)
//
// Why packages see "html":
// - EPUB compilation uses Typst's HTML export internally
// - Packages like bullseye explicitly call std.target() to get the "real" target
// - This is expected behavior - std.target() returns the underlying format
//
// For package authors:
// - Packages can adopt rheo's pattern to detect rheo output format
// - The pattern: `if "rheo-target" in sys.inputs { sys.inputs.rheo-target } else { target() }`
// - This provides graceful degradation when compiled outside rheo

#import "@preview/bullseye:0.1.0": on-target

= Target Function in Package

== Using bullseye package

// Expected: "html" in both HTML and EPUB modes (bullseye calls std.target())
#context on-target(
  html: [Package sees: *html*],
  paged: [Package sees: *paged*],
)

== Using target()

// Expected: "html" for HTML, "epub" for EPUB (uses polyfill)
Main file target: #context [*#target()*]
