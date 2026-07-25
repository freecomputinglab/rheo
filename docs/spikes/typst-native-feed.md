# Spike: Typst-native Atom feed generation (rheo-z23)

**Verdict: PARTIAL — mechanically possible, but a net regression. Recommendation: do NOT replace the Rust feed.**

Timeboxed investigation into replacing rheo's Rust-side Atom feed
(`crates/html/src/feed.rs`) with a Typst-native feed derived from
`rheo-context().spine-flat`, mirroring the `exemel`/`atom.typ` approach used by
[wensimehrp.github.io](https://github.com/wensimehrp/wensimehrp.github.io).

## Q2 — the file-write mechanism (the interesting result)

Typst can, in fact, emit an arbitrary side file into rheo's build **without**
`typst query` or a second compile. The `bundle` target — which rheo already
drives (`world.compile_bundle()` → `compile::export_bundle` →
`typst_bundle::VirtualFs`) — accepts a top-level `asset()` element:

- `typst-bundle-0.15.x/src/lib.rs`: `BundleFile::Asset(Bytes)` — *"Raw file
  data, resulting from an `asset` element."* `collect()` accepts `AssetElem` at
  the bundle top level alongside `DocumentElem`, and `bundle_impl()` writes each
  asset's bytes into the `VirtualFs` at its `VirtualPath`.
- This is exactly what the reference's `#asset("atom.xml", …)` relies on.

rheo synthesizes the bundle main itself — `reticulate/bundle_source.rs`
(`BundleSource`, a list of `#document(…)[…]` blocks) via `VirtualSpine::mould()`
(`reticulate/mould.rs`). So rheo could append one top-level
`#context asset("feed.xml", <atom-bytes>)` statement to that synthesized main and
the bytes would land in the `VirtualFs` next to the HTML pages, flushed to
`build/html/feed.xml` by the existing bundle writer (`build.rs:323`). **The
mechanism is real and fits the current architecture** — no new subprocess, no
new compile pass.

## Q1 — feasibility of the data

Sufficient. `rheo-context().spine-flat` already carries per-entry `handle`,
`path`, `title`, and `metadata`. Since **rheo-wdq landed**, `metadata.date` is
now a real Typst `datetime` (see `crates/core/CLAUDE.md` rheo-context section) —
so a Typst feed reads the true document date instead of the Rust path's
chrono-reparse. Feed `<id>`/`<link>`/`<title>`/`<updated>`/`<author>` are all
derivable Typst-side. XML encoding is a package concern (`exemel`, or hand-rolled
string building with `&`/`<`/`>` escaping).

## Q3 — per-entry `<content>` rendered HTML (the blocker)

**This is why it's only PARTIAL.** The current Rust feed emits full
`<content type="html">` for every entry — the page's rendered `<main>` / body,
extracted from already-compiled HTML (`feed.rs:141-145` →
`util::html::feed_content_inner_html`, resolution `first <main>` → `.rheo-feed-content`
→ whole `<body>`).

A Typst-native feed **cannot reproduce this**. In the bundle each page is a
separate `document()`; the top-level `asset()` statement sees spine *metadata*
only, never another document's *rendered HTML string*. Typst has no primitive to
serialize a sibling document's HTML body into an embeddable string. The reference
confirms the ceiling: `wensimehrp/atom.typ` sets **`content: none`** and ships
only a summary/description — it does not include rendered body HTML at all.

So going Typst-native means regressing `<content>` from full rendered HTML to
summary-only (or dropping it). That is the feature the Rust path is *uniquely*
well-placed to provide: `compile()` already holds every page's compiled HTML in
`outputs: &[CastVertebra]` (`crates/html/src/lib.rs:116`), so extraction is free
and lossless there.

## Q4 — what would move vs stay Rust

| Concern | Today | Typst-native |
| --- | --- | --- |
| Feed skeleton (id/title/updated/self link/author) | Rust `AtomFeed` | Typst template ✓ |
| Per-entry title/updated/link/id | Rust, from `CastVertebra` | Typst, from `spine-flat` + `metadata.date` ✓ |
| Per-entry `<content>` rendered HTML | Rust, extracted from compiled body | **regresses to summary-only** ✗ |
| XML serialization | `atom_syndication` crate | `exemel` package (new dep) |
| `feed_base_url` / `feed_author` / `feed_title` (`[html]` TOML) | Rust config | must be plumbed into Typst via `sys.inputs.rheo-context` (new surface) |
| `rheo-feed-title` / `-updated` / `-exclude` overrides | harvested Rust-side (`output.vars`) | spine metadata carries **no** `rheo-*` vars today → would need new plumbing, or read `#set document` only (partial) |
| Autodiscovery `<link>` in every page `<head>` | Rust DOM post-process (`html.rs:inject_feed_link`, `lib.rs:157`) | stays Rust regardless |

## Code delta estimate

- **Removed:** most of `feed.rs` (~339 lines incl. tests; ~150 non-test) and the
  `atom_syndication` dep.
- **Added:** a feed Typst template (`@rheo/feed` or injected), a new `exemel`
  dependency, `sys.inputs` plumbing for feed config + `rheo-feed-*` var
  surfacing into spine metadata, and Rust glue to inject the top-level
  `asset("feed.xml", …)` into `BundleSource`.
- Autodiscovery-link injection stays Rust either way. Net LOC is roughly a wash,
  and it **adds** cross-cutting surface (a package dep + `sys.inputs` config) to
  **lose** the `<content>` feature.

## Recommendation

**Keep the Rust feed.** The one genuinely novel finding — that `asset()` on the
bundle target gives Typst a real file-write hook inside rheo — does not overcome
the `<content>` regression, and the reference project itself concedes that point
(`content: none`). The Rust path sits exactly where all rendered HTML already
lives, which is its decisive advantage.

Revisit only if a future goal is **author-customizable feed templates**; then a
summary-only Typst feed could ship as an opt-in alongside (not replacing) the
Rust one, using the `asset()` mechanism documented above.

## Throwaway PoC (illustrative — not wired)

Minimal top-level statement rheo could append to the synthesized bundle main to
prove the mechanism (summary-only, no `<content>`):

```typst
#context {
  let entries = rheo-context().spine-flat.map(e => {
    let updated = e.metadata.at("date", default: none)
    "<entry><title>" + e.title + "</title>" +
    "<link rel=\"alternate\" href=\"" + base + "/" + e.path + "\"/>" +
    "<id>" + base + "/" + e.path + "</id>" +
    (if updated != none { "<updated>" + updated.display("[year]-[month]-[day]T00:00:00Z") + "</updated>" } else { "" }) +
    "</entry>"
  }).join()
  asset("feed.xml",
    bytes("<?xml version=\"1.0\"?><feed xmlns=\"http://www.w3.org/2005/Atom\">" + entries + "</feed>"))
}
```

(`base` would come from `sys.inputs.rheo-context` config plumbing that does not
exist yet. Escaping and RFC-3339 formatting are elided for brevity.)
