// Module that uses sys.inputs.rheo-target for format detection
// Tests whether sys.inputs propagates to imported files

// Helper to get the rheo output format, with fallback to Typst's target()
#let rheo-target() = {
  if "rheo-target" in sys.inputs {
    sys.inputs.rheo-target
  } else {
    target()
  }
}

#let get_format() = {
  rheo-target()
}

#let format_specific_content() = context {
  let fmt = rheo-target()
  if fmt == "epub" {
    [Module: EPUB]
  } else if fmt == "html" {
    [Module: HTML]
  } else if fmt == "pdf" or fmt == "paged" {
    [Module: PDF]
  } else {
    [Module: Unknown (#fmt)]
  }
}
