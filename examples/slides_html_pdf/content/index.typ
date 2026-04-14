#import "../rheo-slides/dist/rheo-slides.typ": slide, template

#show: template

// adapted from https://www.ohrg.org/writing-with-rheo

#slide[
  Writing with Rheo
]

You've no doubt already heard about #link("https://github.com/freecomputinglab/rheo")[Rheo], the hot new typesetting and static site engine from the #link("https://freecomputinglab.ohrg.org/")[Free Computing Lab] (FCL).
In the unlikely event that you haven't, let me clue you in.

With Rheo, you can flow a folder of Typst files into various different publishable formats, such as PDF (paged documents), EPUB (reflowable documents), and HTML (websites).
It's developed and maintained by... well, me, with contributions from colleagues at the FCL and beyond.

#slide[
  The Problem with LaTeX
]

I've written before about #link("./writing-setup.typ")[what I need in a writing setup], and more recently about how I've been #link("./writing-in-typst.typ")[experimenting with Neovim and Typst] towards a modern implementation of such a setup.
I don't like LaTeX.
It's complicated, labyrinthine, and I don't understand it.
But it has always been necessary to cobble in as part of my writing system, because it is the only reasonable way to produce sophisticated layouts in PDF.

#slide[
  Enter: Typst
]

The only reasonable way, until #link("https://typst.app/")[Typst].
Typst takes the ergonomics of Markdown and combines it with the power of LaTeX, packaging it as a modern programming language with a #link("https://github.com/typst/typst")[performant and well-maintained toolchain].
The Typst compiler has excellent support for producing PDFs, and experimental support for producing HTML.

Because EPUB is essentially just HTML in a straitjacket, it wasn't too difficult to take the HTML that Typst produces and wrap it up nicely as a valid EPUB.
There are a few essentials of modern websites such as CSS that Typst isn't scoped to provide, so Rheo bridges this gap as well.
Leveraging this kind of wrapping and a #link("https://rheo.ohrg.org/spines")[little bit of thinking] about how to expose packaging options to the user, Rheo is now basically functional as an #link("https://github.com/freecomputinglab/rheo")[open source] static site engine for Typst.

#slide[
  ```txt
  ├── writing
      ├── blog
      ├── dissertation
      ├── papers
      ├── applications
      └── draft.typ
  ```
]

I now have a `writing` folder on my machine which contains all of my various blogs, academic papers, my dissertation (in-progress), and job materials.
Each document consists chiefly of Typst, and different subfolders contain CSS, Typst templates, and images and other assets as appropriate to the targeted formats.
All of the writing in this folder is, in principle, exportable triply as an EPUB, as PDFs (combined or individual), and as webpages.

#slide[
  ```typ
  // Define a variable
  #let greeting = "Hello, Typst!"

  // Create a heading
  = My First Typst Document

  // Use the variable and add some text
  #greeting

  // Example of math
  $ a^2 + b^2 = c^2 $
  ```
]

It is also highly interpretable by agential LLMs such as #link("https://claude.com/product/claude-code")[Claude Code].
In a matter of minutes, for example, I fixed some forty typos in this blog with a prompt to Claude Code as simple as 'find typos in this blog and fix them'.
Furthermore, because Typst encodes formatting in a well-documented and well-structured way, I can let an agent loose on other editorial tasks such as finding incorrect citations, inconsistently formatting book titles, and so on.

#slide[
  More soon...
]

Rheo is now working smoothly for these personal writing projects.
In the coming weeks, I'm excited to attempt to port editions of a major critical journal to Rheo.
If you're interested to follow along or have questions about how to use Rheo in your own projects, please #link("https://freecomputinglab.zulipchat.com/join/dit724hcwgbhic3xxwkdpkqs/")[join our Zulip].
