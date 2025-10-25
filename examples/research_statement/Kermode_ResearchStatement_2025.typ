#import "../../src/typst/rheo.typ": book_template 
#show: book_template
#import "@preview/blinky:0.2.0": link-bib-urls

#let department = [the School of Information at the University of Michigan]
#let name = [Lachlan Kermode] 

#let subsidiarytextsize = 8pt;

#show link: it => { text(fill: rgb("#2563eb"))[#it] }

// #show heading.where(level: 1): it => [ 
//   #set text(size: 12pt) 
//   #pad(bottom: 0.3em)[#it]
// ]
//
// #set page(
//   margin: 1in,
//   header-ascent: 0.3in,
//   header: context {
//     if counter(page).get().first() == 1 {
//       // First page header with Brown logo
//       grid(
//         columns: (auto, 1fr),
//         column-gutter: 12pt,
//         align: bottom,
//         image("brown-logo.png", height: 0.6in),
//         block(
//           width: 100%,
//           stack(
//             spacing: 6pt,
//             [*#name* --- Research Statement],
//             text(size: subsidiarytextsize)[Department of Modern Culture and Media],
//             line(length: 100%, stroke: 0.5pt)
//           )
//         )
//       )
//     } else {
//       // Subsequent pages header
//       
//       show text: smallcaps
//       align(right)[Research Statement: #name]
//     }
//   }
// )

#set par(leading: 0.5em)

= Research Statement
== Lachlan Kermode

// #text(size: subsidiarytextsize)[Note: This document contains hyperlinks to external websites that are indicated by blue, underlined text. The statement has been written to read fluidly without needing to visit any of the links: but they are included to provide further context for the interested reader.]

As an interdisciplinary scholar trained both in the humanities (PhD Modern Culture and Media, Brown) and in computer science (ScM Computer Science, Brown; AB Computer Science, Princeton), my research seeks to contribute wholeheartedly to both sets of disciplines. 
The foremost imperative of my publications and projects is to critically understand, evaluate, and reconceptualize the role that computing plays-- and _should_ play-- in society.
Drawing on philosophy, history, and literary theory, my work studies how computing works in society in relation to modern conceptions of freedom. My academic program focuses on three kinds of output:

+ *The publication of conceptual work* on computing freedom-- in the fields of philosophy, history, and literary/critical theory @kermodeItStupidThink2024 @kermodeOneZeroCapital2026.
+ *The practice of new pedagogy* towards computing freedom-- within the walls of the academy and beyond it through online and public-facing work @kermodeCapitalTechWorkers2025 @kermodeMarxismFormFredric2025 @kermodeGradLog2025.
+ *The production of practical systems* realizing computing freedom-- published as software that is actively used by researchers and in HCI and computing ethics venues @kermodeTimemap2020 @kermodeObjectsViolenceSynthetic2020a  @dcruzDetectingTearGas2022a @kermodeMtriage2023.

Exhibitions to which I have contributed have been shown at venues such as the #link("https://forensic-architecture.org/programme/exhibitions/uncanny-valley-being-human-in-the-age-of-ai")[San Francisco De Young Museum], the #link("https://forensic-architecture.org/programme/exhibitions/triple-chaser-at-the-whitney-biennial-2019")[NYC Whitney Biennial], #link("https://critical-zones.zkm.de/#!/detail:cloud-studies")[Germany's ZKM], and #link("https://artspace-aotearoa.nz/")[New Zealand Aotearoa's Artspace]. My research has been presented and published in both humanities and computer science venues, and has been recognized through fellowships at the #link("https://www.iwm.at/")[IWM in Vienna], #link("https://fi2.zrc-sazu.si/en")[ZRC SAZU] in Ljubljana, the University of Auckland, and through the #link("https://graduateschool.brown.edu/phd-experience/interdisciplinary-research/open-graduate-education")[Open Graduate Fellowship] at Brown University. I have taught original courses as the Instructor of Record in the departments of both Computer Science and Modern Culture and Media at Brown University; I have given seminars and workshops in art museums, architecture schools such as the #link("https://www.aaschool.ac.uk/")[Architectural Association] in London, and online; and open source software that I have written has been used by researchers investigating human rights abuses in Ukraine, Palestine, the United States, Northern Africa, and the Mediterranean.

