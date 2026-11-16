# Spike: why a narrowed rebuild's link anchors differ from a full compile's

**Verdict: yes, the anchors can match, but not through Typst's anchor pass.
Instead, every anchor a page can be linked at needs a literal HTML `id`,
present whether or not anything links to it. Typst's anchor pass reuses an
existing `id` and only falls back to its own `loc-N` counter when there is
none, so a literal id is the same in every build and does not consume a
`loc-N` slot. Two owners, one each:**

1. **rheo: render the handle anchor as `<div id="<label>">` in HTML** instead of
   nothing. That removes all three anchor differences that remain on waterline
   (`index.html`, `airborne-review-4sep26.html`, `digitaltheory/review.html`),
   which a prototype confirmed. It also stops the handle anchor taking `loc-1`
   ahead of a page's real anchors.
2. **rookery: link `#ideas-outline` entries to the note's existing literal
   `id="idea:<id>"`** instead of to `el.location()`. That fixes the *live*
   case, rheo-tests' `notes/n05.html` `<figure id="loc-1">`, which a prototype
   also confirmed.

Feeding the previous compile's targets back in is not possible through any
public typst-bundle API. Stripping dead anchors does not fix the numbering.
Widening the narrowed set to pull in linking pages is correct, but on a
rookery site with outlines it grows into a full build, and it truncates the
outline it pulls in.

## Measurement conditions

