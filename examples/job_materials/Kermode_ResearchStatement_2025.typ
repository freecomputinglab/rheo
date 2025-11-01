#import "./lib/aspirationally.typ": aspirationally

#let department = [the School of Information at the University of Michigan]
#let name = [Lachlan Kermode] 
#let online-materials-link = "https://ohrg.org/materials/michigan"

#show: aspirationally(
  name: [Lachlan Kermode],
  title: [Teaching Statement],
  current-department: [Department of Modern Culture and Media | Computer Science],
  has-references: true,
  bib-references: "./references.bib",
  leader: [Note: This document contains hyperlinks to external websites that are indicated by blue, underlined text. The statement has been written to read fluidly without needing to visit any of the links, but they are included to provide further context for the interested reader.],
)[
  The central aim of my research and publications is to understand, evaluate, and reconceptualize the role that computing plays---and _should_ play---in society.
  Drawing on philosophy, history, and literary theory, I study how computing both works and doesn't work in society by critically examining modern conceptions of freedom, as they are operative in software production and also more generally.
  As an interdisciplinary scholar trained both in the humanities (Ph.D. Modern Culture and Media, Brown) and in information science (Sc.M. Computer Science, Brown; A.B. Computer Science, Princeton), I seek to contribute fully to both sets of disciplines through three categories of academic output: 

  + *The publication of conceptual work* on computing freedom---in the fields of philosophy, history, and literary/critical theory @kermodeDeconstructingUrbitPolitics2022a @kermodeItStupidThink2024 @kermodeScreeningSubject2025 @kermodeOneZeroCapital2026.
  + *The practice of new pedagogy* towards computing freedom---within the walls of the academy and beyond it through online and public-facing work @kermodeCSSocialChange2023 @kermodeCapitalismComputersEra2024 @kermodeCapitalTechWorkers2025 @kermodeMarxismFormFredric2025 @kermodeGradLog2025.
  + *The production of practical systems* realizing computing freedom---published as software that is actively used by researchers and presented in HCI and computing ethics venues @kermodeTimemap2020 @kermodeObjectsViolenceSynthetic2020a  @dcruzDetectingTearGas2022a @kermodeMtriage2023.

  My work has been exhibited in art galleries such as the San Francisco #link("https://forensic-architecture.org/programme/exhibitions/uncanny-valley-being-human-in-the-age-of-ai")[de Young Museum], New York City's #link("https://forensic-architecture.org/programme/exhibitions/triple-chaser-at-the-whitney-biennial-2019")[Whitney Museum of American Art], London's #link("https://www.tate.org.uk/whats-on/tate-britain/exhibition/turner-prize-2018")[Tate Modern], Germany's #link("https://critical-zones.zkm.de/#!/detail:cloud-studies")[ZKM], and Aotearoa New Zealand's #link("https://artspace-aotearoa.nz/")[Artspace]. 
  I have published in both humanities and computer science academic venues, and my work has been recognized through fellowships at the #link("https://www.iwm.at/")[IWM in Vienna], #link("https://fi2.zrc-sazu.si/en")[ZRC SAZU] in Ljubljana, the University of Auckland, and through the #link("https://graduateschool.brown.edu/phd-experience/interdisciplinary-research/open-graduate-education")[Open Graduate Fellowship] at Brown University. 
  I have presented research in diverse and interdisciplinary venues such as the #link("https://www.bbk.ac.uk/annual-events/london-critical-theory-summer-school")[Birkbeck Critical Theory Summer School], the #link("https://www.americanacademy.de")[American Academy] in Berlin, the #link("https://augmented-authorship.ch/")[Lucerne University of Applied Sciences], and the #link("https://toronto-geometry-colloquium.github.io/")[Toronto Geometry Colloquium], among others.
  I have taught original courses as the Instructor of Record in the departments of both Computer Science and Modern Culture and Media at Brown University; I have given seminars and workshops in art museums, architecture schools such as the #link("https://www.aaschool.ac.uk/")[Architectural Association] in London, and online; and open source software that I have written has been used by researchers investigating human rights abuses in Ukraine, Palestine, the United States, Northern Africa, and the Mediterranean.

  = Research background and motivation 
  After completing my undergraduate degree in 2018, I moved to London to work for the interdisciplinary human rights research agency at Goldsmiths, University of London, #link("https://forensic-architecture.org/")[Forensic Architecture] (hereafter FA).
  Over the next few years, I contributed to #link("https://forensic-architecture.org/about/team/member/lachie-kermode")[more than fifteen investigations] into human rights abuses across the globe, ranging from military malfeasance in #link("https://forensic-architecture.org/investigation/destruction-and-return-in-al-araqib")[Palestine], #link("https://forensic-architecture.org/investigation/the-destruction-of-yazidi-cultural-heritage")[Iraq], and #link("https://forensic-architecture.org/investigation/the-battle-of-ilovaisk")[Ukraine], to #link("https://forensic-architecture.org/investigation/police-brutality-at-the-black-lives-matter-protests")[police brutality at the 2020 BLM protests], to #link("https://forensic-architecture.org/investigation/triple-chaser")[the negligent export of 'non-lethal' weapons by the ultra-rich].  
  As the sole full-time software specialist at FA for most of my time as an Advanced Software Researcher there (2018--2021), I was responsible for activities ranging from developing #link("https://www.digitalviolence.org/#/explore")[new interactive platforms] for investigations, conceptualizing and producing code where required for exhibitions, migrating between email providers, maintaining #link("https://forensic-architecture.org/investigation/the-enforced-disappearance-of-the-ayotzinapa-students")[live platforms] developed before my time, and developing a critical framework for the practice of #link("https://forensic-architecture.org/subdomain/oss")[software in general] and #link("https://forensic-architecture.org/investigation/cv-in-triple-chaser")[AI in particular] as part of FA's 'counter-forensic' method.

  My work at FA forms the backdrop for my research motivation as it provides a concrete example of the critical practice that I would seek to continue at #department. 
  (I have remained a Research Fellow at the agency since my departure in 2021.)
  One of the initiatives of which I am most proud from my time at FA is the #link("https://forensic-architecture.org/subdomain/oss")[Open Source Software Initiative] that I have stewarded from 2020 onwards.
  Prior to my arrival, FA had no obviously public source code or assets related to investigations, a characteristic that struck me as odd given that the organization positioned itself as #quote(link("https://forensic-architecture.org/about/agency")[born in the 'open source revolution']).
  The phrase 'open source' in journalism, I learned, represents a different ethic than the same phrase in software communities.
  Whereas I understand open source software as a deradicalization of free software's insistence on the legally innovative copyleft licensing requirement, and a characterization that implies a system's source code is freely available for public inspection, in journalism 'open source' refers to the use of sources (information and informants) that are exclusively or predominantly publicly accessible.
  The distinct genealogies of the phrase in journalism and software produced, for me, a tension in FA's self-presentation as an open source research agency.
  The open source software #link("https://github.com/forensic-architecture/timemap")[timemap], a project that now has more than 350 stars on GitHub and has been forked and used by human rights groups such as Bellingcat to #link("https://ukraine.bellingcat.com/")[document ongoing civilian harm in Ukraine], is a standalone frontend application that I built and open sourced (in the software sense) to address this tension.

  The parable above reveals that complexity can hide in seemingly simple turns of phrase such as 'open source'. 
  The recognition that there are political stakes to software's semantic issues is the conceptual grounding for the multimodal approach that I take to computing research.
  As I experienced firsthand through my work at FA, a self-consciously efficacious political practice, in computing research as in anything else, must take philosophy and critical theory seriously.
  To take meaningful action on a street, one must know where the street stands in the symbolic (or social) order of things.
  Why would taking action on _this_ street---or, in a more apposite example, developing and releasing _this_ kind of software---result in the change one seeks to be in the world?
  To answer this question effectively, to avoid acting in a vacuum of expected outcomes, we need a critical theory of language (and code) which is attuned to how the question concerning technology has shaped social and political meaning throughout history.

  == 1) Conceptual work 
  Through coursework and research for my dissertation in the Department of Modern Culture and Media at Brown University (2021--26), two critical traditions have become essential to my thinking about the nature of freedom in computing and social life: the *critique of political economy* and *psychoanalysis*, methods shorthanded by the monikers Marx and Freud respectively. 
  Over the course of the long 20#super[th] century, Marx and Freud have consistently featured in an astonishing set of practical and philosophical uprisings, from the social history of the Soviet Union and China to the literary theory of Fredric Jameson and Alain Badiou.
  The materialist and psychoanalytic accounts of the subject and society in Marx and Freud can (still) serve as critical starting points for both 1) a *political theory of computation* to show us the role that the computer _should_ play in a society where freedom is flourishing rather than deprecated or defunct, and 2) *a critically grounded practice of software production, development and maintenance* that does not so easily succumb to the capitalist fantasy of value.

  My dissertation and first book project, _From One to Zero: Capital, Calculus, and the Cradle of Computer Science_, argues that Marx's philosophy in _Capital_, the _Grundrisse_, and---crucially---the mathematical manuscripts that he worked on concurrently alongside his critique of political economy (1858--83) provide a thoroughgoing critique of automation's function in modern society that still has relevance in an age of AI. 
  I argue that the computer is a concept that casts a long historical shadow by showing that it bears essential similarities, when we see it as a structure of automation rather than a historically specific substance, as Alan Turing did in his seminal work on the matter, to what Marx calls a machine.
  The machine for Marx is a structure of automation through which all movement is measured as work, and thus all desire in society is distorted such that it can be counted out as cash.
  Though it finds new footing in the fantasies and fears associated with language models and neural nets in our time, there is thus a greater precedent for critique of a society besotted with computation than might at first be thought.
  Projects that fantasize about computation's potential impact on the production of capitalist value, my dissertation shows, date back at least to Charles Babbage, whose factory travelogues Marx explicitly criticizes in chapter 15 of _Capital_ Volume I.
  The fact that Marx's 19#super[th]-century critique of political economy comprises a firm notion of the computer shows that the problems it presents in society are in fact not quite as novel or unprecedented as they are made out to be.

  In my second book project, I want to show how both Marx's theory of capitalism and psychoanalysis can serve as _practical_ frameworks through which we, as software developers and computer scientists, can take action on the street today.
  Psychoanalysis is, I believe, the intellectual tradition that has most seriously and successfully taken up Marx's mantle in developing a theoretical practice of an anti-capitalist ethics.
  Sigmund Freud's discovery of the unconscious through his clinical practice in the 20#super[th] century revealed the same distinctive feature of modern subjectivity that Marx sketched out in his critique of political economy: the human subject cannot, logically speaking, simply be a self-contained rational totality, or as political theorist Wendy Brown puts it, _homo oeconomicus_ @brownUndoingDemosNeoliberalisms2015.
  The modern subject is not capable of uncovering every inch of itself, but is rather split and scarred in a fundamental sense.
  We cannot know everything about ourselves, nor even can we be sure of what it is that we _do_ seem to know. 
  This psychoanalytic insight into the nature of our own incompleteness is the substance of Freud's discovery and the premise of his clinical practice.
  As I argue in my dissertation, Marx's philosophy also implicitly recognizes this contradiction at the heart of modern subjectivity.
  Psychoanalysis thus puts Marx's philosophy on its feet and asks the question: what should we do with ourselves now that we know we can never know everything about ourselves?

  On account of the curious nature of modern subjectivity and the semantic subtleties of software freedom that I allude to in my opening anecdote, we cannot rely simply on 'technical' characterizations of computing systems to produce rather than prohibit freedom.
  Though I support FLOSS (Free/Libre Open Source Software), decentralized, local-first, federated, and/or privacy-preserving systems, no single class of technical architecture is self-sufficient as a proxy to ensure that a piece of software will do unqualified good when it is put to work in the world. 
  Rather, we must study history and philosophy, both of computing and more broadly, to more concretely conceive of the social and political consequences that software systems effect.
  To give a specific and particularly topical example: the problem of how we should (or shouldn't) use LLMs in the university could be studied with reference to Marx's critical theory of value (use-value, exchange-value, etc.) to better understand the risks of ceding pedagogical infrastructure to private interests, and of delegating jobs once done by humans to a machine. 
  Critical theory can be the basis of a more robust guide for our practice as developers, policymakers, and computing specialists.

  I have presented work arguing this point in recent years at venues such as #link("https://www.historicalmaterialism.org/event/twenty-second-annual-conference/")[Historical Materialism], #link("https://lackorg.com/2025-conference/")[LACK], and the #link("https://caiml.org/dighum/")[TU Wien Digital Humanism] circle, seeking to speak to computer scientists and humanities folk alike.
  In January 2026, I will present a series of seminars as a visiting fellow at the #link("https://fi2.zrc-sazu.si/en")[ZRC SAZU Institute of Philosophy] in Ljubljana arguing that a contemporary theory of the subject cannot wall it off from its work in the world.
  This dialectic of self/society appears in Marx's mature work through the notion of labor, and in Freud and psychoanalysis more generally through the thematization of the unconscious.
  I have work under review at both #link("https://criticalinquiry.uchicago.edu/")[Critical Inquiry] (_The cybernetic conjecture_) and #link("https://direct.mit.edu/octo")[October] (_The machine that therefore I am_), both attached as writing samples in this dossier, which are representative of the journals in which I aim to publish theoretical research.  
   
  == 2) New pedagogy 
  The second arm of my research develops new pedagogy in computer science, software engineering, and critical theory which takes into account the material entanglement of capitalism and computing, particularly as it manifests at the North American university.
  I include this pedagogy here as I see it as integral to the other conceptual and critical work I discuss in this research statement.
  I focus below exclusively on the rationale and research outputs of my _public_ teaching and scholarship beyond the university, as I go into more detail regarding my experience as an institutional instructor in the accompanying teaching statement.

  Throughout my Ph.D., I have taken numerous courses at the 'para-academy' #link("https://www.bicar.org/")[BICAR] with #link("https://www.bicar.org/rohit-goel")[Rohit Goel], culminating in a four-week intensive seminar with six participants in the summer of 2023 studying psychoanalytic critique in Bombay, India.  
  These courses impressed upon me the idea that public-facing pedagogy has an impact that can complement and augment teaching and writing in the university setting by reaching students and readers beyond the academy.
  At the same time, I believe that the university occupies an essential space in the topology of modern society, one where it is plausible to practice a unique syntax of freedom, academic freedom, against capital's vociferous demands to abolish it.

  My public-facing work and pedagogy are aligned with the conceptual work I outline above in their attention to the lineaments of a computing practice which produces a surplus of freedom.
  Three examples of new pedagogy in my recent work are:

  + *Online courses.* In the summer and fall of 2025, I offered an experimental online seminar titled #quote[#link("https://cftw.ohrg.org/")[_Capital_ for Tech Workers]] in collaboration with Erika Bussman, a software engineer at Google.
    The course considered the extent to which Marx's critique in _Capital_ travels to the contemporary context and conundrum of technology companies and the students' practice, white- and blue-collar alike, as a part of them.
    Unlike in the university setting, the students in this course were professional engineers, product managers, founders and investors from companies and startups in Silicon Valley such as Google and Meta.
    We aspired to give students the conceptual tools to understand the first ten chapters of _Capital_ on its own terms, but also to read Marx in a way that would be relevant for their own ongoing labor in the software industry.
    One key takeaway from the first iteration of this course was that chapter ten (_The Working-Day_) resonated more directly with the concrete lives of tech workers than the opening chapters of _Capital_ (which are infamously dense and philosophical).
    Erika and I will run a follow-up course on _Capital: Volume I_ from chapter ten onwards starting in January 2026, for which we will develop and provide a guidebook for concepts we have already covered titled _Capital for Tech Workers: Chapters 1-9_ to respond to this particular learning.

  + *Public writing.* Since the start of 2024, I have maintained a 'grad log' of various writings at #link("https://www.ohrg.org/")[https://ohrg.org].  
    Many of these logs are intended as reading notes or introductory assessments of critical theory that I find important rather than as argumentative papers intended for publication.
    I also provide resources for students such as my '#link("https://www.ohrg.org/writing-academic-essays")[Writing Academic Essays]' guide, reflections on teaching and the running of reading groups such as '#link("https://www.ohrg.org/24-01-29")[Why we should give feedback to students]', and meditations on the trials and tribulations of running Linux such as '#link("https://www.ohrg.org/typst/writing-in-typst")[Writing in Typst]'.

  + *Teaching livestreams.* Inspired by educational content such as #link("https://www.youtube.com/c/JonGjengset")[Jon Gjengset's marathon Rust coding streams], I have recently ventured to livestream the reading and teaching of difficult texts such as Fredric Jameson's _Marxism and Form_ and Eric Santner's _The Royal Remains: The People's Two Bodies and the Endgames of Sovereignty_ in two- to three-hour seminars on #link("https://www.youtube.com/@LachieKermode")[my YouTube channel].

  These modes of non-traditional teaching and publication represent my commitment to freedom beyond its preconceived institutional understanding as academic freedom alone.
   I outline in the accompanying teaching statement my complementary commitment to reconceptualizing the pedagogy of critical theory and CS _within_ the academy.
  I intend to publish papers reflecting on this pedagogy's success (or failure) at CS education conferences such as #link("https://sigcse.org/")[SIGCSE] in the future.

  == 3) Practical systems 
  The third arm of my research develops practical software systems that aim to advance freedom in the real world.
  I am actively working on three software projects that will be open sourced and published at an HCI or otherwise appropriate conference venue.

  + *Rostra.* 
    Through my work developing and maintaining #link("https://github.com/forensic-architecture/timemap")[timemap] (2018-2022), several problems became apparent even as the software proved useful for FA and other organizations.
    Like timemap, rostra is a frontend framework to contextualize and correlate time-series events in time and space by plotting them cartographically.
    Unlike timemap, rostra is a _modular_ framework which is _additively_ configured, meaning that a deployment can selectively include panels for a timeline and other forms of data visualization from a library ecosystem.
    I first saw the need for rostra through work on the #link("https://www.adamartgallery.nz/exhibitions/archive/2020/violent-legalities")[Violent Legalities exhibition] in Aotearoa New Zealand in 2020, secured its concept through subsequent exhibitions in #link("https://artspace-aotearoa.nz/exhibitions/slow-boil")[Auckland 2021] and #link("https://www.mutualart.com/Exhibition/The-Moral-Drift/6B553CE14552BAD4")[Tauranga 2022], and began development in earnest in partnership with #link("https://profiles.auckland.ac.nz/k-muller")[Karamia Muller]'s group at the University of Auckland in mid-2025. 

  + *Acta.* In 2024, I began working with #link("https://www.unibo.it/sitoweb/lorenzo.pezzani")[Lorenzo Pezzani] at the University of Bologna, the director of #link("https://liminal-lab.org/")[LIMINAL Lab], to visualize #link("https://www.hrw.org/video-photos/interactive/2022/12/08/airborne-complicity-frontex-aerial-surveillance-enables-abuse")[the correlation between drone surveillance and migrant pushbacks] in the Mediterranean. 
    Working from a redacted dataset that was retrieved through freedom of information requests to Frontex, the EU border control agency, I co-designed and built a platform to present XLSX data more intelligibly by temporally correlating it with other data sources such as aerial asset flight hours and social media reports of certain pushbacks (forthcoming 2026).
    Acta is a framework for describing the political import of any time-series spreadsheet by visually aligning it with other data sources and by narrativizing its possible redactions.
    Acta is conceived as part of the same suite of investigative human rights tooling as rostra, which is used by agencies such as FA and Bellingcat.

  + *Rheo.*
    More recently in 2025, I have begun collaborative work with #link("https://willcrichton.net/")[Will Crichton] (Assistant Professor at Brown University) investigating the potential of #link("https://typst.app/")[Typst] as the basis for a more pragmatic document authoring and publishing pipeline.
    By making several #link("https://github.com/typst/typst/pulls?q=is%3Apr+is%3Aclosed+author%3Abreezykermo")[contributions to the upstream Typst codebase], I enhanced Typst's capabilities to export document structure such as bibliographic entries and citations to an HTML document, a compilation target that is secondary to Typst's full-featured support for PDF.
    Rheo is a static site and experimental typesetting engine based on Typst that will eventually support PDF, HTML, EPUB, and #link("https://willcrichton.net/notes/portable-epubs/")[Portable EPUB] with richer semantics in the latter format than standalone Typst.
    It is envisioned as a tool that will enable more freedom in the domain of document dissemination in line with the original vision of the Internet as a mechanism for lively and reasonably unfettered academic exchange, rather than the densely commercial space of 'platform capitalism' that it has become. 
    (An early prototype of rheo is what powers the ability to #link(online-materials-link)[read all the materials in the dossier as HTML documents].)

  == Future work
  In my conceptual work thus far, I have primarily focused on the relevance of Marx's and Freud's philosophy for the project of computing freedom.
  In future work, I seek to deepen my appraisal of psychoanalysis as a critical method by engaging with the Ljubljana School's philosophical inflection of the method via the midcentury French philosopher, Jacques Lacan.
  Alain Badiou, a militant Maoist philosopher who came of age during the years in which Lacan presented his infamous seminars (1953--80), also offers important resources for a political theory of computing freedom on account of his deep engagement with set theory and history of 20#super[th]-century mathematics.

  I hope to continue to be inspired by my students to produce new pedagogy for critical education. 
  Specifically, I would like to develop my livestream work to show how the real-world process of coding a system such as rostra or acta can proceed by way of substantive philosophical and critical reasoning, effectively marrying #link("https://www.youtube.com/watch?v=tXx8Tu24RWo&list=PLKML-_b5aqpNrpFe26AIg0NIfxfgIcPH7")[coding livestreams] with #link("https://www.youtube.com/watch?v=1AFQbDXe2Vc&t=3096s")[critical theory livestreams] that I have done.

  My work developing practical systems such as rostra and acta is driven by the needs of investigations at #link("https://liminal-lab.org/")[LIMINAL Lab] and #link("https://forensic-architecture.org/")[Forensic Architecture], as I work closely with both organizations.
  I am also committed to building a suite of tools complementing rheo to enable academic writing, research, and publication that is more fluid and more free. 
  One idea that I have for such a tool is a Unix-based document storage system that can be accessed through the browser, inspired by #link("https://www.devontechnologies.com/apps/devonthink")[DEVONthink], a Mac-only indie app for organizing PDFs, notes, and other files.

  == Summary
  My research program is trained on the simultaneous conceptualization and execution of a more critically attuned computing practice in academia and in industry in modern society. 
  I would be thrilled to continue with my program to more rigorously practice a politics of computing freedom in the 21#super[st] century at #department.

  As Marx famously pronounced in his _Theses on Feuerbach_: #quote[Philosophers have hitherto only _interpreted_ the world in various ways; 
  the point is to _change_ it.] @marxThesesFeuerbach1845
  My research program aspires to change the world for the better by acknowledging the necessity of critical interpretation in any consequential understanding of what it means to be free today.
]
