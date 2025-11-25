/*
Core Back-end Team
Description

As a member of the Backend team, you will be a graceful conductor of all the data flowing through the Kagi system. We are the beating drum that keeps everything running like clockwork and the harmony that brings all of our team's work together into a little package in the user's browser. Our skillset is wide, and our responsibility is huge.

Our Backend ethos is distilling the work that needs to happen into its simplest possible components and stringing them together. We have a critical eye for dependency and hidden complexity. We have no overarching frameworks that we use - we make the rules, and aren't afraid to break them when there is context that we can exploit and optimize against.

You will wrangle a novel, highly concurrent runtime to deliver the world's best search results as fast as possible, and help Kagi further refine its quality, robustness, and taste. Your Kagi colleagues will be counting on you to deliver their hard work  - from SSR'd frontend, to LLM token streams - to our loyal customers in a robust, and debuggable fashion.

Responsibilities

- Develop and own features and business logic operating at the heart of the Kagi experience end to end
- Continuously identify improvements, simplifications, and optimizations in our workflows
- Build development tooling and provide support for other Kagi teams that integrate deeply with our backend
- Work with our infra team to install observability (metrics, logs) to ensure stability and give business insights
- Debug production systems when issues arise to identify impact and root cause
- Proactively respond to internal and user feedback to rapidly address bugs and minor feature changes

Requirements

- Thrives in a fully remote, globally distributed team, with ruthless communication habits
- Experience with our core technologies:
    - Crystal (or equivalent sister language experience in Go or Ruby)
    - Python
    - HTML/JS/CSS
    - PostgreSQL
    - Redis
    - GCP
    - Docker
    - Sentry
    - Prometheus/Grafana
- Deeply familiar with the lower level details that our systems are built on top of (OS, networking protocols, ...), unafraid to open black boxes to see how they work
- Comfortable operating without a heavy framework or ORMs. You know and can implement web standards, and are happy writing raw SQL

Preferred qualifications

- CS degree or veteran industry experience (>5 years)
- Comfortable building frontend skeletons or prototypes for our FE team to polish
- Experience with writing FFI bindings and/or familiarity with C
- Worked with actor-based architectures and/or structured concurrency systems
- Familiarity with high level compiler architecture
- Shipped software that uses a GC, with an eye for code that creates unecessary GC waste
- Built software that integrates with modern LLM APIs
- Experience integrating with Stripe

When applying, please focus on crafting a compelling cover letter. This document serves as your personal introduction - revealing to us who you are, why you aspire to join Kagi, and what drives your professional journey and what your ambition is. Use this opportunity to articulate your story and demonstrate why your talents would enhance our organisation. 
*/

#import "lib.typ": aspirationally

