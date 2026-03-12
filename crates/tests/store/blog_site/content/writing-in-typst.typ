// @rheo:test
// @rheo:formats html
// @rheo:description Blog post with HTML video elements and custom functions

#let video(path, width: "auto", height: "auto", controls: "true", autoplay: "false", loop: "false") = {
  html.elem("video", attrs: (
    src: path,
    width: width,
    height: height,
    controls: controls,
    autoplay: autoplay,
    loop: loop,
    style: "max-width: 100%",
  ))
}

= Writing in Typst | Hacking on Neovim with Claude
== What is a 'good' writing system?
I have been #link("https://www.ohrg.org/devonthink-part-i")[incrementally] #link("https://www.ohrg.org/devonthink-part-ii")[hacking] #link("https://www.ohrg.org/devonthink-part-iii")[on] my writing environment for some time now, since at least 2013 when I started seriously using computers in undergrad.
A couple of years ago, I migrated to Orgmode as the best markup syntax for my needs, and #link("https://www.ohrg.org/writing-setup")[wrote aa post about how Emacs and Orgmode serviced my writing needs].

Here's a summary of that post and the core tenets of what I consider an acceptable writing environment, parsed out over the five or so years I've been experimenting with one through grad school:

+ *Flexible, powerful and distraction-free*.
  In short, this means that the environment needs to be an extension to a modal editor in the terminal.
  I started using a #link("https://carlosbecker.com/posts/ed/")[modal text editor] around 2018, and use a range of ergonomic keyboards in #link("https://www.ohrg.org/cycling-typing")[funky ways] that make using a mouse undesirable in most cases.
  (The web browser is the one environment where I still get some mileage out of a mouse.
  I do a lot with keyboard shorcuts via #link("https://vimium.github.io/")[Vimium], but there are still some contexts where it's just quicker or more comfortable to use a mouse.) 
  One of the main reasons that I settled on Orgmode rather than, say, Markdown at the time was because of its #link("https://orgmode.org/manual/Citations.html")[more standardized bibliographic management].

+ *Non-proprietary and sane markup format*. 
  Microsoft Word documents and Google Docs are great for a lot of things, but I refuse to rely on either of them as a primary format for all of the writing I do, as their formats are to hard to parse (to write custom software for) and bound to Microsoft's and Google's ecosystems respectively.
  The ability to run Unix-style comands on a simple markup format from a terminal to search and replace, for example, is an essential.
  Writing documents in a plain-text markup language also gives me the safety of knowing that, if it really came down to it, I could write my own parser and compilers.
  My writing archive shouldn't strictly rely on some company's infrastructure to host, search, or otherwise make use of the thought it contains. 
  Using such a format also means that cross-platform editing is made simpler and possible.
  (I run linux mostly, but still regrettably use Android as my phone's operating system.)

+ *Multi-format export*.
  #link("https://willcrichton.net/notes/portable-epubs/")[Most of the world's documents are still PDF].
  There's no getting away from needing to export writing as PDF in many cases-- for e-readers like #link("https://www.ohrg.org/using-two-remarkables")[the reMarkable that I use], or for submission to conferences.
  But we increasingly read writing on a web page of some sort, and so I also need a workflow to export fully functional documents to HTML and CSS, too. 
  Other formats that are interesting if not essential include some kind of presentation file (PowerPoint, or better: just a website that has slideshow-like interactions), Markdown for rich formatting to copy somewhere, and plain text.
  
I have up until very recently used Orgmode as my markup language of choice, exported them to PDF with exported them to PDF with #link("https://www.latex-project.org/")[latex], and exported them to HTML with #link("https://pandoc.org/")[pandoc].  
But I am very attached to the Neovim ecosystem for my code editing and writing, and so it was clunky to open up an Emacs installation (that I barely understood) exclusively to edit Orgmode.
So I switched to editing Orgmode in Neovim along with everything else, #link("https://github.com/breezykermo/nixos/blob/f79c84baa8433767189c9d7b434137ba80c63531/home-manager/server/neovim/init.lua#L765-L990")[using plugins] and #link("https://github.com/breezykermo/nixos/blob/f79c84baa8433767189c9d7b434137ba80c63531/home-manager/server/neovim/init.lua#L765-L990")[custom functions] to get towards the writing experience that I wanted.

This has actually worked surprisingly well, but it has some sharp edges.
One of the more significant ones is that any time I want to produce anything more complicated than basic, formatted text with citations and footnotes-- for all of which pandoc transformations produce reasonable output in both HTML and PDF-- I need to start embedding LaTeX into Orgmode, and deal with the LaTeX toolchain / dependency management in order to compile a PDF.
Similarly, if I want to produce an interactive HTML document, I need to embed the source code directly in Orgmode and ensure that the export process handles dependencies and the like appropriately.

Some of this is unavoidable.
If I want to run custom Javascript in a website that is well beyond the expressive capacities of a markup language, at some point I just want to be able to write Javascript.
But what I found frustrating about my Orgmode / LaTeX / HTML workflow is that there wasn't any reasonable way to work towards extending the markup language in _some_ ways, unless I was willing to start developing my own bespoke flavor of Orgmode plus plus. 
I also don't particularly like wrestling with the LaTeX ecosystem, because-- and this is hardly controversial to say-- #link("https://tex.stackexchange.com/questions/222500/why-is-latex-so-complicated")[LaTeX has a lot of bloat].
What I wanted was a more _extensible_ system which had saner defaults.

== Enter: Typst

A few months ago, I started seriously considering #link("https://typst.app/")[typst] as a potential replacement for LaTeX. 
At the very least, I thought, it would be more fun to wrestle with a modern ecosystem when struggling to produce some custom table or figure in my output PDF, as typst has a #link("https://typst.app/docs/reference/layout/")[layout system] that uses terms that are a lot more intuitive to me than the black magic of laying out LaTeX documents.

It just so happened, however, that I started to follow typst development more closely at a time when the final touches to the #link("https://github.com/typst/typst/issues/5512")[basic foundations of HTML export], such as footnotes and bibliography, were just about to be added to the upstream.
So I made #link("https://github.com/typst/typst/pulls?q=is%3Apr+author%3Abreezykermo+is%3Aclosed")[a few contributions] to spirit it along, and started more serious experimentation using typst as a unified way to produce _both_ PDF and HTML in my writing environment.
Pandoc #link("https://pandoc.org/MANUAL.html#typst")[can convert to and from typst], so I originally intended to keep writing documents in Orgmode and then transiently convert them to typst in order to produce PDF and HTML both.
But I quickly found that the typst syntax natively accommodates all of the features that I make use of regularly in Orgmode such as citations, footnotes, headings, links and text decoration-- and then some. 

So why not write my blogs, papers, and documents directly in typst?
I considered the critical features of my Neovim / Orgmode writing environment that I didn't want to abandon:

+ *Shortcuts for markup*.
  The #link("https://github.com/nvim-orgmode/orgmode")[nvim-orgmode plugin] makes writing Orgmode in Neovim pleasurable, providing shortcuts to insert a link and basic text decoration while composing.
+ *Citation and link picking*. 
  Though I've gone without it for a few months for reasons that are immaterial here, I used to have a shortcut to bring up a fuzzy finder for all of my bibliography entries to easily insert a citation. 
  The same fuzzy finder would make it easy to link to local files (in a website, for example, to link to other posts).
+ *Document folding*.
  The ability to fold away all of the text beneath a heading is very useful when navigating larger documents, as it helps me to compartmentalize writing tasks and organize longer documents such as a dissertation chapter.
+ *Export shortcuts*.
  I have customized my Neovim editor so that I can easily export the active Orgmode document (through the pandoc and LaTeX processes described above).
  Personally, I don't feel that I need a real-time live preview of the document as I type, as I generally just want to check that it looks reasonable at certain junctures in the writing process, rather than continuously. 

The one other features of Orgmode that I have come to rely on heavily is its #link("https://orgmode.org/manual/TODO-Basics.html")[TODO functionality].
I typically only use this in notes related to projects or tasks more generally, however, and not in documents that are intended for publication such as a paper or blog post.
  
== Enter: Claude Code
At this point in the past of a new writing technology's prospecting, I would go searching for a Neovim plugin for typst and hope that it provides features that satisfy a majority of these requirements.
I've spent a fair bit of time #link("https://github.com/breezykermo/nixos/blob/f79c84baa8433767189c9d7b434137ba80c63531/home-manager/server/neovim/init.lua")[tinkering with my init.lua], the entrypoint for customizing Neovim, but I've never had the time nor interest to sit down and write a plugin from scratch.

LLMs, of course, are at time of writing taking the coding world by storm.
I have started moderately relying on #link("https://github.com/anthropics/claude-code")[Claude Code] when writing some-- though certainly not all-- kinds of code.
As is well-known by now, Claude is especially good at scaffolding hacky scripts or modules from scratch, when no large codebase or domain-specific knowledge needs to be kept in context.
A Neovim plugin, I realized this morning, is a pretty ideal domain for LLM-assisted coding.
The 'codebase' is often just a single configuration file, and the domain-specific knowledge is the Neovim editor itself, a well-documented and expansively customized software for which there are many examples on Reddit.#footnote[It's impossible to mention LLM coding at this time without adding some sort of disclaimer that, no, I don't think AGI is around the corner, and yes, I do expect both programming languages and language writ large to remain 'a thing' in the foreseeable future. LLMs are an incredibly powerful tool to write and analyze code and text, but the purpose of code and text-- as a medium of symbolic communication amongst social beings-- has not been rendered valueless since ChatGPT became publically available. If anything, the value of adeptly and adroitly handling written language has taken deeper root. For my preliminary thoughts on why we are so keen to imagine that computers will supplant the usefulness of the human, I refer the reader to #link("https://caiml.org/dighum/announcements/digital-humanism-salon-capital-and-the-computer-by-lachlan-kermode-2024-06-24/")[this talk I gave in 2024].] 

So I fired up Claude Code earlier this afternoon, and-- fast-forward an hour or two-- I have a fully functional writing environment for Typst that essentially has feature-parity with my Orgmode environment.
Moreover, my #link("https://github.com/breezykermo/nixos/tree/main/home-manager/server/neovim")[Neovim config] is now much more comprehensibly modularized; and I have a tried-and-tested method for extending it without needing to spend days learning the ins-and-outs of Neovim's API; and #link("https://github.com/breezykermo/nixos/commit/67cdbbae0dd77db766289b7f6eb278091ab937dd")[some bugbears in my NixOS config were eliminated] while I was at it.
(If that last bit means nothing to you, count yourself lucky!)

== My new writing environment 
I use #link("https://tree-sitter.github.io/tree-sitter/")[treesitter] for syntax highlighting, and Typst already looks pretty good with it.
I get function completion #link("https://github.com/breezykermo/nixos/blob/main/home-manager/server/neovim/lua/plugins/lsp.lua#L17-L24")[by integrating an LSP for the format], for which I'm using #link("https://github.com/Myriad-Dreamin/tinymist")[tinymist].

As I noted above, I haven't had dynamic link or citation insertion for some time.
It was one of the features that got lost in my move from writing Orgmode in Emacs to writing it in Neovim.
I use #link("https://github.com/nvim-telescope/telescope.nvim")[telescope.nvim] for general search and file-picking when coding in Neovim, and I figured that I could use a customized pop-up to dynamically pick available citations from the relevant #link("https://www.overleaf.com/learn/latex/Bibliography_management_with_bibtex")[BibTeX] file, too.
After a few minutes of #link("https://simonwillison.net/2025/Oct/7/vibe-engineering/")[vibe-engineering], I have the following: 

#video("../img/typst-links-citations-demo.mp4")

When I am writing in Typst, and I want to bring in a reference, I can open a panel.
Note that the search is full-text, not just using the reference ID.
I also have a shortcut to specify which bib file to use through the `#bibliography` function in Typst.
I can insert links in the same way as citations, both references files relative to the current one (blog posts on the same site), and external links.
Both the citation and link insertion work either by highlighting text and annotating it, or to insert new links/citations.  
I also have a similar shortcut to add footnotes.

This is pretty functional now for generic writing!

== Future work 
Typst isn't ideal for producing fully-featured websites currently, as HTML export is experimental.
Even when it becomes better supported, the project is-- understandably, given its priority supporting PDF-- taking a #link("https://github.com/typst/typst/issues/5512")[relatively conservative approach] to HTML generation.
Anything that doesn't have a robust analog in a PDF document, such as videos and hover panels, will have to be 'embedded' in Typst with HTML/CSS/JS, rather than being written in Typst syntax.
The current experience isn't much worse than Orgmode with Pandoc, though, and the Typst roadmap promises that it will become much better in the relatively short-term future.

There is a longstanding issue that I've had with links in Orgmode that I haven't yet tackled with Typst.
When I'm writing, I like hyperlinked text to appear as it will in the final document, i.e. without the underlying URL on display.
When editing any particular line, though, it's better that all of the links are 'expanded' to their full source syntax (`#link("...")[...]`) so that its feasible to edit the markup without requiring any fancy shortcuts.
The effective shortening of lines that occurs when hiding these URLS results in different Neovim line-wrapping requirements, with which the Orgmode plugin I have been using does a bad job, giving ugly linebreaks in documents with long links.
This link presentation will likely be the next feature I add to my Neovim Typst plugin.

I'll add to the capabilities in #link("https://github.com/breezykermo/nixos/tree/main/home-manager/server/neovim/lua/typst")[my Neovim config files], and might eventually release a separate plugin if the features become significant/mature enough.
