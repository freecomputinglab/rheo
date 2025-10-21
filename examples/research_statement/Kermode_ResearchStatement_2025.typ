#import "../../src/typst/bookutils.typ": book_template 
#show: book_template

#let department = [the School of Information at the University of Michigan]
#let name = [Lachlan Kermode] 

#let subsidiarytextsize = 8pt;

#show link: it => { text(fill: rgb("#2563eb"))[#it] }

#set par(leading: 0.5em)

#context {
  if target() == "html" {
  [
    = Research Statement
    == #name 
  ]
  } else {
    show heading.where(level: 1): it => [ 
      #set text(size: 12pt) 
      #pad(bottom: 0.3em)[#it]
    ]


    set page(
      margin: 1in,
      header-ascent: 0.3in,
      header: context {
        if counter(page).get().first() == 1 {
          // First page header with Brown logo
          grid(
            columns: (auto, 1fr),
            column-gutter: 12pt,
            align: bottom,
            image("brown-logo.png", height: 0.6in),
            block(
              width: 100%,
              stack(
                spacing: 6pt,
                [*#name* --- Research Statement],
                text(size: subsidiarytextsize)[Department of Modern Culture and Media],
                line(length: 100%, stroke: 0.5pt)
              )
            )
          )
        } else {
          // Subsequent pages header
          
          show text: smallcaps
          align(right)[Research Statement: #name]
        }
      }
    )

    text(size: subsidiarytextsize)[Note: This document contains hyperlinks to external websites that are indicated by blue, underlined text. The statement has been written to read fluidly without needing to visit any of the links: but they are included to provide further context for the interested reader.]
  }
}

As an interdisciplinary scholar trained both in the humanities (PhD Modern Culture and Media, Brown) and in computer science (ScM Computer Science, Brown; AB Computer Science, Princeton), my research seeks to contribute wholeheartedly to both sets of disciplines. 
The foremost imperative of my publications and projects is to critically understand, evaluate, and reconceptualize the role that computing plays-- and _should_ play-- in society.
Drawing on philosophy, history, and literary theory, my work explores the extent to which the current computing paradigms and institutions at work in society cultivate or inhibit our modern paradigms and instutions of freedom. 

The rubric under which my different research outputs are conceptually brought together is what I will call the *Initiative for Computing Freedom*, hereafter *ICF*. 
My work as an academic towards the ICF consists of three broad categories of output:

+ *The publication of conceptual research* on computing freedom-- in the fields of philosophy, history, and literary/critical theory.
+ *The practice of new software pedagogy* towards computing freedom-- within the walls of the academy and beyond it through online and public-facing work.
+ *The production of practical systems* realizing computing freedom-- to be showcased in HCI, systems, and computing ethics venues.

During my PhD at Brown, I have both been a Teaching Assistant and taught original courses as the Instructor of Record in the departments of both Computer Science (TA: _Database Management Systems_, Instructor: _CS for Social Change_) and Modern Culture and Media (TA: _Digital Media_ and _Theories of Modern Culture and Media_, Instructor: _Capitalism and Computers in the Era of AI_).
My investment in literary and critical theory is complemented by a practice committed to building real software systems, undertaking human rights investigations, producing exhibitions, and rethinking computing education.

Exhibitions to which I have contributed have been shown at venues such as the #link("https://forensic-architecture.org/programme/exhibitions/uncanny-valley-being-human-in-the-age-of-ai")[San Francisco De Young Museum], the #link("https://forensic-architecture.org/programme/exhibitions/triple-chaser-at-the-whitney-biennial-2019")[NYC Whitney Biennial], #link("https://critical-zones.zkm.de/#!/detail:cloud-studies")[Germany's ZKM], and #link("https://artspace-aotearoa.nz/")[New Zealand Aotearoa's Artspace]. My research has been presented and published in both humanities and computer science venues, and has been recognized through fellowships at the #link("https://www.iwm.at/")[IWM in Vienna], #link("https://fi2.zrc-sazu.si/en")[ZRC SAZU] in Ljubljana, the University of Auckland, and through the #link("https://graduateschool.brown.edu/phd-experience/interdisciplinary-research/open-graduate-education")[Open Graduate Fellowship] at Brown University. Additionally, open source software that I have written has been used by researchers investigating human rights abuses in Ukraine, Palestine, the United States, Northern Africa, and the Mediterranean.

= The Initiative for Computing Freedom
After completing my undergraduate degree in 2018, I moved to London to work as a software researcher at the interdisciplinary human rights research agency at Goldsmiths University, #link("https://forensic-architecture.org/")[Forensic Architecture] (hereafter FA).
Over the next few years, I contributed to #link("https://forensic-architecture.org/about/team/member/lachie-kermode")[more than fifteen investigations] into human rights abuses across the globe, ranging from contextualizing military malfeasance in #link("https://forensic-architecture.org/investigation/destruction-and-return-in-al-araqib")[Palestine], #link("https://forensic-architecture.org/investigation/the-destruction-of-yazidi-cultural-heritage")[Iraq], and #link("https://forensic-architecture.org/investigation/the-battle-of-ilovaisk")[Ukraine], to #link("https://forensic-architecture.org/investigation/police-brutality-at-the-black-lives-matter-protests")[police brutality at the 2020 BLM protests], to #link("https://forensic-architecture.org/investigation/triple-chaser")[the negligent export of 'non-lethal' weapons by the ultra-rich].  
As the sole full-time software researcher at FA for most of my time as an Advanced Software Researcher there (2018-201), my responsibilities ranged from developing #link("https://www.digitalviolence.org/#/explore")[new interactive platforms] for investigations, conceptualizing and producing code where required for exhibitions, migrating email servers, maintaining #link("https://forensic-architecture.org/investigation/the-enforced-disappearance-of-the-ayotzinapa-students")[existing platforms] developed before my time, and developing a critical framework for the practice of #link("https://forensic-architecture.org/subdomain/oss")[software in general] and #link("https://forensic-architecture.org/investigation/cv-in-triple-chaser")[A.I. in particular] in our work.

I introduce my work at FA as the backdrop for my research agenda in the ICF, as it gives a concrete example of the kind of research and practice that I would seek to continue at #department. 
(I have remained a Research Fellow at the agency since my departure in 2021.)
One of the initiatives of which I am most proud from my time at FA is the #link("https://forensic-architecture.org/subdomain/oss")[Open Source Software Initiative] that I conceived and initiated.
Prior to my arrival, FA had no obviously public source code or assets related to investigations, a characteristic that struck me as odd given that the organization positioned itself as #quote(link("https://forensic-architecture.org/about/agency")[born in the 'open source revolution']).
The phrase 'open source' in journalism, I learned, represents a different ethic than the same phrase in software communities.
Whereas I understand open source software as a deradicalization of free software's insistence on the legally innovative 'copyleft' licensing requirement that nonetheless implies that a system's source code is available for public inspection, at the very least#footnote[Whether software initiatives that characterize themselves as open souce actually do make all or a majority of their source code public is a matter for a different debate.], in journalism, open source traditionally refers to the use of sources (information and informants) that are exclusively or predominantly publically accessible.
The distinct genealogies of the phrase in journalism and software produced, for me, a tension in FA's self-presentation as an open source research agency.
The open source software #link("https://github.com/forensic-architecture/timemap")[timemap], a project that now has more than 350 stars on GitHub and has been forked and used by investigative agencies such as Bellingcat to #link("https://ukraine.bellingcat.com/")[document ongoing civilian harm in Ukraine], is a standalone frontend application that I built and open sourced (in the software sense) in order to address this tension.

This parable of the complexity contained in such a seemingly simple phrase as open source is the conceptual grounding for the my research agenda at the ICF. 
Even when we are working with or on software that is free, libre, or open source-- the standard acronym to collapse these qualifiers into a single 'kind' of software is FLOSS-- we are not necessarily, by sole virtue of such a qualifier's existence, working towards a software ecosystem that is fundamentally _free_ in the meaningfully material sense.
Freedom is a concept-- and, I believe, a practice-- that cannot simply be asserted; it must also be experienced. 

The philosophical subtleties of characterizing software or our use of it as free have escaped neither the computing communities that develop and maintain software with code, nor those who interact with computers in more visually intricate ways.
Is Linux-- perhaps the highest profile FLOSS 'success' story-- #link("https://www.reddit.com/r/linux/comments/585xeu/why_the_expression_free_as_in_beer_beer_is_not/")[free as in beer], #link("https://www.reddit.com/r/linuxquestions/comments/np1zti/what_is_the_difference_in_freedoms_in_free/")[free as in speech], or #link("https://opensource.com/article/17/2/hidden-costs-free-software")[free as in puppies]?
We can observe the hermeneutic haywire at stake in this question by reading the first sentence of the GNU project's #link("https://www.gnu.org/philosophy/free-sw.en.html")[official definition of free software]: #quote[Free software means software that respects users' freedom and community.]
Software is most fulsomely free, apparently, when it enables a community of users-- a society of subjects-- to be free.

