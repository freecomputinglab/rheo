#import "../rheo-slides/dist/rheo-slides.typ": slide

// adapted from https://www.ohrg.org/writing-with-rheo
= Writing with Rheo

#slide[]

You've no doubt already heard about #link("https://github.com/freecomputinglab/rheo")[Rheo], the hot new typesetting and static site engine from the #link("https://freecomputinglab.ohrg.org/")[Free Computing Lab] (FCL).
In the unlikely event that you haven't, let me clue you in.
With Rheo, you can flow a folder of Typst files into various different publishable formats, such as PDF (paged documents), EPUB (reflowable documents), and HTML (websites).
It's developed and maintained by... well, me, with contributions from colleagues at the FCL and beyond.

I've written before about #link("./writing-setup.typ")[what I need in a writing setup], and more recently about how I've been #link("./writing-in-typst.typ")[experimenting with Neovim and Typst] towards a modern implementation of such a setup.
I outline in the first post how I've hackily realized many aspects of my writing environment with Orgmode, Emacs, Pandoc, and LaTeX over the past few years.
I don't like LaTeX.
It's complicated, labyrinthine, and I don't understand it.
But it has always been necessary to cobble in as part of my writing system, because it is the only reasonable way to produce sophisticated layouts in PDF such as tables, maths, careful image placement, and so on.

The only reasonable way, until #link("https://typst.app/")[Typst].
Typst takes the ergonomics of Markdown and combines it with the power of LaTeX, packaging it as a modern programming language with a #link("https://github.com/typst/typst")[performant and well-maintained toolchain].
(You can read more about why I see Typst as the best foundation on offer for a modern hypertext writing system on the #link("https://rheo.ohrg.org/why-is-rheo")[Rheo docs site].)
The Typst compiler has excellent support for producing PDFs, and experimental support for producing HTML: but the syntax is designed in a way that it's not unreasonable to demarcate sections of its syntax and find an appropriate representation in a range of different document formats.

Rheo is my first pass at how this could work.
Because EPUB is essentially just HTML in a straitjacket, it wasn't too difficult to take the HTML that Typst produces and wrap it up nicely as a valid EPUB.
There are a few essentials of modern websites such as CSS that Typst isn't scoped to provide, so Rheo bridges this gap as well.
Leveraging this kind of wrapping and a #link("https://rheo.ohrg.org/spines")[little bit of thinking] about how to expose packaging options to the user, Rheo is now basically functional as an #link("https://github.com/freecomputinglab/rheo")[open source] static site engine for Typst.

In the past couple of days, I've ported almost all of my writing projects to Rheo.
My most recent projects were already using Typst, and so for these it was as trivial as dropping them into a folder and adding a #link("https://rheo.ohrg.org/rheotoml")[rheo.toml].
A few of the websites I maintain---like this one---were largely written in Orgmode (as per my old/original writing system), so I used #link("https://pandoc.org/")[pandoc] to get Typst from the original source and added a few post-processing steps to the conversion where pandoc wasn't capable of capturing the syntax.

This means that I now have a `writing` folder on my machine which contains all of my various blogs, academic papers, my dissertation (in-progress), and job materials.
Each document consists chiefly of Typst, and different subfolders contain CSS, Typst templates, and images and other assets as appropriate to the targeted formats.
All of the writing in this folder is, in principle, exportable triply as an EPUB, as PDFs (combined or individual), and as webpages.

Besides being consistent, which makes it much easier to copy-and-paste writing between projects among other things, it is also all essentially editable in my text processor of choice, #link("https://github.com/breezykermo/nixos/tree/main/home-manager/server/neovim")[a customized Neovim].
It is also highly interpretable by agential LLMs such as #link("https://claude.com/product/claude-code")[Claude Code], a product that has slowly but surely become a key part of my coding workflow.
There are many justified concerns about how LLMs will impact writing in the humanities and beyond.
But if you wash away the fears AI ideologues are stoking regarding the imminent automation of intelligence, a fear that I believe #link("https://caiml.org/dighum/announcements/digital-humanism-salon-capital-and-the-computer-by-lachlan-kermode-2024-06-24/")[thoroughly misapprehends how mercurial the notion of 'intelligence' really is], LLMs can do amazing things in a writing workflow.
In a matter of minutes, for example, I fixed some forty typos in this blog with a prompt to Claude Code as simple as 'find typos in this blog and fix them'.
Some of these typos have been around for years!
Furthermore, because Typst encodes formatting in a well-documented and well-structured way, I can let an agent loose on other editorial tasks such as finding incorrect citations, inconsistently formatting book titles, and so on.
All of the Typst files in my `writing` folder are code-like, and so I version-control them using #link("https://github.com/jj-vcs/jj")[Jujutsu] and #link("https://github.com/")[Github], which means that I can always roll back or discard any reckless changes an LLM agent might suggest.

Rheo is now working smoothly for these personal writing projects.
In the coming weeks, I'm excited to attempt to port editions of a major critical journal to Rheo.
This will avail the journal's readership to new formats (webpages and EPUB in addition to the PDF that is already available), and avail the editorial team to LLM-enhanced techniques to correct spelling, formatting, and more.
I expect to push a few features and fixes to Rheo through this first real case study in what it offers to the open publishing ecosystem.
If you're interested to follow along or have questions about how to use Rheo in your own projects, please #link("https://freecomputinglab.zulipchat.com/join/dit724hcwgbhic3xxwkdpkqs/")[join our Zulip].
