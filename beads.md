# beads.md — transport document

Beads state (`.beads/`) is machine-local and never committed, so it does not
travel between computers. This file carries the current queue in a form that
does travel. Each entry below is a full bead description, written so it can be
re-created with `br create` on another machine and handed to an implementer
with no other context.

Two sections: **Open work** (beads that existed in the local db and were
deleted after being written down here) and **Proposed work** (the packages
refactor, discussed but never filed).

Recreate a bead with:

```sh
br create "<title>" -t <bug|task|feature> -p <0-4> -l feat-rssfeed -d '<description>'
```

**Status, 2026-08-19.** Every entry below was reviewed against the local db and
against current source, and five were filed. The local label for this branch's
work is `feat-rssfeed`, not `feat-transclusion` — that is what the already-closed
parents (`rheo-transclude-3yw`, `rheo-head-hoist-qz6`, `rheo-head-control-cbr`)
carry, so the filed beads use it. Each entry now opens with a **Filed as** or
**Decision** line. Note that this file is a snapshot of the transclusion queue
only: the local db additionally holds work this document never knew about
(the `feat-rheo-version-floor` pair, `rheo-spine-include-df5`, `rheo-ap3`,
`rheo-onp`, and the `feat-rssfeed` removal chain).

All entries relate to the `feat/transclusion` branch (13 commits off `main`,
version 0.6.0): Typst-native document metadata via per-vertebra metadata
beacons, `<rheo-content>` post-compile transclusion, and the `<rheo-head>` /
`.rheo/head.html` head-contribution mechanisms.

---

## Open work

These three were filed as `rheo-zft`, `rheo-ul6` and `rheo-dr6`, then deleted
from the local db in favour of this document.

### 1. Hoist `<rheo-head>` on the dev-server preview path

**Type:** bug — **Priority:** 1 — **Label:** `feat-rssfeed`

**Filed as `rheo-head-hoist-watch-mhp`.** Every line reference below re-verified
2026-08-19 and still accurate. The filed bead adds one thing this draft missed:
it interacts with `rheo-onp` (the head-injection DOM round-trip empties a
`<template>`), because it makes the watch path take that round-trip on more
pages than it does today. A note to that effect is on `rheo-onp` as well.

The HTML plugin's disk-writing path hoists `<rheo-head>` wrappers into each
page's `<head>` (`crates/html/src/lib.rs:177`, `dom.hoist_rheo_head()?`), but
the dev-server in-memory path in `Build::compile_html_to_memory`
(`crates/core/src/build.rs`, the closure at lines 497-517) never calls
`hoist_rheo_head()`. It only injects CSS/JS links and appends the site-wide
`.rheo/head.html` fragment.

Consequence: under `rheo watch`, a page authored with
`html.elem("rheo-head", html.elem("meta", ..))` keeps the literal
`<rheo-head>` element in its body and its `<meta>`/`<link>` children never
reach `<head>` — so the served preview differs from what `rheo compile`
writes. The comment at `crates/core/src/build.rs:464-469` already claims this
path mirrors the compile path, so today the comment is wrong.

Second, smaller problem in the same place: the decision to parse the DOM at
all is `needs_head_mutation` (`crates/core/src/build.rs:471`), which is only
`needs_injection || control_head_fragment.is_some()`. Hoisting is a PER-PAGE
need, so a project with no CSS/JS and no `.rheo/head.html` would skip the
parse entirely and still need hoisting. The gate must therefore be computed
per page, not once up front.

Steps:

1. In `crates/core/src/build.rs`, inside the `filter_map` closure, in the
   branch handling non-asset `.html` entries (currently guarded by
   `if needs_head_mutation && path_str.ends_with(".html")` at line 497):
   decode the bytes to a string BEFORE the guard decision for `.html` paths
   and compute `let has_rheo_head = html.contains("<rheo-head");` — the same
   cheap substring pre-check `crates/html/src/lib.rs:154` uses.