My dissertation and first book project argues that Marx's critique of political economy in _Capital_, the _Grundrisse_, and other work offers a sage critique of the function of and fantasies about automation in modern society that still has relevance in an age of AI. 
I argue that the computer is a concept that casts a long historical shadow by showing that it bears essential similarities, when we see it as a structure of automation as Alan Turing did in his seminal work on its concept, to what Marx calls a machine.
Though it finds new footing in the fantasies and fears associated with large language models (LLMs) and neural nets in our time, there is greater precedent for a critique of the computer's questionable function in society than might at first be imagined, as projects that fantasize about its potential impact on the production of capitalist value date back at least to Charles Babbage.

= Research motivation 
After completing my undergraduate degree in 2018, I moved to London to work as a software researcher at the interdisciplinary human rights research agency at Goldsmiths University, #link("https://forensic-architecture.org/")[Forensic Architecture] (hereafter FA).
Over the next few years, I contributed to #link("https://forensic-architecture.org/about/team/member/lachie-kermode")[more than fifteen investigations] into human rights abuses across the globe, ranging from contextualizing military malfeasance in #link("https://forensic-architecture.org/investigation/destruction-and-return-in-al-araqib")[Palestine], #link("https://forensic-architecture.org/investigation/the-destruction-of-yazidi-cultural-heritage")[Iraq], and #link("https://forensic-architecture.org/investigation/the-battle-of-ilovaisk")[Ukraine], to #link("https://forensic-architecture.org/investigation/police-brutality-at-the-black-lives-matter-protests")[police brutality at the 2020 BLM protests], to #link("https://forensic-architecture.org/investigation/triple-chaser")[the negligent export of 'non-lethal' weapons by the ultra-rich].  
As the sole full-time software researcher at FA for most of my time as an Advanced Software Researcher there (2018-2019), my responsibilities ranged from developing #link("https://www.digitalviolence.org/#/explore")[new interactive platforms] for investigations, conceptualizing and producing code where required for exhibitions, migrating email servers, maintaining #link("https://forensic-architecture.org/investigation/the-enforced-disappearance-of-the-ayotzinapa-students")[existing platforms] developed before my time, and developing a critical framework for the practice of #link("https://forensic-architecture.org/subdomain/oss")[software in general] and #link("https://forensic-architecture.org/investigation/cv-in-triple-chaser")[A.I. in particular] in our work.

I introduce my work at FA as the backdrop for my research motivation as it gives a concrete example of the kind of research and practice that I would seek to continue at #department. 
(I have remained a Research Fellow at the agency since my departure in 2021.)
One of the initiatives of which I am most proud from my time at FA is the #link("https://forensic-architecture.org/subdomain/oss")[Open Source Software Initiative] that I conceived and initiated.
Prior to my arrival, FA had no obviously public source code or assets related to investigations, a characteristic that struck me as odd given that the organization positioned itself as #quote(link("https://forensic-architecture.org/about/agency")[born in the 'open source revolution']).
The phrase 'open source' in journalism, I learned, represents a different ethic than the same phrase in software communities.
Whereas I understand open source software as a deradicalization of free software's insistence on the legally innovative 'copyleft' licensing requirement that nonetheless implies that a system's source code is available for public inspection, at the very least#footnote[Whether software initiatives that characterize themselves as open souce actually do make all or a majority of their source code public is a matter for a different debate.], in journalism, open source traditionally refers to the use of sources (information and informants) that are exclusively or predominantly publically accessible.
The distinct genealogies of the phrase in journalism and software produced, for me, a tension in FA's self-presentation as an open source research agency.
The open source software #link("https://github.com/forensic-architecture/timemap")[timemap], a project that now has more than 350 stars on GitHub and has been forked and used by investigative agencies such as Bellingcat to #link("https://ukraine.bellingcat.com/")[document ongoing civilian harm in Ukraine], is a standalone frontend application that I built and open sourced (in the software sense) in order to address this tension.

This parable of the complexity contained in such a seemingly simple phrase as open source is the conceptual grounding for the multimodal approach that I take to computing research.
The best way to build and teach software is to always be thinking critically about its history concurrently. 
As I experienced first-hand during my tenure at FA, there is no such thing as a truly self-consciously efficacious political practice that does not take philosophy and critical theory seriously.
To take meaningful action on a street, we must know where the street stands in the symbolic (or social) order of things.
Why, in other words, would taking action on _this_ street-- or, in a more apposite example, developing and releasing _this_ kind of software-- result in the change we are after?
To answer this question effectively and not act in a vacuum of expected outcome, we need a critical theory of language and political meaning.

