# Spike: why a narrowed rebuild's output still differs from a full compile

**Verdict: four separate causes account for every difference in the eight
files, and none of them is per-key tracking or the stable feed. One belongs to
rheo (with Typst underneath it); three belong to rookery.**

1. **The `index` and `loc-1` spans are link anchors.** Typst gives an anchor
   to every element that any link in the compiled bundle points at. That
   includes label links that `rheo-link-rule` has already turned into plain
   hrefs. A narrowed compile leaves out the page holding the link, so the
   target page loses its anchor. Every anchor that differs here is dead, since
   no href in either build points at it. The same mechanism would drop *live*
   anchors that a real `Location` link from an uncompiled page needs, though
   this reproduction shows none. **Owner: rheo**, with Typst's anchor pass
   underneath.
2. **The missing `todo-fields` block comes from a stale closure.** A todo's
   stored body (`idea-body:<id>`) holds a `context` block, which is a closure
   created in the compile that attached it. That closure reaches
   `_reg-final()`, which reads `_LIG-RECS`, a module-level `let` frozen at that
   compile. In a full build `_LIG-RECS` is `none`, so on the next narrowed
   compile the replayed closure sees only the live registry (69 todos, not
   198). The blocking dep is missing from that registry and the block renders
   empty. **Owner: rookery** (`@rookery/core`).
3. **Backlink windows on minted pages come out in a different order.**
   `_reg-final()` puts the live (recompiled) notes first and the fed ones
   after, and rheo feeds them sorted by page handle. A full build walks the
   registry in spine order. **Owner: rookery** (`@rookery/core`).
4. **A minted page's footer page list differs** (`chi2026` missing,
   `motivation`/`institutions`/`lack-vi` extra). For pages that did not
   compile, `_page-links()` substitutes *note-level* `backlinks:` edges for the
   *page-level* links it cannot query. **Owner: rookery** (`@rookery/core`).

Two more findings came out of the same runs. They are outside the five groups,
but they matter to the rheo-tests scenarios:

5. **Stale backlink-window text on minted pages** (rheo-tests' rookery
   scenarios) has a different cause from 3 and 4. A target note's owning
   vertebra binds `backlinks:<id>` (who links here) but not the linking notes'
   own `idea:`/`idea-body:` keys. So editing a linking note's prose never
   recompiles the target's minted page. **Owner: rookery** (`@rookery/core`).
6. **A deleted note's minted page survives a narrowed rebuild**
   (`ideas/zoom-room.html` here, `ideas/n07a.html` in rheo-tests). Only full
   runs prune (`rh-prune-output-of-deleted-vertebrae-59325112` scoped it that
   way on purpose), and rheo has no record of which vertebra minted a
   contributed page. **Owner: rheo.**

The five groups as filed have moved since the spike was written. The three
"whole window omitted" pages (`onward`, `lack`, `lack-vi-26-27`) now render
every window. What still differs there is the window order (cause 3) and the
page list (cause 4). The earlier omission was the stale-fed-id problem that
rookery's `_reg-final(handle:)` has since fixed.

## Measurement conditions