2. Change the guard so the DOM round-trip happens when
   `needs_injection || control_head_fragment.is_some() || has_rheo_head`. Keep
   `needs_head_mutation` if useful as the format-global part of that
   expression, but the final decision must include the per-page
   `has_rheo_head`.
3. Inside the mutation block, call `dom.hoist_rheo_head()?` AFTER the
   `inject_head_links` call (lines 501-511) and BEFORE the
   `append_head_fragment` call (lines 512-514). This ordering is required:
   `crates/core/src/util/html.rs` documents that site-wide `.rheo/head.html`
   content must land after per-page `<rheo-head>` content, and
   `crates/html/src/lib.rs:177-183` uses exactly that order.
4. Add a test of the pipeline behaviour, not of `HtmlDom` directly (DOM-level
   tests already exist in `crates/core/src/util/html.rs`). In
   `crates/core/src/build.rs`'s `mod tests`, build a temp project — see the
   existing `test_run_resolves_title_from_imported_template_via_document_info`
   test in the same file for the `ProjectConfig`/`Build::prepare` setup shape
   — whose content file emits an `html.elem("rheo-head", ...)` wrapper, call
   `Build::compile_html_to_memory`, and assert the returned VirtualFs entry
   for that page contains the meta inside `<head>` and no longer contains the
   string `rheo-head`.

Do NOT change the HTML plugin path (`crates/html/src/lib.rs`) — it is already
correct. Do NOT add feed-autodiscovery-link injection to the dev-server path;
that is a separate pre-existing difference and out of scope here. Do NOT alter
`hoist_rheo_head` itself in `crates/core/src/util/html.rs`.

VERIFY:

1. `cargo test` passes.
2. The new test fails if the `dom.hoist_rheo_head()?` line is removed (check
   by commenting it out once).
3. `cargo fmt && cargo clippy -- -D warnings` is clean.

### 2. Document that EPUB `dc:creator` no longer reads `rheo-author` or `<meta name=author>`

**Type:** task — **Priority:** 2 — **Label:** `feat-typst-native-metadata`

**Filed as `rheo-epub-author-docs-5i3`, narrowed to `docs/limitations.md` only.**
Step 2 below (a sentence in `CLAUDE.md`'s "rheo-\* variables and the Atom feed"
section) was DROPPED: `rheo-claude-md-zb3` deletes that entire section, so the
sentence would be written and then deleted. Telling upgrading users about
`#let rheo-author` is already step 4 of `rheo-migrate-feed-dhe`. The label also
changed — this belongs to `feat-typst-native-metadata`, the epic that made
`dc:creator` Typst-resolved, and it is unblocked NOW (that change has landed on
this branch) whereas the whole `feat-rssfeed` chain sits behind a cross-repo gate.

On this branch the EPUB plugin's `extract_author` helper was deleted (it used
to live at the end of `crates/epub/src/lib.rs`, before `pub struct EpubItem`).
It had two fallbacks that are now gone:

1. a harvested `rheo-author` variable (`vars.get("author")`, i.e. an authored
   `#let rheo-author = "Jane"` in a vertebra), and
2. an HTML `<meta name="author" content="...">` scrape of the first output.

EPUB `dc:creator` now comes only from Typst-resolved `CastVertebra::author`
(`crates/epub/src/lib.rs:90-93`), i.e. from `#set document(author: ...)`, with
multiple authors joined by `", "`.

This is the right direction — it matches the branch's Typst-native metadata
design — but it is a user-visible behaviour removal that is currently
documented NOWHERE. `grep -rn "rheo-author" .` returns nothing at all: not in
`CLAUDE.md`, not in `docs/limitations.md`. Contrast with the
`datetime.today()` behaviour change, which the branch does document in
`docs/limitations.md`. The decision is already made: keep the removal,
document it. Do not restore the old code.

Steps:

