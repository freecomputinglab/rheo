# Spike: Typst-native document metadata via bundle introspection

**Verdict: YES — a single bundle compile can derive per-vertebra `#set document(...)` metadata via introspection, with no measurable iteration or wall-clock cost, and it closes every gap `docs/limitations.md` attributes to the static AST scan. The one real hazard is the combined-PDF layout, where beacons must be gated to `OnePerVertebra` only (confirmed empirically, Q6). `datetime.today()` resolving to a concrete date (Q8) is a deliberate behavior change to decide on, not a blocker. Recommendation: adopt introspection-based metadata for `OnePerVertebra` (HTML/EPUB); keep the Rust static scan (or a Rust-side empty fallback) for `SingleCombined` (PDF).**

Timeboxed investigation into whether rheo can stop statically parsing each
vertebra's `#set document(...)` in Rust (`crates/core/src/parser/document_metadata.rs`,
`document_date.rs`) and instead let each vertebra emit a small, hand-authored
`#metadata(...)` "beacon" that any other vertebra or package reads via
`query()`, all within the ONE bundle compile rheo already performs per format
(`crates/core/src/world.rs:309-318`, `crates/core/src/build.rs:249`).

## Setup

A throwaway rheo project was built under the scratchpad (not `rheo-tests`),
built and run from this workspace (`rheo-spike-dy2`) via `direnv exec . cargo
run -- compile <project> --html|--pdf|--epub`:

- `content/index.typ` — literal top-level `#set document(title:, author:,
  description:, date: datetime(...), keywords:)`.
- `content/chapters/intro.typ` — **no** `#set document(...)` at all.
- `content/chapters/templated.typ` — title/author/description/keywords set
  only via `#show: book`, where `book` is imported from `template.typ`
  (a module living outside `content_dir`, so it isn't scanned as its own
  vertebra).
- `content/chapters/auto_date.typ` — `#set document(date: auto)`.
- `content/chapters/today.typ` — `#set document(date: datetime.today())`.
- `content/chapters/ref_context.typ` — a hand-authored `#figure(..., kind:
  "rheo-handle")` whose body is itself a `#context` query (Q4).
- `content/.marrow.typ` — a bare beacon spliced at the bundle root, used only
  for the Q3 marrow-probe test.

Every vertebra ends with the design's beacon statement, e.g. (`content/index.typ`):

```typst
#context [#metadata((handle: "index", title: document.title, author: document.author, description: document.description, keywords: document.keywords, date: document.date)) <rheo-meta:index>]
```

Full files are in the Throwaway PoC section below.

Three background facts (already verified by reading typst 0.15 sources per the
task) were re-confirmed against the sources actually resolved by this
workspace's `Cargo.lock` (typst **0.15.0**, not the 0.15.1 paths named in the
task — diffed the two: `typst`, `typst-realize`, `typst-library::model::document`,
and `typst-library::introspection::metadata` are byte-identical between
0.15.0 and 0.15.1; `typst-bundle` differs by one unrelated `bail!` →
`delayed_error` nuance). The standalone `typst` binary available in this
environment is 0.15.1, so it was used for fast supplementary sandbox checks
(`--features bundle,html`) alongside the real rheo pipeline; every claim below
is also confirmed through the actual `cargo run` rheo binary.

## Q1 — cross-bundle query (gates the whole design)

**Yes.** A labelled `#metadata(...)` is queryable from any other vertebra in
the same bundle compile.

Real-pipeline evidence — `direnv exec . cargo run -- compile <proj> --html`,
then `build/html/index.html`:

```
Q1 cross-vertebra: chapters:intro's title as seen from index: [Intro]
Q1 cross-vertebra: chapters:templated's title as seen from index: [Templated Title From Book]
```

`chapters:templated`'s title ("Templated Title From Book") is set only inside
`template.typ`, so this value could not have come from anything but a real
cross-document `query()` at compile time — static parsing of `index.typ`
cannot see it. `RUST_LOG=rheo=trace cargo run -- compile <proj> --html` shows
exactly one `DEBUG compiling spine via bundle target` line, confirming this
all happens inside rheo's existing single bundle compile, not a second pass.