My research aims both to keep software like Linux free, and to work to make that software _more_ free.
Doing so is not simply a matter of writing more 'free' software.
We must also think critically about the sense of freedom that (certain kinds of) software produce and reprooduce in society, practically, institutionally, and politically.
Thus my research charter with ICF consists of three primary outputs: #link(label("critical-theory"), [critical theory]), #link(label("multimodal-pedagogy"), [multimodal pedagogy]), and #link(label("practical-software"), [practical software]).
The remainder of this statement provides more granular resolution on each of these branches of my scholarship in the ICF.

== Critical theory <critical-theory>
Through coursework and research for my dissertation in the Department of Modern Culture and Media at Brown University (2021-2026), two critical traditions have become essential to my thinking about the nature of freedom in computing and social life writ large, the *critique of political economy* and *psychoanalysis*, methods that have come to be known by the monikors Marx and Freud respectively. 
Over the course of the long twentieth century, Marx and Freud have intransigently featured in an astonishing set of practical and philosophical uprisings, from the social history of the Soviet Union and China to the literary theory of Fredric Jameson and Alain Badiou.

The initial program of my critical-theoretical research in the ICF is to seriously reckon with what lessons can be learned from the philosophy and practice of Marx and Freud for the project of computing freedom.
As I experienced first-hand during my tenure at FA, there is no such thing as a truly self-consciously efficacious political practice that does not take philosophy and critical theory seriously.
To take meaningful action on a street, we must know where the street stands in the symbolic (or social) order of things.
Why, in other words, would taking action on _this_ street-- or, in a more apposite example, developing and releasing _this_ kind of software-- result in the change we are after?
To answer this question effectively and not act in a vacuum of expected outcome, we need a critical theory of language and political meaning.

