# Spine inference

## Document metadata is resolved by Typst, not scanned pre-compile

Per-vertebra `#set document(...)` values (`title`, `author`, `description`,
`keywords`, `date`) are no longer harvested by statically parsing each
vertebra's own source text before compilation. Instead:

- **Typst-side reads.** Every vertebra compiled under a per-page layout
  (HTML, EPUB) gets a labelled `#metadata(...)` "beacon" appended after its
  own body (`crates/core/src/util/typst_source.rs`'s `MetadataBeacon`),
  queryable from any other vertebra — or from a `.marrow.typ` at the bundle
  root — via `rheo-metadata(handle)` / `rheo-metadata-all()`
  (`rheo-context()` exposes the same thing as its `metadata-of` field). These
  read `document.title` etc. live, off the real style chain Typst resolves
  during realization, so most authoring forms work: a title set via an
  imported `#show:` template, from a non-literal expression, or by multiple
  `#set document(...)` rules all resolve correctly now. A title set inside a
  bounded `#{ }`/`#[ ]` code block does NOT — see below.
- **Rust-side reads.** EPUB (and other Rust-side consumers) read the
  compiled bundle's own resolved `DocumentInfo` directly
  (`crates/core/src/plugins/document_meta.rs`'s `DocumentMeta`, wrapping
  `doc.info()`) — the same authoritative, fully-resolved metadata Typst
  itself used to build `<title>`/PDF Info `/Title`, not a re-parse of source
  text.

This closes every gap the old pre-compile scan had (it used to miss a title
set via an imported template, a show rule, a code block, or a non-literal
expression, and only ever read the first of several `#set document(...)`
rules). What's left, honestly:

### Combined PDF has no per-vertebra metadata

PDF's default layout (`SpineLayout::SingleCombined`, declared in
`crates/pdf/src/lib.rs`) puts every vertebra in ONE `#document(...)` block,
so all their `#set document(...)` rules pile into a single shared style
chain — there is no well-defined "this vertebra's own metadata" inside it (a
vertebra with no rule of its own would otherwise silently inherit whatever
the previous vertebra set). Beacons are therefore only emitted for per-page
(`OnePerVertebra`) layouts (the gate lives at
`crates/core/src/reticulate/spine.rs:908`). Under combined PDF,
`rheo-metadata(handle)` returns `(:)` and `rheo-metadata-all()` returns one
empty-metadata entry per vertebra — there is no per-vertebra metadata read
available there, by design, not a bug to work around.

### A title set inside a bounded code block is invisible to the beacon

`#set document(...)` wrapped in a bounded `#{ }` or `#[ ]` block sets the
vertebra's own compiled `<title>`/DocumentInfo correctly — Typst's
document-info collection is unscoped, so the final `<title>`/PDF `/Title` is
unaffected. But the metadata beacon reads `document.title` via `#context`,
appended once per vertebra after its own body — and a `#context` read
respects ordinary Typst block scoping: it cannot see a `set` whose bounded
block already closed earlier in the same file. So `rheo-metadata(handle)`,
`metadata-of`, and any `@handle` anchor referencing that vertebra silently
get rheo's path-derived fallback title instead of the real one.

`#show:` templates are unaffected by this — they have no closing brace of
their own; a `#show: template` applies to everything through the end of the
enclosing block, which is where the beacon lives too. Only a genuinely
*bounded* block (one that closes before the vertebra's own end) triggers this.
Give a title at the vertebra's top level, or via a `#show:` template, if it
needs to be visible to another vertebra's metadata read or to a handle
anchor. Tracked for a future fix: a gated second compile pass that resolves
this from Rust's already-correct `DocumentInfo` instead of a live `#context`
read (`feat-metadata-two-pass`).

### Typst-side metadata reads require `#context`

`rheo-metadata(handle)`, `rheo-metadata-all()`, and
`(rheo-context().metadata-of)(handle)` all call `query(...)` internally,
which only resolves inside a `#context` scope. This is unlike the rest of
`rheo-context()` — `handle`, `spine`, `spine-flat`, `target`, `ext` — which
reads straight off `sys.inputs` and needs no `#context` at all. Metadata
reads are the one part of `rheo-context()` that needs it, because they're
backed by Typst's introspection machinery (a bundle-wide `query()`), not by
a plain data dict — the price of staying at a single bundle compile rather
than compiling every vertebra a second time just to read its own metadata.

### `datetime.today()` now resolves to a real, build-varying date

The old static scan deliberately rejected `#set document(date: datetime.today())`
— it couldn't tell it apart from a literal `datetime(...)` by reading source
text alone. The new mechanism reads the *resolved* value instead, so it
can't tell them apart either: `datetime.today()` now resolves to a real date
that changes on every build. If a vertebra's date is syndicated downstream
(e.g. by the `@rheo/feeds` Typst package), one using `datetime.today()`
will churn its published/updated timestamp on every rebuild. Use a literal
`datetime(year:, month:, day:)` for anything that needs a stable timestamp.

### Titles read via the Typst-side beacon may be rich content, not plain text

`rheo-metadata(handle).title` (and `.description`) come back as real Typst
**content** — whatever `document.title` resolves to, markup and all — not a
flattened string. This is new and deliberate: unlike the old scan (which had
to flatten everything to plain text, since it worked from raw source text),
the beacon reads the real, live value, so a nav or index built from it can
render a formatted title. `author`/`keywords` are unaffected — Typst's
`document` element types those as plain strings/arrays already (`author` is
always an array, even for a single author), and `date` is a real `datetime`
value (compare a date-only `datetime(year:, month:, day:)` against another
date-only value, not a zero-padded full `datetime`, since they're distinct
Typst datetime kinds).

Rust-side sinks are different: `DocumentInfo`'s own `title`/`description`
(what EPUB and PDF Info `/Title` read via `DocumentMeta`) are plain-text
flattened by Typst itself (`content.plain_text()`) before rheo ever sees
them — those stay plain strings, as before.

### `rheo-context().spine`/`spine-flat` titles are path-derived only

Neither the spine tree nor the flat vertebra list carries a `metadata` key
any more (removed along with the AST scan). Their `title` field was not
switched over to the new mechanism either: it is purely path-derived (the
file/directory name, prettified via `DocumentTitle::to_readable_name`) for
every vertebra, whether or not it authors a `#set document(title: ...)` —
literal or otherwise. If a vertebra's real authored title is needed from
Typst, read it via `(rheo-context().metadata-of)(handle)` (or
`rheo-metadata(handle)` from marrow) instead of
`rheo-context().spine-flat[...].title`. `@handle` cross-reference display
text already does this — it resolves live via the metadata beacon, with the
path-derived title as its fallback only when no beacon exists (combined
PDF).

### The reserved `rheo-meta:` label prefix

Beacons are labelled `<rheo-meta:HANDLE>`. An authored label starting with
that prefix is a hard build error naming the file and label — there is no
silent fallback, unlike the canonical `<handle>` collision rule (which
silently skips injecting the canonical label when a user-authored label
already claims it).
