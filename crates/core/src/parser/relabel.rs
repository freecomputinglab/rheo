//! Transform: prefix a vertebra's authored labels with its handle.
//!
//! rheo collates every spine file into one virtual Typst bundle, so labels live
//! in a single flat namespace. To keep authored anchors unique without making
//! authors repeat the file stem, [`prefix_labels`] prepends a vertebra's handle
//! (with the `:` nesting divider) to every label the vertebra *defines*, and
//! rewrites that vertebra's *local* references so bare in-file refs still
//! resolve. A heading `=== Et alia <etal>` in `26w27.typ` becomes `<26w27:etal>`
//! in the bundle — globally referenceable as `@26w27:etal` — while a bare
//! `@etal` in the same file is rewritten to `@26w27:etal`.
//!
//! All rewriting is driven by the Typst syntax AST via [`LabelSites::from_source`],
//! never by regex or naive string search. References whose target is *not* a
//! local markup definition are left untouched — this is precisely what keeps
//! bibliography citations (`@karataniTranscritique2005`) and already-qualified
//! cross-file references (`@26w24:framework`) alone.

use super::LabelSites;
use std::collections::HashSet;
use std::ops::Range;
use typst::syntax::Source;

/// Prefix every markup label defined in `source` with `handle:`, and rewrite
/// every local reference to a defined label to match.
///
/// `handle` is the full vertebra handle (e.g. `chapters:intro`), so a nested
/// file's `<foo>` becomes `<chapters:intro:foo>`. Labels that already contain
/// `:` simply gain a further prefix (`<fig:chart>` → `<handle:fig:chart>`).
///
/// The rewrite runs exactly once per vertebra; it is not idempotent.
pub fn prefix_labels(source: &str, handle: &str) -> String {
    let parsed = Source::detached(source);
    let sites = LabelSites::from_source(&parsed);

    // Names defined in *this* source (markup context). Only references to these
    // are rewritten; every other reference (citations, cross-file refs) is left
    // as authored.
    let local: HashSet<&str> = sites.definitions.iter().map(|s| s.name.as_str()).collect();

    // Collect (range, replacement) edits from the AST-derived byte ranges.
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();

    // Every markup definition is prefixed.
    for def in &sites.definitions {
        edits.push((def.range.clone(), format!("<{handle}:{}>", def.name)));
    }

    // A reference is prefixed only if its target is defined locally. The token
    // is either an `@name` marker or a code-context `<name>`; preserve its form.
    for r in &sites.references {
        if !local.contains(r.name.as_str()) {
            continue;
        }
        let replacement = if source[r.range.clone()].starts_with('@') {
            format!("@{handle}:{}", r.name)
        } else {
            format!("<{handle}:{}>", r.name)
        };
        edits.push((r.range.clone(), replacement));
    }

    // Splice by descending start offset so earlier ranges stay valid. Label and
    // reference tokens never overlap, so ordering by start is sufficient.
    edits.sort_by(|a, b| b.0.start.cmp(&a.0.start));
    let mut out = source.to_string();
    for (range, replacement) in edits {
        out.replace_range(range, &replacement);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_ref_and_def_are_prefixed() {
        let src = "=== Et alia <etal>\n\nSee @etal for more.";
        let out = prefix_labels(src, "26w27");
        assert_eq!(out, "=== Et alia <26w27:etal>\n\nSee @26w27:etal for more.");
    }

    #[test]
    fn bib_citation_without_local_def_is_untouched() {
        let src = "As shown @someKey, the result holds.";
        let out = prefix_labels(src, "26w27");
        assert_eq!(out, src);
    }

    #[test]
    fn already_qualified_cross_file_ref_is_untouched() {
        // No local `<26w24:framework>` def here, so the cross-file ref stays.
        let src = "Recall @26w24:framework from earlier.";
        let out = prefix_labels(src, "26w27");
        assert_eq!(out, src);
    }

    #[test]
    fn nested_handle_prefixes_definition() {
        let src = "= Foo <foo>";
        let out = prefix_labels(src, "chapters:intro");
        assert_eq!(out, "= Foo <chapters:intro:foo>");
    }

    #[test]
    fn label_already_containing_colon_gains_further_prefix() {
        let src = "#figure([], caption: [c]) <fig:chart>\n\nSee @fig:chart.";
        let out = prefix_labels(src, "H");
        assert_eq!(
            out,
            "#figure([], caption: [c]) <H:fig:chart>\n\nSee @H:fig:chart."
        );
    }

    #[test]
    fn all_reference_forms_are_handled() {
        // `@ref`, `#link(<x>)`, `#ref(<x>)` all rewritten when defined locally.
        let src = "= X <x>\n\n@x and #link(<x>)[here] and #ref(<x>)";
        let out = prefix_labels(src, "H");
        assert_eq!(
            out,
            "= X <H:x>\n\n@H:x and #link(<H:x>)[here] and #ref(<H:x>)"
        );
    }

    #[test]
    fn ref_without_local_def_is_left_untouched() {
        // `@y` has no `<y>` definition in this file.
        let src = "= X <x>\n\nSee @x but not @y.";
        let out = prefix_labels(src, "H");
        assert_eq!(out, "= X <H:x>\n\nSee @H:x but not @y.");
    }

    #[test]
    fn ref_supplement_is_preserved() {
        // The `[p.9]` supplement is outside the rewritten `@x` marker range.
        let src = "= X <x>\n\nCitation @x[p.9] here.";
        let out = prefix_labels(src, "H");
        assert_eq!(out, "= X <H:x>\n\nCitation @H:x[p.9] here.");
    }

    #[test]
    fn multiple_definitions_are_each_prefixed() {
        let src = "= A <a>\n\n== B <b>\n\n@a then @b.";
        let out = prefix_labels(src, "26w27");
        assert_eq!(
            out,
            "= A <26w27:a>\n\n== B <26w27:b>\n\n@26w27:a then @26w27:b."
        );
    }
}