The studied accounts of the subject and society in the work of Marx and Freud are, I believe, the most promising philosophical starting points for both a grounded practice of software development and maintenance, as well as for a theory of the role that computation should play in a society where freedom is flourshing rather than deprecated or defunct.
My PhD dissertation and first book project, therefore, argues that the computer is a concept that casts a long historical shadow, and exhorts a return to Marx as a powerful, untimely critic of the computer's concept.
Though it finds new footing in the fantasies and fears associated with large language models (LLMs) and neural nets in our time, there is greater precedent for its critique than might at first be imagined.
In particular, a historicist myopia which prohibits an account of the computer as a philosophical structure that dates back through earlier attempts to mechanize society's functioning prevents us from identifying Marx as a brilliant and distinguished critic of the role it plays in the structure of modern society, despite his never having email, an Apple machine, or an interface that produces a credible acrobatics of text and voice such as ChatGPT.

Throughout the course of my PhD, I have presented work making arguments of this kind at venues such as #link("https://www.historicalmaterialism.org/event/twenty-second-annual-conference/")[Historical Materialism], #link("https://lackorg.com/2025-conference/")[LACK], and the #link("https://www.americanacademy.de/")[American Academy in Berlin] among others.
In January 2026, I will present dissertation work in a series of seminars at the #link("https://fi2.zrc-sazu.si/en")[ZRC SAZU Institute of Philosophy] in Ljubljana arguing that our theory of the subject cannot be walled off from its work in the world-- a conundrum that appears in Marx's mature work through the notion of labor, and in Freud and Lacan through their thematizations of the unconscious.
// In the same spirit, I have work under review at #link("https://criticalinquiry.uchicago.edu/")[Critical Inquiry] and #link("https://direct.mit.edu/octo")[October magazine], both of which are representative of the journals in which I aim to publish critical theoretical work as part of the ICF.  
In the same spirit, I have work under review at #link("https://criticalinquiry.uchicago.edu/")[Critical Inquiry], a journal that is representative of the journals in which I aim to publish critical theoretical work as part of the ICF.  
 
