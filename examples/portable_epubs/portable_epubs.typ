#let custom-element(name) = context {
  if target() == "html" {
    html.elem(name, attrs: attrs, body)
  } else {
    block(children)
  }
}


#let header = custom-element("header")
#let authors = custom-element("doc-authors")
#let author = custom-element("doc-author")
#let author-name = custom-element("doc-author-affiliation")
#let author-affiliation = custom-element("doc-author-affiliation")
#let publication-date = custom-element("doc-publication-date")
#let abstract = custom-element("doc-abstract")
#let section = custom-element("section")
#let definition = custom-element("dfn-container")
#let defined-word(id, body) = custom-element("dfn")(attrs: (id: id), body)
#let callout(body) = custom-element("div")(attrs: (class: "callout"), body)
#let def-link(id, body) = custom-element("a")(attrs: (href: "#" + id, data-target: "dfn"), body)
#let code-description = custom-element("code-description")
#let code-step = custom-element("code-step")
#let pre = custom-element("pre")
#let span = custom-element("span")
#let code-def(id, body) = span(attrs: (id: id), body)
#let code-description(body) = {
  let verbatimize(items, indent) = {
    items
      .filter(child => child.func() == enum.item)
      .map(item => {
        if item.body.has("children") {
          let children = item.body.children
          let item-idx = children.position(child => child.func() == enum.item)
          if item-idx != none {
            let prefix = children.slice(0, item-idx)
            (span(indent), prefix.join(), "\n", verbatimize(children.slice(item-idx), indent + "  ")).join()
          } else {
            (span(indent), children.join()).join()
          }
        } else {
          (span(indent), item.body).join()
        }
      })
      .join("\n")
  }
  pre(verbatimize(body.children, ""))
}
#let code-steps(body) = {
  body
    .children
    .map(child => {
      if child.has("body") {
        code-step(child.body)
      } else {
        child
      }
    })
    .join()
}

#set document(title: "Portable EPUBs")
#title()


#header[
  #authors[
    #author[
      #author-name[Will Crichton]
      #author-affiliation[Brown University]
    ]
  ]
  #publication-date[January 25, 2024]
  #abstract[
    Despite decades of advances in document rendering technology, most of the world's documents are stuck in the 1990s due to the limitations of PDF.
    Yet, modern document formats like HTML have yet to provide a competitive alternative to PDF. This post explores what prevents HTML documents from being portable, and I propose a way forward based on the EPUB format. To demonstrate my ideas, this post is presented using a prototype EPUB reading system.
  ]
]

= The Good and Bad of PDF <good-and-bad-pdf>

PDF is the de facto file format for reading and sharing digital documents like papers, textbooks, and flyers. People use the PDF format for several reasons:

- *PDFs are self-contained.* A PDF is a single file that contains all the images, fonts, and other data needed to render it. It's easy to pass around a PDF. A PDF is unlikely to be missing some critical dependency on your computer.

- *PDFs are rendered consistently.* A PDF specifies precisely how it should be rendered, so a PDF author can be confident that a reader will see the same document under any conditions.

- *PDFs are stable over time.* PDFs from decades ago still render the same today. PDFs have a #link("https://www.iso.org/standard/75839.html")[relatively stable standard]. PDFs cannot be easily edited.

Yet, in the 32 years since the initial release of PDF, a lot has changed. People print out documents less and less. People use phones, tablets, and e-readers to read digital documents. The internet happened; web browsers now provide a platform for rendering rich documents. These changes have laid bare the limitations of PDF:

- *PDFs cannot easily adapt to different screen sizes.* Most PDFs are designed to mimic 8.5x11" paper (or worse, #link("https://en.wikipedia.org/wiki/PDF#/media/File:Seitengroesse_PDF_7.png")[145,161 km#super[2]]). These PDFs are readable on a computer monitor, but they are less readable on a tablet, and far less readable on a phone.

- *PDFs cannot be easily understood by programs.* A plain PDF is just a scattered sequence of lines and characters. For accessibility, screen readers #link("https://dl.acm.org/doi/10.1145/2851581.2892588")[may not know] which order to read through the text. For data extraction, scraping tables out of a PDF is an #link("https://openaccess.thecvf.com/content/CVPR2022/html/Smock_PubTables-1M_Towards_Comprehensive_Table_Extraction_From_Unstructured_Documents_CVPR_2022_paper.html")[open] #link("https://ieeexplore.ieee.org/document/5277546")[area] of #link("https://www.sciencedirect.com/science/article/pii/S030645731830205X?casa_token=jNV6uhUNLs0AAAAA:p6EMBh3X54Ulv9Ghtca1WPR2iL6fkhpVOVsbXj7zzinRYVa72HUGQb6VBOIPFdFoHwjEGDSB")[research].