- rheo: the prebuilt `--release` binary of rheo main (`rheo 0.6.4`,
  `/tmp/main-target/release/rheo`) for the baseline. The prototypes were
  `/tmp` copies of this workspace at `825a56a9` ("Moves the spine behind a
  constant-hash store"), built with their own `CARGO_TARGET_DIR`.
- rookery: `/home/lox/code/_fcl/rookery` at `ddd599a4` ("Binds backlinkers on
  their target's page") plus its working copy. The prototype used a `/tmp` copy.
- Projects:
  - rheo-tests' `cases/watch_narrow_rookery` copied to `/tmp`, with
    `ROOKERY_DIR` substituted and a `version` added.
  - A `/tmp` copy of waterline's `rookery/` (build dir excluded) with
    `[packages.rookery]` replaced by a `path =` entry. A full compile writes
    2,394 pages.
- Driver: `rheo watch . --html --trigger stdin --events <file>`, fed
  `rebuild <path…>` lines through a fifo. The driver waited for each
  `build_finished` event. The comparison was a clean `rheo compile . --html`
  into a separate build dir, with `<style>` blocks stripped from both sides.
  Waterline runs also passed `--input today=2026-11-15`.
- The waterline edit is the one `narrowed-vs-full.md` used: delete `#idea(title:
  [Zoom room])[…]` from `nest/tidbits.typ`. It narrows to 7 vertebrae.
- Another spike was compiling on the machine at the same time, so these runs
  give no timings. Everything below is about which bytes came out.

## The mechanism, in one place

`typst-bundle` 0.15.0 (`src/lib.rs`, `bundle_impl`) calls
`introspector.link_targets()` once the whole bundle has compiled.
`link_targets()` (`src/introspect.rs`) collects every location that any
`LinkElem` resolves to, through `LinkElem::find_destinations`. It then hands
that set to `crate::link::create_link_anchors`, which in turn calls
`typst_html::create_link_anchors` (`typst-html` `src/link.rs`) once per
document. That pass walks the DOM. When it meets the start tag of a target, it
gives the target's first node an id through `AnchorGeneratorExt::assign`:

```rust
element.attrs.get(attr::id).cloned().unwrap_or_else(|| {
    let id = self.identify(label);
    ...
```

If the target produces no node, the pass inserts an empty `<span>` and calls
`identify`. `AnchorGenerator::identify` (`typst-library` `src/model/link.rs`)
returns the label when that label is a valid id and occurs exactly once.
Otherwise it returns `loc-{n}` from a counter that is **per document and
counts only targets**. Three consequences follow:

- An anchor exists only if a link *in this compile* points at it.
- A `loc-N` number depends on how many earlier targets on the same page lacked
  a usable id, which again depends on which links were compiled.
- An element that already carries an `id` attribute keeps it, is present
  whether targeted or not, and never advances the counter. This is the lever
  the fix uses.

## 1. The live case reproduces, and it renumbers too — yes

**The dead anchor.** In the rheo-tests rookery fixture, `everything.typ` holds
`#ideas-outline()`. Its entries link to `el.location()`, the idea's `IK`
figure, so the full build has `<figure id="loc-1">` in every note page and
`everything.html` links `notes/n05.html#loc-1`. Edit only `notes/n05.typ`
("A standalone note." to "A changed standalone note."):

```
narrowed: 1 vertebra(e) in 1 round(s) — notes:n05
```

The rebuilt `notes/n05.html` has a bare `<figure>`, with no id at all. The
untouched `everything.html` still links `notes/n05.html#loc-1`, which now
goes nowhere. This is exactly the rheo-tests ignore reason.

**Renumbering.** To show the number moving, a copy of the fixture adds an
in-page Location link to `notes/n11.typ`, which already holds two ideas:

```typst
== Coda

#context link(query(heading.where(level: 2)).last().location())[jump to coda]
```

Full build of `n11.html`: `<figure id="loc-1">`, `<figure id="loc-2">`,
`<h3 id="loc-3">`, `href="#loc-3"`. After editing only the second idea's body,
the narrowed rebuild gives `<h3 id="loc-1">` and `href="#loc-1"`, and neither
figure has an id. The stale `everything.html` still links `notes/n11.html#loc-1`
and `#loc-2`. **The first link now lands on the Coda heading, a wrong target
rather than a dead one**, and the second is dead.

The handle anchor shows the same numbering effect inside full builds. In 12
waterline pages (`nest/dissertation.html`, the `c0`/`c5` chapters and their
slides/talk, `nest/papers/document-infrastructure.html` and five paper slide
decks), the handle span is `loc-1`. Each real `loc-N` after it is therefore
one higher than it would be if the handle were unlinked.

**On waterline today, the live exposure is small.** A probe built into a
`/tmp` rheo (`probe_anchor_edges`, run after each compile) counted, in a full
compile:

```
PROBE location-dest links=6231 handle-dest=2431 cross-doc-non-handle=10
```

All 10 cross-document, non-handle edges are citations from
`ideas/cmu-computational-humanities-25-26-research-statement.html` into the
bibliography `<li id="loc-N">` on
`ideas/georgiatech-digital-media-25-26-research-statement.html`. The 2,431
handle-destination links are the dead anchors. The fixture, by contrast, has
19 cross-document edges: every `#ideas-outline` row.

## 2. Can rheo stop rewritten label links contributing destinations? — no

The `LinkElem` is built by author code (`#link(<handle>)`, or `@handle`
through rheo's own `show ref` rule, which returns `link(it.target, …)`). It is
located before any show rule sees it. `find_destinations` queries every
`LinkElem` in the introspector, whatever its show rule went on to produce. The
probe confirms it: 2,431 of waterline's location-destination links resolve to
a `rheo-handle` figure, even though every one of them was rewritten to a
plain href. No point in rheo's pipeline runs before materialisation. The only
other lever is to take the label off the handle figure, which would break
`@handle` references (the figure exists for them, see the `bundle_source`
comment in `crates/core/src/reticulate/spine.rs`) and the combined PDF.

That question turns out not to matter. The dead destination is harmless once
its target carries a literal id: the anchor is then present in every build
and costs no `loc-N`. Section "Options" below covers this.

## 3. Can rheo feed `create_link_anchors` the previous compile's targets? — no

typst-bundle 0.15.0 declares `mod link;` privately. Its
`create_link_anchors(items: &mut [Item], …)` takes the private `Item` enum,
and `bundle_impl` calls it inline with
`introspector.link_targets()`. No entry point accepts a target set.
`typst_html::create_link_anchors(document, targets)` is public, but it has
already run by the time rheo holds the `Bundle`. Running it again would restart
`loc_counter` at zero and mint duplicate `loc-N` ids. The `Location` values of
the previous compile also mean nothing in this one. Patching the HTML
afterwards would mean reimplementing the anchor pass, including its numbering,
outside Typst. That is a fork in all but name.

## 4. Is stripping empty unreferenced `<span id>`s safe? — no, and it does not fix numbering

- **It does not equalise.** By the time rheo sees the HTML, the anchor pass
  has already numbered everything after the span. Removing the handle span
  from the 12 pages above still leaves their `loc-N` one higher in a full
  build than in a narrowed one.
- **"Unreferenced" cannot be decided per compile.** A page that did not
  compile still holds its old hrefs on disk. Deciding that an id is dead would
  mean scanning the whole output directory on every narrowed rebuild. Even
  then it would miss hrefs outside the site (a shared `…#loc-3` link, a
  bookmark, JavaScript). Stripping a live one breaks that link silently.
- It is only safe for anchors rheo itself knows to be dead (the handle
  figure's). Even there, a literal id does strictly better: it keeps the
  anchor and removes the numbering effect.

## Options

| Option | Fixes handle drift | Fixes live `loc-N` | Cost | Contract impact |
|---|---|---|---|---|
| **A. Literal id on rheo's handle anchor** (rheo) | yes, prototyped | no | One `show` rule in `rheo.typ`; no Rust | Every HTML page gains `<div id="<handle>">` (plus one per `<handle.typ>` alias) at the top of `<body>`. On 12 waterline pages, `loc-N` drops by one in full builds, once. `page.html#<handle>` becomes a stable anchor. |
| **B. rookery outline links to `#idea:<id>`** (rookery) | — | yes, prototyped | Small change in `outline.typ` | Outline hrefs change from `#loc-N` to `#idea:<id>`; the `<figure>`s lose their `loc-N` ids in full builds too. The paged and no-rheo paths keep `el.location()`. |
| C. Widen the narrowed set by link edges (rheo) | yes | yes | Rust: harvest linker/target pairs from the bundle introspector (the probe shows it is possible). The closure is transitive, because a linker compiled without its targets loses rows. | On the fixture, one note edit pulls in `everything`, which pulls in every note: a full build. On waterline, 2 pages. |
| D. Feed previous targets | — | — | Needs a typst-bundle fork | — |
| E. Strip dead anchors | partly | no | HTML post-pass plus an output-wide href scan | Can break hrefs outside the site |
| F. Upstream: deterministic anchors | yes | yes | A Typst change (for example, ids keyed by element rather than a per-target counter, or an "always anchor" attribute) | Out of rheo's hands |

### A, prototyped

In a `/tmp` copy of `crates/core/src/typ/rheo.typ`:

```typst
#show figure.where(kind: "rheo-handle"): it => {
  if std.target() == "html" and it.has("label") {
    html.elem("div", attrs: (id: str(it.label)))
  }
}
```

A first attempt used `span`, which is inline, so Typst wrapped it in a `<p>`.
The anchor pass then put the target id on the `<p>` (`<p id="loc-1"><span
id="…">`), so the drift came back. A block element avoids that. With the
`div`, waterline's Zoom-room edit gives:

```
main:   only narrowed ./ideas/zoom-room.html
        DIFF ./index.html
        DIFF ./nest/clusters/digitaltheory/review.html
        DIFF ./nest/clusters/liminal/airborne-review-4sep26.html
proto:  only narrowed ./ideas/zoom-room.html
```

(`zoom-room.html` is the separate prune problem, cause 6 of
`narrowed-vs-full.md`.) The full build's `index.html` now starts with
`<body><div id="index"></div><div id="index.typ"></div>`.

Against main's full build, after stripping the old leading span and the new
leading divs, 2,382 of 2,394 pages are byte-identical. The other 12 are the
`loc-N` shift described above, and in-page hrefs follow it (`href="#loc-5"`
becomes `#loc-4`). `cargo test -p rheo-core` on the prototype passed 470
tests. The 7 `packages::git` tests failed there because of the `/tmp` copy's
environment, not this change; they touch neither `rheo.typ` nor HTML.

The `<handle.typ>` alias needs its own literal id, because a link to the
alias would otherwise mint a `loc-N` span. In EPUB, `:` is not valid in an
XML `ID`, so the rule is restricted to HTML (`target() == "html"`; rheo's
polyfill returns `"epub"` there), which leaves EPUB as it is today.

### B, prototyped

In a `/tmp` copy of rookery's `core/0.1.1/src/outline.typ`, the HTML branch of
`#ideas-outline` linked each entry to
`_rel-prefix(here) + e.handle.replace(":", "/") + "." + ext + "#" + e.id`. That
is the heading's literal `id`, the same shape that minted pages already use
(`../notes/n05.html#idea:n05a`). The fixture's `n11` renumbering edit then
gives **no differences at all** between the narrowed and full output. The
outline links `notes/n11.html#idea:n11a`, and the probe drops to
`cross-doc-non-handle=0`. This also suits the planned fed outline (rookery
"List every idea in a narrowed ideas-outline"): an entry fed from a page that
did not compile has no `Location` to link to, only an id.

### C, simulated

Sending `rebuild content/notes/n11.typ content/everything.typ` makes `n11.html`
match its full build. However, `everything.html` then comes out *worse* than
the stale copy it replaced: the outline drops to `n11`'s two rows, because a
`Location` can only point at an element compiled in this pass. To be correct,
C needs the linker's targets too, so the closure is transitive. On an outline
site that means a full build. C is worth keeping as a guard for the
citation-shaped edges that B does not cover, but not as the fix.

## 5. Recommendation and owner

**Fix in two places, with A in rheo first.** A is a self-contained rheo
change that removes every anchor difference waterline still shows, removes the
handle anchor's push on `loc-N`, and needs no Rust. B belongs to rookery,
because only rookery knows a note has a stable id, and it already emits that
id. Typst's anchor pass is working as designed, so an upstream request (F) is
optional: it would be a convenience, not a bug report.

Two cases stay uncovered by A+B, and neither shows on waterline's edits today:

- Citation links across pages: the cmu-to-georgiatech edge.
- An author's own `#link(<label>)` or `Location` link across pages to an
  element with no literal id.

A narrowed rebuild of the target page drops those anchors in the same way.
That is the case for C, or for a warning when the bundle has such edges.
Neither is filed here.

## Relation to the `Location`-link anchor spike

That spike asks whether rheo can *resolve* a `Location` destination itself,
because a native `#outline()` across pages fails with `failed to determine
link anchor` during convergence. This one asks whether an anchor *exists* and
keeps its number when the compiled set changes. They share the same Typst
code (`link_targets` and `create_link_anchors`), but neither subsumes the
other:

- If rheo could resolve `Location` links itself, the target would still need
  an anchor, and narrowing would still drop or renumber it.
- Literal ids do nothing for the convergence error.

Both point the same way, though: links across pages should aim at literal ids
and string hrefs, not at Typst-minted `loc-N`.