Supplementary sandbox (`typst compile --features bundle,html --format bundle`
on a hand-written two-`#document` file): document B's context block
`query(<meta:a>).first().value.at("title")` rendered `A Title` in `b.html` —
same mechanism, minimal repro.

## Q2 — title set only via `#show: book` (the main `docs/limitations.md:21-24` win)

**Yes.** `context document.title` at the end of a vertebra sees a title set
purely through `#show: book`, where `book` internally calls `set
document(...)` and is imported from a separate module.

Real-pipeline evidence — `build/html/chapters/templated.html`:

```html
<title>Templated Title From Book</title>
<meta name="description" content="A description set entirely inside the template module.">
<meta name="authors" content="Book Module Author">
<meta name="keywords" content="templated, book">
...
<p>Own title after show: book: [Templated Title From Book] Own author after show: book: ("Book Module Author",)</p>
```

`chapters/templated.typ` contains no literal `#set document(...)` — every one
of these values is invisible to today's static scan (`docs/limitations.md`'s
table: "In an imported module fn, applied via `#show: book`" → "No"). Under
introspection it is not just visible in the compiled title but is legible
*from within the document itself* via `context document.title/.author`, and
(Q1) queryable from every other vertebra too.

## Q3 — position independence, and the marrow-root counterexample

**Both yes**, in the direction the design predicts.

**(a) Query before the beacon, same document — resolves.** `content/index.typ`
places a query for its **own** eventual beacon at line 1, before both the
`#set document(...)` rule and the beacon itself (defined at the bottom of the
same file):

```typst
#context [Q3 top-query of own beacon, BEFORE the set rule and BEFORE the beacon itself: #repr(query(label("rheo-meta:index")).first().value.at("title"))]
```

`build/html/index.html` renders: `Q3 top-query of own beacon, BEFORE the set
rule and BEFORE the beacon itself: [Index Title]` — correctly resolved despite
running textually before the value it queries even exists in source order.
Typst's fixpoint relayout loop (Q5) is exactly what makes this work.

**(b) Beacon at bundle-root (marrow), after every `#include` — reads nothing.**
`content/.marrow.typ` is spliced by rheo at the bundle root, **after** every
vertebra's own `#document(...)[...]` block
(`crates/core/src/reticulate/bundle_source.rs`'s `Display` impl: documents
render first, marrow last):

```typst
#context [#metadata((probe: "marrow-after-includes", title: document.title)) <rheo-marrow-probe>]
```

`index.typ` queries it: `Q3 marrow-probe (title read at bundle root, after
every vertebra's own document block): none` in `build/html/index.html`. Even
though `index.typ`'s own `#set document(title: [Index Title])` runs inside the
very last `#document(...)` block before this marrow statement, the marrow
probe still reads `none` — confirming the design's core placement constraint:
a `#set document(...)` inside a vertebra's own `#include` is scoped to that
vertebra's own `#document(...)` container and does **not** leak to bundle-root
siblings. Beacons must live in vertebra source, not be injected after the
`#include`s into the synthesized main.

(Sandbox cross-check, before the real-pipeline test above was built: a
synthetic `#document("c.html")[#set document(title: [C Title]) ...]` followed
by a bare root-level `#metadata(...)` probe, then a second `#document("d.html")`
querying that probe, gave `(probe: "marrow", title: none)` — same result.)

## Q4 — `#show ref` rule with a `#context` body in the anchor

**Yes**, it renders correctly.

`content/chapters/ref_context.typ` hand-authors a `#figure(...)` using rheo's
own `"rheo-handle"` kind (so the real, auto-injected `crates/core/src/typ/rheo.typ:18-25`
show rules apply to it), but gives it a **dynamic** body instead of the static
plain-text title rheo synthesizes today:

```typst
#figure(context [Dynamic anchor text: #query(label("rheo-meta:index")).first().value.at("title")], kind: "rheo-handle", supplement: none) <q4-manual-anchor>
See @q4-manual-anchor for that handle-style anchor.
```

`build/html/chapters/ref_context.html`:

```html
<p>See <a href="../q4-manual-anchor.html">Dynamic anchor text: Index Title</a> for that handle-style anchor.</p>
```

