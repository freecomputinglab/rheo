use crate::util::typst_literal::TypstLiteral;
use crate::util::typst_source::TypstStmt;
use std::fmt;

/// Typst `state` key carrying the current page's handle, published per-document
/// so `typ/rheo.typ`'s cross-vertebra link rule can read it (see the rule and
/// [`BundleDocument::to_stmt`]).
const HANDLE_STATE_KEY: &str = "rheo-handle";

/// A handle anchor emitted into a `BundleDocument` body so that `@label` cross-references
/// resolve across vertebrae during bundle compilation.
pub struct BundleAnchor {
    pub label: String,
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
}

impl fmt::Display for BundleSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for doc in &self.documents {
            // Each document renders itself as a systematic `TypstStmt::Document`;
            // a blank line separates successive documents.
            writeln!(f, "{}", doc.to_stmt())?;
            writeln!(f)?;
        }
        Ok(())
    }
}

impl BundleDocument {
    /// Render this document as a [`TypstStmt::Document`]: a per-page handle
    /// `state` update, then each segment's handle anchors followed by its
    /// `#include`, in order.
    fn to_stmt(&self) -> TypstStmt {
        let mut body = vec![TypstStmt::StateUpdate {
            key: HANDLE_STATE_KEY.to_string(),
            value: TypstLiteral::str(self.handle.as_str()),
        }];
        for segment in &self.segments {
            for anchor in &segment.anchors {
                body.push(TypstStmt::HandleAnchor {
                    label: anchor.label.clone(),
                    title: anchor.title.clone(),
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
