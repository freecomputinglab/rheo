// Library file that won't be compiled (excluded via rheo.toml)

#let highlight(content) = {
  block(
    fill: rgb("#f0f0f0"),
    inset: 8pt,
    radius: 4pt,
    content
  )
}

#let chapter_header(title) = {
  text(24pt, weight: "bold")[#title]
  v(12pt)
}
