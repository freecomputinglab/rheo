use crate::util::typst_source::TypstStmt;
use std::fmt;

/// A handle anchor emitted into a `BundleDocument` body so that `@label` cross-references
/// resolve across vertebrae during bundle compilation.
pub struct BundleAnchor {
    pub label: String,
    /// The owning vertebra's canonical handle — used to query that vertebra's
    /// `rheo-meta:<handle>` beacon at render time. Shared by every anchor
    /// belonging to the same vertebra (the canonical-handle anchor and any
    /// `<handle.typ>` escape-alias anchors alike), regardless of `label`.
    pub handle: String,
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
    pub handle: String,
    /// Vertebra segments, in order. Each segment's anchors are emitted immediately
    /// before its include so cross-references resolve to the right location.
    pub segments: Vec<BundleSegment>,
}

/// The synthesized Typst source passed to `RheoWorld::compile_bundle`.
///
/// Constructed from a `VirtualSpine`; serialized to a `String` via `Display`.
pub struct BundleSource {
    pub documents: Vec<BundleDocument>,
    /// Raw Typst emitted at bundle root BEFORE all documents — opt-in, since a
    /// `#show`/`#set` rule here is global-by-default and reaches every
    /// pre-existing vertebra (Typst introspection is bundle-wide, not
    /// sequential).
    pub marrow_prologue: Vec<TypstStmt>,
    /// Raw Typst emitted at bundle root AFTER all documents, outside any
    /// `#document` block, so it may itself mint `document()` and `asset()`
    /// elements. Typst has no nested bundles — those elements are legal only as
    /// root children — so this is the one place author or package code can add
    /// output files that no vertebra backs. This is the default position: a
    /// rule here is naturally scoped to marrow's own output only.
    pub marrow: Vec<TypstStmt>,
}

impl fmt::Display for BundleSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for stmt in &self.marrow_prologue {
            writeln!(f, "{stmt}")?;
        }
        for doc in &self.documents {
            // Each document renders itself as a systematic `TypstStmt::Document`;
            // a blank line separates successive documents.
            writeln!(f, "{}", doc.to_stmt())?;
            writeln!(f)?;
        }
        for stmt in &self.marrow {
            writeln!(f, "{stmt}")?;
        }
        Ok(())
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
