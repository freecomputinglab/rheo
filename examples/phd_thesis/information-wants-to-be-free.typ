#import "../../src/typst/rheo.typ": book_template
#show: book_template

= Information wants to be free

It is well-known that Marx's project in his critique of political economy, a project for which we have the most studied attempt in the first (and only published in his own lifetime) volume of #emph[Capital] {#emph[Das Kapital]}, is to critique a particular ossification of the relation between labor and value that emerges in an historically specific social condition of commodity exchange.
The ossification of this relation is precisely what Marx calls capital.
Labor and value are two core concepts that define modern society for Marx.
Labor designates the way in which society's constituents (we 'humans') live and work.
Its concept can thus not be separated from what Marx calls a mode of production, which is a #emph[particular] logical organization of labor in general in a given historical moment.
Value in turn represents the social meaning of that life and work, which is to say that is the way that labor #emph[socially] circulates, its #emph[shared] signification.

This chapter will give an account of how capital is able to insidiously install itself into the shared signification-- or #emph[structure]-- of modern society.
Through this installation, it sponges on the modern subject.
Marx articulates the parasitic relation between capital and the subject/social by arguing that the former reduces the sense of value, labor's shared signification, to its measurement as money.
When the meaning of life and labor is exclusively estimated, objectified, and alienated from itself in the figure of a price tag, we give away the freedom to decide for ourselves the nature of life, what it is that it is worth living #emph[for].
It is the filling in of value's open question with this nefariously simple answer-- money, money, money-- that constitutes capital's core philosophical problematic.

The italicization of the word 'is' in this chapter's title refers to the Lacanian philosopher Alenka Zupančič's illuminating rumination on the philosophical stakes of the psychoanalytic method, #emph[What] is #emph[sex]?
Zupančič stipulates that her emphasis on the single word 'is' in the title attends to the ontological problematic that the term 'sex' indexes:

#quote(block: true)[
in psychoanalysis sex is above all a concept that formulates a persisting contradiction of reality….
[T]his contradiction cannot be circumscribed or reduced to a secondary level (as a contradiction between already well-established entities/beings), but is-- as a contradiction-- involved in the very structuring of these entities, in their very being.
In this precise sense, sex is of ontological relevance: not as an ultimate reality, but as an inherent twist, or stumbling block, of reality. @zupancicWhatSex2017[p.3]
]

My argument in this chapter is that capital, too, is of ontological relevance, and that it also unmoors well-established distinctions as formal castoffs in contradiction.
Capital is not, however, exactly the same #quote[stumbling block of sense] @copjecSexEuthanasiaReason2015[p.204] that sex is.
Whereas the scandalous nature of sex, as Zupančič describes it, is that it has no real anchor through which we can grasp its essence in thought-- that #quote[we don't even know what it #emph[is]] @zupancicWhatSex2017[p.22]-- the ontological scandal of
capital is that, despite apparently knowing what it is, we cannot get our act together and abandon or abolish it.
While Lacan, the French philosopher and assiduous reader of Freud whose work structures Zupančič's, famously quipped that there is no sexual relation {#emph[Il n'y a pas de rapport sexuel]}, capital is by contrast a social relation that we cannot seem to do without.
Thus the adage, usually attributed to Fredric Jameson, that it is easier to imagine the end of the world than it is the end of capitalism.
Capital's ontological density weighs too heavy on us, not too lightly.

Though Lacan and Marx approach the structure of modern subjectivity from different directions, I argue in this chapter that their critical projects nonetheless approximate and attempt to asymptotically apprehend the properties the same fleeting philosophical object: namely, freedom.
For Marx, freedom figures as a capacity to denature value, rendering it an object that does not have solely an objective composition 'out there' in the world, but also a #emph[subjective] aspect.
It is precisely because acts of freedom involve the denaturing of value that contradiction is stood up in Marx's account as the palpable evidence of their necessary existence.
Contradiction is the name for the mathematical consistency that isomorphically occurs as a core component of both Marx's method in #emph[Capital] and Freud's/Lacan's in psychoanalysis.
Over the course of this chapter, I will argue that the positing of contradiction's requisite existence in the fabric of thought and appearance-- what Lacan calls the register of the symbolic-- is what isomorphically bridges Marx's theory of the commodity to the infamous Lacanian formulas of sexuality.
The philosophical piers of this conceptual bridge between Marx and Lacan's reading of Freud are to be found in a critique of the abstract and transcendental subject as it was formulated and developed by Descartes and Kant respectively.
By observing the epistemological centrality of contradiction in Marx's method of critique in the opening chapters of #emph[Capital], and by reading it through the mid-century reader of Marx in Alfred Sohn-Rethel, we will be able to make out the rough draft of a critique of philisophical epistemology writ large in the background of Marx's critique of political economy, a sketch that will in turn make clear the gravity of Kant's thinking in the stature of Marx's critical project.