== 1) Conceptual work 
Through coursework and research for my dissertation in the Department of Modern Culture and Media at Brown University (2021-2026), two critical traditions have become essential to my thinking about the nature of freedom in computing and social life writ large, the *critique of political economy* and *psychoanalysis*, methods that have come to be known by the monikers Marx and Freud respectively. 
Over the course of the long twentieth century, Marx and Freud have intransigently featured in an astonishing set of practical and philosophical uprisings, from the social history of the Soviet Union and China to the literary theory of Fredric Jameson and Alain Badiou.
The studied accounts of the subject and society in the work of Marx and Freud are, I believe, the most important starting points for both 1) a grounded practice of software development and maintenance, and 2) a political theory of computation which would show us the role that it _should_ play in a society where freedom is flourishing rather than deprecated or defunct.

My first book project reconsiders Marx's theory in light of the computer.
The idea that the computer constitutes a fundamentally new substance for which we need new theories, practices, and economies of scale, I argue, is not the best basis for a thoughtful computational praxis.
This idea is rampant among commercial thought-leaders and capitalist pundits, and acts as the unspoken axiom for many corporate dissimulations around the urgent need for 'AI alignment' or unprecedented investment in computing infrastructure.
But it is also the contemporary academic consensus in fields that present themselves as arbiters of the computer's question, such as digital media studies and the history of science and computing, which is strikingly on par with the aforementioned thought-leaders in its premise: the computer is something new, and its problems thus have limited historical and philosophical precedent.
The revelation that Marx in fact has a firm notion of the computer in his critique of political economy already in the 19th century confirms that the problems it presents in society are in fact not quite as novel or unprecedented as they are made out to be.

More specifically, Marx's theory of capitalism should still serve as a critical framework through which we, as software developers and computer scientists, can take action 'on the street' today.
On account of the curious and complicated nature of modern subjectivity in capitalism and the philosophical subtleties of software freedom that I allude to in my opening anecdote, we cannot rely simply on 'technical' characterizations of a software when thinking about whether it produces more or less freedom.
Though I am in general in favor of FLOSS (Free/Libre Open Source Software), local-first, federated, and privacy-preserving systems, no single set of characterizations can ensure that a software is doing unqualified good in the world. 
Rather, we must study history and philosophy, both of computing and more broadly, to more concretely conceive of the consequences that a system effects, and use this critical theory to guide our practice as developers and computing specialists. 
The problem of how LLMs should or shouldn't be used in the university, to invoke an especially topical example, should be contextualized with reference to Marx's critical theory of value (use-value, exchange-value, etc.), if only to understand the risks of ceding critical infrastructure to private interest. 

Throughout the course of my PhD, I have presented work making arguments of this kind at venues such as #link("https://www.historicalmaterialism.org/event/twenty-second-annual-conference/")[Historical Materialism], #link("https://lackorg.com/2025-conference/")[LACK], the #link("https://www.americanacademy.de/")[American Academy in Berlin], and the #link("https://caiml.org/dighum/")[TU Wien Digital Humanism] circle, among others.
In January 2026, I will present dissertation work in a series of seminars at the #link("https://fi2.zrc-sazu.si/en")[ZRC SAZU Institute of Philosophy] in Ljubljana arguing that our theory of the subject cannot be walled off from its work in the world-- a conundrum that appears in Marx's mature work through the notion of labor, and in Freud and Lacan through their thematizations of the unconscious.
I have work under review at both #link("https://criticalinquiry.uchicago.edu/")[Critical Inquiry] and #link("https://direct.mit.edu/octo")[October], both of which are representative of the journals in which I aim to publish theoretical research.  
 
== 2) New pedagogy 
My research also develops new pedagogy in computer science and software engineering which takes into account the material entanglement of capitalism and computing, particularly as it manifests at the North American University.
I include this pedagogy as a part of my research statement-- rather than registering it solely in the accompanying teaching statement-- because I see it as an inseparable continuation of the critical thinking in the conceptual work I discuss above.
As I go into more detail regarding the courses for which I have been the Instructor of Record and my dual-track work in the departments of Computer Science and Modern Culture and Media in that teaching statement, I focus here on the rationale and research outputs of my _public_ teaching and scholarship beyond the university.

Public-facing work and teaching is a critical component of my project.
Throughout my PhD, I have taken numerous courses at the 'para-academy' #link("https://www.bicar.org/")[BICAR] with #link("https://www.bicar.org/rohit-goel")[Rohit Goel], culminating in a 4-week intensive seminar with 6 participants in the summer of 2023 studying psychoanalytic critique in Bombay, India.  
These courses impressed upon me the idea that public-facing writing and teaching has a pedagogical impact which complements and augments the university setting, reaching students and readers in venues beyond the academy.
At the same time, the university remains essential as a unique space in society where a unique syntax of freedom-- academic freedom-- is plausible to practice.

