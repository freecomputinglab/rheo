// Metadata-query helpers, served by RheoWorld at the fixed project-root path
// `METADATA_MODULE_PATH` (crates/core/src/util/constants.rs) and pulled into
// scope via the `#import` statements TypstStmt's MetadataHelper,
// MetadataAllHelper, and HandleTitleHelper variants render
// (crates/core/src/util/typst_source.rs). None of the three baked per-call
// values a vertebra needs — the querying handle, another vertebra's own
// handle, a fallback title — live here; only the logic that is the same for
// every vertebra does.

// A vertebra's title, harvested Rust-side from the compiled bundle's own
// resolved DocumentInfo and fed back in as sys.inputs.rheo-context's
// title-overrides array (VirtualSpine::global_context) -- present only on the
// gated second compile pass (Build::compile_bundle_once), empty on the
// ordinary single pass. An array of (handle, title) dicts rather than a dict
// keyed by handle, since a handle like "chapters:intro" is not a valid Typst
// identifier. `.at(_, default:)` throughout since callers that hand-build a
// sys.inputs (a package mock, or a test predating this key) may omit
// rheo-context, or rheo-context, entirely. Returns none when `handle` has no
// override.
#let rheo-title-override(handle) = {
  let overrides = sys.inputs.at("rheo-context", default: (:)).at("title-overrides", default: ())
  let found = overrides.filter(o => o.handle == handle)
  if found.len() > 0 { found.first().title } else { none }
}

// Reads a vertebra's metadata beacon (a `#metadata(..) <rheo-meta:handle>`
// element, see TypstStmt::MetadataBeacon), returning its published fields as
// a dict — or `(:)` if no beacon with that handle was found (e.g. under a
// SingleCombined layout, which emits no beacons at all). Requires #context at
// the call site, since it calls query(). Wrapped per vertebra as
// `#let rheo-metadata(handle) = rheo-metadata-impl(handle)` so authors keep
// calling the short name; `rheo-metadata-all` below calls it directly.
//
// A title set inside a bounded code block is invisible to the beacon's own
// `#context` read (see docs/limitations.md) -- the beacon still reports SOME
// title in that case (rheo's path-derived fallback, ambient from the
// `#document(...)` wrapper's own `title:` argument), just not the real one,
// so Rust decides per-handle whether `rheo-title-override` applies (a
// beacon-vs-DocumentInfo mismatch it alone can detect) rather than Typst
// guessing from "title" being absent here.
#let rheo-metadata-impl(handle) = {
  let found = query(label("rheo-meta:" + handle))
  let out = (:)
  if found.len() > 0 {
    for (k, v) in found.first().value {
      if k == "handle" or v == none or v == auto { continue }
      if type(v) == array and v.len() == 0 { continue }
      out.insert(k, v)
    }
  }
  let title-override = rheo-title-override(handle)
  if title-override != none { out.insert("title", title-override) }
  out
}

// The bundle-root ("marrow scope") companion to rheo-metadata, for the common
// case of "every vertebra's metadata at once" (a feed, a sitemap, a search
// index). Never imported into a vertebra's own prelude — a vertebra itself
// has no need to enumerate every vertebra's metadata.
#let rheo-metadata-all() = sys.inputs.rheo-context.spine-flat.map(e => (handle: e.handle, path: e.path, ..rheo-metadata-impl(e.handle)))

// Live title lookup for a cross-vertebra handle anchor. rheo-metadata-impl
// already drops a `none` title and already lets an override win, so the
// fallback is just its absent-key default -- reached when there is no beacon at
// all (combined PDF layouts emit none).
#let rheo-handle-title(handle, fallback) = context {
  rheo-metadata-impl(handle).at("title", default: fallback)
}
