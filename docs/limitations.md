# Spine inference

## `#set document(...)` metadata is harvested by a pre-compile AST scan

rheo reads each vertebra's `title`, `date`, and generic `metadata` (every named
argument of `#set document(...)`) by statically parsing the vertebra's **own
source text** before compilation — `DocumentMetadata`/`DocumentDate` in
`crates/core/src/parser/`. This happens pre-compile by necessity: the spine
(handles, titles, `@handle` display text, nav order, Atom feed) is an *input* to
the bundle source rheo generates, so it must exist before Typst runs.

Because it is a static scan of literal syntax — not evaluation — it only sees a
`#set document(...)` rule written at the **top (file-scope) level of the vertebra
file itself**. Typst, however, applies document set rules no matter where they are
evaluated: set rules propagate up to the document element regardless of nesting.
Verified with typst 0.15.0 — all of these produce the given compiled document
title, but only the last is visible to rheo's harvest:

| How the title is set | Compiled `document.title` | Harvested by rheo? |
| --- | --- | --- |
| In an imported module fn, applied via `#show: book` | `FromModule` | **No** |
| Inside `#show: doc => { set document(...); doc }` | `FromShow` | **No** |
| Inside a `#let` helper's body (even if applied) | — | **No** |
| In a code block — `#{ set document(...) }` | `FromCodeBlock` | **No** |
| Literal top-level — `#set document(title: "…")` | `Literal` | **Yes** |

Harvesting is gated to file scope (the same rule `rheo-*` variables use): a set
rule nested in a closure, code block, or `#let` binding is skipped, because such a
rule may apply only where a helper is *invoked* (a different file), not where it is
defined. The cost is that a top-level `#{ set document(...) }` code block — an
unusual way to write what `#set document(...)` expresses directly — is also missed.

### What this means

A vertebra that outsources its metadata to a shared template — e.g.

```typst
#import "template.typ": book
#show: book   // book(doc) internally derives + calls `set document(title: …)`
```

compiles to output with the correct title, but rheo's spine/feed will fall back
to the filename-cased title (and miss any template-set `keywords`/`author`),
because the `#set document(...)` lives in the module, not the vertebra's own
source. The reliable rule today: **put a literal `#set document(...)` in each
vertebra's own source** if you want its metadata reflected in the spine.

### Narrower gaps in the same scan

- **Only the first `#set document(...)` rule** is read. Typst accumulates
  multiple rules; rheo takes the first in source order, so keys set by a later
  rule are missed.
- **Non-literal argument values are dropped**, not errored: `title: my-var`
  (an identifier), `title: "a" + "b"`, `title: upper("x")`, `keywords: ..spread`,
  and `if`/`context` expressions all yield no harvested value for that field.

### Harvested titles/values are plain text, not content

Every harvested value is a plain string, not Typst content — the spine `title`
and each `metadata` value feed plain-text sinks (nav labels, `@handle` display
text, the Atom/RSS `<title>`, PDF Info `/Title`), none of which render markup.
When a bracket value carries formatting, only its `Text`/`Space`/`SmartQuote`
leaves survive; everything else is flattened:

- **Inline formatting markers are stripped**, keeping the words:
  `[_italic_ text]` → `italic text`, `[*bold* text]` → `bold text`,
  `[Good news — #emph[Severance]]` → `Good news — Severance`.
- **Spans that aren't textual leaves drop entirely**, not just their markers:
  `[Chapter $x^2$]` → `Chapter` (math gone), `[See #image("x.png")]` → `See`,
  inline raw `` [`code`] `` contributes nothing.

There is no way to carry a *formatted* title through the spine; if a vertebra
needs to display rich text, author it in the body/heading as content, separate
from the `#set document(title: …)` value.

rheo does not fail on this, but it **warns**: whenever a bracket title loses
anything in the flattening (styling or non-text content), the build prints a
warning naming the file and showing both the original and the stripped title, so
the loss is never silent.

### The proper fix (not yet done)

Reading `document.title` from the *compiled* output (Typst introspection) would
close all of the above, but requires a pre-pass that compiles each vertebra
standalone purely to read its resolved metadata, then builds the spine, then
compiles the bundle — a two-pass design with real cost.
