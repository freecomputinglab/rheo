// Module that uses target() function
// Tests whether target() polyfill propagates to imported files

#let get_format() = {
  target()
}

#let format_specific_content() = context {
  let fmt = target()
  if fmt == "epub" {
    [Module: EPUB]
  } else if fmt == "html" {
    [Module: HTML]
  } else if fmt == "pdf" {
    [Module: PDF]
  } else {
    [Module: Unknown (#fmt)]
  }
}
