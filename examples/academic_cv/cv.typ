#import "lib/template.typ": *

#show: straightforward_cv.with(
  name: "Lachlan John Kermode",
  address: [Providence, RI],
  contacts: (
    [],
    [#link("mailto:lachlan_kermode@brown.edu")],
  ),
  footer-text: [Lachlan Kermode --- Page#sym.space]
)

// Personal website: #link("https://lachlankermode.com")[lachlankermode.com] #linebreak() 
// Open source contributions and code: #link("https://github.com/breezykermo")[github.com/breezykermo] #linebreak()
// Professional software resume: #link("https://lachlankermode.com/live/resume.pdf")[lachlankermode.com/resume]  #linebreak()
// Musical and artistic portfolio: #link("https://portfolio.lachlankermode.com")[portfolio.lachlankermode.com]

#align(center)[
  #grid(
    columns: (auto, auto),
    gutter: 0.6em,
    column-gutter: 1em,
    align: (left, right),
    [Personal website:], link("https://lachlankermode.com")[lachlankermode.com],
    [Open source contributions and code:], link("https://github.com/breezykermo")[github.com/breezykermo],
    [Professional software resume:], link("https://lachlankermode.com/live/resume.pdf")[lachlankermode.com/resume],
    [Musical and artistic portfolio:], link("https://portfolio.lachlankermode.com")[portfolio.lachlankermode.com],
  )
]

= Education
#experience(
  place: [PhD in Modern Culture and Media, Brown University],
  time: [2021--26],
)[
Dissertation committee:
- Joan Copjec (Modern Culture and Media, chair)
- Suresh Venkatasubramanian (Computer Science)
- Holly Case (History)
- Peter Szendy (Comparative Literature)
]

#experience(
  place: "ScM in Computer Science, Brown University",
  time: [2022--25],
)[
Completed concurrently with PhD as an #link("https://graduateschool.brown.edu/phd-experience/interdisciplinary-research/open-graduate-education")[Open Graduate Education] fellow.
Systems track with coursework in databases, operating systems, distributed systems and networking.
Advised:
  - Ugur Cetintemel (Computer Science)
]

#experience(
  place: "AB in Computer Science, Princeton University",
  time: [2013--18],
)[
Undergraduate thesis #quote[Towards critical computing: a template for animated documentary], advised:
- Arvind Narayanan (Computer Science)
]

== Professional experience 
#experience(
  place: [#link("https://liminal-lab.org/")[LIMINAL]],
  title: "Lead Software Researcher",
  time: [2024--], // 06.2024-
  location: "Bologna, Italy"
)[
Engineering software for human rights investigations regarding pushbacks on the Central Mediterranean, remote assistance in Northern Africa and data protection in the United Kingdom.
Leading open source initiative to develop free software tools for investigations building from the #link("https://github.com/forensic-architecture/timemap")[timemap] community.
]

#experience(
  place: [#link("https://forensic-architecture.org")[Forensic Architecture]],
  title: "Advanced Software Researcher | Research Fellow",
  time: [2018--], // 06.2018 - 02.2021
  location: "London, United Kingdom"
)[
Directed engineering for full stack platforms and machine learning workflows.
Contributed to more than 20 investigations and exhibitions through software development and analysis (see #link("https://forensic-architecture.org/about/team/member/lachie-kermode")[my person page] for more information).
Steered data management committee for digital security, initiated and led the #link("https://forensic-architecture.org/subdomain/oss")[Open Source Software] subdomain.
]

#experience(
  place: [Department of Computer Science - Brown University],
  title: "Research Assistant",
  time: [2022--23], // 06-09.2022-2023
  location: "Providence, RI"
)[
Developing and systematizing ethics curriculum for Brown undergraduate Computer Science program with Julia Netter (Philosophy) in 2022.
Auto-scaling mechanisms in Apache Flink and Kubernetes using Reinforcement Learning advised by Ugur Cetintemel (Computer Science) in 2023.
]

#experience(
  place: [#link("https://www.halterhq.com/")[Halter]],
  title: "Senior Software Engineer",
  time: [2021--22], // 02.2021 - 09.2021
  location: "Auckland, Aotearoa New Zealand"
)[
First and specialist machine learning engineer, architeced and built a feature store to compute real-time uplinks from IoT devices using Apache Flink.
Contributed to microservices in Java, Node and Python using SNS/SQS, DynamoDB, Postgres, Terraform and Concourse CI.
]

