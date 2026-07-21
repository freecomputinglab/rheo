//! Extraction of structured data from Typst syntax trees.
//!
//! Everything here is built on one abstraction: [`SyntaxSite`]. A `SyntaxSite`
//! says *what* to pull out of the tree — a label, a reference, a `rheo-*`
//! binding, the document date, a package import — while the shared walker in
//! [`syntax_site`] owns the *how*: one depth-first pass that threads each node's
//! byte offset and markup/code context and visits every node exactly once.
//!
//! Module layout:
//!
//! * [`syntax_site`] — the [`SyntaxSite`] trait and the traversal engine.
//! * [`labels`], [`document_date`], [`rheo_var`], [`imports`] — one extractor
//!   each: a small `impl SyntaxSite` plus its public collectors.
//! * this file — the aggregate [`extract_nodes`], which gathers all per-vertebra
//!   metadata (labels, `rheo-*` vars, document date) in a **single** parse and a
//!   **single** traversal by fanning one walk out to every visitor. That
//!   once-only pass is a design constraint on the spine-building hot path,
//!   enforced by `extract_nodes_parses_and_traverses_once`.

mod document_date;
mod document_metadata;
mod imports;
mod labels;
mod rheo_var;
mod syntax_site;

pub use document_date::DocumentDate;
pub use document_metadata::{DocumentMetadata, MetaValue};
pub use imports::ImportInfo;
pub use labels::{LabelRole, LabelSite, LabelSites};
pub use rheo_var::{RheoValue, RheoVar};
pub use syntax_site::{SyntaxSite, WalkCtx};

use typst::syntax::Source;

/// Everything harvested from a vertebra's source in the canonical parse.
pub struct ExtractedNodes {
    /// Top-level `#let rheo-<key> = "..."` bindings harvested from the source.
    pub rheo_vars: Vec<RheoVar>,
    /// Label definition and reference sites (with byte ranges), partitioned by
    /// role. Definition names drive the canonical-handle machinery; the full
    /// sites are retained so the Mould stage can rewrite them.
    pub labels: LabelSites,
    /// Parsed `#set document(date: datetime(...))` timestamp, if present.
    pub document_date: Option<DocumentDate>,
    /// All representable named arguments of the first `#set document(...)` rule.
    pub document_metadata: DocumentMetadata,
}

/// Harvest labels, `rheo-*` bindings, and the document date from `source` in a
/// **single** parse and a **single** traversal, fanning each node out to every
/// visitor.
///
/// Parsing and traversing exactly once is a design constraint (parse is the
/// costly step; this runs per vertebra during spine building). Enforced by
/// `extract_nodes_parses_and_traverses_once`.
pub fn extract_nodes(source: &Source) -> ExtractedNodes {
    let root = syntax_site::parse_source(source);
    let mut labels = Vec::new();
    let mut rheo_vars = Vec::new();
    let mut dates = Vec::new();
    let mut metadata = Vec::new();
    syntax_site::walk_once(source, &root, |s, n, o, c| {
        LabelSite::visit(s, n, o, c, &mut labels);
        RheoVar::visit(s, n, o, c, &mut rheo_vars);
        DocumentDate::visit(s, n, o, c, &mut dates);
        DocumentMetadata::visit(s, n, o, c, &mut metadata);
    });
    let mut label_sites = LabelSites::default();
    for site in labels {
        match site.role {
            LabelRole::Definition => label_sites.definitions.push(site),
            LabelRole::Reference => label_sites.references.push(site),
        }
    }
    ExtractedNodes {
        rheo_vars,
        labels: label_sites,
        document_date: dates.into_iter().next(),
        document_metadata: metadata.into_iter().next().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_nodes_parses_and_traverses_once() {
        // Source exercising every visitor, so no extractor can justify a second pass.
        let source = Source::detached(
            r#"#set document(date: datetime(year: 2025, month: 1, day: 2))
#let rheo-feed-title = "T"
= Heading <h>
See @h and #link(<h>)[here]."#,
        );
        syntax_site::reset_walk_counts();

        let _ = extract_nodes(&source);

        let (parses, traversals) = syntax_site::walk_counts();
        assert_eq!(
            parses, 1,
            "extract_nodes must parse the source exactly once"
        );
        assert_eq!(
            traversals, 1,
            "extract_nodes must traverse the tree exactly once"
        );
    }

    #[test]
    fn test_extract_nodes_rheo_vars() {
        let source = Source::detached(
            r#"#let rheo-feed-title = "Title"
#let rheo-feed-updated = "2025-01-15T00:00:00Z""#,
        );
        let extracted = extract_nodes(&source);
        assert_eq!(extracted.rheo_vars.len(), 2);
        assert_eq!(extracted.rheo_vars[0].key, "feed-title");
        assert_eq!(extracted.rheo_vars[1].key, "feed-updated");
    }
}
