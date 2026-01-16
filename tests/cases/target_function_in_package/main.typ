// @rheo:test
// @rheo:formats html,epub
// @rheo:description Tests target() in Typst Universe packages (documents a limitation)
//
// This test documents a known limitation with the target() polyfill for EPUB:
//
// - Local modules that call `target()` see our injected override ("epub")
// - Universe packages that call `std.target()` see the real Typst target ("html")
//
// Why this happens:
// 1. Rheo injects `#let target() = "epub"` at the top of all .typ files
// 2. This shadows the unqualified `target` name in each file's scope
// 3. BUT packages like bullseye explicitly call `std.target()` to get the
//    "real" target from Typst's standard library
// 4. EPUB compilation uses Typst's HTML export internally, so `std.target()`
//    returns "html" even when generating EPUB
//
// The bullseye package (lib.typ lines 13-16) does:
//   #let target() = {
//     if "target" in dictionary(std) { std.target() }  // <-- bypasses our override
//     else { "paged" }
//   }
//
// This is a fundamental limitation - we cannot override `std.target()`.
// The test captures the current behavior: packages see "html", local code sees "epub".

#import "@preview/bullseye:0.1.0": on-target

= Target Function in Package

== Using bullseye package

// Expected: "html" in both HTML and EPUB modes (bullseye calls std.target())
#context on-target(
  html: [Package sees: *html*],
  paged: [Package sees: *paged*],
)

== Direct comparison

// Expected: "html" for HTML, "epub" for EPUB (uses our injected override)
Main file target: #context [*#target()*]