- *PDFs cannot easily express interaction.* PDFs were primarily designed as static documents that cannot react to user input beyond filling in forms.

These pros and cons can be traced back to one key fact: the PDF representation of a document is fundamentally unstructured. A PDF consists of commands like:

#figure[
  ```
  Move the cursor to the right by 0.5 inches.
  Set the current font color to black.
  Draw the text "Hello World" at the current position.
  ```
]

PDF commands are unstructured because a document's organization is only clear to a person looking at the rendered document, and not clear from the commands themselves. Reflowing, accessibility, data extraction, and interaction _all_ rely on programmatically understanding the structure of a document. Hence, these aspects are not easy to integrate with PDFs.

This raises the question: *how can we design digital documents with the benefits of PDFs but without the limitations?*

= Can't We Just Fix PDF? <cant-fix-pdf>

A simple answer is to improve the PDF format. After all, we already have billions of PDFs — why reinvent the wheel?

The designers of PDF are well aware of its limitations. I carefully hedged each bullet with "easily", because PDF does make it _possible_ to overcome each limitation, at least partially. PDFs can be annotated with their #link("https://opensource.adobe.com/dc-acrobat-sdk-docs/library/pdfmark/pdfmark_Logical.html")[logical structure] to create a #link("https://www.washington.edu/accesstech/documents/tagged-pdf/")[tagged PDF]. Most PDF exporters will not add tags automatically — the simplest option is to use Adobe's subscription-only #link("https://www.adobe.com/acrobat/acrobat-pro.html")[Acrobat Pro], which provides an "Automatically tag PDF" action. For example, here is #link("https://arxiv.org/abs/2310.04368")[a recent paper of mine] with added tags:

#figure(
  image("img/tags.jpg"),
  caption: [A LaTeX-generated paper with automatically added tags.],
)

If you squint, you can see that the logical structure closely resembles the HTML document model. The document has sections, headings, paragraphs, and links. Adobe characterizes the logical structure as an accessibility feature, but it has other benefits. You may be surprised to know that Adobe Acrobat allows you to reflow tagged PDFs at different screen sizes. You may be unsurprised to know that reflowing does not always work well. For example:

#figure[
  #figure(
    image("img/before-resize.jpg"),
    caption: [A section of the paper in its default fixed layout. Note that the second paragraph is wrapped around the code snippet.],
  )

  #figure(
    image("img/after-resize.jpg"),
    caption: [The same section of the paper after reflowing to a smaller width. Note that the code is now interleaved with the second paragraph.],
  )
]

In theory, these issues could be fixed. If the world's PDF exporters could be modified to include logical structure. If Adobe's reflowing algorithm could be improved to fix its edge cases. If the reflowing algorithm could be specified, and if Adobe were willing to release it publicly, and if it were implemented in each PDF viewer. And that doesn't even cover interaction! So in practice, I don't think we can just fix the PDF format, at least within a reasonable time frame.

= The Good and Bad of HTML <good-and-bad-html>

In the meantime, we already have a structured document format which can be flexibly and interactively rendered: HTML (and CSS and Javascript, but here just collectively referred to as HTML). The HTML format provides almost exactly the inverse advantages and disadvantages of PDF.

- *HTML can more easily adapt to different screen sizes.* Over the last 20 years, web developers and browser vendors have created a wide array of techniques for #link("https://developer.mozilla.org/en-US/docs/Learn/CSS/CSS_layout/Responsive_Design")[responsive design].
- *HTML can be more easily understood by a program.* HTML provides both an inherent structure plus #link("https://developer.mozilla.org/en-US/docs/Web/Accessibility/ARIA")[additional attributes] to support accessibility tools.
- *HTML can more easily express interaction.* People have used HTML to produce amazing interactive documents that would be impossible in PDF. Think: #link("https://distill.pub/")[Distill.pub], #link("https://explorabl.es/")[Explorable Explanations], #link("https://ciechanow.ski/")[Bartosz Ciechanowski], and #link("http://worrydream.com/")[Bret Victor], just to name a few.

Again, these advantages are hedged with "more easily". One can easily produce a convoluted or inaccessible HTML document. But on balance, these aspects are more true than not compared to PDF. However, HTML is lacking where PDF shines:

