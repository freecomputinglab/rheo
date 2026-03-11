# Spike: Cross-Document Label Linking in Typst Bundles

**Issue:** rheo-5tg
**Date:** 2026-03-11
**Status:** Complete

## Goal

Verify that Typst's native `#link(<label>)` syntax works across documents in a bundle, and document the HTML fragment link format.

---

## Verdict: YES — cross-document label linking works

Typst's test suite includes explicit passing tests for this in `tests/suite/model/link.typ` under the `bundle` target:
- `link-bundle-to-doc` — links across HTML and PDF bundle documents
- `link-bundle-relative` — links across documents at different path depths
- `link-bundle-label-disambiguation` — ID generation for duplicate labels across documents

---

## How It Works

### Compilation and Resolution

1. The bundle is compiled to a `Bundle` struct containing all documents and assets.
2. A `BundleIntrospector` spans all documents — labels in any document are resolvable from any other.
3. After compilation, `create_link_anchors()` assigns HTML `id` attributes and PDF named destinations to all elements that are linked-to from anywhere in the bundle.
4. During export, `LateLinkResolver` computes relative paths between source and destination documents.

### Link Format in HTML Output

For `#link(<label>)` in HTML documents within a bundle:

| Link target | href format |
|-------------|-------------|
| Same document | `#anchor-name` |
| Sibling document (`other.html`) | `other.html#anchor-name` |
| Child document (`sub/page.html`) | `sub/page.html#anchor-name` |
| Parent document (`../index.html`) | `../index.html#anchor-name` |
| Link to document itself (no element) | `other.html` (no fragment) |
| Link to own document | `#` |

Paths are always **relative** from the linking document. Typst computes `to.relative_from(&parent_of_from)`.

### Link Format in PDF Output

PDF documents in bundles use **named destinations** (not fragment anchors). A label `<my-section>` in a PDF becomes a named destination. Cross-bundle links to that PDF resolve to the named destination rather than a page number.

### Anchor Name Generation

Labels → HTML anchor IDs follow a disambiguation scheme:
- Unique label `<foo>` in a document → `id="foo"`
- Duplicate `<foo>` in same document → `id="foo"`, `id="foo-2"`, `id="foo-3"`, ...
- Same label `<foo>` in different documents → each gets `id="foo"` independently (local to their document)
- Unlabelled elements that are link targets → `id="loc-1"`, `id="loc-2"`, ...

### Minimal Working Example

```typ
// bundle-entry.typ — compile with Feature::Bundle + Feature::Html enabled

#document("index.html")[
  = Home
  Go to #link(<my-section>)[the section in page2].
]

#document("page2.html")[
  = Introduction
  == My Section <my-section>
  This is the section.
  Go #link(<index>)[back to home].
] <index-doc>  // labelling the document itself
```

This produces two HTML files. In `index.html`, the link resolves to:
```html
<a href="page2.html#my-section">the section in page2</a>
```

In `page2.html`, the "back to home" link resolves to:
```html
<a href="index.html">back to home</a>
```
(no fragment since `<index>` is the label on the `#document()` element itself, which links to the document root)

### Labels Do Not Need to Be Exported

Labels do not need any explicit "export" or declaration. Any label attached to a locatable element (headings, paragraphs, figures, etc.) is automatically discoverable across the bundle via the `BundleIntrospector`.

---

## Implications for Rheo

### Drop custom link transformer for bundle output

Rheo's current `LinkTransformer` (`crates/core/src/reticulate/transformer.rs`) rewrites `#link("./file.typ")[...]` to format-specific targets. For bundle output, this custom transformer is **not needed** — Typst handles cross-document links natively via `#link(<label>)`.

### User-facing pattern

The idiomatic pattern for cross-document navigation in Rheo bundle output:

1. Label destination elements: `= My Heading <my-heading>`
2. Link to them from anywhere: `#link(<my-heading>)[Go there]`
3. Typst resolves paths and anchors automatically at bundle export time

This is a strict improvement over Rheo's current `#link("./file.typ")[...]` syntax, which is Rheo-specific and breaks if file paths change.

### Limitations

- **PNG** documents in bundles do not support named destinations — links to elements within PNG docs are not supported.
- Linking to metadata that is not within any document (top-level in bundle) is not supported and will error.
- Cross-bundle links require the label to be attached to a locatable element. `#metadata(...)` elements are not linkable targets.

---

## Reference: Relevant Typst Source Files

| File | Purpose |
|------|---------|
| `crates/typst-bundle/src/lib.rs` | `Bundle`, `BundleFile`, `BundleDocument`, `bundle()` |
| `crates/typst-bundle/src/export.rs` | `export()`, `BundleOptions`, `VirtualFs` |
| `crates/typst-bundle/src/introspect.rs` | `BundleIntrospector` — cross-document query |
| `crates/typst-bundle/src/link.rs` | `create_link_anchors()` |
| `crates/typst-library/src/model/link.rs` | `LateLinkResolver`, `ResolvedLink`, `into_uri()` |
| `tests/suite/model/link.typ` | Test cases: `link-bundle-to-doc`, `link-bundle-relative`, etc. |
