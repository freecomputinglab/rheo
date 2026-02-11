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

#show: template.with(current-page: "index")

// To show content only in HTML, we can use Typst's `target` function:
#context if target() == "html" [
- #link("./about.typ")[About] 
]

== The philosophy of Rheo
Rheo is a prefix or combining form in English that originates from the Greek word _rheos_ (ῥέος), meaning flow, stream, or current.
Rheo flows Typst documents into a number of concurrent output formats in PDF, HTML, and EPUB.
But other meanings lurk beneath the surface of this basic idea.
Sarah Pourciau has argued that the oceanic is a deep-rooted metaphor in computing, as all computation at some level seeks solid space in a sea of digital noise @pourciauDigitalOcean2022.
From Alan Turing's partial solution to David Hilbert's _Entscheidungsproblem_ in the universal machine, to Claude Shannon's information theory, to Leslie Lamport's ordering of events in a distributed system, the key issue at hand is how to carve out clarity from uncertainty and confusion.
Writing has played a magisterial role in calming the storm of imprecise thought.
Long before computation arrived on the scene, the written word has served as the steward of reason, in the Western world and beyond, from Mesopotamian cunieform to Twitter.
_Nota bene_ ('Take note'): that writing can also herald chaos and confusion doesn't invalidate its capacity for spreading sensibility.

== Bibliography
#bibliography("references.bib", title: none, style: "chicago-author-date")