1. In `docs/limitations.md`, add a short subsection under the existing
   "Document metadata is resolved by Typst, not scanned pre-compile" section
   (alongside sibling subsections such as "`datetime.today()` now resolves to
   a real, build-varying date"). It must state: EPUB `dc:creator` is taken
   from `#set document(author: ...)` only; the previously-supported
   `#let rheo-author = "..."` variable and an HTML `<meta name="author">` tag
   are no longer consulted; multiple authors are joined with `", "` because
   EPUB's `dc:creator` is a single string.
2. In `CLAUDE.md`, in the "rheo-* variables and the Atom feed" section, add
   one sentence making clear that `rheo-author` is not a recognized variable
   for EPUB authorship — use `#set document(author: ...)`. Keep it to one or
   two sentences; that section is a reference list, not prose.
3. Match the surrounding documentation register: plain declarative sentences,
   concrete file paths, no marketing tone. The `lachlanify` skill describes
   this voice if the register is unclear.

Do NOT re-add `extract_author` or any `rheo-author` handling to
`crates/epub/src/lib.rs`. Do NOT touch any Rust file at all — this is
documentation only. Do NOT rewrite unrelated parts of `docs/limitations.md`.

VERIFY:

1. `grep -rn "rheo-author" docs/limitations.md CLAUDE.md` returns at least one
   hit in each file.
2. `jj diff --stat` shows exactly two changed files, both markdown.

### 3. Error rather than silently skip a malformed `<rheo-content>` placeholder

**Type:** task — **Priority:** 3 — **Label:** `feat-rssfeed`

**Filed as `rheo-transclude-strict-ofv`.** Re-verified 2026-08-19: patterns at
`transclude.rs:45` and `:49`, `scan` at `:78`, the silent `let page = page?;` at
`:97`. One correction folded into the filed bead — the existing test
`test_scan_ignores_placeholder_missing_required_page` asserts the OLD silent
behaviour, so it must be rewritten, not merely added next to.

File: `crates/core/src/transclude.rs`, `ContentTransclusion::scan` (lines
44-50 define the regexes, lines 78-107 do the scan).

`<rheo-content .../>` placeholders are matched with a regex, not an HTML
parser:

```rust
static ref TAG_PATTERN: Regex = Regex::new(r#"<rheo-content\b([^>]*)/>"#)
static ref ATTR_PATTERN: Regex = Regex::new(r#"([a-zA-Z][a-zA-Z0-9_-]*)\s*=\s*"([^"]*)""#)
```

Two failure modes, both currently SILENT (the placeholder is left in the
output as literal text, and the build succeeds):

1. `[^>]*` stops at the first `>`, so an attribute value containing `>` (e.g.
   `select=".a > .b"`) never matches `TAG_PATTERN`.
2. A placeholder with no `page` attribute is deliberately dropped by the
   `let page = page?;` line in the `filter_map`, per the doc comment on
   `scan`.

Both are acceptable behaviours in themselves; what is not acceptable is that
an author gets no signal. A literal `<rheo-content .../>` string surviving
into a shipped feed or sitemap is always a mistake.

Steps:

1. Keep `TAG_PATTERN` and `ATTR_PATTERN` as they are — do NOT attempt a real
   HTML/XML parse of the asset, and do NOT try to support `>` inside attribute
   values. The regex approach is deliberate: the asset may be XML (an Atom
   feed), not HTML.
2. Add a second, deliberately loose detection regex, e.g. `<rheo-content\b`
   (opening tag only, no closing constraint), and use it to count candidate
   placeholders in the text.
3. In `ContentTransclusion::rewrite_text` (around line 122), after
   `let placeholders = Self::scan(text);`, compare the count of loose
   candidates against `placeholders.len()`. If the loose count is higher,
   return an error via `RheoError::invalid_data` naming the asset
   (`asset_name` is already a parameter) and stating that a `<rheo-content>`
   placeholder could not be parsed — mention the two known causes: a missing
   required `page` attribute, and a `>` character inside an attribute value.
4. Update the doc comment on `scan` (lines 74-77) so it no longer says a
   malformed placeholder is silently skipped, and the module-level docs (lines
   1-30) if they claim the same.
5. Add unit tests in the existing `mod tests` at the bottom of the same file,
   next to `test_scan_ignores_placeholder_missing_required_page`: (a) a
   placeholder missing `page` now makes `rewrite_assets` fail with an error
   naming the asset; (b) a placeholder with `>` inside an attribute value also
   fails; (c) an asset with no `<rheo-content` at all is still byte-identical
   (this case already has a test — `test_asset_with_no_placeholder_is_byte_identical`
   — confirm it still passes rather than duplicating it).

Do NOT change the resolution/encoding logic (`resolve`, `resolve_from_map`,
`resolve_html`). Do NOT change the `has_rheo_head` substring pre-check in
`crates/html/src/lib.rs:154` — a page merely mentioning that string pays for
one extra DOM round-trip, which is harmless and out of scope.

VERIFY:

1. `cargo test` passes, including the two new failing-placeholder tests.
2. `cargo fmt && cargo clippy -- -D warnings` is clean.
3. An asset containing exactly `<rheo-content select="main"/>` (no `page`)
   produces a build error whose message contains both the asset name and the
   words `rheo-content`.

---

## Proposed work — keeping rheo minimal, pushing metadata into packages

Context for all four: the question was whether the document-metadata spine
machinery could move out of rheo into a standalone Typst package under
`/home/lox/code/_fcl/rheo-packages` (the `@rheo` namespace; each package lives
at `<name>/<version>/`).

Findings that shape the proposals, so they need not be re-derived:

- **The handover mechanism already exists.** rheo seeds
  `sys.inputs.rheo-context` once per bundle compile with the format-global
  spine (`spine`, `spine-flat`, `target`, `ext`), readable from any scope
  including package code — no `#context` needed. Packages can additionally
  contribute bundle-root marrow, spliced into the synthesized main
  (`crates/core/src/build.rs:277-285`). Nothing new needs inventing; what is
  missing is a written, versioned contract.
- **Beacon emission cannot move to a package.** The beacon must be appended
  inside each vertebra's own module, because a `#set document(...)` rule in an
  `#include`d file does not leak to bundle-root siblings — so the bundle main
  cannot emit it after the `#include`, and no package can inject text into
  every vertebra. Only core's per-file epilogue
  (`VirtualSpine::vertebra_injections`) can do this. A package version would
  be opt-in per vertebra, trading automatic correctness for an import.
- **Rust-side reads cannot move either:** EPUB's OPF `dc:*` and PDF Info are
  written from Rust (`crates/core/src/plugins/document_meta.rs`, 162 lines),
  and `<rheo-content>` transclusion (`crates/core/src/transclude.rs`, 518
  lines) is post-compile by definition.

Suggested order was 4 → 5 → 6, with 7 gated on a decision. As resolved
2026-08-19: 7 was already decided and filed (as an outright removal, gated
cross-repo), 4 and 5 are filed with 4 blocked on the version-floor work, and 6 is
declined for now. See each entry's status line.

### 4. Freeze and document the rheo ↔ package metadata contract

**Type:** task — **Priority:** 2 — label `chore-package-contract`

**Filed as `rheo-package-contract-pki`, narrowed and blocked on
`rheo-rheo-version-key-n5k`.** Two things changed since this was drafted.

First, most of it is already written. The commit "Rewrites limitations.md and the
rheo-context contract for Typst-native metadata" landed a `docs/limitations.md`
that already covers items 2, 3 and 4 below plus the payload value types and the
reserved `rheo-meta:` prefix. What is genuinely missing is a *reference* — a flat
key inventory a package author can check a field name against — rather than prose
about caveats. The filed bead is scoped to that, and to cross-linking rather than
restating what `limitations.md` says.

Second, it must wait. `rheo-rheo-version-key-n5k` adds `rheo-version` to
`sys.inputs.rheo-context` and `rheo-pkg-min-version-1fn` adds `[tool.rheo]
min_version` — version negotiation is the whole point of a package-facing
contract, so writing the inventory first guarantees it ships stale. Priority
dropped 1 → 2 for the same reason. (The inventory also has to include
`reset-footnotes`, which this draft did not know about.) `rheo-ap3` would add a
marrow-ordering clause if built; the filed bead records that as a known gap
instead of waiting on it.

The original list of four items, for reference:

1. The beacon label format `<rheo-meta:HANDLE>` and its payload keys
   (`handle`, `title`, `author`, `description`, `keywords`, `date`), including
   which are content vs string vs array vs `datetime`, and that absent keys
   are omitted rather than `none`.
2. That beacons are emitted for `OnePerVertebra` layouts only (HTML/EPUB) and
   never for combined PDF, where `rheo-metadata(handle)` returns `(:)`.
3. The `sys.inputs.rheo-context` shape (`spine`, `spine-flat`, `target`,
   `ext`), including that `spine`/`spine-flat` titles are path-derived only.
4. That reading a beacon requires `#context`, since it is backed by `query`.

Once this exists, any package can read metadata without rheo exporting a
helper at all — that is the whole point. Cross-reference it from
`rheo-packages/CLAUDE.md`, which already documents the two `rheo-context`
consumption patterns.

Do NOT change any behaviour in this bead — it is documentation of what the
code already does.

VERIFY: the document states all four items above, and every claim in it is
checkable against `crates/core/src/util/typst_source.rs` (beacon and helper
rendering) and `crates/core/src/reticulate/spine.rs` (the `OnePerVertebra`
gate and the `global_context` construction).

### 5. Move injected Typst out of Rust format strings into real `.typ` files

**Type:** task — **Priority:** 2 — label `chore-typ-extraction`

**Filed as `rheo-typ-extraction-wna`, unchanged in substance.** Re-verified
2026-08-19: still 336 lines, all four items still built in `format!`, `\x20`
escapes still at lines 129-137 and 167-168. No open bead overlapped it. One note
added — land `rheo-rheo-version-key-n5k` first to avoid a needless conflict
around `typst_source.rs:71`; there is no logical dependency between them.

`crates/core/src/util/typst_source.rs` (336 lines) builds Typst source inside
Rust `format!` strings for `MetadataHelper`, `MetadataAllHelper`,
`MetadataBeacon` and `HandleAnchor` — including a multi-line function body
written with `\x20` escapes to control indentation, and unit tests that assert
on that exact rendered text.

Move those bodies into real `.typ` files next to `crates/core/src/typ/rheo.typ`
and have `RheoWorld` serve them (it already serves synthetic sources: the
bundle main, the per-vertebra overlay, and the prelude/epilogue). The
per-vertebra prelude then becomes an `#import` of that synthetic module rather
than a re-emitted `#let` per file.

Two reasons this is worth doing independently of any package move: the Typst
becomes editable and syntax-highlightable as Typst, and the import site is
exactly the seam that would later let the import point at an external
`@rheo/metadata` package instead — without which any package move means
rewriting this code twice.

Keep the beacon itself generated per vertebra (it must bake a handle) — only
the helper *bodies* move. Do NOT change the rendered semantics; the existing
tests in that file are the safety net and should keep passing in spirit even
if their exact assertions move.

VERIFY: `cargo test` passes; a project still resolves
`(rheo-context().metadata-of)("some-handle")` and `@handle` display text
correctly; `crates/core/src/util/typst_source.rs` no longer contains a
multi-line Typst function body in a format string.

### 6. Move `rheo-metadata-all()` into a package's marrow

**Type:** task — **Priority:** 3 — filed in `rheo-packages`, not here

**Decision 2026-08-19: NOT filed, deliberately declined for now.** Two reasons
beyond the ergonomic cost this entry already names. `rheo-marrow-meta-d5v` ("Make
`rheo-metadata` reachable from the bundle root") closed only days ago — this
proposes removing the thing that bead just landed, which is churn, not
simplification. And its stated prerequisite, the contract, is now blocked behind
the version-floor pair, so the earliest sensible moment for it is after
`rheo-package-contract-pki`. The right sequencing is: contract → `.typ`
extraction (`rheo-typ-extraction-wna`, which creates the import seam this would
use) → revisit. Left recorded here rather than filed so it does not sit in the
ready queue looking like agreed work.

`TypstStmt::MetadataAllHelper` is a one-line
`sys.inputs.rheo-context.spine-flat.map(...)` that core injects only into the
bundle main (marrow scope). Packages already contribute marrow to that same
scope, so a package can define it itself with no new mechanism, and core can
drop it.

Depends on bead 4 (the contract) being written, since the package version
would rely on the documented `spine-flat` shape and beacon label rather than
on core's helper.

Note the ergonomic cost to weigh before doing this: authors currently get
`rheo-metadata-all()` in marrow scope for free, and afterwards would need an
import. This is the one genuinely optional item in this list.

### 7. DECISION NEEDED — retire the built-in Rust Atom feed in favour of a Typst package feed

**Type:** feature — **Priority:** 2 — keep open until decided

**Decision 2026-08-19: ALREADY DECIDED — the feed goes. This entry is stale and
nothing new was filed for it.** The local db has held the decision since
2026-08-15 as a four-bead chain, all labelled `feat-rssfeed`:

- `rheo-drop-feed-bdo` (P1) — delete `crates/html/src/feed.rs`, the `HtmlConfig`
  feed keys, `inject_feed_link`, and the `atom_syndication` dependency. Explicitly
  an outright removal in 0.6.0, **not** the deprecation path this entry asks for.
- `rheo-drop-vars-1ay` (P2) — delete the `rheo-*` variable convention outright.
- `rheo-migrate-feed-dhe` (P2) — `rheo migrate` reports the removed keys and
  variables and points at `@rheo/rssfeed`. This is what carries the user-facing
  warning instead of a release-cycle deprecation.
- `rheo-claude-md-zb3` (P2) — rewrite `CLAUDE.md` around the layering rule that a
  `FormatPlugin` does format transport only.

The replacement is the Typst package `@rheo/rssfeed` in `../rheo-packages`. Note
the cross-repo sequencing gate recorded on `rheo-drop-feed-bdo`: beads cannot
express cross-repo dependencies, so although it is dep-ready inside `rheo/`, it
must NOT start until `rheo-tests`' `rheo-tests-marrow-feed-g0y` is green,
preferably also `rheo-packages`' `rheo-packages-parity-qrd`. Landing it on
dep-readiness alone ships a release with no feed capability anywhere.

The analysis below stands as the rationale; only its "DECISION NEEDED" framing and
its deprecation-plan recommendation are superseded.

This is the change that would actually make rheo smaller. `crates/html/src/feed.rs`
is 392 lines of Rust generating Atom XML. With `<rheo-content>` transclusion
now landed, a Typst package can build the same feed from marrow: it can read
every vertebra's metadata via the beacons and embed each page's compiled inner
HTML via `<rheo-content page="..." select="main"/>`.

If the built-in feed goes:

- `crates/html/src/feed.rs` (392 lines) is deleted.
- `crates/core/src/plugins/document_meta.rs` (162 lines) shrinks to EPUB's
  needs only.
- `CastVertebra`'s `title`/`date`/`description`/`keywords` plumbing through
  `flatten_bundle_outputs` largely goes with it.

What it costs: `[html] feed_base_url`, `feed_author`, `feed_title` and the
`rheo-feed-*` variables become a deprecation, and feeds stop working
out-of-the-box for projects that add no package.

This is a product decision, not a refactor — do not implement it without an
explicit go-ahead, and if it goes ahead it needs its own deprecation plan
(one release warning, then removal) rather than a straight deletion.
