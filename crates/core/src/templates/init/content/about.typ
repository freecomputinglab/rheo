#import "index.typ": template

#show: template.with(current-page: "about")

#context if target() == "html" [
  - #link(<index>)[Home]
]

= About

This project was built with #link("https://rheo.ohrg.org/")[Rheo].