== Multimodal pedagogy <multimodal-pedagogy>
The second branch of my research is to develop and deploy pedagogy in computer science and related engineering fields that takes into account the material entanglement of capitalism and computing, particularly as it manifests at the North American University.
I include this branch as a part of my research statement-- rather than registering it solely in the accompanying teaching statement-- because I consider my pedagogical work in both computer science and critical theory an essential part of my research agenda writ large in the ICF.
As I go into more detail regarding the courses for which I have been the Instructor of Record and my dual-track work in the departments of Computer Science and Modern Culture and Media in that teaching statement, I focus here on the rationale and research outputs of my _public_ teaching and scholarship beyond the university.

Public-facing work and teaching is a critical component of my project with the ICF.
Throughout my time as a PhD student, I have taken numerous courses at the 'para-academy' #link("https://www.bicar.org/")[BICAR] with #link("https://www.bicar.org/rohit-goel")[Rohit Goel], culminating in a 4-week intensive seminar with 6 participants in the summer of 2023 studying psychoanalytic critique in Bombay, India.  
These courses have impressed upon me the way in which public-facing writing and teaching can have a pedagogical impact that complements and augments an experience of the university setting, reaching students and readers who do not traffic in the academic venues that remain essential to the value of the university as a unique space in society where a unique syntax of freedom-- academic freedom-- is the imperative.

In addition to academic conferences and journals, therefore, I seek to 'publish' and teach in non-traditional venues, many of which are made possible through the incredible project in computing freedom called the Internet.
The following is a non-exhaustive list of modes in which I envision the ICF to produce public-facing work and pedagogy.

- *Online courses.* In the Summer and Fall of 2025, I offered an experimental online seminar titled #quote[#link("https://cftw.ohrg.org/")[_Capital_ for Tech Workers]] in collaboration with Erika Bussman, a software engineer at Google.
  The course sought to consider the extent to which Marx's critique in _Capital_ travels to the contemporary context and conundrum of technology companies and the students' practice, white- and blue-collar alike, as a part of them.
  Unlike the university setting, the students in this course were professional technologists, users, engineers, product managers, even as founders and investors, from Google, Meta, various startups, and other such companies.
  The course's dual aim, in addition to the software-instrumental one, was to give students the conceptual tools to understand the first ten chapters _Capital_ on its own terms.
  Given the experiment's success, we will run a follow-up course on the remaining chapters of _Capital: Volume I_ in January 2026.

- *Public writing.* Througout my PhD, I have maintained a 'grad log' of various writings at #link("https://www.ohrg.org/")[https://ohrg.org].  
  Many of these writings are not intended as argumentative papers, but rather as reading notes or general assessments of thinkers and work that I find important. 
  I also provide resources for students such as my '#link("https://www.ohrg.org/writing-academic-essays")[Writing Academic Essays]' guide, as well as reflections on teaching and the problematics of reading groups such as '#link("https://www.ohrg.org/24-01-29")[Why we should give feedback to students]', and general blogs on my day-to-day tribulations running Linux such as ' #link("https://www.ohrg.org/writing-setup")[Writing Setup]'.

