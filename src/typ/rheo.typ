#let lemmacount = counter("lemmas")
#let lemma(it) = block(inset: 8pt, [
  #lemmacount.step()
  #strong[Lemma #context lemmacount.display()]: #it
])

#let rheo_template(doc) = context {
  // Inject CSS and font links for HTML target
  if target() == "html" {
    // FIX: very hacky way to get the styles
    html.elem("script", attrs: (type: "text/javascript"))[
      var cssLink = document.createElement(\"link\");
      cssLink.href = \"style.css\";
      cssLink.type = \"text/css\";
      cssLink.rel = \"stylesheet\";
      document.head.appendChild(cssLink);
      var fontLink = document.createElement(\"link\");
      fontLink.href = \"https\:\/\/fonts.googleapis.com/css2?family=Inter:wght\@400;500;700&display=swap\";
      fontLink.rel = \"stylesheet\";
      document.head.appendChild(fontLink);
      console.log(\"CSS and font inserted.\");
    ]
  }

  doc
}
