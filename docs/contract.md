# The rheo ↔ package contract

A flat reference of every key, function, label, and file a `@rheo` (or
`@preview`) Typst package may read from or ship into a rheo build. Where
`docs/limitations.md` explains *why* a field behaves the way it does and what
its caveats are, this document only answers "what is the name, the type, and
is it always there" — check a field name against this before assuming it
exists. `CLAUDE.md`'s `## rheo-context` and `## Cross-file references`
sections already narrate most of this in prose; this document does not repeat
that narration, only the flat key list it implies.

## Stability

rheo is pre-1.0 (`0.6.0` at time of writing). Read stability against that:

- **Stable** — the field/function/file exists, with this name and this shape,
  for the rest of the current `0.x` line. A `0.x` → `0.(x+1)` bump may change
  it, called out in `changelog.md`. A package pinning `[tool.rheo] min_version`
  (below) can rely on it not moving underneath an already-released version.
- **Internal** — present in the data today but not part of the contract: no
  stability guarantee, may change or disappear in a patch release with no
  changelog entry. Listed here anyway so a package author who stumbles on it
  (e.g. iterating `rheo-context()`'s keys) knows not to depend on it.

## `rheo-context()` / `sys.inputs.rheo-context`

`rheo-context()` is `(handle: <this file's handle>, metadata-of: rheo-metadata, ..sys.inputs.rheo-context)`
(`crates/core/src/util/typst_source.rs:137-141`, `TypstStmt::ContextBinding`).
The spread half — everything except `handle` and `metadata-of` — is exactly
`sys.inputs.rheo-context`, built once per build by
`VirtualSpine::global_context` (`crates/core/src/reticulate/spine.rs:1046-1086`)
and detectable directly (no per-file call needed) via `"rheo-context" in
sys.inputs`.

| Key | On | Type | Always present? | Stability | Notes |
| --- | --- | --- | --- | --- | --- |
| `handle` | `rheo-context()` only | `str` | yes | Stable | This file's own `:`-joined handle. The only per-file field; never on `sys.inputs.rheo-context`. |
| `metadata-of` | `rheo-context()` only | function `(handle) => dict` | yes | Stable | `= rheo-metadata`. Dict field, not a method — call as `(rheo-context().metadata-of)(handle)`. Requires `#context`. |
| `spine` | both | `array` (recursive node dicts: `title`/`handle`/`path`/`children`) | yes | Stable | Titles are path-derived only, never a `#set document(title:)` value — see `docs/limitations.md`. |
| `spine-flat` | both | `array` (dicts: `handle`/`path`/`title`) | yes | Stable | Pre-order, groups excluded. Same path-derived-title caveat. |
| `rheo-version` | both | `str`, semver (`x.y.z`) | yes | Stable | This build's own rheo version (`env!("CARGO_PKG_VERSION")`); pairs with `[tool.rheo] min_version` below for version negotiation. Verified by `rheo-tests/cases/rheo_context_all_formats/a.typ`. |
| `target` | both | `str` (`"html"` \| `"epub"`) | **no** — omitted for PDF | Stable | Present for per-page formats only. `target()` (the polyfill, see `CLAUDE.md`) is the per-file-friendly way to read it. |
| `ext` | both | `str` (`"html"` \| `"xhtml"`) | **no** — omitted for PDF, gated identically to `target` | Stable | `target`/`ext` always appear together or not at all — asserted in `rheo-tests/cases/rheo_context_all_formats/a.typ`. |
| `reset-footnotes` | both | `bool` | yes | Stable | Per-format `reset_footnotes` toggle (`rheo.toml`, default `true`); read by `typ/rheo.typ`'s page-init, ANDed with the `ext` gate so it only takes effect for per-page formats regardless of its own value. |
| `title-overrides` | both | `array` of `(handle: str, title: str)` dicts | yes (usually `()`) | **Internal** | Feeds `rheo-title-override`/the metadata beacon fallback for the gated `--metadata-two-pass` second compile (`crates/core/src/build.rs:355-401`). Empty on an ordinary build. Not a data source for packages — read `metadata-of`/`rheo-metadata` instead, which already fold this in. |

Everything in this table not marked Internal is safe to read straight off
`sys.inputs.rheo-context` with no `#context` — only `metadata-of`/`rheo-metadata`
need it (they call `query(...)`).

## Metadata beacon — `<rheo-meta:HANDLE>`

Every vertebra compiled under a per-page layout (HTML/EPUB) publishes a
labelled `#metadata(...)` element after its own body
(`crates/core/src/util/typst_source.rs:153-157`, `TypstStmt::MetadataBeacon`):

```typst
#context [#metadata((handle: "<h>", title: document.title, author: document.author,
  description: document.description, keywords: document.keywords, date: document.date)) <rheo-meta:<h>>]
```

No package reads this element directly — always go through `rheo-metadata(handle)` /
`rheo-metadata-all()` / `metadata-of`, which query it and post-process the
result (`crates/core/src/typ/metadata.typ:41-54`, `rheo-metadata-impl`):

| Payload key | Typst type | Included when |
| --- | --- | --- |
| `title` | content | `document.title` is not `none`/`auto` |
| `author` | array (of `str`) | non-empty array |
| `description` | content | not `none`/`auto` |
| `keywords` | array (of `str`) | non-empty array |
| `date` | `datetime` | not `none`/`auto` |

`handle` is in the raw beacon payload but always stripped by
`rheo-metadata-impl` before the dict reaches a caller — never expect it in
`rheo-metadata(handle)`'s return value. **Omission rule:** a key absent from
the vertebra's own `#set document(...)` is *omitted* from the dict entirely
(never present as `none`), and an empty array (e.g. no `keywords` at all)
is omitted too, not returned as `()`. Combined PDF emits no beacon at all —
`rheo-metadata(handle)` returns `(:)` there, unconditionally.

## Metadata helpers (`crates/core/src/typ/metadata.typ`)

| Helper | Signature | Scope | Notes |
| --- | --- | --- | --- |
| `rheo-metadata` | `(handle) => dict` | every vertebra (its own prelude) **and** marrow root | Requires `#context`. Same beacon-reading logic in both places — the marrow-root copy re-imports the identical `MetadataHelper` statement (`crates/core/src/world.rs:447`), so the two sites cannot drift apart. |
| `rheo-metadata-all` | `() => array` | marrow root **only** | `sys.inputs.rheo-context.spine-flat.map(e => (handle: e.handle, path: e.path, ..rheo-metadata(e.handle)))` — one entry per `spine-flat` vertebra, each `(handle, path, ..resolved fields)`. Never injected per-vertebra (`crates/core/src/world.rs:448`). |
| `rheo-handle-title` | `(handle, fallback) => content` | marrow root **only** | Live title lookup: the harvested title-override if `--metadata-two-pass` flagged one, else the beacon's current `document.title`, else `fallback`. This is what every `@handle` cross-reference anchor calls internally (`TypstStmt::HandleAnchor`); a package building its own nav/index over `spine-flat` can call it directly instead of re-deriving the same fallback logic. |

`rheo-metadata`/`rheo-metadata-all`/`rheo-handle-title` are injected into the
bundle main only, immediately after `typ/rheo.typ` and before
`#show: rheo_template` (`crates/core/src/world.rs:447-459`) — so they, and
anything a package's own marrow contribution defines, are marrow-scope, not
per-vertebra scope.

## Bundle-output primitives (`crates/core/src/transclude.rs`)

**`<rheo-content page="..." select="..." as="..."/>`** — resolved after
compilation, replaced with the *inner* HTML of a selected region of the named
already-compiled page.

- `page` (required) — a compiled page's plugin-output-relative path.
- `select` (optional) — a bare tag name (`"article"`) or a leading-dot class
  (`".rheo-content"`). Absent uses the default cascade, first match wins
  (`crates/core/src/util/html.rs:317-321`, `HtmlDom::select_inner_html`):
  1. first `<main>` element;
  2. first element carrying class `rheo-content`;
  3. first element carrying class `rheo-feed-content` (compatibility alias
     only — prefer `rheo-content`, since transclusion isn't feed-specific);
  4. whole `<body>`.
- `as` (optional) — `escaped` (default; entity-escaped, for `<content
  type="html">`), `raw` (verbatim, for `<content type="xhtml">`), or `json`
  (escaped as a JSON string body, for a JSON Feed's `content_html`) — all
  three read from `Encoding` in `crates/core/src/transclude.rs:88-99`.

Executable proof: `rheo-tests/cases/transclude_content/content/.marrow.typ`
exercises the default cascade, an explicit `select="main"`, and `as="raw"`;
`rheo-tests/cases/marrow_atom_feed/.marrow.typ` is a full hand-rolled Atom feed
built from `<rheo-content>` + `rheo-metadata-all()` + `spine-flat` alone.

**`<rheo-head>`** — a wrapper anywhere in a page's body; its children are
hoisted into that page's own `<head>`.

**`.rheo/head.html`** — a control asset minted from marrow (e.g.
`asset(".rheo/head.html", "...")`) whose decoded contents are appended to
*every* page's `<head>`, after each page's own `<rheo-head>` content
(`crates/core/src/transclude.rs:272-341`, `ControlAssets`).

**`.rheo/` prefix** — reserved for bundle assets consumed internally by rheo:
never written to a plugin's output directory, never embedded in EPUB, never
served by the dev server (`crates/core/src/util/constants.rs:18-25`,
`CONTROL_ASSET_PREFIX`). An unrecognized `.rheo/*` member is dropped with a
`warn!`, not an error.

## Package manifest keys (`typst.toml`, `crates/core/src/plugins/typst_manifest.rs`)

Read once per resolved package (`PackageManifest::load`, silently `None` on a
missing/unreadable/unparseable manifest — a malformed sibling package must
never break the build):

| Table | Key | Type | Meaning |
| --- | --- | --- | --- |
| `[tool.rheo.<format>]` | `css_stylesheet` | `str` (path, relative to the package's own root) | Consulted only by plugins that declare an `AssetConfig` under that name — today only `html` (`crates/html/src/lib.rs:48`). |
| `[tool.rheo.<format>]` | `js_scripts` | `str` (path) | Same as above (`crates/html/src/lib.rs:49`). |
| `[tool.rheo.<format>]` | `copy` | `array` of glob strings | Copied into that format's output dir for every format `manifest_blocks_for` is called with (html/pdf/epub alike), independent of whether that format defines named asset keys. |
| `[tool.rheo]` | `min_version` | `str`, semver | A floor: if this build's own version is below it, `check_package_min_versions` fails the build, naming every offending import in one error (`crates/core/src/plugins/typst_manifest.rs:199-219`). Runs for every scanned `@`-import **unconditionally** — unlike asset/marrow auto-detection, it is *not* gated by the package auto-detect opt-out (`crates/core/src/build.rs:920-927`, called from both the full-build and dev-server-preview paths). Absent or unparseable → no floor, not an error. |

Any other key under `[tool.rheo.<format>]` is preserved verbatim in `extra`
(the whole table, `crates/core/src/plugins/typst_manifest.rs:128`) but nothing
in core reads it today — it rides along for a plugin that might one day
declare a matching `AssetConfig` name.

**Could not verify: the "`[tool.rheo]` must come last" ordering claim.**
`rheo-packages/feeds/0.1.0/typst.toml` carries a comment asserting
`[tool.rheo]` (holding `min_version`) must be the last table in the file, or a
later `[tool.rheo.<format>]` subtable would "capture `min_version` into the
subtable instead." Testing both orderings directly against the exact `toml`
crate version this workspace pins (`toml = "1.1"`, resolved to `0.8.23`/
`1.1.2+spec-1.1.0` in `Cargo.lock`) — `[tool.rheo]` before `[tool.rheo.html]`
and after — produced an *identical* parsed tree either way, and
`typst_manifest.rs`'s own `min_version_alongside_format_subtable` test only
exercises the tool-rheo-first order. I could not reproduce an order-dependent
bug, so this document does not assert that ordering rule as part of the
contract; flagging it here rather than documenting an unverified claim.

## Package-shipped marrow

A package contributes to the bundle root exactly the way a project does — by
shipping a marrow file whose text is inlined verbatim, chosen by which
filename it ships (`crates/core/src/plugins/typst_manifest.rs:227-263`):

| File | Position | Constant |
| --- | --- | --- |
| `.marrow.typ` | epilogue — spliced after every `#document(...)` | `MARROW_FILE` (`crates/core/src/util/constants.rs:10`) |
| `.marrow-prologue.typ` | prologue — spliced before every `#document(...)`, so a `#show`/`#set` rule in it reaches pre-existing vertebrae | `MARROW_PROLOGUE_FILE` (`crates/core/src/util/constants.rs:16`) |

A package may ship either or both (`crates/core/src/plugins/typst_manifest.rs`
tests: `package_marrow_prologue_source_reads_sibling_file`,
`package_may_ship_both_marrow_positions`). Within each position, **packages
contribute first in import order, then the project's own marrow**, so a
project's marrow can build on what a package registered
(`crates/core/src/build.rs:289-296`).

A project has no per-position filename choice — it always writes
`.marrow.typ` (or whatever `rheo.toml`'s `marrow` key renames it to,
`crates/core/src/config/mod.rs:192-195`) and opts into the prologue position
with `rheo.toml`'s `marrow_prologue = true` (default `false`, i.e. epilogue —
today's byte-identical-on-upgrade behaviour;
`crates/core/src/config/mod.rs:197-203,326-329`). Paths inside any marrow file
resolve against the *project* root, not the package's own directory — a
package's marrow must reach its own code through its package spec
(`@ns/name:version`), never a relative import.

**Gap, not yet built:** nothing here documents the *relative* ordering of two
packages' own prologue/epilogue contributions against each other beyond
"import order" — a finer-grained marrow-position API (tracked as `rheo-ap3`)
would add that clause here if it lands; noted as a gap rather than waited on.

## Label / anchor semantics

Fully covered in `CLAUDE.md`'s `## Cross-file references` — canonical
`<handle>` labels, the `<handle.typ>` escape form, the canonical-skip rule,
and the escape-collision hard error. Verified unchanged against
`crates/core/src/reticulate/spine.rs` (canonical/escape assignment around
line 910; escape-collision error around line 923).

## Reserved `rheo-meta:` label namespace

`RESERVED_META_LABEL_PREFIX = "rheo-meta:"`
(`crates/core/src/util/constants.rs:31`). An authored label starting with this
prefix is a hard build error naming the offending file and label
(`crates/core/src/reticulate/spine.rs:879-891`) — unconditionally, unlike the
canonical-label collision rule above (which silently skips injection instead
of erroring).

## Not part of this contract, on purpose

- `@rheo/feeds`'s and `@rheo/rookery`'s own package-level API — see their own
  repos.

## Asset precedence

A package's declared `css_stylesheet`/`js_scripts` are **additive**: they never
suppress the project's own conventional `style.css`/`index.js` at the project
root. Only rheo's embedded fallback stylesheet (`rheo-default.css`) is
suppressed, and only when the project supplies no CSS of its own while a
package does. `AssetResolver::resolve`
(`crates/core/src/assets/mod.rs`) decides the project default's emptiness from
user-declared pairs alone, so a package dependency — an additive third scope,
outside the CLI > `rheo.toml` > defaults chain — cannot answer the question of
whether the project overrode its own default.