#show: aspirationally(
  name: [Lachlan Kermode],
  title: [Cover Letter],
  current-department: [Brown University],
  has-references: false,
)[

  == Kagi Cover Letter: Lachlan Kermode (Core Back-end Team)

  I have been using Kagi Ultimate as my primary search engine and AI assistant since March 2025, and it is no exaggeration to say that it has changed the way I think about information and the Internet. 
  As a Ph.D. student at Brown University working on distributed systems, dev tools, and the history of the Internet, I critically examine my computer use on a regular basis.
  In 2018, as the primary software developer and researcher at the human rights research agency #link("https://forensic-architecture.org")[Forensic Architecture], I leaned into vim in an attempt to navigate code more fluidly.
  In 2021, as senior software developer at the now-unicorn New Zealand startup #link("https://www.halterhq.com/")[Halter], I was one of the few backend developers who ran Arch Linux instead of MacOS through a commitment to dog-fooding the OS (Linux) on which our production systems. 
  (Halter supported Arch because a subset of firmware engineers used it, and so this was a carefully considered choice rather than a brash one that generated unreasonable difficulties for the platform team.)
  In 2023, in grad school experimentation, I switched to running NixOS as my daily driver; in 2024, I moved to using #link("https://ergodox-ez.com/")[ZSA split keyboards] regularly for ergonomics and customization; and in early 2025, I finally bought a #link("https://frame.work/")[Framework laptop] to replace my Dell XPS after listening to an #link("https://oxide-and-friends.transistor.fm/episodes/framework-computer-with-nirav-patel")[interview with its founder, Nirav Patel], on the #link("https://oxide-and-friends.transistor.fm/")[Oxide and Friends podcast].   

  I enumerate this set of esoteric technology adoptions because I believe it demonstrates my commitment to making technology _personal_ and _usable_.
  These qualities are what I find refreshing, impressive, and empowering about Kagi.
  They appear against the grain of the reigning paradigm and business model in Internet technologies today, what we could call their _over_-commercialization, that Vladimir Prelovac critiques in his #link("https://blog.kagi.com/age-pagerank-over")[first PageRank blogpost], and that still drives research and development at Kagi as one can see from Matt Ranger's excellent #link("https://blog.kagi.com/llms")[recent post on LLMs].   
  My professional background is as a full stack software engineer with a backend focus on ML engineering and distributed systems, and I have done some research over in grad school on both streaming execution engines such as #link("https://flink.apache.org/")[Apache Flink] and also on vector databases such as #link("https://qdrant.tech/")[Qdrant], and have a Sc.M. in Computer Science from Brown that I received through being accepted to the #link("https://graduateschool.brown.edu/phd-experience/interdisciplinary-research/open-graduate-education")[Open Graduate fellowship].
  But I came to grad school as a Ph.D. to study the history of the Internet and computing in addition to these technical particulars, as I do not believe that we can easily divorce the political and economic context in which we write software from the 'hard' skills writing code (and perhaps more importantly now, especially with the rise of LLMs, _reviewing_ code) that we undoubtedly need to master as software engineers. 
  When we write new software, we need to reason with the technicalities of a programming language, hardware constraints, the implications of architectural design decisions, and so on: but we also need to think carefully about _why_ we are writing the software in the first place, how we hope it will be used, and what effect we hope it will have on the world.
  To reason meaningfully about this second set of complexities, software developers, I believe, would do well to turn to the history of programming languages, the Internet, and of the politics and legislation of technology more generally.  
  This history---of programming languages like Rust and Ocaml, and of the discipline of Computer Science---is what I have studied and written about in my Ph.D. in the department of Modern Culture and Media at Brown.

  As I am now applying to roles in both industry and academia as I finish my Ph.D., I can quite genuinely think of few companies at which I would be more excited to work than Kagi. 
  This is not only because Search and Assistant are fantastic products that have truly allowed me, at last, to stop using Google.
  (Like many others, I have repeatedly failed to substantively switch it out for an alternative such as DuckDuckGo since the beginning of my professional career in software.)
  It is also because of the ethos that Kagi exudes as a company, across its products, blog posts, and Discord presence.
  Kagi's ethos seems to me to be one of the closest approximations of the original 1990s ethos of the World Wide Web, and the Internet before that through the 70s and the 80s, that is available as a company culture in the contemporary landscape. 
  As I am now based in Italy as I finish my PhD (and with the intention of staying here going forward), Kagi's remote-first culture is deeply important to me.
  Though there are undoubtedly tradeoffs, I believe---perhaps controversially---that the most forward-thinking technology companies today are all remote-first.  
  Kagi's open source presence in the release of tools such as `smallweb`, `kagimcp`, and the small-but-mighty `ask` CLI all index this ethos.
  These and other repos show me that Kagi's rhetoric about empowering users is not just rhetoric, as it is in so many other tech companies.
  Kagi says it aims to empower users _and it also actually acts_ to empower them. 

  I believe that I would be a good fit for Kagi because I have broad experience with backend engineering in small-to-medium sized companies (Forensic Architecture, Halter, and the various other kinds of software work I have done over the years), and because I have specialized in that work in communicating between various stakeholders such as backend engineers, machine learning engineers, and even architects to architect systems that scale with respect to a company's needs.
  While I have no direct experience in Crystal, most of my professional coding has been in Javascript, Python, and Java, from which experience I believe I could pick it up quickly. 
  I have used Rust for almost all of my grad school projects in the past five years and am quite comfortable in that language, and have built on GCP with Docker, Redis, PostgreSQL, and Grafana before (most notably at Halter, but also through contract-work with #link("https://openmeasures.io/")[OpenMeasures]). 
  Through databases work with Ugur Cetintemel at Brown on vector databases and in other Rust projects, I also have experience working without frameworks, working at the FFI boundary (between C++ and Rust), and building software that integrates with modern LLM APIs.
  As another example, in 2025 I took a graduate seminar on Datacenter Operating Systems, where we looked at OSes such as #link("https://barrelfish.org/")[Barrelfish] and kernel-bypass systems built with DPDK. 

  Most recently, I have been working with #link("https://willcrichton.net/")[Will Crichton] on leveraging #link("https://typst.app/")[Typst] to make a better document authoring workflow, a project space about which I wrote some #link("https://www.ohrg.org/typst/writing-in-typst")[preliminary thoughts on my blog]. 
  By making several #link("https://github.com/typst/typst/pulls?q=is%3Apr+is%3Aclosed+author%3Abreezykermo")[contributions to the upstream Typst codebase], I enhanced Typst's capabilities to export document structure such as bibliographic entries and citations to an HTML document, a compilation target that is secondary to Typst's full-featured support for PDF.
  Rheo, the tool we are building, is a static site and experimental typesetting engine based on Typst that will eventually support PDF, HTML, EPUB, and #link("https://willcrichton.net/notes/portable-epubs/")[Portable EPUB] with richer semantics in the latter format than standalone Typst, some of which is a continuation of Will's project #link("https://nota-lang.org/")[Nota].
  Rheo is envisioned as a tool that will enable more freedom in the domain of document dissemination in line with the original vision of the Internet as a mechanism for lively and reasonably unfettered academic exchange, rather than the densely commercial space of 'platform capitalism' that it has become. 
  (An early prototype of rheo is also what allows you to #link("https://ohrg.org/materials/kagi/cover-letter.html")[read this cover letter on the web], if you prefer that to a PDF.)

  Thank you for considering my application, and I look forward to hearing from you.

  #v(1em)

  Lachlan Kermode#linebreak()
  Ph.D. Candidate in Modern Culture and Media#linebreak()
  Sc.M. in Computer Science#linebreak()
  Brown University

]

