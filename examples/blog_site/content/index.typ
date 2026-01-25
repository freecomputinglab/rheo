// @rheo:test
// @rheo:formats html,pdf
// @rheo:description Main blog index page with post listings

#let div(_class: "", ..body) = html.elem("div", attrs: (class: _class), ..body)
#let br() = html.elem("br")
#let hr() = html.elem("hr")
#let ul(_class: "", ..body) = html.elem("ul", attrs: (class: _class), ..body)
#let li(_class: "", ..body) = html.elem("li", attrs: (class: _class), ..body)

#let template(doc) = {
  doc
  context if target() == "html" or target() == "epub" {
    div[
      #br()
      #hr()
      #ul[
        #li[#link("./index.typ")[Home]]
        #li[#link("https://lachlankermode.com")[Learn more] about me]
        #li[#link("https://ohrg.org")[Read other musings]]
      ]
    ]
  }
}

#show: template

= Screening the subject

_Screening the subject_ is a blog that analyses content on both the big and small screen in reasonable detail, i.e. episode-by-episode or scene-by-scene. 
Contact us at #link("mailto:info@ohrg.org")[info\@ohrg.org] for enquiries.

// Be alerted of new content by subscribing to the #link("https://screening-the-subject.ohrg.org/feed.xml")[RSS feed].

- #link("./severance-ep-1.typ")[Severance, s1/e1]
- #link("./severance-ep-2.typ")[Severance, s1/e2]
- #link("./severance-ep-3.typ")[Severance, s1/e3]