- rheo: `--release` build of `/home/lox/code/_fcl/rheo-bd19` at `7ebdc10c`
  ("Lets a ligament bind match changed keys by prefix", the same revision as
  `/home/lox/code/_fcl/rheo`'s working copy parent), target dir
  `/tmp/bd19-target`.
- rookery: `/home/lox/code/_fcl/rookery` at "Drops a recompiled page's stale
  fed ligament ids" for the reproduction. The probes used a copy,
  `/tmp/d19-rookery`.
- Project: a `/tmp` copy of waterline's `rookery/` (build dir excluded), with
  `[packages.rookery]` replaced by a `path =` entry. `[watch] narrow = true`
  was already set. A full compile writes 2,394 pages.
- Driver: `rheo watch rookery --input today=2026-11-15 --html --trigger stdin
  --events <file>`, fed `rebuild <path>` lines through a fifo. The driver waited
  for a new `build_finished` line after each one. The comparison is `rheo
  compile . --html --input today=2026-11-15` from a clean build dir, with every
  `<style>…</style>` block stripped from both sides first.
- The edit: delete `#idea(title: [Zoom room])[…]` from `nest/tidbits.typ`,
  then send `rebuild nest/tidbits.typ`. The narrowed rebuild took 12.1s:
  `narrowed: 7 vertebra(e) in 2 round(s) — nest:tidbits,
  nest:jobs:cycle-25-26, nest:clusters:liminal:airborne-review-4sep26,
  nest:clusters:digitaltheory:review, nest:jobs:cycle-26-27, nest:jobs,
  index`.
- Machine: `nproc` = 12, load 2.5–3.5.

The reproduction gives the same eight files as the bird:

```
only narrowed ideas/zoom-room.html
DIFF ideas/chi-star-2026-25-26.html
DIFF ideas/chi-star-2026.html
DIFF ideas/lack-vi-26-27.html
DIFF ideas/lack.html
DIFF ideas/onward.html
DIFF index.html
DIFF nest/clusters/digitaltheory/review.html
DIFF nest/clusters/liminal/airborne-review-4sep26.html
```

## 1. Link anchors depend on which links were compiled (rheo / Typst)

**What differs.** The full build has `<span id="index"></span>` as the first
child of `<body>` in `index.html`. It has `<span id="loc-1"></span>` in the
same spot in `airborne-review-4sep26.html` and `digitaltheory/review.html`.
The narrowed build has none of these.

**Mechanism.** `typst-bundle` (0.15, `src/lib.rs`, `let targets =
introspector.link_targets()`) collects every link destination in the bundle
and hands it to `typst_html::create_link_anchors`. That pass gives each
target element an id, inserting an empty `<span>` when the element produces
no node of its own. `link_targets()` comes from
`LinkElem::find_destinations`, which queries **every** `LinkElem` and resolves
its destination late. A label destination resolves to the labelled element's
`Location`.

rheo's handle anchors (the `#figure` per vertebra that `BundleAnchor` emits
before each include) carry the handle as their label. A link such as
`#link(<nest:clusters:liminal:airborne-review-4sep26>)` in
`nest/clusters/liminal/index.typ` is rewritten by `rheo-link-rule`
(`crates/core/src/typ/rheo.typ`) into a string href. The original `LinkElem`
is still in the introspector, though, and still counts as a destination. So
the full build anchors the handle figure. The label `index` is a valid id, so
the anchor takes it. A handle containing `:` does not, so it falls back to
`loc-1`. In the narrowed build `liminal/index`, `digitaltheory/index` and the
weeknotes (`link(label("index"))`) are not compiled, nothing points at those
figures, and no span appears.

**Confirmed** by removing the one `#link(<…airborne-review-4sep26>)` from
`liminal/index.typ` in the `/tmp` copy and compiling in full: the
`loc-1` span disappears from `airborne-review-4sep26.html`, and
`review.html` keeps its own (its linker was untouched). No href anywhere in
either build points at these ids. `rg 'href="[^"]*(loc-1|#index)'` over
the three pages finds nothing, and the only incoming href to the airborne page
is the rewritten `../../nest/clusters/liminal/airborne-review-4sep26.html`.

**Not the same as** `wl-spike-rheo-side-location-link-anchors-fa7cb366`, which
asks whether rheo can resolve `Location`-typed destinations for a native
`#outline()`. This is a different question: whether an anchor's *existence*
can depend on which pages a narrowed compile included. It can. Here that only
produces dead spans, but the same pass serves real `Location` links: rookery's
`#ideas-outline` links each entry to `el.location()`, and `#idea`'s hidden
anchor figure exports as `loc-N`. A target page recompiled without its linker
would drop a live anchor, and the `loc-N` numbering after it could shift. The
rheo-tests ignore reason "notes/n05.html's `<figure id="loc-1">` loses its id"
is very likely this; it was not reproduced here.

**Fix owner: rheo.** Filed as a spike (below). Typst only exposes this through
the bundle export, so the choice between feeding the previous compile's targets
back, stripping dead anchors, or accepting the drift needs a decision first.

## 2. A fed body's `context` replays against a frozen registry (rookery core)

**What differs.** In `digitaltheory/review.html`, three window bodies
(todos `dtl-b`, `dtl-c`, `dtl-d`) open with `<dl class="todo-fields">Blocked
by …</dl>` in the full build and with the prose alone in the narrowed one.

**Mechanism.** `#todo` (`todos/0.1.1/src/todo.typ`) builds its body as
`{ todo-blocked-by(deps); body }`. `todo-blocked-by` (`fields.typ`) is a
`context` block calling `todo-graph()` → `todos()` → `ideas()` /
`tag-data()` → `_reg-final()`. That body is what `#idea` registers as
`raw`, and `_idea-ligaments` attaches it under `idea-body:<id>`. rheo
stores the attached `Value` as it is (`harvest_ligaments` clones it), so the
content carries its closure. In the narrowed compile, `review` is
recompiled but `digitaltheory/index` (where the todos are written) is not.
The window renders the *fed* body, and its closure runs.