Why should we work towards such a sketch of the Marx/Lacan relation?
Recognizing the outline of a theory of the modern subject in Marx is the critical acumen that will resource the assessment in later chapters of this dissertation that specifiies the ongoing applicability of capital's logic when reckoning with computation.
As we will show in the pages that follow, contradiction's centrality in the Marxian method reveals that its analytic purchase pertains to mathematics, an argument that will be extended through an appraisal of Marx's mathematical manuscripts in chapter two.
It is incorrect to relegate Marx's critique of capital to the echelons of history done and dusted, for he locates through it a philosophical problematic of quantification at work in the self-conception of the supposedly free subject that still haunts us today.
The critique of political economy and psychoanalysis are isomorphically part of a broader and more abstract project which struggles to define what it would mean to be free in the first place, in both practice and theory.
Seeing these two facets as different faces of a single conceptual unity is the grammatical foundation for a critical vocabulary that I will construct over the course of this dissertation to critique the overwhelming influence of computation in the logic of society's ongoing organization in the service of capital.
If information wants to be free, as the slogan of the free software movement avers, there is evidently something that keeps holding this desire back from its fleshy fulfillment.
Psychoanalysis and the Marxian conception of capital as an ontological stricture will help us to see what this 'something' really #emph[is];.

As Moishe Postone argues in his 1993 book #emph[Time, Labor, and Social Domination], Marx's critique in #emph[Capital] is not an account that strictly denounces capital as a mode of production (a particular organization of labor and value) from the moral highground of a transhistorical standpoint.
Such a denouncement would presuppose a philosophical vantage point outside Marx's own social condition, a viewpoint that has an eye over all plausible modalities of human existence, of all possible articulations of labor and value in the abstract, untethered to the philosopher's own historical conditions.
Philosophy, for Marx, cannot speak from such an abstract standpoint.
The disavowal of a transhistorical standpoint which is not subject to the substance of its time and context is what many have observed as the materialist commitment in Marx's method.
#footnote[Postone interprets the transcendentalizing of Marx's critique as a flaw in what he calls the 'traditional Marxist' interpretation of #emph[Capital]: #quote[the labor which constitutes value should not be identified with labor as it may exist transhistorically] @postoneTimeLaborSocial1996[p. 29].]
According to Postone, Marx's analysis of labor and value is not categorical, in the sense that he sees them as existing in all places and times, but rather #emph[categorial] in that their logic unfolds in the more historically specific logic of capital.
Note that this does not mean that capital logic cannot apply across manufactured categorizations of space and time, such as countries and continents, decades and centuries.
It only means that capital-logic, which gives definition to the notions of labor and value, is not transhistorically the case across #emph[all] possible spatio-temporal categorizations.
Marx's critique, Postone argues, is not a transcendental but an #emph[immanent] critique of capital's forms of appearance from the standpoint of an already existing structure of labor and value.
Marx does not speak with the authority of a God's eye, summarizing all things that might ever be in a word as an 'idealist', but as a 'materialist', from an historically specific and thus epistemologically constrained standpoint.
#footnote[We will unpack the terms idealist and materialist with greater definition later in this chapter.]
Acknowledging that not only his, but #emph[all] philosophy begins from this epistemologically constrained position in thought, Marx's project in #emph[Capital] is to elaborate how capital as a mode of production logically constructs a set of overdetermined conclusions about the nature of value and labor from a set of false assumptions about #emph[human] nature.
Marx's critique denaturalizes the set of axioms of the British and bourgeois political economy, espoused by the likes of Adam Smith and David Ricardo, which have been falsely assumed as unproblematic 'laws' of society's operation.
#footnote[I variously shorthand this nineteenth-century British political economy as both 'political economy' and simply 'economics' in this chapter to point to the idea that much of what is considered classical economics in the twenty-first-century is still subject to Marx's critique.
The misunderstanding of economic value as autonomous from the use of labor in production is arguably as widespread and riotous today as it was in the nineteenth century, and indeed part of my intention in this dissertation is to shore up this point again by returning to Marx's analysis.]

