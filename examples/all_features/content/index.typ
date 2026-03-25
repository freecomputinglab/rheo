#import "shared.typ": template

// HTML pages
#document("index.html", title: "Home")[
  #show: template.with(current-page: "index")
  #context if target() == "html" [
  - #link("about.html")[About]
  ]
  #title()
  - #link(<blog>)[Go to blog]
]

#document("blog.html", title: "Blog")[
  #show: template.with(current-page: "blog")
  Welcome to my blog!

  ...

  This blog also exists as a #link(<blog-pdf>)[single PDF].
] <blog>

// PDF output
#document("blog.pdf", title: "Blog")[
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
] <blog-pdf>

// About page
#document("about.html", title: "About")[
  #include "about.typ"
]

#asset("favicon.svg", read("img/favicon.svg", encoding: none))