`it.element.body` containing a `#context` block passed straight through
`link(it.target, it.element.body)` and resolved correctly ("Index Title", read
live via `query()`). (The href itself is a dead link — this test's label isn't
a real vertebra handle — which is irrelevant to the question; the point is
that the anchor's *body* renders.) This means a future change making handle
anchors themselves dynamic (reading `document.title` live rather than baking
in the static-scan title) is mechanically sound.

## Q5 — extra introspection iterations and wall-clock cost on a real project

**No measurable cost.** Built `../../rheo.ohrg.org`'s 23-page `pages/` (copied
to the scratchpad, once unmodified and once with the beacon appended to every
`.typ` file) from this workspace.

**Iteration count** — measured with a small scratch harness
(`scratchpad/q5-timing`, a separate Cargo project outside `crates/`, path-depending
on this workspace's `rheo-core`/`rheo-html`) that calls the same
`rheo_core::build::Build::prepare`/`run` the CLI uses, wrapped in
`typst_timing::enable()` and counting `"iter (N)"` scope-start events from
`typst::compile_impl`'s relayout loop (`typst-0.15.0/src/lib.rs:138-183`,
`typst-library-0.15.0/src/introspection/convergence.rs:15-17`, `MAX_ITERS = 5`):

```
$ /home/lox/.cargo-target/release/q5-timing <ohrg_before> <bd>   # ×5
relayout_iterations=4   (every run, before)
$ /home/lox/.cargo-target/release/q5-timing <ohrg_after> <bd>    # ×5
relayout_iterations=4   (every run, after)
```

**Exactly 4 relayout iterations in both cases, every run** — adding the
beacons did not add a single extra fixpoint iteration.

**Wall clock** — release binary (`cargo build --release`), 10 interleaved runs
each (`before`/`after` alternated to cancel any machine-load drift), `--build-dir`
fixed, discarding nothing (already warm):

```
before (ms): 198 179 148 164 161 149 148 155 167 152   avg 162.1ms
after  (ms): 198 148 148 164 160 153 172 154 147 162   avg 160.6ms
```

`after` was fractionally *faster* on average (−1.5ms) — within run-to-run
noise (the spread inside each column is ~50ms). The dedicated timing harness's
own 5-run wall-clock samples agree (before avg 153.0ms, after avg 137.2ms;
again `after` nominally faster, still noise). Since the extra iteration count
is **zero**, the "would extra iterations cost more than a whole extra bundle
compile" branch does not trigger — there is no basis here for recommending the
two-pass design over the single-pass one on cost grounds.

## Q6 — combined-PDF leakage

**Confirmed: yes, it leaks.** `--emit-bundle-source` (`cargo run -- compile
<proj> --pdf --emit-bundle-source`) dumps the synthesized bundle main;
`SingleCombined` (PDF) wraps **every** vertebra in one shared `#document(...)`:

```typst
#document("proj.pdf", format: "pdf", title: [Auto Date Chapter])[
  #include "content/chapters/auto_date.typ"
  #include "content/chapters/intro.typ"
  ...
  #include "content/index.typ"
]
```

`content/chapters/intro.typ` has **no** `#set document(...)` of its own.
Compiling `--pdf` and reading the rendered text (`pdftotext -layout
build/pdf/proj.pdf -`):

```
Intro (no #set document rule at all)
...
Own title (should be none under HTML/EPUB per-page compile; watch this same
line under combined PDF): [Auto Date Chapter]
```

`chapters/intro`'s own `context document.title` beacon reads **"Auto Date
Chapter"** — the immediately preceding vertebra's title — under combined PDF,
even though the *same* content compiled per-page (HTML) correctly read
`[Intro]` (its own vertebra's `#document(...)` wrapper supplies that as its
title argument; see Q3's discussion of how each `OnePerVertebra` document gets
its own title). This is ordinary Typst set-rule cascading: all the `#include`s
share one style-chain scope inside the single combined `#document(...)`, and
`set document(...)` propagates forward with no per-`#include` boundary — `set
document(...)` inside a container is a hard error, so it cannot be
scoped by wrapping each `#include`, confirming the task's premise.

