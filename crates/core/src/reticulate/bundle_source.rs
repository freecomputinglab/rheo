use crate::reticulate::handle::Handle;
use crate::synth::source_map::{AuthoredFile, SourceMap};
use crate::synth::typst_source::TypstStmt;
use std::fmt;

/// One marrow contribution: the Typst text spliced into the bundle root, and
/// the display path of the file it was read from, so a diagnostic inside it
/// points at that file rather than at the synthesized main.
#[derive(Debug, Clone)]
pub struct MarrowSource {
    pub origin: String,
    pub text: String,
}

/// A handle anchor emitted into a `BundleDocument` body so that `@label` cross-references
/// resolve across vertebrae during bundle compilation.
pub struct BundleAnchor {
    pub label: String,
    /// The owning vertebra's canonical handle — used to query that vertebra's
    /// `rheo-meta:<handle>` beacon at render time. Shared by every anchor
    /// belonging to the same vertebra (the canonical-handle anchor and any
    /// `<handle.typ>` escape-alias anchors alike), regardless of `label`.
    pub handle: Handle,
    /// The path-derived fallback title, used only when no beacon is found for
    /// `handle` (combined PDF layouts, which emit no beacons at all).
    pub title: String,
}

/// One vertebra within a `BundleDocument`: its handle anchors followed by its include.
pub struct BundleSegment {
    /// Handle anchors (`#figure` elements) emitted before this segment's include.
    pub anchors: Vec<BundleAnchor>,
    /// `#include` path for this vertebra.
    pub include: String,
}

/// One Typst `#document(…)[…]` block within a bundle compile.
pub struct BundleDocument {
    pub output_path: String,
    pub format: String,
    pub title: String,
    /// The `:`-joined handle of this document's page, published per-document via
    /// Typst `state` so `typ/rheo.typ`'s cross-vertebra link rule can read the
    /// current page's handle (empty for the combined PDF, which has no rule).
    pub handle: Handle,
    /// Vertebra segments, in order. Each segment's anchors are emitted immediately
    /// before its include so cross-references resolve to the right location.
    pub segments: Vec<BundleSegment>,
}

/// The synthesized Typst source passed to `RheoWorld::compile_bundle`.
///
/// Constructed from a `VirtualSpine`; serialized to a `String` via `Display`.
pub struct BundleSource {
    pub documents: Vec<BundleDocument>,
    /// Marrow emitted at bundle root BEFORE all documents — opt-in, since a
    /// `#show`/`#set` rule here is global-by-default and reaches every
    /// pre-existing vertebra (Typst introspection is bundle-wide, not
    /// sequential).
    pub marrow_prologue: Vec<MarrowSource>,
    /// Marrow emitted at bundle root AFTER all documents, outside any
    /// `#document` block, so it may itself mint `document()` and `asset()`
    /// elements. Typst has no nested bundles — those elements are legal only as
    /// root children — so this is the one place author or package code can add
    /// output files that no vertebra backs. This is the default position: a
    /// rule here is naturally scoped to marrow's own output only.
    pub marrow: Vec<MarrowSource>,
}

impl BundleSource {
    /// Render to Typst source, building the map back to the marrow files
    /// spliced into it. Everything else — the per-document scaffolding rheo
    /// generates — is injected; no source map exists yet for what a document
    /// itself carries (its `#include`d vertebra is mapped separately, by
    /// `SourceInjector::vertebra`).
    pub fn render(&self) -> (String, SourceMap) {
        let mut text = String::new();
        let mut map = SourceMap::default();
        for source in &self.marrow_prologue {
            text.push_str(&source.text);
            text.push('\n');
            map.push_authored(
                AuthoredFile {
                    name: source.origin.clone(),
                    text: source.text.as_str().into(),
                },
                0,
                source.text.len(),
            );
            map.push_injected(1);
        }
        for doc in &self.documents {
            let rendered = doc.to_stmt().to_string();
            text.push_str(&rendered);
            text.push_str("\n\n");
            map.push_injected(rendered.len() + 2);
        }
        for source in &self.marrow {
            text.push_str(&source.text);
            text.push('\n');
            map.push_authored(
                AuthoredFile {
                    name: source.origin.clone(),
                    text: source.text.as_str().into(),
                },
                0,
                source.text.len(),
            );
            map.push_injected(1);
        }
        (text, map)
    }
}

impl fmt::Display for BundleSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render().0)
    }
}

impl BundleDocument {
    /// Render this document as a [`TypstStmt::Document`]: a per-page init hook
    /// (`#rheo-page-init`, which publishes the handle to `state` and resets the
    /// footnote counter for per-page output), the cross-vertebra link rule closed
    /// over this page's handle ([`TypstStmt::LinkRule`]), then each segment's
    /// handle anchors followed by its `#include`, in order.
    ///
    /// The link rule is per-document rather than bundle-global precisely so it can
    /// take the handle as an argument and stay free of `#context` — see
    /// [`TypstStmt::LinkRule`] and `typ/rheo.typ`'s `rheo-link-rule`. It has to
    /// come before every anchor and include, since a `#show` scopes to the rest of
    /// its own block.
    fn to_stmt(&self) -> TypstStmt {
        let mut body = vec![
            TypstStmt::PageInit {
                handle: self.handle.clone(),
            },
            TypstStmt::LinkRule {
                handle: self.handle.clone(),
            },
        ];
        for segment in &self.segments {
            for anchor in &segment.anchors {
                body.push(TypstStmt::HandleAnchor {
                    label: anchor.label.clone(),
                    handle: anchor.handle.clone(),
                    fallback_title: anchor.title.clone(),
                });
            }
            body.push(TypstStmt::Include {
                path: segment.include.clone(),
            });
        }
        TypstStmt::Document {
            output_path: self.output_path.clone(),
            format: self.format.clone(),
            title: self.title.clone(),
            body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_resolves_marrow_offset_and_leaves_document_text_unattributed() {
        let bundle = BundleSource {
            documents: vec![BundleDocument {
                output_path: "a.html".to_string(),
                format: "html".to_string(),
                title: "A".to_string(),
                handle: Handle::default(),
                segments: vec![BundleSegment {
                    anchors: Vec::new(),
                    include: "content/a.typ".to_string(),
                }],
            }],
            marrow_prologue: Vec::new(),
            marrow: vec![MarrowSource {
                origin: "content/.marrow.typ".to_string(),
                text: "#let feed = broken-marrow-fn()".to_string(),
            }],
        };

        let (text, map) = bundle.render();

        let marrow_text = "#let feed = broken-marrow-fn()";
        let offset_in_text = marrow_text.find("broken-marrow-fn").unwrap();
        let offset = text.rfind("broken-marrow-fn").unwrap();
        let (file, range) = map
            .resolve(&(offset..offset + "broken-marrow-fn".len()))
            .expect("resolves");
        assert_eq!(file.name, "content/.marrow.typ");
        assert_eq!(
            range,
            offset_in_text..offset_in_text + "broken-marrow-fn".len()
        );

        // An offset inside the document's own `#document(...)` block is not
        // authored marrow — the vertebra it `#include`s is mapped separately.
        let doc_offset = text.find("#document(").unwrap();
        assert!(map.resolve(&(doc_offset..doc_offset + 1)).is_none());
    }
}
