//! Extraction of structured data from Typst syntax trees.
//!
//! Everything here is built on one abstraction: [`SyntaxSite`]. A `SyntaxSite`
//! says *what* to pull out of the tree — a label, a reference, a package
//! import — while the shared walker in [`syntax_site`] owns the *how*: one
//! depth-first pass that threads each node's byte offset and markup/code
//! context and visits every node exactly once.
//!
//! Module layout:
//!
//! * [`syntax_site`] — the [`SyntaxSite`] trait and the traversal engine.
//! * [`labels`], [`imports`] — one extractor each: a small `impl SyntaxSite`
//!   plus its public collectors.
//! * this file — the aggregate [`extract_nodes`], which gathers all per-vertebra
//!   metadata (labels) in a **single** parse and a **single** traversal by
//!   fanning one walk out to every visitor. That once-only pass is a design
//!   constraint on the spine-building hot path, enforced by
//!   `extract_nodes_parses_and_traverses_once`.

mod imports;
mod labels;
mod syntax_site;

pub use imports::ImportInfo;
pub use labels::{LabelRole, LabelSite, LabelSites};
pub use syntax_site::{SyntaxSite, WalkCtx};

use typst::syntax::Source;

/// Everything harvested from a vertebra's source in the canonical parse.
pub struct ExtractedNodes {
    /// Label definition and reference sites (with byte ranges), partitioned by
    /// role. Definition names drive the canonical-handle machinery; the full
    /// sites are retained so the Mould stage can rewrite them.
    pub labels: LabelSites,
}

/// Harvest labels from `source` in a **single** parse and a **single**
/// traversal, fanning each node out to every visitor.
///
/// Parsing and traversing exactly once is a design constraint (parse is the
/// costly step; this runs per vertebra during spine building). Enforced by
/// `extract_nodes_parses_and_traverses_once`.
pub fn extract_nodes(source: &Source) -> ExtractedNodes {
    let root = syntax_site::parse_source(source);
    let mut labels = Vec::new();
    syntax_site::walk_once(source, &root, |s, n, o, c| {
        LabelSite::visit(s, n, o, c, &mut labels);
    });
    let mut label_sites = LabelSites::default();
    for site in labels {
        match site.role {
            LabelRole::Definition => label_sites.definitions.push(site),
            LabelRole::Reference => label_sites.references.push(site),
        }
    }
    ExtractedNodes {
        labels: label_sites,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_nodes_parses_and_traverses_once() {
        // Source exercising every visitor, so no extractor can justify a second pass.
        let source = Source::detached(
            r#"= Heading <h>
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
}
