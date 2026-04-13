#let slide(body) = {
  [#metadata(body) <slide>]
  context if target() != "html" {
    box(
      fill: rgb("#ff4444"),
      stroke: 2pt + rgb("#ff0000"),
      inset: (x: 8pt, y: 4pt),
      radius: 4pt,
      text(fill: white, weight: "bold", size: 0.9em)[SLIDE],
    )
  }
}

#let template(doc) = {
  context if target() == "html" {
    for s in query(<slide>) {
      s.value
    }
  } else {
    doc
  }
}


