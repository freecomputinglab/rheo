#let slide(body) = {
  context if target() == "html" {
    html.elem("section", body)
  } else {
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
    html.elem("div", attrs: (class: "reveal"),
      html.elem("div", attrs: (class: "slides"), doc)
    )
  } else {
    doc
  }
}