My public-facing work and pedagogy is aligned with my conceptual work in its focus on a practice of software and computing that produces a surplus of freedom.
Three examples of new pedagogy in my work are:

+ *Online courses.* In the Summer and Fall of 2025, I offered an experimental online seminar titled #quote[#link("https://cftw.ohrg.org/")[_Capital_ for Tech Workers]] in collaboration with Erika Bussman, a software engineer at Google.
  The course sought to consider the extent to which Marx's critique in _Capital_ travels to the contemporary context and conundrum of technology companies and the students' practice, white- and blue-collar alike, as a part of them.
  Unlike the university setting, the students in this course were professional technologists, users, engineers, product managers, even as founders and investors, from Google, Meta, various startups, and other such companies.
  The course's dual aim, in addition to the software-instrumental one, was to give students the conceptual tools to understand the first ten chapters _Capital_ on its own terms.
  One key takeaway from the first iteration of this course was that the opening chapters of _Capital_ (which are infamously dense and philosophical) did not resonate with the concrete lives of tech workers as much as, say, chapter ten (on the working day).
  Erika and I will run a follow-up course on the remaining chapters of _Capital: Volume I_ in January 2026, and will develop a guidebook for the first nine chapters similarly titled _Capital for Tech Workers_ to respond to this particular learning.

+ *Public writing.* Througout my PhD, I have maintained a 'grad log' of various writings at #link("https://www.ohrg.org/")[https://ohrg.org].  
  Many of these writings are not intended as argumentative papers, but rather as reading notes or general assessments of thinkers and work that I find important. 
  I also provide resources for students such as my '#link("https://www.ohrg.org/writing-academic-essays")[Writing Academic Essays]' guide, as well as reflections on teaching and the problematics of reading groups such as '#link("https://www.ohrg.org/24-01-29")[Why we should give feedback to students]', and general blogs on my day-to-day tribulations running Linux such as ' #link("https://www.ohrg.org/writing-setup")[Writing Setup]'.

+ *Teaching livestreams.* Inspired by educational content such as #link("https://www.youtube.com/c/JonGjengset")[Jon Gjengset's marathon Rust coding streams], I have also more recently ventured to live-stream the reading and teaching of difficult works such as Fredric Jameson's _Marxism and Form_ and Eric Santner's _The Royal Remains: The People's Two Bodies and the Endgames of Sovereignty_ in 2-3 hour seminars on #link("https://www.youtube.com/@LachieKermode")[my Youtube channel].

In combination with my pedagogical commitment to reconceptualizing Computer Science education within the academy (outlined in the accompanying teaching statement), these modes of non-traditional teaching and publication represent my commitment to freedom beyond its preconceived institutional understandings, i.e. as academic freedom alone.
Though I am yet to publish papers that reflect on this pedagogy's success (or failure) in CS education conferences such as #link("https://sigcse.org/")[SIGCSE], I have an interest to do so in the future.

== 3) Practical systems 
I am actively working on three software projects that will be open sourced and published at an HCI or otherwise appropriate conference venue.

+ *Rostra.* 
  Through my work developing and maintaining #link("https://github.com/forensic-architecture/timemap")[timemap] (2018-2022), several problems became apparent even as the software proved useful for FA and other human rights organizations' investigations.
  Like timemap, rostra is a frontend framework for the cartographic visualization of time-series events that provides temporal and spatial context and correlation.
  Unlike timemap, rostra is a _modular_ framework which is _additively_ configured, meaning that a deployment can selectively include panels for a timeline and other forms of data visualization from a library ecosystem.
  I determined the need for rostra through work on the #link("https://www.adamartgallery.nz/exhibitions/archive/2020/violent-legalities")[Violent Legalities exhibition] in Wellington, Aotearoa New Zealand in 2020, iterated on its concept through subsequent exhibitions in #link("https://artspace-aotearoa.nz/exhibitions/slow-boil")[Auckland 2021] and #link("https://www.mutualart.com/Exhibition/The-Moral-Drift/6B553CE14552BAD4")[Tauranga 2022], and began development in earnest in partnership with #link("https://profiles.auckland.ac.nz/k-muller")[Karamia Muller]'s group at the University of Auckland in mid 2025. 

