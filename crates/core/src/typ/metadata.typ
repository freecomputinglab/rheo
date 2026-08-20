// Metadata-query helpers, served by RheoWorld at the fixed project-root path
// `METADATA_MODULE_PATH` (crates/core/src/util/constants.rs) and pulled into
// scope via the `#import` statements TypstStmt's MetadataHelper,
// MetadataAllHelper, and HandleTitleHelper variants render
// (crates/core/src/util/typst_source.rs). None of the three baked per-call
// values a vertebra needs — the querying handle, another vertebra's own
// handle, a fallback title — live here; only the logic that is the same for
// every vertebra does.

// Reads a vertebra's metadata beacon (a `#metadata(..) <rheo-meta:handle>`
// element, see TypstStmt::MetadataBeacon), returning its published fields as
// a dict — or `(:)` if no beacon with that handle was found (e.g. under a
// SingleCombined layout, which emits no beacons at all). Requires #context at
// the call site, since it calls query(). Wrapped per vertebra as
// `#let rheo-metadata(handle) = rheo-metadata-impl(handle)` so authors keep
// calling the short name; `rheo-metadata-all` below calls it directly.
#let rheo-metadata-impl(handle) = {
  let found = query(label("rheo-meta:" + handle))
  if found.len() == 0 { return (:) }
  let out = (:)
  for (k, v) in found.first().value {
    if k == "handle" or v == none or v == auto { continue }
    if type(v) == array and v.len() == 0 { continue }
    out.insert(k, v)
  }
  out
}

// The bundle-root ("marrow scope") companion to rheo-metadata, for the common
// case of "every vertebra's metadata at once" (a feed, a sitemap, a search
// index). Never imported into a vertebra's own prelude — a vertebra itself
// has no need to enumerate every vertebra's metadata.
#let rheo-metadata-all() = sys.inputs.rheo-context.spine-flat.map(e => (handle: e.handle, path: e.path, ..rheo-metadata-impl(e.handle)))

// Live title lookup for a cross-vertebra handle anchor: the owning vertebra's
// current document.title via its metadata beacon, or `fallback` when no
// beacon was found (combined PDF layouts, which emit no beacons at all).
#let rheo-handle-title(handle, fallback) = context {
  let m = query(label("rheo-meta:" + handle))
  if m.len() > 0 and m.first().value.title != none { m.first().value.title } else { fallback }
}