- *Teaching livestreams.* Inspired by educational content such as #link("https://www.youtube.com/c/JonGjengset")[Jon Gjengset's marathon Rust coding streams], I have also more recently ventured to live-stream the reading and teaching of difficult works such as Fredric Jameson's _Marxism and Form_ and Eric Santner's _The Royal Remains: The People's Two Bodies and the Endgames of Sovereignty_ in 2-3 hour seminars on #link("https://www.youtube.com/@LachieKermode")[my Youtube channel].

In combination with my pedagogical commitment to more critically-aware forms of computing practice that I outline in the accompanying teaching statement, these modes of teaching and non-traditional publication represent my commitment to practicing computing and acadimic freedom beyond the inherited understandings have of those notions, and to prototyping new kinds of computing and critical pedagogy.
When such pedagogy feels sufficiently matured and conceptually coherent, I intend to publish papers that reflect on its success in CS Education conferences such as #link("https://sigcse.org/")[SIGCSE].

== Practical software <practical-software>
The third branch of my research in the ICF produces practical software systems that realize computing freedom as it has been critically conceptualized in this statement so far.
I am actively working on three software projects that will be open sourced and published at an HCI or otherwise appropriate conference venue.

+ *Rostra.* 
  Through my work developing and maintaining #link("https://github.com/forensic-architecture/timemap")[timemap] (2018-2022), several problems became apparent even as the software proved useful for FA and other human rights organizations' investigations.
  Like timemap, rostra is a frontend framework for the cartographic visualization of time-series events that provides temporal and spatial context and correlation.
  Unlike timemap, rostra is a _modular_ framework which is _additively_ configured, meaning that a deployment can selectively include panels for a timeline and other forms of data visualization from a library ecosystem.
  I determined the need for rostra through work on the #link("https://www.adamartgallery.nz/exhibitions/archive/2020/violent-legalities")[Violent Legalities exhibition] in Wellington, Aotearoa New Zealand in 2020, iterated on its concept through subsequent exhibitions in #link("https://artspace-aotearoa.nz/exhibitions/slow-boil")[Auckland 2021] and #link("https://www.mutualart.com/Exhibition/The-Moral-Drift/6B553CE14552BAD4")[Tauranga 2022], and began development in earnest in partnership with #link("https://profiles.auckland.ac.nz/k-muller")[Karamia Muller]'s group at the University of Auckland in mid 2025. 

+ *Acta.* In 2024, I began working with #link("https://www.unibo.it/sitoweb/lorenzo.pezzani")[Lorenzo Pezzani] at the University of Bologna, the director of #link("https://liminal-lab.org/")[LIMINAL Lab], to visualize #link("https://www.hrw.org/video-photos/interactive/2022/12/08/airborne-complicity-frontex-aerial-surveillance-enables-abuse")[the correlation between drone surveillance and migrant pushbacks] in the Mediterranean. 
  Working from a redacted dataset that was retrieved through freedom of information requests to Frontex, the EU border control agency, I co-designed and built a platform to present the 'raw' XLSX data more intelligibly by correlating it with other information sources such as aerial asset flight hours and social media reports of certain pushbacks (forthcoming 2025).
  Acta is a generalized framework for describing the political import of time-series spreedsheet documents by correlating it with other data sources and through features that allow coherent narrativization of possible redactions in the document.
  Acta is conceived as part of the same suite of investigative human rights tooling as rostra, and is designed to be used by agencies such as FA and Bellingcat.

