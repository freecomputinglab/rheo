//! A [`SyntaxRewrite`] that namespaces a vertebra's labels under its handle.
//!
//! rheo collates every spine file into one virtual Typst bundle, so labels live
//! in a single flat namespace. To keep authored anchors unique without making
//! authors repeat the file stem, [`LabelRewrite::collect`] prepends a vertebra's
//! handle (with the `:` nesting divider) to every label the vertebra *defines*,
//! and rewrites the vertebra's *local* references so bare in-file refs still
//! resolve. A heading `=== Et alia <etal>` in `26w27.typ` becomes `<26w27:etal>`
//! (globally referenceable as `@26w27:etal`), while a bare `@etal` in the same
//! file is rewritten to `@26w27:etal`. References whose target is not a local
//! definition — bibliography citations, already-qualified cross-file refs — are
//! left untouched.

use super::mould::SyntaxRewrite;
use super::spine::Vertebra;
use crate::parser::LabelSites;
use std::collections::HashSet;
use std::ops::Range;

/// A label token rewritten to its handle-prefixed form.
pub struct LabelRewrite {
    range: Range<usize>,
    replacement: String,
}

impl SyntaxRewrite for LabelRewrite {
    fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    fn map(&self, _original: &str) -> String {
        self.replacement.clone()
    }
}

impl LabelRewrite {
    /// The rewrites that handle-prefix this vertebra's labels.
    pub fn collect(vertebra: &Vertebra) -> Vec<LabelRewrite> {
        Self::build(&vertebra.source, &vertebra.handle, &vertebra.sites)
    }

    /// Prefix every markup definition, and every reference whose target is a
    /// local definition, with `handle:`. The reference token keeps its form —
    /// `@name` stays an `@`-marker, a code-context `<name>` stays bracketed.
    fn build(source: &str, handle: &str, sites: &LabelSites) -> Vec<LabelRewrite> {
        let local: HashSet<&str> = sites.definitions.iter().map(|s| s.name.as_str()).collect();
        let mut rewrites = Vec::new();

        for def in &sites.definitions {
            rewrites.push(LabelRewrite {
                range: def.range.clone(),
                replacement: format!("<{handle}:{}>", def.name),
            });
        }

        for reference in &sites.references {
            if !local.contains(reference.name.as_str()) {
                continue;
            }
            let replacement = if source[reference.range.clone()].starts_with('@') {
                format!("@{handle}:{}", reference.name)
            } else {
                format!("<{handle}:{}>", reference.name)
            };
            rewrites.push(LabelRewrite {
                range: reference.range.clone(),
                replacement,
            });
        }

        rewrites
    }
}

#[cfg(test)]
mod tests {
    use super::super::mould::Rewrites;
    use super::*;
    use typst::syntax::Source;

    /// Build the handle-prefix rewrites for `src` and apply them, as moulding does.
    fn prefixed(src: &str, handle: &str) -> String {
        let sites = LabelSites::from_source(&Source::detached(src));
        let rewrites = LabelRewrite::build(src, handle, &sites)
            .into_iter()
            .map(|r| Box::new(r) as Box<dyn SyntaxRewrite>)
            .collect();
        Rewrites(rewrites).apply(src)
    }

    #[test]
    fn local_ref_and_def_are_prefixed() {
        let src = "=== Et alia <etal>\n\nSee @etal for more.";
        assert_eq!(
            prefixed(src, "26w27"),
            "=== Et alia <26w27:etal>\n\nSee @26w27:etal for more."
        );
    }

    #[test]
    fn bib_citation_without_local_def_is_untouched() {
        let src = "As shown @someKey, the result holds.";
        assert_eq!(prefixed(src, "26w27"), src);
    }

    #[test]
    fn already_qualified_cross_file_ref_is_untouched() {
        // No local `<26w24:framework>` def here, so the cross-file ref stays.
        let src = "Recall @26w24:framework from earlier.";
        assert_eq!(prefixed(src, "26w27"), src);
    }

    #[test]
    fn nested_handle_prefixes_definition() {
        let src = "= Foo <foo>";
        assert_eq!(
            prefixed(src, "chapters:intro"),
            "= Foo <chapters:intro:foo>"
        );
    }

    #[test]
    fn label_already_containing_colon_gains_further_prefix() {
        let src = "#figure([], caption: [c]) <fig:chart>\n\nSee @fig:chart.";
        assert_eq!(
            prefixed(src, "H"),
            "#figure([], caption: [c]) <H:fig:chart>\n\nSee @H:fig:chart."
        );
    }

    #[test]
    fn all_reference_forms_are_handled() {
        // `@ref`, `#link(<x>)`, `#ref(<x>)` all rewritten when defined locally.
        let src = "= X <x>\n\n@x and #link(<x>)[here] and #ref(<x>)";
        assert_eq!(
            prefixed(src, "H"),
            "= X <H:x>\n\n@H:x and #link(<H:x>)[here] and #ref(<H:x>)"
        );
    }

    #[test]
    fn ref_without_local_def_is_left_untouched() {
        // `@y` has no `<y>` definition in this file.
        let src = "= X <x>\n\nSee @x but not @y.";
        assert_eq!(prefixed(src, "H"), "= X <H:x>\n\nSee @H:x but not @y.");
    }

    #[test]
    fn ref_supplement_is_preserved() {
        // The `[p.9]` supplement is outside the rewritten `@x` marker range.
        let src = "= X <x>\n\nCitation @x[p.9] here.";
        assert_eq!(prefixed(src, "H"), "= X <H:x>\n\nCitation @H:x[p.9] here.");
    }

    #[test]
    fn ref_matching_local_def_is_rewritten_even_if_also_a_citation_key() {
        // Known limitation: `@name` is rewritten whenever `name` is a local
        // markup definition. If a file BOTH defines `<someKey>` AND cites
        // `@someKey` (a bibliography key of the same name), the citation is
        // rewritten to `@H:someKey` and thereby detached from the bibliography.
        // rheo cannot tell a citation from a label ref by name alone, and bib
        // keys are conventionally distinct from section labels — so this is
        // accepted and documented rather than special-cased. See CLAUDE.md
        // "Cross-file references".
        let src = "= Some key <someKey>\n\nAs shown @someKey, it holds.";
        assert_eq!(
            prefixed(src, "H"),
            "= Some key <H:someKey>\n\nAs shown @H:someKey, it holds."
        );
    }

    #[test]
    fn multiple_definitions_are_each_prefixed() {
        let src = "= A <a>\n\n== B <b>\n\n@a then @b.";
        assert_eq!(
            prefixed(src, "26w27"),
            "= A <26w27:a>\n\n== B <26w27:b>\n\n@26w27:a then @26w27:b."
        );
    }
}