#experience(
  place: [#link("https://www.smat-app.com/")[Social Media Analysis Toolkit [SMAT]]],
  title: "Volunteer Research Engineer",
  time: [2021--22], // 09.2021-2022
  location: "Remote"
)[
Full stack development and MLOPs engineering for the SMAT website and associated tooling.
]

#experience(
  place: [#link("https://the-syllabus.com")[The Syllabus]],
  title: "Developer",
  time: [2020--21], // 04.2020-01.2021
  location: "Berlin, Germany"
)[
Designed and developed the frontend for two websites in Vue: #link("https://the-syllabus.com")[the-syllabus.com] and #link("https://cabinet.the-syllabus.com")[cabinet.the-syllabus.com] (by subscription only).
User-based authentication, account management (reset and deletion), payment via Stripe, and a page-display algorithm to simulate A4 pages on the web.
// In 2020 these websites were used by some 20,000 users.
]

#experience(
  place: [#link("https://merantix.com")[Merantix]],
  title: "Full stack engineer",
  time: [2019--21], // 01.2019-01.2021
  location: "Berlin, Germany"
)[
Develop tooling for deep learning workflows on a part time basis for Merantix ventures.
Working mainly in Python and the Jupyter notebook stack to augment tooling in industry DL.
Technologies include Kubernetes, GCP, Tensorflow, Jupyter notebooks, Svelte, Vue, and React.
]


// #experience(
//   place: [#link("https://directco.co.nz/")[Direct Drinks]],
//   title: "CTO and co-founder",
//   time: [2016--17], // 08.2016-03.2017
//   location: "Wellington, Aotearoa New Zealand (remote)"
// )[
// - Provide direct sales channels between large corporate suppliers and small resellers in the FMCG industry, disrupting a pipeline currently dominated by mid-tier redistributors.
// - As CTO, developed from scratch an end-to-end platform for web, mobile, and tablet.
// ]


= Academic Fellowships
#experience(
  place: [#link("https://fi2.zrc-sazu.si/en")[ZRC SAZU Institute of Philosophy]],
  title: [Visiting Research Fellow],
  time: [2026], // 06.2024-
  location: "Ljubljana, Slovenia"
)[
1-month fellowship presenting work in Ljubljana.
Project title: _From one to zero: capital, computers and the critique of calculus_.
]

#experience(
  place: [#link("https://www.iwm.at/")[Institute for Human Sciences]],
  title: [#link("https://www.iwm.at/fellow/lachlan-kermode")[Digital Humanism Junior Fellow]],
  time: [2024], // 06.2024-
  location: "Vienna, Austria"
)[
3-month fellowship at the institute in Vienna.
Project title: _Computer Science, cybernetics and the philosophy of error: the humanist critique of capitalism in an age of artificial intelligence_.
]

#experience(
  place: [Department of Architecture - University of Auckland],
  title: "Visiting Research Fellow",
  time: [2019--20], // 11.2019-03.2020
  location: "Auckland, Aotearoa New Zealand"
)[
Conceptualising and undertaking an investigation to recenter Muslim and Māori communities in Aotearoa New Zealand by documenting how a racialised figuration of 'the terrorist' persists in the wake of the Christchurch mosque shootings in March 2019.
In partnership with Forensic Architecture.
]

#experience(
  place: [#link("https://www.paideiainstitute.org/")[Paideia Institute]],
  title: [Digital Humanities Fellow],
  time: [2015--16], // 06.2015-02.2016
  location: "Rome, Italy"
)[
After an internship developing an iOS application in 2015, became the inaugural Digital Humanities Fellow, taking a year off from Princeton to head digital operations from the office in Rome.
After the fellowship's completion, sat on the institute's advisory board until 2020.
]

#experience(
  place: [#link("http://piirs.princeton.edu/")[Princeton PIIRS]],
  title: [Undergraduate Research Fellow],
  time: [2017], // 06.2017
  location: "Berlin, Germany"
)[
3-month fellowship to research contemporary media and surviellance art towards my undergraduate thesis, #quote[Towards critical computing: a template for animated documentary].
]

