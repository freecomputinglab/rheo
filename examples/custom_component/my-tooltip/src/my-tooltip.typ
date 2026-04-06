#let html-element(body, name: "div", hideOffTarget: false, attrs: (:)) = context {
    if target() == "html" or target() == "epub" {
      html.elem(name, attrs: attrs, body)
    } else {
      if hideOffTarget {
        []
      } else {
        body
      }      
    }
}

#let tooltip(placement: "top-end", body) = html-element(body, name: "my-tooltip", attrs: (placement: placement))

#let tooltip-modal(body) = html-element(body, name: "my-tooltip-modal", hideOffTarget: true)
#let tooltip-content(body) = html-element(body, name: "my-tooltip-content")
