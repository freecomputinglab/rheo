#let template(current-page: none, doc) = {
  context if target() == "html" {
    html.elem("div", attrs: (class: "header"))[
      #image("img/header.svg")
    ]
    html.elem("hr")
  } else if target() == "paged" {
    image("img/header.svg")
  } else {}
  doc
}