- *HTML is not self-contained.* HTML files may contain URL references to external files that may be hosted on a server. One can rarely download an HTML file and have it render correctly without an internet connection.
- *HTML is not always rendered consistently.* HTML's dynamic layout means that an author may not see the same document as a reader. Moreover, HTML layout is not fully specified, so browsers may differ in their implementation.
- *HTML is not fully stable over time.* Browsers try to maintain backwards compatibility (#link("https://www.spacejam.com/1996/")[come on and slam!]), but the HTML format is still evolving. The #link("https://html.spec.whatwg.org/")[HTML standard] is a "living standard" due to the rapidly changing needs and feature sets of modern browsers.

So I've been thinking: *how can we design HTML documents to gain the benefits of PDFs without losing the key strengths of HTML?* The rest of this document will present some early prototypes and tentative proposals in this direction.

= Self-Contained HTML with EPUB <epub-intro>

First, how can we make HTML documents self-contained? This is an old problem with many potential solutions. #link("https://en.wikipedia.org/wiki/WARC_(file_format)")[WARC], #link("https://en.wikipedia.org/wiki/Webarchive")[webarchive], and #link("https://en.wikipedia.org/wiki/MHTML")[MHTML] are all file formats designed to contain all the resources needed to render a web page. But these formats are more designed for snapshotting an existing website, rather than serving as a single source of truth for a web document. From my research, the most sensible format for this purpose is EPUB.

EPUB is a "distribution and interchange format for digital publications and documents", per the #link("https://www.w3.org/TR/epub-overview-33/#")[EPUB 3 Overview]. Reductively, an EPUB is a ZIP archive of web files: HTML, CSS, JS, and assets like images and fonts. On a technical level, what distinguishes EPUB from archival formats is that EPUB includes well-specified files that describe metadata about a document. On a social level, EPUB appears to be the HTML publication format with the most adoption and momentum in 2024, compared to moribund formats like #link("https://en.wikipedia.org/wiki/Mobipocket")[Mobi].

The #link("https://www.w3.org/TR/epub-33")[EPUB spec] has all the gory details, but to give you a rough sense, a sample EPUB might have the following file structure:

#figure[
  ```
  sample.epub
  ├── META-INF
  │   └── container.xml
  └── EPUB
      ├── package.opf
      ├── nav.xhtml
      ├── chapter1.xhtml
      ├── chapter2.xhtml
      └── img
          └── sample.jpg
  ```
]

An EPUB contains #link("https://www.w3.org/TR/epub-33/#sec-contentdocs")[content documents] (like `chapter1.xhtml` and `chapter2.xhtml`) which contain the core HTML content. Content documents can contain relative links to assets in the EPUB, like `img/sample.jpg`. The #link("https://www.w3.org/TR/epub-33/#sec-nav")[navigation document] (`nav.xhtml`) provides a table of contents, and the #link("https://www.w3.org/TR/epub-33/#sec-package-doc")[package document] (`package.opf`) provides metadata about the document. These files collectively define one "rendition" of the whole document, and the #link("https://www.w3.org/TR/epub-33/#sec-container-metainf-container.xml")[container file] (`container.xml`) points to each rendition contained in the EPUB.

The EPUB format optimizes for machine-readable content and metadata. HTML content is required to be in XML format (hence, #strong[X]HTML). Document metadata like the title and author is provided in structured form in the package document. The navigation document has a carefully prescribed tag structure so the TOC can be consistently extracted.

Overall, EPUB's structured format makes it a solid candidate for a single-file HTML document container. However, EPUB is not a silver bullet. EPUB is quite permissive in what kinds of content can be put into a content document.

For example, a major issue for self-containment is that EPUB content can embed external assets. A content document can legally include an image or font file whose `src` is a URL to a hosted server. This is not hypothetical, either; as of the time of writing, Google Doc's EPUB exporter will emit CSS that will `@include` external Google Fonts files. The problem is that such an EPUB will not render correctly without an internet connection, nor will it render correctly if Google changes the URLs of its font files.

#par[#definition[
  Hence, I will propose a new format which I call a #defined-word("portable-epub")[*portable EPUB*], which is an EPUB with additional requirements and recommendations to improve PDF-like portability. The first requirement is:
]]

#callout[
  *Local asset requirement:* All assets (like images, scripts, and fonts) embedded in a content document of a portable EPUB must refer to local files included in the EPUB. Hyperlinks to external files are permissible.
]

= Consistency vs. Flexibility in Rendering <consistency-vs-flexibility>