That closure's function values, `_reg-final` included, are the ones evaluated
in the compile that attached the body: the initial full build. `_reg-final`
reads `_LIG-RECS`, a top-level `#let` in `core/0.1.1/src/state.typ`
(`#let _LIG-BUILT = {`). In a full build it is `none`, so `_reg-final()`
returns the live registry alone.

**Confirmed with probes** in `/tmp/d19-rookery`. The probe was a `<span>`
inside `todo-blocked-by` printing the graph and registry state, plus a fresh
`#context` written directly on `review.typ`:

| | full build | narrowed, inside fed closure | narrowed, fresh `#context` on the page |
|---|---|---|---|
| todo graph nodes | 198 | 69 | — |
| `"idea:dtl-a" in _reg-final()` | true | **false** | **true** |
| `_reg-final().len()` | — | — | 1591 |
| `sys.inputs.rheo-ligaments.get("ideas")` holds `dtl-a` | — | yes (page `nest:clusters:digitaltheory`) | — |

So the feed is complete and reading `sys.inputs` from the old closure sees it.
The stale part is the captured module binding. Every `context` inside any
stored note body has the same exposure, references included:
`_ref-text-live` also reads `_reg-final()`. Only the todo block shows up in
this diff.

**Fix owner: rookery** (`@rookery/core`). `_reg-final` should compute the
merged records from the store at call time, not from a module-level value
captured at definition. A zero-argument memoised function reading
`_lig-get(_LIG-IDEAS)` still builds once per compile. rheo cannot rebind a
closure inside a stored value, and should not try.

## 3. Minted backlink windows follow live-first order (rookery core)

**What differs.** On `ideas/onward.html` the full build lists backlink windows
as `what-does-this-mean`, `onward-2027-26-27`; the narrowed build swaps them.
`ideas/lack.html` is the same (`uvm`, `lack-vi-26-27` vs the reverse). The
windows themselves are byte-identical.

**Mechanism.** `.marrow.typ` builds `backlinks` by walking
`merged.pairs()`, where `merged = _reg-final()`, and the windows render in
that order. `_reg-final` returns `(: ..live, .._LIG-RECS, ..live)`, so
ids registered this pass come first. `onward-2027-26-27` lives in
`nest:jobs:cycle-26-27`, which recompiled; `what-does-this-mean` lives in
`nest:clusters:fcl:motivation`, which did not. The fed records that follow
are in rheo's feed order, and `Ligaments::by_key` (`crates/core/src/build.rs`)
sorts pages by handle string, not by spine position. A full build's live
registry is in document order, which is spine order.