+ *Acta.* In 2024, I began working with #link("https://www.unibo.it/sitoweb/lorenzo.pezzani")[Lorenzo Pezzani] at the University of Bologna, the director of #link("https://liminal-lab.org/")[LIMINAL Lab], to visualize #link("https://www.hrw.org/video-photos/interactive/2022/12/08/airborne-complicity-frontex-aerial-surveillance-enables-abuse")[the correlation between drone surveillance and migrant pushbacks] in the Mediterranean. 
  Working from a redacted dataset that was retrieved through freedom of information requests to Frontex, the EU border control agency, I co-designed and built a platform to present the 'raw' XLSX data more intelligibly by correlating it with other information sources such as aerial asset flight hours and social media reports of certain pushbacks (forthcoming 2025).
  Acta is a generalized framework for describing the political import of time-series spreadsheet documents by correlating it with other data sources and through features that allow coherent narrativization of possible redactions in the document.
  Acta is conceived as part of the same suite of investigative human rights tooling as rostra, and is designed to be used by agencies such as FA and Bellingcat.

+ *Rheo.*
  More recently in 2025, I have begun collaborative work with #link("https://willcrichton.net/")[Will Crichton] (Assistant Professor at Brown University) investigating the potential of #link("https://typst.app/")[Typst] as the basis for a more pragmatic document authoring and publishing pipeline.
  By making several #link("https://github.com/typst/typst/pulls?q=is%3Apr+is%3Aclosed+author%3Abreezykermo")[contributions to the upstream Typst codebase], I progressed Typst's capabilities to export document structure such as bibliographic entries and citations to an HTML document, a compilation target that is secondary to Typst's full-featured support for PDF.
  Rheo is a static site and experimental typesetting engine based on Typst that will eventually support PDF, HTML, EPUB, and #link("https://willcrichton.net/notes/portable-epubs/")[Portable EPUB] with a richer semantics in the latter format than standalone Typst.
  It is envisioned as a tool that will enable more freedom in the domain of document dissemination in line with the original vision of the Internet as a mechanism for lively and reasonably unfettered academic exchange idea, rather than as the densely commercial space of 'platform capitalism' that it has become. 
  (An early prototype of rheo is what powers the option to #link("https://lachlankermode.com/live/michigan/kermode_researchstatement_2025")[read this statement on the web], if you prefer to do so.)

== Future work
Thus far in my conceptual work, I have primarily focused on the relevance of Marx's and Freud's philosophy for the project of computing freedom.
In my second book project, I seek to deepen my appraisal of psychoanalysis as a critical method by engaging with the Ljubljana School's philosophical inflection of the method via the midcentury French philosopher, Jacques Lacan.
Alain Badiou, a militant Maoist philosopher who came of age during the same period, also stands to offer important resources for a political theory of computing freedom on account of his deep engagement with set theory and the history of 20#super[th] century mathematics, an intellectual lineage that gave rise to the discipline of Computer Science in the British and American contexts.

Regarding new pedagogy, I hope to continue to be inspired by my students to produce the resources and public-facing writing that I believe will be most constructive for their critical education. 
In my livestream work, my primary aspiration at present is to develop content that demonstrates how the real-world process of coding a system such as rostra or acta can and should proceed by way of substantive philosophical and critical reasoning, effectively marrying #link("https://www.youtube.com/watch?v=tXx8Tu24RWo&list=PLKML-_b5aqpNrpFe26AIg0NIfxfgIcPH7")[coding livestreams] with #link("https://www.youtube.com/watch?v=1AFQbDXe2Vc&t=3096s")[critical theory livestreams] that I have done.

My work developing practical systems has been driven by the needs of investigations at #link("https://liminal-lab.org/")[LIMINAL Lab] and #link("https://forensic-architecture.org/")[Forensic Architecture], and in my future work in this branch, I will continue to think carefully about when and how it makes sense to abstract frameworks from our case-specific work in the form of tools such as rostra and acta.  
I am also committed to building a suite of tools to complement rheo which enables academic writing, research, and publication to be done more fluidly and freely. 
An idea that I have for one such tool is a Unix-based document storage system that can be accessed through the browser inspired by #link("https://www.devontechnologies.com/apps/devonthink")[DEVONthink], a Mac-only indie software for organizing PDFs, notes, and other files.

== Summary
In summary, my research program is trained on the simultaneous conceptualization and execution of a more critically attuned computer science in modern society. 
I would be thrilled to continue this program to more rigorously practice the politics of computing freedom in the 21#super[st] century at #department.

As Marx famously pronounced in his _Theses on Feuerbach_: #quote[The philosophers have only _interpreted_ the world in various ways. 
The point, however, is to _change_ it.]
The Initiative in Computing Freedom aspires to change the world for the better by acknowledging the necessity of critical interpretation in any consequential understanding of what it means to be free today.

#pagebreak()

= Bibliography
#link-bib-urls(link-fill: blue)[
#bibliography("./references.bib", style: "./style.csl", title: none)
]