There is a fundamental tension between consistency and flexibility in document rendering. A PDF is consistent because it is designed to render in one way: one layout, one choice of fonts, one choice of colors, one pagination, and so on. Consistency is desirable because an author can be confident that their document will look good for a reader (or at least, not look bad). Consistency has subtler benefits --- because a PDF is chunked into a consistent set of pages, a passage can be cited by referring to the page containing the passage.

On the other hand, flexibility is desirable because people want to read documents under different conditions. Device conditions include screen size (from phone to monitor) and screen capabilities (E-ink vs. LCD). Some readers may prefer larger fonts or higher contrasts for visibility, alternative color schemes for color blindness, or alternative font faces for #link("https://opendyslexic.org/")[dyslexia]. Sufficiently flexible documents can even permit readers to select a level of detail appropriate for their background (#link("https://tomasp.net/coeffects/")[here's an example]).

Finding a balance between consistency and flexibility is arguably the most fundamental design challenge in attempting to replace PDF with EPUB. To navigate this trade-off, we first need to talk about #defined-word("reading-system")[EPUB reading systems], or the tools that render an EPUB for human consumption. To get a sense of variation between reading systems, I tried rendering this post as an EPUB (without any styling, just HTML) on four systems: #link("https://calibre-ebook.com/")[Calibre], #link("https://www.adobe.com/solutions/ebook/digital-editions.html")[Adobe Digital Editions], #link("https://www.apple.com/apple-books/")[Apple Books], and #link("https://www.amazon.com/dp/B09SWW583J")[Amazon Kindle]. This is how the first page looks on each system (omitting Calibre because it looked the same as Adobe Digital Editions):

#figure[
  #figure(
    image("img/adobe-digital-edition.jpg"),
    caption: [Adobe Digital Editions],
  )

  #figure(
    image("img/apple-books.jpg"),
    caption: [Apple Books],
  )

  #figure(
    image("img/kindle.jpg"),
    caption: [Amazon Kindle],
  )
]

Calibre and Adobe Digital Editions both render the document in a plain web view, as if you opened the HTML file directly in the browser. Apple Books applies some styling, using the #link("https://en.wikipedia.org/wiki/New_York_(2019_typeface)")[New York] font by default and changing link decorations. Amazon Kindle increases the line height and also uses my Kindle's globally-configured default font, #link("https://en.wikipedia.org/wiki/Bookerly")[Bookerly].

As you can see, an EPUB may look quite different on different reading systems. The variation displayed above seems reasonable to me. But how different is _too_ different? For instance, I was recently reading #link("https://press.uchicago.edu/ucp/books/book/distributed/H/bo70558916.html")[_A History of Writing_] on my Kindle. Here's an example of how a figure in the book renders on the Kindle:

#figure(
  image("img/history-of-writing-kindle.jpg"),
  caption: [A figure in the EPUB version of _A History of Writing_ on my Kindle],
)

When I read this page, I thought, "wow, this looks like crap." The figure is way too small (although you can long-press the image and zoom), and the position of the figure seems nonsensical. I found a PDF version online, and indeed the PDF's figure has a proper size in the right location:

#figure(
  image("img/history-of-writing-pdf.jpg"),
  caption: [A figure in the PDF version of _A History of Writing_ on my Mac],
)

This is not a fully fair comparison, but it nonetheless exemplifies an author's reasonable concern today with EPUB: _what if it makes my document looks like crap?_

= Principles for Consistent EPUB Rendering <rendering-principles>

I think the core solution for consistently rendering EPUBs comes down to this:

+ The document format (i.e., #def-link("portable-epub")[portable EPUB]) needs to establish a subset of HTML (call it "portable HTML") which could represent most, but not all, documents.
+ Reading systems need to guarantee that a document within the subset will always look reasonable under all reading conditions.
+ If a document uses features outside this subset, then the document author is responsible for ensuring the readability of the document.

If someone wants to write a document such as this post, then that person need not be a frontend web developer to feel confident that their document will render reasonably. Conversely, if someone wants to stuff the entire Facebook interface into an EPUB, then fine, but it's on them to ensure the document is responsive.

For instance, one simple version of portable HTML could be described by this grammar:

#figure[
  ```
  Document ::= <article> Block* </article>
  Block    ::= <p> Inline* </p> | <figure> Block* </figure>
  Inline   ::= text | <strong> Inline* </strong>
  ```
]

The EPUB spec already defines a comparable subset for #link("https://www.w3.org/TR/epub-33/#sec-nav-def-model")[navigation documents].
I am essentially proposing to extend this idea for content documents, but as a soft constraint rather than a hard constraint. Finding the right subset of HTML will take some experimentation, so I can only gesture toward the broad solution here.

