use crate::util::path::escape_typst_content;
use std::fmt;

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
            let escaped_title = escape_typst_content(&doc.title);
            writeln!(
                f,
                "#document(\"{}\", format: \"{}\", title: [{escaped_title}])[",
                doc.output_path, doc.format
            )?;
            for segment in &doc.segments {
                for anchor in &segment.anchors {
                    let escaped = escape_typst_content(&anchor.title);
                    writeln!(
                        f,
                        "  #figure([{escaped}], kind: \"rheo-handle\", supplement: none) <{}>",
                        anchor.label
                    )?;
                }
                writeln!(f, "  #include \"{}\"", segment.include)?;
            }
            writeln!(f, "]")?;
            writeln!(f)?;
        }
        Ok(())
    }
}
