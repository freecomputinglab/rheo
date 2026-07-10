//! Extractor: `<label>` definitions and `@ref` / `#link(<label>)` references.

use super::{SyntaxSite, WalkCtx};
use std::ops::Range;
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// Whether a [`LabelSite`] defines a label or references one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelRole {
    /// A markup-context `<name>` — defines a label on the surrounding content.
    Definition,
    /// A `@name` marker or a code-context `<name>` — references a label.
    Reference,
}

/// A label occurrence with the byte range of the token to rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelSite {
    /// Bare label name: the `<name>` brackets or a leading `@` stripped.
    pub name: String,
    /// Whether this site defines or references a label.
    pub role: LabelRole,
    /// Byte range of the rewriteable token in the source — the full `<name>`
    /// for a label, or the `@name` marker for a reference (a trailing `[..]`
    /// supplement is excluded).
    pub range: Range<usize>,
}

impl SyntaxSite for LabelSite {
    fn visit(
        _source: &Source,
        node: &SyntaxNode,
        offset: usize,
        ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        match node.kind() {
            // A `<name>` label token (leaf). Markup context → definition; code
            // context (function args) → reference, e.g. `#link(<name>)`.
            SyntaxKind::Label => {
                let text = node.leaf_text();
                let name = text.trim_start_matches('<').trim_end_matches('>');
                if !name.is_empty() {
                    let role = if ctx.in_code {
                        LabelRole::Reference
                    } else {
                        LabelRole::Definition
                    };
                    out.push(LabelSite {
                        name: name.to_string(),
                        role,
                        range: offset..offset + node.len(),
                    });
                }
            }
            // An `@name` reference marker (leaf). Always a reference; any `[..]`
            // supplement is a sibling node and is excluded from the range.
            SyntaxKind::RefMarker => {
                let name = node.leaf_text().trim_start_matches('@');
                if !name.is_empty() {
                    out.push(LabelSite {
                        name: name.to_string(),
                        role: LabelRole::Reference,
                        range: offset..offset + node.len(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Label definition and reference sites in a source, partitioned by role.
///
/// `definitions` are markup-context `<name>` labels (attached to content).
/// `references` are `@name` markers plus code-context `<name>` labels such as
/// `#link(<name>)` / `#ref(<name>)`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LabelSites {
    pub definitions: Vec<LabelSite>,
    pub references: Vec<LabelSite>,
}

impl LabelSites {
    /// Collect the label sites in `source` and partition them by role.
    ///
    /// A view over [`LabelSite::collect`]: ranges are exact byte offsets into
    /// `source`, suitable for splicing.
    pub fn from_source(source: &Source) -> Self {
        let mut sites = LabelSites::default();
        for site in LabelSite::collect(source) {
            match site.role {
                LabelRole::Definition => sites.definitions.push(site),
                LabelRole::Reference => sites.references.push(site),
            }
        }
        sites
    }
}

/// Return all `<label>` names **defined** in the source (markup context only).
///
/// Labels inside function-call arguments (`#link(<label>)`) are references, not
/// definitions, and are excluded. Surrounding `<`/`>` are stripped.
pub fn collect_user_labels(source: &Source) -> Vec<String> {
    LabelSites::from_source(source)
        .definitions
        .into_iter()
        .map(|site| site.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_user_labels() {
        let source = Source::detached(
            r#"= Introduction <intro>

Some text. <fig:chart>

#figure([], caption: [Chart]) <fig:chart>

== Section <sec-one>"#,
        );
        let mut labels = collect_user_labels(&source);
        labels.sort();
        assert_eq!(labels, vec!["fig:chart", "fig:chart", "intro", "sec-one"]);
    }

    #[test]
    fn test_collect_user_labels_empty() {
        let source = Source::detached("= No labels here\n\nJust text.");
        let labels = collect_user_labels(&source);
        assert!(labels.is_empty());
    }

    /// Assert that each site's recorded range slices back to `expected` text.
    fn assert_sites_slice(src: &str, sites: &[LabelSite], expected: &[(&str, &str)]) {
        assert_eq!(
            sites.len(),
            expected.len(),
            "site count mismatch: {sites:?}"
        );
        for (site, (name, text)) in sites.iter().zip(expected) {
            assert_eq!(&site.name, name);
            assert_eq!(&src[site.range.clone()], *text);
        }
    }

    #[test]
    fn test_sites_heading_definition() {
        let src = "= Intro <intro>";
        let sites = LabelSites::from_source(&Source::detached(src));
        assert_sites_slice(src, &sites.definitions, &[("intro", "<intro>")]);
        assert!(sites.references.is_empty());
    }

    #[test]
    fn test_sites_figure_label_definition() {
        let src = "#figure([], caption: [c]) <fig:chart>";
        let sites = LabelSites::from_source(&Source::detached(src));
        assert_sites_slice(src, &sites.definitions, &[("fig:chart", "<fig:chart>")]);
        assert!(sites.references.is_empty());
    }

    #[test]
    fn test_sites_ref_marker_is_reference() {
        let src = "See @intro for more.";
        let sites = LabelSites::from_source(&Source::detached(src));
        assert!(sites.definitions.is_empty());
        assert_sites_slice(src, &sites.references, &[("intro", "@intro")]);
    }

    #[test]
    fn test_sites_ref_with_supplement_excludes_supplement() {
        // The `[p.9]` supplement is a sibling node; the range covers only `@key`.
        let src = "Citation @key[p.9] here.";
        let sites = LabelSites::from_source(&Source::detached(src));
        assert_sites_slice(src, &sites.references, &[("key", "@key")]);
    }

    #[test]
    fn test_sites_link_and_ref_calls_are_references() {
        let src = "#link(<a>)[text] and #ref(<b>)";
        let sites = LabelSites::from_source(&Source::detached(src));
        assert!(sites.definitions.is_empty());
        assert_sites_slice(src, &sites.references, &[("a", "<a>"), ("b", "<b>")]);
    }

    #[test]
    fn test_sites_definition_and_reference_classified() {
        let src = "== Section <sec>\n\nBack to @sec.";
        let sites = LabelSites::from_source(&Source::detached(src));
        assert_sites_slice(src, &sites.definitions, &[("sec", "<sec>")]);
        assert_sites_slice(src, &sites.references, &[("sec", "@sec")]);
    }

    #[test]
    fn test_syntaxsite_collect_tags_roles() {
        // The SyntaxSite trait's collect() returns every site in one flat list,
        // each tagged with its role; LabelSites::from_source is the partition view.
        let src = "= A <a>\n\n#link(<a>)[x] and @a.";
        let roles: Vec<LabelRole> = LabelSite::collect(&Source::detached(src))
            .into_iter()
            .map(|s| s.role)
            .collect();
        assert_eq!(
            roles,
            vec![
                LabelRole::Definition,
                LabelRole::Reference,
                LabelRole::Reference
            ]
        );
    }

    #[test]
    fn test_syntaxsite_first_returns_leading_site() {
        // first() pairs with the MAX_SITES cap: the leading site in document order.
        let src = "= A <a>\n\n== B <b>";
        let first = LabelSite::first(&Source::detached(src)).expect("a label");
        assert_eq!(first.name, "a");
    }

    #[test]
    fn test_collect_user_labels_matches_definitions() {
        // collect_user_labels is a thin wrapper over LabelSites::from_source definitions.
        let src = "= A <a>\n\n#link(<a>)[x]\n\n== B <b>";
        let source = Source::detached(src);
        let mut labels = collect_user_labels(&source);
        labels.sort();
        assert_eq!(labels, vec!["a", "b"]);
    }
}