#callout[
  *Portable HTML rendering requirement:* if a document only uses features in the portable HTML subset, then a #def-link("portable-epub")[portable EPUB] reading system must guarantee that the document will render reasonably.
]

#callout[
  *Portable HTML generation principle:* when possible, systems that generate #def-link("portable-epub")[portable EPUB] should output portable HTML.
]

A related challenge is to define when a particular rendering is "good" or "reasonable", so one could evaluate either a document or a reading system on its conformance to spec. For instance, if document content is accidentally rendered in an inaccesible location off-screen, then that would be a bad rendering. A more aggressive definition might say that any rendering which violates accessibility guidelines is a bad rendering. Again, finding the right standard for rendering quality will take some experimentation.

If an author is particularly concerned about providing a single "canonical" rendering of their document, one fallback option is to provide a #link("https://www.w3.org/TR/epub-33/#sec-fixed-layouts")[fixed-layout rendition]. The EPUB format permits a rendition to specify that it should be rendered in fixed viewport size and optionally a fixed pagination. A fixed-layout rendition could then manually position all content on the page, similar to a PDF. Of course, this loses the flexibility of a reflowable rendition. But an EPUB could in theory provide #link("https://www.w3.org/TR/epub-multi-rend-11/")[multiple renditions], offering users the choice of whichever best suits their reading conditions and aesthetic preferences.

#callout[
  *Fixed-layout fallback principle:* systems that generate #def-link("portable-epub")[portable EPUB] can consider providing both a reflowable and fixed-layout rendition of a document.
]

It's possible that the reading system, the document author, and the reader can each express preferences about how a document should render. If these preferences are conflicting, then the renderer should generally prioritize the reader over the author, and the author over the reading system. This is an ideal use case for the "cascading" aspect of CSS:

#callout[
  *Cascading styles principle:* both documents and reading systems should express stylistic preferences (such as font face, font size, and document width) as CSS styles which can be overriden (e.g., do not use `!important`). The reading system should load the CSS rules such that the priority order is reading system styles < document styles < reader styles.
]

= A Lighter EPUB Reading System <lighter-reading-system>

The act of working with PDFs is relatively fluid. I can download a PDF, quickly open it in a PDF reading system like #link("https://en.wikipedia.org/wiki/Preview_(macOS)")[Preview], and keep or discard the PDF as needed. But EPUB reading systems feel comparatively clunky. Loading an EPUB into Apple Books or Calibre will import the EPUB into the application's library, which both copies and potentially decompresses the file. Loading an EPUB on a Kindle requires waiting several minutes for the #link("https://www.amazon.com/sendtokindle")[Send to Kindle] service to complete.

Worse, EPUB reading systems often don't give you appropriate control over rendering an EPUB. For example, to emulate the experience of reading a book, most reading systems will chunk an EPUB into pages. A reader cannot scroll the document but rather "turn" the page, meaning textually-adjacent content can be split up between pages. Whether a document is paginated or scrolled should be a reader's choice, but 3/4 reading systems I tested would only permit pagination (Calibre being the exception).

Therefore I decided to build a lighter EPUB reading system, #link("https://github.com/nota-lang/bene/")[Bene]. You're using it right now. This document is an EPUB — you can download it by clicking the button in the top-right corner. The styling and icons are mostly borrowed from #link("https://github.com/mozilla/pdf.js")[pdf.js]. Bene is implemented in #link("https://tauri.app/")[Tauri], so it can work as both a desktop app and a browser app. Please appreciate this picture of Bene running as a desktop app:

#figure(
  image("img/bene.png"),
  caption: [The Bene reading system running as a desktop app. Wow! It works!],
)

Bene is designed to make opening and reading an EPUB feel fast and non-committal. The app is much quicker to open on my Macbook (\<1sec) than other desktop apps. It decompresses files on-the-fly so no additional disk space is used. The backend is implemented in Rust and compiled to Wasm for the browser version.

The general design goal of Bene is to embody my ideals for a #def-link("portable-epub")[portable EPUB] reader. That is, a utilitarian interface into an EPUB that satisfies my additional requirements for portability. Bene allows you to configure document rendering by changing the font size (try the +/- buttons in the top bar) and the viewer width (if you're on desktop, move your mouse over the right edge of the document, and drag the handle). Long-term, I want Bene to also provide richer document interactions than a standard EPUB reader, which means we must discuss scripting.

= Defensively Scripting EPUBs <defensive-scripting>