**Fix owner: rookery.** `_reg-final` should order its records by spine
position (`rheo-context`'s `spine-flat`), taking each page's ids from the
live registry when that page compiled and from the feed otherwise. Changing
rheo's feed order alone would not be enough, because the live-first spread
stays. Rookery already has every input it needs.

## 4. Page backlinks of uncompiled pages come from note links (rookery core)

**What differs.** Minted pages' `idea-page-list`:

| page | full | narrowed |
|---|---|---|
| `ideas/chi-star-2026.html`, `ideas/chi-star-2026-25-26.html` | `nest/papers/document-infrastructure/chi2026` | — |
| `ideas/onward.html` | — | `nest/clusters/fcl/motivation` |
| `ideas/lack.html` | — | `nest/institutions` |
| `ideas/lack-vi-26-27.html` | — | `nest/papers/lack-vi` |

**Mechanism.** Page backlinks are meant to be pages that link to a note *in
their own right*. A page whose only links sit inside its notes is
deliberately left out (`.marrow.typ`'s PAGE backlinks banner). `_page-links()`
(`core/0.1.1/src/outline.typ`) gets those from `<rookery-page-links>` beacons
and `<rookery-window-mark>`s, both `query()`-only, so they cover compiled
pages only. For every other page its ligament fallback reads
`backlinks:<id>` hits (`let hits = _lig-get(_LIG-BACKLINKS + id`). Those are
*note-level* edges, attached by the linking note's owning page. So:

- `chi2026.typ` writes `@idea:chi-star-2026` in page prose, outside any note.
  That is a page link, and no ligament carries it, so the narrowed compile
  cannot see it.
- `motivation.typ`'s note "What does this mean?" links `@idea:onward`.
  That is a note link, which the fallback counts as a page link of
  `motivation`. The other two extra rows are the same shape.

**Fix owner: rookery.** Publish each vertebra's page-level targets as their own
ligament (from the marrow loop that already walks `_page-links()`) and read
that key, not `backlinks:`, for pages that did not compile.

## 5. Stale backlink-window text after editing a linking note (rookery core)

Not among the five groups, because the reproduction edit changes no linking
note. Checked separately because rheo-tests' rookery scenarios report it
(`watch_narrow_rookery_structure.rs`, `watch_narrow_rookery_notes.rs`: "…'s
backlink window keeps its pre-edit text").

**Reproduced:** after a fresh full watch build, `nest/clusters/fcl/motivation.typ`'s
"What does this mean?" body was edited (`literate programming` → `D19MARK
programming`) and `rebuild nest/clusters/fcl/motivation.typ` sent. The result
was `narrowed: 2 vertebra(e) … nest:clusters:fcl:motivation,
nest:clusters:fcl:show`. `D19MARK` appears in `motivation.html` and
`ideas/what-does-this-mean.html`, but **not** in `ideas/onward.html`, whose
backlink window transcludes that body.

**Mechanism.** A minted page is rebuilt with its note's owning vertebra
(`onward` → `nest:jobs:cycle-26-27`). `_idea-ligaments`
(`core/0.1.1/src/state.typ`) has that vertebra bind `backlinks:onward`. That
key changes only when the *set* of linking notes changes. The linking side
attaches `backlinks:onward` and binds the *target's* `idea:`/`idea-body:` on
its own page. Nothing binds the linking note's `idea-body:what-does-this-mean`
on the target's owning vertebra, so a prose edit there recompiles nothing that
mints `onward`.

So this is **not** the cause of the minted-page differences in groups 3–4.
Those happen on minted pages that *were* recompiled; this one is a page that
is never recompiled. **Fix owner: rookery.** In marrow's backlinks loop,
bind `idea:<src>` and `idea-body:<src>` on the target's `origin` for every
backlinker `src`.

## 6. A deleted note's minted page outlives a narrowed rebuild (rheo)

`ideas/zoom-room.html` exists only in the narrowed build, and rheo-tests'
`note_lifecycle` reports the same (`ideas/n07a.html survives the idea's
deletion`). `prune_stale_pages` (`crates/core/src/build.rs`) runs only after
a full `Build::run`, and the `Build` field it reads is documented "Narrowed
rebuilds neither prune nor update it". That is a deliberate limit of
`rh-prune-output-of-deleted-vertebrae-59325112`, not a regression. Pruning in a
narrowed rebuild needs to know which vertebra minted each contributed page,
and rheo does not record that today. **Fix owner: rheo.** Filed as a spike,
since the ownership record is a design choice.

## Already covered, named here so they are not re-filed

- `wl-list-every-idea-in-a-narrowed-ideas-76b96a6c`: `#ideas-outline` is
  query-only, so a narrowed outline truncates. No difference in this
  reproduction comes from it (no narrowed page here carries a rookery-wide
  outline), but rheo-tests' `everything.html` failures do.
- `wl-spike-rheo-side-location-link-anchors-fa7cb366`: `Location`-typed link
  destinations under a native outline. It is related to cause 1 (same Typst
  anchor pass) but asks a different question.

## Birds filed (waterline tracker)

| cause | bird | owner |
|---|---|---|
| 1 | `wl-spike-narrowed-link-anchor-drift-38d9e7ec`: Spike narrowed link-anchor drift | rheo |
| 2 | `wl-read-the-fed-registry-at-call-time-41ec7800`: Read the fed registry at call time | rookery |
| 3 | `wl-order-the-merged-registry-by-spine-b5e90a83`: Order the merged registry by spine | rookery |
| 4 | `wl-feed-page-level-links-as-a-ligament-73d716f9`: Feed page-level links as a ligament | rookery |
| 5 | `wl-bind-backlinkers-on-the-target-s-page-0b05788d`: Bind backlinkers on the target's page | rookery |
| 6 | `wl-spike-pruning-minted-pages-when-narrowed-5711805f`: Spike pruning minted pages when narrowed | rheo |

The four rookery birds edit overlapping files (`state.typ`, `.marrow.typ`),
so they are chained 2 → 3 → 4 → 5 and fly one at a time.