Marx strives to show a philosophical truth in #emph[Capital], in other words-- what capital really #emph[is]-- not through its direct pronouncement, but through an #emph[indirection] in the art of discourse.
When one starts with the naturalized axioms of British political economy-- that man is inherently selfish; that acting selfishly can counter-intuitively act in service of a greater good; that the market constitutes a rising tide that lifts all boats-- these axioms logically lead to a series of apparent contradictions.
Adam Smith claims that capital is a mode of production in which everyone is free to make one's own way: Marx extracts this claim's premises and follows them to their logical conclusion to show that Smith's vaunted egalitarianism (for example) is directly contradicted by producing a system that tends towards deepened states of social inequality, rather than the palpable and generalized experience of 'freedom' implied by its premises.

Labor and value, the two Marxian concepts introduced above, are two key sites of contradiction in the structure of capital. Capital's logic, as
Marx develops it over the course of #emph[Capital], can estimate labor only as an abstract quantity that works as an input in the production process to valorize an abstract end in the accumulation of surplus value.
That is, it can estimate labor only as labor-power, a particular and overdetermined form of labor that services first and foremost the
production of surplus-value, a particular and overdetermined form of value.
In the historically specific conditions of capital, labor thus appears only as a shadow of itself.
Rather than being recognized as the very cause and purpose of social cooperation and production, it is instead foggily perceived as a repository of labor-power, as nothing but one of many machinic inputs in the never-ending process of producing surplus-value.
Marx's critique impels us to ask, is this production of surplus-value in service of the fundamental question in secular modernity, the ulterior and open-ended question that could be called the betterment of society, or is it foreclosing this question and producing surplus-value not for 'us' ('humans'), but only for its own sake?

This fundamental question is at the heart of Marx's #emph[Capital].
What does it mean to be free, and how should society be organized so that we can maintain this freedom?
The answer to this question becomes distinctly secular when it refuses to subsume the idea of freedom beneath the banner of a supra-social or supernatural entity such as God.
The modern subject is thought to be responsible ontologically, in a word, to no-one and nothing other than itself.
As the philosopher Frank Ruda argues, secular modernity's possibility was introduced in the philosophy of Descartes.
Modernity renders us free not only to finally be our own masters, but also to misperceive freedom as such:

#quote(block: true)[
[F]or Descartes, only those beings that can be called free are responsible for that which one calls true and false; if humans are beings that can be called free, then it must hold that they are responsible for their own errors.
Which is to say that one cannot blame God for human error. @rudaIndifferenceRepetitionModern2023[p.20]
]

There is a dual ecstasy and agony in modernity. Being conceptually free means that we are also free to make mistakes about what it means to be free: #quote[Thus is man doomed to freedom] @rudaIndifferenceRepetitionModern2023[ p.20].
Capital's logic compels us to make one such capital error regarding freedom.
The freedom to sell one's labor as labor-time on the market should not, Marx systematically and loquaciously cautions us, be mistaken as freedom writ large.
Capital craftily confines us in yet another captive subjectivity in secular modernity, despite its conscious claim to have freed us from domination by an explicit master, whether paranormal or worldly.
#footnote[Capital's clamping-down on freedom in the context of secular modernity is precisely why there are so many religious references in Marx, such as his adoption of the term 'fetishism' in his analysis of capital-logic. Two important works exploring the import of the religious reference in Marx for the interested reader are @weberProtestantEthicSpirit2002 and @benjaminCapitalismReligion1996.]
It is the specific paradigm of the modern subject's captivation in Marx's theory of capital's logic that we will now explore.

#bibliography("references.bib", style: "chicago-author-date")

= Footnotes