To some people, the idea of code in their documents is unappealing. Last time one of my #link("https://nota-lang.org/")[document-related projects] was posted to Hacker News, the #link("https://news.ycombinator.com/item?id=37951616")[top comment] was complaining about dynamic documents. The sentiment is understandable — concerns include:

- *Bad code:* your document shouldn't crash or glitch due to a failure in a script.
- *Bad browsers:* your document shouldn't fail to render when a browser updates.
- *Bad actors:* a malicious document shouldn't be able to pwn your computer.
- *Bad interfaces:* a script shouldn't cause your document to become unreadable.

Yet, document scripting provides many opportunities for improving how we communicate information. For one example, if you haven't yet, try hovering your mouse over any instance of the term portable EPUB (or long press it on a touch screen). You should see a tooltip appear with the term's definition. The goal of these tooltips is to simplify reading a document that contains a lot of specialized notation or terminology. If you forget a definition, you can quickly look it up without having to jump around.

The key design challenge is how to permit useful scripting behaviors while limiting the downsides of scripting. One strategy is as follows:

#callout[
  *Structure over scripts principle:* documents should prefer structural annotations over scripts where possible. Documents should rely on reading systems to utilize structure where possible.
]

As an example of this principle, consider how the portable EPUB definition and references are expressed in this document:

#figure[
  #figure(
    ```html
    <p><dfn-container>Hence, I will propose a new format which I call a <dfn id="portable-epub">portable EPUB</dfn>, which is an EPUB with additional requirements and recommendations to improve PDF-like portability.</dfn-container> The first requirement is:</p>
    ```,
    caption: [Creating a definition],
  )

  #figure(
    ```html
    For one example, if you haven't yet, try hovering your mouse over any instance of the term <a href="#portable-epub" data-target="dfn">portable EPUB</a> (or long press it on a touch screen).
    ```,
    caption: [Referencing a definition],
  )
]

The definition uses the #link("https://developer.mozilla.org/en-US/docs/Web/HTML/Element/dfn")[`<dfn>`] element wrapped in a custom `<dfn-container>` element to indicate the scope of the definition. The reference to the definition uses a standard anchor with an addition `data-target` attribute to emphasize that a definition is being linked. The document itself does not provide a script. The Bene reading system automatically detects these annotations and provides the tooltip interaction.

= Encapsulating Scripts with Web Components <web-components>

But what if a document wants to provide an interactive component that isn't natively supported by the reading system? For instance, I have recently been working with *The Rust Programming Language*, a textbook that explains the different features of Rust. It contains a lot of passages #link("https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html#shadowing")[like this one:]

#figure[
  ```rust
  let x = 5;
      let x = x + 1;
      {
          let x = x * 2;
          println!("The value of x in the inner scope is: {x}");
      }
      println!("The value of x is: {x}");
  }
  ```

  This program first binds `x` to a value of `5`. Then it creates a new variable `x` by repeating `let x =`, taking the original value and adding `1` so the value of `x` is then `6`. Then, within an inner scope created with the curly brackets, the third `let` statement also shadows `x` and creates a new variable, multiplying the previous value by `2` to give `x` a value of `12`. When that scope is over, the inner shadowing ends and `x` returns to being `6`. When we run this program, it will output the following:
]

A challenge in reading this passage is finding the correspondences between the prose and the code. An interactive code reading component can help you track those correspondences, like this (try mousing-over or clicking-on each sentence):

//  <figure>
//       <code-description>
//         <pre><code>fn main() {
//     let <span id="code-1">x</span> = <span id="code-2">5</span>;
//     <span id="code-4">let <span id="code-3">x</span> =</span> <span id="code-15">x</span> <span id="code-16">+</span> <span id="code-5">1</span>;
//     <span id="code-18"><span id="code-7">{</span>
//         <span id="code-8">let</span> <span id="code-9">x</span> = <span id="code-10">x</span> <span id="code-17">*</span> <span id="code-11">2</span>;
//         println!("The value of x in the inner scope is: {x}");
//     <span id="code-13">}</span></span>
//     println!("The value of x is: {<span id="code-14">x</span>}");
// }</code></pre>
//         <p>
//           <code-step>This program first binds <a href="#code-1"><code>x</code></a> to a value of <a href="#code-2"><code>5</code></a>.</code-step>
//           <code-step>Then it creates a new variable <a href="#code-3"><code>x</code></a> by repeating <a href="#code-4"><code>let x =</code></a>,</code-step>
//           <code-step>taking <a href="#code-15">the original value</a> and <a href="#code-16">adding</a> <a href="#code-5">1</a>
//           so the value of <a href="#code-3"><code>x</code></a> is then 6.</code-step>
//           <code-step>Then, within an <a href="#code-18">inner scope</a> created with the <a href="#code-7">curly</a> <a href="#code-13">brackets</a>,</code-step>
//           <code-step>the third <a href="#code-8"><code>let</code></a> statement also shadows <a href="#code-3"><code>x</code></a> and creates
//           <a href="#code-9">a new variable</a>,</code-step>
//           <code-step><a href="#code-17">multiplying</a> <a href="#code-10">the previous value</a> by <a href="#code-11">2</a>
//           to give <a href="#code-9"><code>x</code></a> a value of 12.</code-step>
//           <code-step>When <a href="#code-18">that scope</a> <a href="#code-13">is over</a>, <a href="#code-9">the inner shadowing</a> ends and <a href="#code-14"><code>x</code></a> returns to being 6.</code-step>
//         </p>
//       </code-description>
//       </figure>