**Recommendation: CONFIRMED — emit beacons only for `OnePerVertebra` layouts
(HTML/EPUB); for `SingleCombined` (PDF) the metadata helper should return an
empty dict (or rheo should keep sourcing PDF metadata from the existing static
scan) rather than emit a beacon at all.** The empirical leak above is exactly
what motivates that gate; there is no cheap in-Typst mitigation (no
containerized scoping is possible per the hard-error above), so gating by
`SpineLayoutKind` in Rust, at emission time, is the right fix.

## Q7 — leakage into HTML, EPUB XHTML, and the feed content region

**No leakage found on any of the three surfaces**, checked directly on output
files/strings.

- **HTML** (`build/html/*.html`): `grep -io metadata` across all pages
  matched only my own prose sentence in `intro.html` ("sets no document
  metadata of its own"); no `#metadata(...)` element produced any DOM node.
  (`typst-html-0.15.0`'s source has no `MetadataElem` special-case at all —
  the element is realized away as inherently invisible content before HTML
  serialization ever sees it.)
- **EPUB XHTML** (`crates/epub/src/xhtml.rs` output, unzipped
  `build/epub/proj.epub`): same — only the same prose match in
  `chapters/intro.xhtml`, plus the legitimate, unrelated `<metadata>` element
  in `EPUB/package.opf` (standard EPUB package metadata).
- **Feed content region** (`HtmlDom::feed_content_inner_html`,
  `crates/core/src/util/html.rs:196-204`): enabled `[html] feed_base_url` and
  rebuilt; `build/html/feed.xml`'s per-entry `<content type="html">` is an
  HTML-escaped copy of each page's body/`<main>` region — since that body
  already contains zero trace of `#metadata(...)`, the extracted feed content
  is equally clean. Confirmed directly by reading `feed.xml`; no additional
  code path needed since `feed_content_inner_html` only re-slices the same DOM
  already shown metadata-free above.

## Q8 — `Smart::Auto` and `datetime.today()` in real `DocumentInfo`

**Both confirmed, and the second is a behavior change worth flagging.** Added
a throwaway `#[cfg(test)] fn document_date_auto_and_today_resolve_in_bundle_info`
to `crates/core/src/world.rs` (next to the existing `RheoWorld::new_for_bundle`
fixture tests, e.g. `serves_moulded_body_overlay_instead_of_disk`), building a
two-document bundle main by hand and calling `.compile_bundle()`:

```rust
let world = RheoWorld::new_for_bundle(root, main /* two #document blocks,
    one `date: auto`, one `date: datetime.today()` */, HashMap::new(),
    HashMap::new(), None, Some("html"), vec![]).unwrap();
let bundle = world.compile_bundle().unwrap();
// for each BundleFile::Document(doc): doc.info().date
```

`cargo test -p rheo-core --lib world::tests::document_date_auto_and_today_resolve_in_bundle_info`
— **passes**: `date: auto` stays `Smart::Auto` (asserted via `assert_eq!`);
`datetime.today()` resolves to `Smart::Custom(Some(_))`, a concrete date.

Visible confirmation from the real project too — `content/chapters/today.typ`
uses `#set document(date: datetime.today())`; `build/html/chapters/today.html`
renders:

```
Own date (datetime.today()): datetime(year: 2026, month: 8, day: 15)
```

(2026-08-15 is this environment's current date.) `content/chapters/auto_date.typ`
renders `Own date (set to auto): auto`.

**This is a behavior change to flag.** Today's static scan
(`crates/core/src/parser/document_date.rs:119-127`,
`test_document_date_today_is_none`) deliberately returns `None` for
`datetime.today()` — "can't be resolved statically". Introspection resolves
it to a *real, concrete date that changes on every build*. Any consumer that
starts sourcing feed/spine dates from introspected `DocumentInfo` instead of
the static scan would newly get build-time-varying timestamps for any
vertebra using `datetime.today()`, which is a meaningfully different contract
(Atom `<updated>` churn on every rebuild) and should be a deliberate product
decision, not an incidental side effect of switching harvesting mechanisms.

## Recommendation

Adopt introspection-based per-vertebra metadata (the beacon pattern above) for
`OnePerVertebra` layouts (HTML, EPUB) — it strictly closes every gap
`docs/limitations.md` attributes to the static AST scan (title via `#show:
template`, via a code block, via a non-literal expression — anything that
ends up in the real style chain — Q2), costs nothing measurable in iterations
or wall clock on a real 23-page project (Q5), and composes correctly with the
existing cross-vertebra `@handle` machinery (Q1, Q3, Q4). Retire
`DocumentMetadata`/`DocumentDate`'s role for these formats accordingly (they
may still be worth keeping around for the `SingleCombined`/PDF path below, or
for any pre-compile need such as choosing a spine handle before compilation
starts — which does not depend on document metadata today).

Do **not** use the beacon pattern for `SingleCombined` (PDF): Q6 empirically
confirms cross-vertebra title leakage inside the one shared `#document(...)`.
Gate beacon emission to `OnePerVertebra` layouts in Rust and keep sourcing PDF
metadata from the existing static scan (or accept an empty per-vertebra
metadata dict there) — this is the mitigation to ship, not a two-pass compile.

Before implementing, make an explicit call on Q8's `datetime.today()` finding:
either keep excluding it from feed/spine timestamps by convention (mirroring
today's static-scan behavior even after switching mechanisms), or accept that
feed `<updated>` values may now legitimately vary from build to build for
vertebrae that use it.

This supersedes `docs/limitations.md`'s claim (lines 81-86) that closing these
gaps "requires a pre-compile pre-pass ... a two-pass design with real cost" —
the single bundle compile rheo already performs is sufficient, for the
`OnePerVertebra` layouts at least. (`docs/limitations.md` itself is left
untouched here; a separate task rewrites it.)

## Throwaway PoC

The beacon appended to every vertebra source (`content/index.typ` shown; every
other vertebra follows the same shape with its own handle):

```typst
#context [#metadata((handle: "index", title: document.title, author: document.author, description: document.description, keywords: document.keywords, date: document.date)) <rheo-meta:index>]
```

Read from any other vertebra, position-independently:

```typst
#context [#repr(query(label("rheo-meta:index")).first().value.at("title"))]
```

`template.typ` (project root, outside `content_dir`, for the Q2 test):

```typst
#let book(doc) = {
  set document(
    title: [Templated Title From Book],
    author: "Book Module Author",
    description: [A description set entirely inside the template module.],
    keywords: ("templated", "book"),
  )
  doc
}
```

`content/chapters/templated.typ` applying it:

```typst
#import "/template.typ": book
#show: book

= Templated Chapter

#context [#metadata((handle: "chapters:templated", title: document.title, author: document.author, description: document.description, keywords: document.keywords, date: document.date)) <rheo-meta:chapters:templated>]
```

The marrow-root counterexample (`content/.marrow.typ`, Q3):

```typst
#context [#metadata((probe: "marrow-after-includes", title: document.title)) <rheo-marrow-probe>]
```

The Q8 Rust test (`crates/core/src/world.rs`, `#[cfg(test)]`, left in place):

```rust
#[test]
fn document_date_auto_and_today_resolve_in_bundle_info() {
    use typst::foundations::Smart;
    use typst_bundle::BundleFile;
    use typst_library::model::Document;

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let main = r#"
#document("auto.html", format: "html")[
  #set document(date: auto)
  Auto date page.
]

#document("today.html", format: "html")[
  #set document(date: datetime.today())
  Today date page.
]
"#.to_string();

    let world = RheoWorld::new_for_bundle(
        root, main, HashMap::new(), HashMap::new(), None, Some("html"), vec![],
    ).unwrap();
    let bundle = world.compile_bundle().unwrap();

    for (path, file) in bundle.files.iter() {
        let BundleFile::Document(doc) = file else { continue };
        let info = doc.info();
        if path.get_without_slash().contains("auto") {
            assert_eq!(info.date, Smart::Auto);
        } else if path.get_without_slash().contains("today") {
            assert!(matches!(info.date, Smart::Custom(Some(_))));
        }
    }
}
```