= Teaching
#experience(
  place: [#link("https://cftw.ohrg.org/")[_Capital_ for Tech Workers]],
  title: [Instructor, Online], // (MCM 0903E)
  time: [2025], // Fall 2024
)[
A free, online course taught with Erika Bussman (a software engineer at Google).
Students were tech workers, those actively engaged in the operational edifice of 'big tech', including engineers from Meta (Facebook), Google, startups, indendent game developers and software researchers. The primiary aim of this course is to consider the extent to which Marx's critique in _Capital_ travels to the contemporary context and conundrum of technology companies.]


#experience(
  place: [#link("https://cceai.ohrg.org")[Capitalism and Computers in the Era of A.I.]],
  title: [Instructor, Modern Culture and Media ], // (MCM 0903E)
  time: [2024], // Fall 2024
)[
Taught as a 3-hour seminar for upper-level undergraduates in Modern Culture and Media and Computer Science.
The first half of the course builds an understanding of important concepts in the history of the computer and Marx's critique of political economy.
The second half of the course examines texts that critically assess the state of the relationship between society and the computer. 
]

#experience(
  place: [#link("https://cs.brown.edu/courses/cs1951i/index.html")[Computer Science for Social Change]],
  title: [Instructor, Computer Science], //  (CSCI 1951I)
  time: [2022--23], // Spring 2023
)[
Taught twice in 2022 and 2023.
Students are placed in small groups to investigate a non-profit partnering organization's needs and to constructively contribute to their mission with software over the course of the semester.
In addition to project work, students reflect on the ethics of software and socially impactful work through weekly readings and group discussions.
]

#experience(
  place: [#link("https://cs.brown.edu/courses/csci1270/")[Database Management Systems]],
  title: [Teaching Assistant, Computer Science], // (CSCI 1270)
  time: [2023], // Fall 2023
)[
]

#experience(
  place: [#link("https://www.coursicle.com/brown/courses/MCM/0150/")[Theories of Modern Culture and Media]],
  title: [Teaching Assistant, Modern Culture and Media],  // (MCM 0150)
  time: [2023], // Fall 2023
)[
]

#experience(
  place: [#link("https://www.brown.edu/academics/modern-culture-and-media/courses-manual/full-course-listing")[Digital Media]],
  title: [Teaching Assistant, Modern Culture and Media], // (MCM 0230)
  time: [2022], // Fall 2022
)[
]

#experience(
  place: [#link("https://responsible.cs.brown.edu/")[Socially Responsible Computing]],
  title: [Instructor, Computer Science],
  time: [2022], // 09.2022
)[
1-week intensive for new Undergraduate Teaching Assistants in the Department of Computer Science.
]

= Exhibitions
#experience(
  place: [#link("https://www.104.fr/en/")[Centiquatre Paris]],
  title: [#link("https://www.104.fr/en/event/liminal-forensic-oceanography-border-forensics-from-sea-to-sky.html")[From Sea to Sky]],
  time: [2024], // 10.05 > 2024.11.03 
  location: "Paris, France"
)[
  In partnership with #link("https://liminal-lab.org/")[LIMINAL], exposes the aggressive methods of surveillance operations in the Mediterranean where more than 40,000 migrants have lost their lives at sea.
  Contributed to the design and development of a physical installation contextualizing redacted information from Frontex, the European Border and Coast Guard Agency.
  Select reviews:
  - #link("https://fisheyeimmersive.com/article/loeuvre-du-jour-from-sea-to-sky-de-liminal-forensic-et-border/")[Fisheye Immersive] 
]

#experience(
  place: [#link("https://chsi.harvard.edu/")[Harvard Collection of Scientific Instruments]],
  title: [#link("https://chsi.harvard.edu/exhibitions/surveillance")[Surveillance: From Vision to Data]],
  time: [2023--24], // 09.2023-06.2024
  location: "Boston, USA"
)[
Considers surveillance beyond the realm of cameras and their watchers, exposing the profound influence of data.
Contributed an original video on loop that introduced #link("https://github.com/forensic-architecture/mtriage")[mtriage], a software I developed to scrape public domain data.
]

#experience(
  place: [#link("https://www.artgallery.org.nz/")[Tauranga Art Gallery]],
  title: [#link("https://www.mutualart.com/Exhibition/The-Moral-Drift/6B553CE14552BAD4")[The Moral Drift]],
  time: [2021--22], // 10.2021-01.2022
  location: "Tauranga, Aotearoa New Zealand"
)[
In collaboration with Fraser Crichton and Malcolm Richards, this exhibition offers a partial history of Aotearoa's state care system, uncovering a legacy of abuse and the resiliency of the survivors who continue to seek justice today.
// https://www.artgallery.org.nz/exhibitions/id/1751
]

#experience(
  place: [#link("https://artspace-aotearoa.nz/")[Artspace Aotearoa]],
  title: [#link("https://artspace-aotearoa.nz/exhibitions/slow-boil")[Slow Boil]],
  time: [2021], // 05-09.2021
  location: "Auckland, Aotearoa New Zealand"
)[
In collaboration with kaupapa Māori community group and kai security advocates Boil Up Crew, as well as a group of contributing practitioners spanning architecture, community advocacy, design, food sovereignty, software and the visual arts.
Explored the relationship between the mahi ngā-kai/kai rituals, and tā wahi/notions of space, mana motuhake/sovereignty, and mapping.
Works were collectively produced and installed over the course of the exhibition, alongside existing investigative works by Forensic Architecture.
Select reviews:
- #link("https://architecturenow.co.nz/articles/review-slow-boil/")[Architecture Now] 
- #link("https://www.e-flux.com/announcements/394932/boil-up-crew-forensic-architecture-sky-hopinka-jumana-manna-slow-boil-collective-slow-boil")[e-flux] 
// https://artspace-aotearoa.magnolia-office.de/reading-room/open-on-saturday
// https://forensic-architecture.org/programme/exhibitions/slow-boil
]

#experience(
  place: [#link("https://critical-zones.zkm.de/#!/detail:cloud-studies")[ZKM | Karlsruhe]],
  title: [#link("https://forensic-architecture.org/programme/exhibitions/critical-zones-observations-for-earthly-politics")[Critical Zones: Observations for Earthly Politics]],
  time: [2020--21], // 02.06.2020
  location: "Karlsruhe, Germany"
)[
Dedicated to the critical situation of this fragile membrane of life.
Contributed with a project on the toxic clouds that are mobilised by state and corporate powers such as tear gas, herbicide and chlorine bombs. 
]

#experience(
  place: [#link("http://www.adamartgallery.org.nz/past-exhibitions/dane-mitchell_ken-ken-friedman_violent-legalities_julia-morison_/")[Adam Art Gallery]],
  title: [#link("https://www.adamartgallery.nz/exhibitions/archive/2020/violent-legalities")[Violent Legalities]],
  time: [2020], // 02.06.2020
  location: "Wellington, Aotearoa New Zealand"
)[
Cartographically chronologises three separate projects, each aiming to spatialise relationships between legislative activity and occurrences of violence to inspect enduring trends between settler-colonial law-making processes and state policies in Aotearoa/New Zealand.
Nominated to be showcased in the #link("https://www.ars.nz/violent-legalities/")[2020 Ars Electronica showcase] in Aotearoa New Zealand.
Select reviews:
- #link("https://www.scoop.co.nz/stories/CU2006/S00059/new-exhibition-maps-racialised-over-policing-and-law-changes-in-new-zealand.htm")[Scoop News] 
- #link("https://architecturenow.co.nz/articles/review-violent-legalities/")[Architecture Now] 
]

#experience(
  place: [#link("https://deyoung.famsf.org/")[De Young Museum]],
  title: [#link("https://forensic-architecture.org/investigation/model-zoo")[Model Zoo] in #link("https://forensic-architecture.org/programme/exhibitions/uncanny-valley-being-human-in-the-age-of-ai")[Uncanny Valley: Being human in the age of AI]],
  time: [2020], // 20.02.2020
  location: "San Francisco, USA"
)[
  // https://deyoung.famsf.org/exhibitions/uncanny-valley
Presented the research in computer vision and synthetic data from my work in investigations at Forensic Architecture.
Accompanying technical details are available at #link("https://forensic-architecture.org/investigation/detecting-tear-gas")[forensic-architecture.org/investigation/detecting-tear-gas].
Select reviews:
- #link("https://www.frieze.com/article/big-datas-deal-devil")[Frieze]  
- #link("http://www.artfixdaily.com/artwire/release/8348-groundbreaking-exhibition-uncanny-valley-being-human-in-the-age-o")[Artfix Daily] 
]

#experience(
  place: [#link("https://whitney.org/")[Whitney Museum of American Art]],
  title: [#link("https://forensic-architecture.org/investigation/triple-chaser")[Triple Chaser] in #link("https://whitney.org/exhibitions/2019-biennial")[Whitney Biennial 2019]],
  time: [2019], // 13.05.2019
  location: "New York, USA"
)[
Forensic Architecture's invited contribution with filmmaker Laura Poitras and Praxis Films presenting machine learning research I conducted in Forensic Architecture investigations.
Accompanying technical details are available at #link("https://forensic-architecture.org/investigation/cv-in-triple-chaser")[forensic-architecture.org/investigation/cv-in-triple-chaser].
Select reviews:
- #link("https://www.e-flux.com/journal/104/299286/climate-control-from-emergency-to-emergence")[T.J. Demos in e-flux].
- #link("https://www.newyorker.com/magazine/2019/05/27/the-whitney-biennial-in-an-age-of-anxiety")[New Yorker].
- #link("https://hyperallergic.com/500055/forensic-architecture-whitney-biennial/")[Hyperallergic].
]

#experience(
  place: [#link("https://www.tate.org.uk/visit/tate-britain")[Tate Britain]],
  title: [#link("https://forensic-architecture.org/programme/exhibitions/long-duration-split-second")[The Long Duration of a Split Second] in #link("https://www.tate.org.uk/whats-on/tate-britain/exhibition/turner-prize-2018")[Turner Prize nomination 2018]],
  time: [2018], // 13.05.2019 (note: date in LaTeX appears to be 2019 but exhibition was Turner Prize 2018)
  location: "London, United Kingdom"
)[
An exhibition produced for Forensic Architecture's nomination for the 2018 Turner Prize
Developed an interactive platform displaying more than 30Gb of point clouds and other citizen-mapped data in the Negev/Naqab desert.
Select reviews:
- #link("https://www.haaretz.com/israel-news/2022-01-31/ty-article-magazine/.premium/documents-reveal-israels-intent-to-forcibly-expel-bedouin-from-their-lands/0000017f-e30e-d9aa-afff-fb5e038a0000")[Haaretz] 
- #link("https://www.thetimes.com/article/forensic-architecture-the-human-rights-group-up-for-a-turner-prize-ss6b3qbwd")[The Times] 
- #link("https://www.theguardian.com/artanddesign/2018/apr/26/turner-prize-2018-shortlisted-artists-timely-probing-questions")[The Guardian] 
- #link("https://www.nytimes.com/2018/04/26/arts/turner-prize-nominees.html")[The New York Times] 
]

#experience(
  place: [#link("https://www.fridmangallery.com/")[Fridman Gallery]],
  title: [#link("https://ninakatchadourian.bandcamp.com/album/talking-popcorns-last-words")[Talking Popcorn's First Last Words] with #link("http://www.ninakatchadourian.com/index.php")[Nina Katchadourian]],
  time: [2019], // 03.2019
  location: "New York, USA"
)[
I was one of sixteen people interviewed from a wide variety of fields and areas of expertise, asked to address Talking Popcorn's last words from a particular disciplinary perspective.
The soundtrack played in a room with the burned, damaged carcass of the first machine standing on a black plinth upon which was written the machine's final pronouncement.
]

= Peer-reviewed Publications
#paper(
  [[submitted]],
  [The cybernetic conjecture],
  [2026]
)

#paper(
  [[submitted]],
  [The machine that therefore I am],
  [2026]
)

#paper(
  [#link("https://wacv.thecvf.com/")[WACV]],
  [#link("https://openaccess.thecvf.com/content/WACV2022/papers/DCruz_Detecting_Tear_Gas_Canisters_With_Limited_Training_Data_WACV_2022_paper.pdf")[Detecting tear gas canisters with limited training data]],
  [2021]
)

#paper(
  [#link("https://neurips.cc/")[NeurIPS]],
  [#link("https://arxiv.org/abs/2004.01030")[Objects of violence: synthetic data for practical ML in human rights investigations]],
  [2019]
)

= Public-facing Publications
#paper(
  [#link("https://www.iwm.at/publication/iwmpost/iwmpost-133-false-prophets-false-promises")[IWM Post]],
  [#link("https://www.iwm.at/publication/iwmpost-article/is-it-stupid-to-think-information-wants-to-be-free")[Is it stupid to think information wants to be free?]],
  [2024]
)

#paper(
  [#link("https://forensic-architecture.org/subdomain/oss")[Forensic Architecture OSS]],
  [#link("https://forensic-architecture.org/investigation/detecting-tear-gas")[Detecting tear gas: vision and sound]],
  [2020]
)

#paper(
  [#link("https://forensic-architecture.org/subdomain/oss")[Forensic Architecture OSS]],
  [#link("https://forensic-architecture.org/investigation/cv-in-triple-chaser")[Computer vision in Triple Chaser]],
  [2020]
)


#paper(
  [#link("https://forensic-architecture.org/subdomain/oss")[Forensic Architecture OSS]],
  [#link("https://forensic-architecture.org/investigation/timemap-for-cartographic-platforms")[Using timemap for cartographic platforms]],
  [2018]
)

#paper(
  [#link("https://www.ohrg.org/")[https://ohrg.org]],
  [Public blog site],
  [2018--]
)

= Conference or invited papers
#paper(
  [#link("https://www.historicalmaterialism.org/event/twenty-second-annual-conference/")[Historical Materialism]],
  [The machine that therefore I am],
  [2025] // 06-09.11.2025
)

// #paper(
//   [University of Dundee Student Colloquium],
//   [Value is an automatic subject],
//   [2025] // 07.05.2025
// )

#paper(
  [#link("https://lackorg.com/2025-conference/")[LACK V]],
  [Is there a theory of the subject in Marx?],
  [2025] // 13.03.2025
)

#paper(
  [#link("https://caiml.org/dighum/")[TU Wien Digital Humanism Initiative]],
  [#link("https://caiml.org/dighum/announcements/digital-humanism-salon-capital-and-the-computer-by-lachlan-kermode-2024-06-24/")[Capital and the computer]],
  [2024] // 24.06.2024
)

#paper(
  [#link("https://www.iwm.at/event/spring-fellows-conference-2024")[IWM Spring Fellows Conference]],
  [Love's first site], // (respondent: Amanda Holmes)
  [2024] // 17.05.2024
)

#paper(
  [#link("https://www.americanacademy.de/")[American Academy in Berlin]],
  [The automatic subject],
  [2023] // 05.12.2023
)

#paper(
  [#link("https://mcm.brown.edu/")[MCM] Graduate Colloquium],
  [Men dressed as ghosts],
  [2023] // 17.03.2023
)

#paper(
  [#link("https://gristsconference.wordpress.com/grists-2022/")[GRiSTS 2022]],
  [Political economies of the cloud],
  [2022] // 13.10.2022
)

// (#link("https://toronto-geometry-colloquium.github.io/posters/tgc_poster_045.pdf")[poster])
#paper(
  [#link("https://toronto-geometry-colloquium.github.io/")[Toronto Geometry Colloquium]],
  [#link("https://www.youtube.com/watch?v=d8unXfzCpZk")[3-D in public]],
  [2022] // 27.05.2022
)

#paper(
  [#link("https://artspace-aotearoa.nz/")[Artspace Aotearoa]],
  [Slow Boil #link("https://artspace-aotearoa.nz/events/slow-boil-relations")['Relations'] seminar],
  [2021] // 19.06.2021
)

#paper(
  [#link("https://augmented-authorship.ch/")[Lucerne University of Applied Sciences]],
  [Augmented Authorship],
  [2021] // 11.05.2021
)

#paper(
  [#link("https://www.coursicle.com/brown/courses/MCM/0902O/")[Neural Media: a Cultural History of Machine Learning], Brown University],
  [Deep learning intro],
  [2020], // Fall 2020
)

#paper(
  [#link("https://www.sigrid-rausing-trust.org/")[Sigrid Rausing Trust]],
  [AI and Human Rights],
  [2019] // 24.09.2019
)

#paper(
  [#link("https://www.schoolofma.org/")[School of Machines]],
  [Citizen Forensics workshop],
  [2019] // 16.09.2019
)

#paper(
  [#link("https://www.hkw.de/en/programm/projekte/veranstaltung/p_155571.php")[Flaneur Festival] at #link("https://hkw.de/de/index.php")[HKW]],
  [Spatial Research Practice Roundtable],
  [2019] // 31.08.2019
)

#paper(
  [United Nations / #link("https://www.turing.ac.uk/")[ATI] conference],
  [Machine Learning in Forensic Investigations],
  [2019] // 23.07.2019
)

#paper(
  [#link("https://www.gold.ac.uk/")[Goldsmiths University]],
  [Machine Learning and Synthetic Data],
  [2019] // 17.07.2019
)

#paper(
  [#link("https://www.aaschool.ac.uk/")[Architectural Association]],
  [Programming for open-source architects],
  [2019] // 2019
)

#paper(
  [#link("http://www.bbk.ac.uk/bih/lcts")[Birkbeck's Critical Theory Summer School]],
  [On Algorithmic Vision],
  [2019] // 04.07.2019
)

#paper(
  [#link("https://german.princeton.edu/ssms/")[Princeton-Weimar Summer School]],
  [Computer Visions],
  [2019] // 19.06.2019
)

#paper(
  [#link("https://www.elementai.com/")[ElementAI]],
  [Using Synthetic Data],
  [2019] // 21.03.2019
)

#paper(
  [#link("https://www.princeton.edu/")[Princeton University]],
  [Machine Learning in Counter Forensics], // guest lecture in Tom Levin's class 
  [2018] // 10.10.2018
)

#paper(
  [Turner Prize seminars at #link("https://www.tate.org.uk/visit/tate-britain")[Tate Britain]],
  [#link("https://forensic-architecture.org/programme/events/lessons-in-counter-forensics-interpretation")[Lessons in Counter Forensics: Interpretation]],
  [2018] // 03.11.2018
)

= Selected software contributions
#paper(
  [Adding HTML export features and fixes],
  [#link("https://github.com/typst/typst/pulls?q=is%3Apr+is%3Aclosed+author%3Abreezykermo")[Typst open source contributions]],
  [2025]
)
#paper(
  [Interactive platform developed in partnership with #link("https://www.inferstudio.com/")[Inferstudio]],
  [#link("https://antarctic-resolution.org/")[Antarctic Resolution]],
  [2023] // 02.2023
)

#paper(
  [Open source standalone frontend application, 350+ Github stars],
  [#link("https://github.com/forensic-architecture/timemap")[Timemap]],
  [2019]
)

#paper(
  [Open source tool to download and transform media, 100+ Github stars.],
  [#link("https://github.com/forensic-architecture/mtriage")[Mtriage]],
  [2020]
)

#paper(
  [Investigative #link("https://github.com/forensic-architecture/timemap")[timemap] deployment documenting police brutality],
  [#link("https://forensic-architecture.org/investigation/police-brutality-at-the-black-lives-matter-protests")[Brutality at BLM Protests]],
  [2020] // 28.10.2020
)

#paper(
  [Investigative #link("https://github.com/forensic-architecture/timemap")[timemap] documenting military presence in Ukraine],
  [#link("https://forensic-architecture.org/investigation/the-battle-of-ilovaisk")[The Battle of Ilovaisk]],
  [2019] // 19.08.2019
)

#paper(
  [Investigative #link("https://github.com/forensic-architecture/mtriage")[mtriage] deployment documenting tear gas use],
  [#link("https://forensic-architecture.org/investigation/triple-chaser")[Triple Chaser]],
  [2018] // 21.03.2018
)

= Service
#paper(
  [Maths, Philosophy and History reading group, #link("https://humanities.brown.edu/")[Cogut Institute]],
  [Organizer],
  [2024--], // 06.2024-
)

#paper(
  [_The geopolitics of software at scale_, #link("https://history.brown.edu/news/2024-05-21/models-scale-context")[Cogut Humanities Lab]],
  [Conference organizer],
  [2026] // 06.2025
)

#paper(
  [#link("https://informationplusconference.com/2025/")[Information\+]],
  [Program committee],
  [2025] // 06.2025
)

#paper(
  [#link("https://vivo.brown.edu/display/lcaplan1")[Lindsay Caplan], Department of Art History, Brown University],
  [Research Assistant],
  [2024] // 17.03.2023
)

#paper(
  [#link("https://mcm.brown.edu/")[MCM] Graduate Colloquium],
  [Conference organizer],
  [2023] // 17.03.2023
)

#paper(
  [#link("https://sites.brown.edu/gsc/")[GSC] representative for Modern Culture and Media],
  [Department representative],
  [2022--23] // 09.2022-23
)

#paper(
  [#link("https://events.brown.edu/event/229231-critical-computing-speaker-series-joy-lisi-rankin")[Critical Computing Speaker Series], Modern Culture and Media / Computer Science],
  [Organizer],
  [2022] // 09.2022-23
)

#paper(
  [#link("http://ethics.cs.brown.edu/")[Responsible Computing Initiative] Brown University],
  "Graduate Advisor",
  [2021--23], // 09.2021-2023
)

#paper(
  [#link("https://www.paideiainstitute.org/")[Paideia Institute for Humanistic Study]],
  [Advisory Board],
  [2016--19]
)

= Workshops and groups 
#paper(
  [with #link("https://www.chrflagship.uwc.ac.za/fellowship-programme/fellows/jon-soske/")[Jon Soske], and #link("https://quietbabylon.com/tim-maly")[Tim Maly]],
  [#link("https://complexity.risd.edu/")[RISD Center for Complexity] _Systems thinking from the margins_ ],
  [2024--25] 
)

#paper(
  [with #link("https://janvitek.org/")[Jan Vitek], Shriram Krishnamurthi],
  [#link("https://pliss.org/2025/")[PLISS] summer school],
  [2025] // 05.2025
)

#paper(
  [with Eli Upfal and Lindsay Caplan],
  [AI faculty reading group],
  [2022--24] // 06.2022-2024
)

#paper(
  [with #link("https://www.bicar.org/rohit-goel")[Rohit Goel]],
  [#link("https://www.bicar.org/")[BICAR] Summer Intensive, 4-week intensive],
  [2023] // 06-08.2023
)


#paper(
  [with #link("https://vivo.brown.edu/display/hcase1")[Holly Case]],
  [_Simultaneity_ Sommerfrische, 1-week retreat in Poland],
  [2022] // 07.2022
)

#paper(
  "Department of Computer Science, Brown University",
  [#link("https://brown.argnotes.club/")[ARGNOTES] reading group],
  [2021--22] 
)

#paper(
  "Goldsmiths University",
  [#link("https://docs.google.com/document/d/1jupL1t0mS2cu4vJqEwrbXHE-n7YHLFq83c42NOG7mRk/edit?usp=sharing")[A.R.G. Critical Reading Group]],
   [2021--23], // 09.2021-01.2023
)

= Recognition and Press
#paper(
  [Grad Center Bar / Narangansett Brewery],
  [GCB / Narangansett Travel Grant],
  [2025]
)

#paper(
  [Brown University],
  [Joukowsky Summer Research Award, (awarded 2 times)],
  [2023-25]
)

#paper(
  [Brown University],
  [International Travel Fund Award, (awarded 3 times)],
  [2022-25]
)

#paper(
  [Brown University],
  [Forbes Fund Travel Award, (awarded 5 times)],
  [2022-25]
)

#paper(
  [Brown University],
  [Graduate School Travel Award (awarded 3 times)],
  [2022-25]
)




#paper(
  [Interstices magazine],
  [#link("https://interstices.ac.nz/index.php/Interstices/article/view/654/603")[Interview with Anthony Brand]],
  [2021]
)

#paper(
  [The Indy magazine],
  [#link("https://theindy.org/article/2492")[_Software Reconnaissance_: an interview with Lucas Gelfond]],
  [2021]
)

#paper(
  [#link("https://www.versobooks.com/products/2565-investigative-aesthetics")[_Investigative Aesthetics_] by Matthew Fuller and Eyal Weizman],
  [For developing the #quote[practice and theory of open-source software research] at Forensic Architecture],
  [2021]
)

#paper(
  [Architecture Now],
  [#link("https://architecturenow.co.nz/articles/signs-of-use-and-past-presences/")[_Signs and use of past presences_]],
  [2020]
)


#paper(
  [#link("https://digitalfestival.ch/en/HACK/past-events")[HackZurich 2018]],
  [Top 5 (of 125+) 'Hive'],
  [2018] // 09.2018
)

#paper(
  [#link("https://digitalfestival.ch/en/HACK/past-events")[HackZurich 2017]],
  [Top 25 (of 125+) 'Midnight Grain to Georgia'],
  [2017] // 09.2017
)

#paper(
  [Burda Hackday, Munich],
  [1st Place 'ClaireCutApp'],
  [2015] // 12.2015
)

#paper(
  [#link("https://digitalfestival.ch/en/HACK/past-events")[HackZurich 2015]],
  [Dropbox API Prize 'Stitch'],
  [2015] // 10.2015
)

#paper(
  [Bic Lazio Talenttime Hackathon, Rome],
  [1st Place 'Pocketbadge'],
  [2015] // 09.2015
)

// #paper(
//   [Auckland Grammar School],
//   [Ian Turner Cup for Best All Rounder],
//   [2012] // 12.2012
// )
//
// #paper(
//   [Auckland Grammar School],
//   [H.O Ingram Prize for Senior Latin],
//   [2012] // 12.2012
// )
//
// #paper(
//   [Auckland Grammar School],
//   [Senior Prefect (1 of 5)],
//   [2012] // 02.2012
// )
//
// #paper(
//   [Auckland University Latin Speaking Competition],
//   [1st Place],
//   [2012] // 10.2012
// )