#figure[
  #code-description[
    + fn main() {
      + let #code-def("code-1")[x] = #code-def("code-1")[5];
      + #code-def("code-4")[let #code-def("code-3")[x] =] #code-def("code-15")[x] #code-def("code-16")[+] #code-def("code-5")[1];
      + #code-def("code-7")[{]
        + #code-def("code-8")[let] #code-def("code-9")[x] = #code-def("code-10")[x] #code-def("code-17")[\*] #code-def("code-11")[2];
        + println!("The value of x in the inner scope is: {x}");
      + #code-def("code-13")[}]
      + println!("The value of x is: {#code-def("code-14")[x]}");
    + }
  ]

  #par[
    #code-steps[
      + This program first binds #link("#code-1")[`x`] to a value of #link("#code-2")[`5`].
      + Then it creates a new variable #link("#code-3")[`x`] by repeating #link("#code-4")[`let x =`],
      + taking #link("#code-15")[the original value] and #link("#code-16")[adding] #link("#code-5")[1] so the value of #link("#code-3")[`x`] is then 6.
      + Then, within an #link("#code-18")[inner scope] created with the #link("#code-7")[curly] #link("#code-13")[brackets],
      + the third #link("#code-8")[`let`] statement also shadows #link("#code-3")[`x`] and creates #link("#code-9")[a new variable],
      + #link("#code-17")[multiplying] #link("#code-10")[the previous value] by #link("#code-11")[2] to give #link("#code-9")[`x`] a value of 12.
      + When #link("#code-7")[that scope] #link("#code-13")[is over], #link("#code-9")[the inner shadowing] ends and #link("#code-14")[`x`] returns to being 6.
    ]
  ]
]

The interactive code description component is used as follows:

#figure[
  ```html
  <code-description>
    <pre><code>fn main() {
      let <span id="code-1">x</span> = <span id="code-2">5</span>;
      <!-- rest of the code... -->
  }</code></pre>
    <p>
      <code-step>This program first binds <a href="#code-1"><code>x</code></a> to a value of <a href="#code-2"><code>5</code></a>.</code-step>
      <!-- rest of the prose... -->
    </p>
  </code-description>
  ```
]

Again, the document content contains no actual script. It contains a custom element `<code-description>`, and it contains a series of annotations as spans and anchors. The `<code-description>` element is implemented as a #link("https://developer.mozilla.org/en-US/docs/Web/API/Web_components")[web component].

Web components are a programming model for writing encapsulated interactive fragments of HTML, CSS, and Javascript. Web components are one of many ways to write componentized HTML, such as #link("https://react.dev/")[React], #link("https://www.solidjs.com/")[Solid], #link("https://svelte.dev/")[Svelte], and #link("https://angular.io/")[Angular]. I see web components as the most suitable as a framework for portable EPUBs because:

- *Web components are a standardized technology.* Its key features like #link("https://html.spec.whatwg.org/multipage/custom-elements.html#custom-elements")[custom elements] (for specifying the behavior of novel elements) and #link("https://dom.spec.whatwg.org/#shadow-trees")[shadow trees] (for encapsulating a custom element from the rest of the document) are part of the official HTML and DOM specifications. This improves the likelihood that future browsers will maintain backwards compatibility with web components written today.
- *Web components are designed for tight encapusulation.* The shadow tree mechanism ensures that styling applied within a custom component cannot accidentally affect other components on the page.
- *Web components have a decent ecosystem to leverage.* As far as I can tell, web components are primarily used by Google, which has created notable frameworks like #link("https://lit.dev")[Lit].
- *Web components provide a clear fallback mechanism.* If a renderer does not support Javascript, or if a renderer loses the ability to render web components, then an HTML renderer will simply ignore custom tags and render their contents.