+ *Rheo.*
  More recently in 2025, I have begun collaborative work with #link("https://willcrichton.net/")[Will Crichton] (Assistant Professor at Brown University) investigating the potential of #link("https://typst.app/")[Typst] as the basis for a more pragmatic document authoring and publishing pipeline.
  By making several #link("https://github.com/typst/typst/pulls?q=is%3Apr+is%3Aclosed+author%3Abreezykermo")[contributions to the upstream Typst codebase], I progressed Typst's capabilities to export document structure such as bibliographic entries and citations to an HTML document, a compilation target that is secondary to Typst's full-featured support for PDF.
  Rheo is a static site and experimental typesetting engine based on Typst that will eventually support PDF, HTML, EPUB, and #link("https://willcrichton.net/notes/portable-epubs/")[Portable EPUB] with a richer semantics in the latter format than standalone Typst.
  It is envisioned as a tool that will enable more freedom in the domain of document dissemination in line with the original vision of the Internet as a mechanism for lively and reasonably unfettered academic exchange idea, rather than as the densely commercial space of 'platform capitalism' that it has become. 
  (An early prototype of rheo is what powers the option to #link("https://lachlankermode.com/live/mic.html")[read this statement on the web], if you prefer to do so.)

// These three projects are representative of the sort of software that my research intends to develop through the ICF.
// Though I am strongly committed to open sourcing software and making it free through permissive licensing where it is sensible to do so, my time at FA revealed that publicly available source code is not a panacea to the thorny politics of software freedom. 
// (In many investigations, FA works with sensitive data that would likely do more harm than good if it were made categorically public.)
// Every practical software project in my research cannot be separated from the conceptual work in philosophy and history that I do concurrently, as this dimension of my critical practice as a researcher informs why, when, how, and for whom I build systems with code.

== Future work
My future work will maintain its emphasis on these three branches and the modalities they represent.
In each branch, I have ambitions to both deepen and expand the work that I have already begun. 
In the domain of critical theory, while I have primarily focused so far on the relevance of Marx's and Freud's philosophy for the project of computing freedom, in future work (my second book project) I seek to deepen my appraisal of psychoanalysis in particular as a critical method that could inform computing freedom through engagement with the Ljubljana School's mobilization of it by way of Jacques Lacan, as well as by studying the history of its clinical practice. On account of his deep engagement with set theory and the history of 20#super[th] century mathematics-- an engagement that volunteers his project as potentially germane to Computer Science, given the discipline's shared investment in that lineage-- I would also like to engage more seriously with the life and work of militant Maoist French philosopher, Alain Badiou.

In the multimodal pedagogy branch of the ICF, I hope to continue to be inspired by my students to produce the resources and public-facing writing that I believe will be most constructive for their critical education. 
Erika Bussman and I intend to offer our online seminar #quote[#link("https://cftw.ohrg.org/")[_Capital_ for Tech Workers]] again in 2026 given the interest expressed in its first iteration.
In my livestream work, my primary aspiration at present is to develop content that demonstrates how the real-world process of coding a system such as rostra or acta can and should proceed by way of substantive philosophical and critical reasoning, effectively marrying the #link("https://www.youtube.com/watch?v=tXx8Tu24RWo&list=PLKML-_b5aqpNrpFe26AIg0NIfxfgIcPH7")[coding livestreams] with the #link("https://www.youtube.com/watch?v=1AFQbDXe2Vc&t=3096s")[critical theory livestreams] that I have done.

My work developing practical software has been driven by the needs of investigations at #link("https://liminal-lab.org/")[LIMINAL Lab] and #link("https://forensic-architecture.org/")[Forensic Architecture], and in my future work in this branch, I will continue to think carefully about when and how it makes sense to abstract frameworks from our case-specific work in the form of tools such as rostra and acta.  
I am also committed to building a suite of tools to complement rheo which enables academic writing, research, and publication to be done more fluidly and freely. 
An idea that I have for one such tool is a Unix-based document storage system that can be accessed through the browser inspired by #link("https://www.devontechnologies.com/apps/devonthink")[DEVONthink], a Mac-only indie software for organizing PDFs, notes, and other files.

== Summary
In summary, the Initiative for Computing Freedom-- which I intend to someday progress to becoming an Institute in its own right-- is the rubric for the research that I would be thrilled to continue at #department.
It consists of three co-constitutive branches 1) critical theory, 2) multimodal pedagogy, and 3) practical software, which cohere to more rigorously comprehend the politics of computing freedom in the 21#super[st] century.

As Marx famously pronounced in his _Theses on Feuerbach_: #quote[The philosophers have only _interpreted_ the world in various ways. 
The point, however, is to _change_ it.]
The Initiative in Computing Freedom aspires to change the world for the better by acknowledging the necessity of critical interpretation in any consequential understanding of what it means to be free today.