Thus, I propose one principle and one requirement:

#callout[
  *Encapsulated scripts principle:* interactive components should be implemented as web components when possible, or otherwise be carefully designed to avoid conflicting with the base document or other components.
]

#callout[
  *Components fallback requirement:*  interactive components must provide a fallback mechanism for rendering a reasonable substitute if Javascript is disabled.
]

= Where To Go From Here? <where-to-go>

Every time I have told someone "I want to replace PDF", the statement has been met with extreme skepticism. Hopefully this document has convinced you that HTML-via-EPUB could potentially be a viable and desirable document format for the future.

My short-term goal is to implement a few more documents in the #def-link("portable-epub")[portable EPUB] format, such as my #link("https://willcrichton.net/nota")[PLDI paper]. That will challenge both the file format and the reading system to be flexible enough to support each document type. In particular, each document should look good under a range of reading conditions (screen sizes, font sizes and faces, etc.).

My long-term goal is to design a document language that makes it easy to generate #def-link("portable-epub")[portable EPUBs]. Writing XHTML by hand is not reasonable. I designed #link("https://nota-lang.org/")[Nota] before I was thinking about EPUBs, so its next iteration will be targeted at this new format.

If you have any thoughts about how to make this work or why I'm wrong, let me know by #link("mailto:crichton.will@gmail.com")[email] or #link("https://twitter.com/tonofcrates")[Twitter] or #link("https://mastodon.social/@tonofcrates")[Mastodon] or wherever this gets posted. If you would like to help out, please reach out! This is just a passion project in my free time (for now...), so any programming or document authoring assistance could provide a lot of momentum to the project.

= But What About... <but-what-about>

A brief postscript for a few things I haven't touched on.

*...security?* You might dislike the idea that document authors can run arbitrary Javascript on your personal computer. But then again, you presumably use both a PDF reader and a web browser on the daily, and those both run Javascript. What I'm proposing is not really any _less_ secure than our current state of affairs. If anything, I'd hope that browsers are more battle-hardened than PDF viewers regarding code execution. Certainly the designers of EPUB reading systems should be careful to not give documents any _additional_ capabilities beyond those already provided by the browser.

*...privacy?* Modern web sites use many kinds of telemetry and cookies to track user behavior. I strongly believe that EPUBs should not follow this trend. Telemetry must _at least_ require the explicit consent of the user, and even that may be too generous. Companies will inevitably do things like offer discounts in exchange for requiring your consent to telemetry, similar to Amazon's #link("https://www.amazon.com/gp/help/customer/display.html?nodeId=GFNWCZJAM3SBQQZD")[Kindle ads policy]. Perhaps it is better to preempt this behavior by banning all tracking.

*...aesthetics?* People often intuit that LaTeX-generated PDFs look prettier than HTML documents, or even prettier than PDFs created by other software. This is because Donald Knuth took his job #link("https://www-cs-faculty.stanford.edu/~knuth/dt.html")[very seriously]. In particular, the #link("https://onlinelibrary.wiley.com/doi/abs/10.1002/spe.4380111102?")[Knuth-Plass line-breaking algorithm] tends to produce better-looking justified text than whatever algorithm is used by browsers.

There's two ways to make progress here. One is for browsers to provide more typography tools. Allegedly, `text-wrap: pretty` is #link("https://developer.chrome.com/blog/css-text-wrap-pretty/")[supposed to help], but in my brief testing it doesn't seem to improve line-break quality. The other way is to #link("https://mpetroff.net/2020/05/pre-calculated-line-breaks-for-html-css/")[pre-calculate line breaks], which would only work for fixed-layout renditions.

*...page citations?* I think we just have to give up on citing content by pages. Instead, we should mandate a consistent numbering scheme for block elements within a document, and have people cite using that scheme. (Allison Morrell #link("https://twitter.com/AllisonDMorrell/status/1750728545905823856")[points out] this is already the standard in the Canadian legal system.) For example, Bene will auto-number all blocks. If you're on a desktop, try hovering your mouse in the left column next to the top-right of any paragraph.

*...annotations?* Ideally it should be as easy to mark up an EPUB as a PDF. The #link("https://www.w3.org/TR/annotation-model/#selectors")[Web Annotations specification] seems to be a good starting point for annotating EPUBs. Web Annotations seem designed for annotations on "targetable" objects, like a labeled element or a range of text. It's not yet clear how to deal with free-hand annotations, especially on reflowable documents.
