use std::path::Path;

use super::tracer::TracedSpine;

/// Generate a synthetic bundle entry `.typ` file for the Typst bundle API.
///
/// Produces a complete Typst source string that uses `#document()` and `#asset()`
/// elements, letting Typst's bundle API handle multi-file output natively.
///
/// Cross-document links use native Typst label syntax (#link(<label>)) instead of
/// the old Rheo-specific #link("./file.typ") syntax. Link transformation has been
/// removed — users must update their .typ files to use explicit labels.
///
/// # Arguments
/// * `traced` - Traced spine with documents and assets
/// * `root` - Project root directory (for computing root-relative include paths)
/// * `format` - Output format name (e.g. "html", "epub", "pdf")
/// * `plugin_library` - Optional plugin-contributed Typst library code
pub fn generate_bundle_entry(
    traced: &TracedSpine,
    root: &Path,
    format: &str,
    plugin_library: &str,
) -> String {
    let mut out = String::new();

    // Preamble — exact order is critical
    //
    // NOTE: Bundle API only allows #let, #document(), and #asset() at top level.
    // No #set, #show, or other directives allowed.
    //
    out.push_str(&format!("#let target() = \"{format}\"\n\n"));

    // Include only the #let definitions from rheo.typ, skip the #set text(...) rule
    // The #set rule will be added inside each document's content
    out.push_str(
        r#"
// Get the rheo output format, with fallback to Typst's target()
#let rheo-target() = {
  if "rheo-target" in sys.inputs {
    sys.inputs.rheo-target
  } else {
    target()
  }
}

// Check if we're compiling for a specific rheo format
#let is-rheo-epub() = "rheo-target" in sys.inputs and sys.inputs.rheo-target == "epub"
#let is-rheo-html() = "rheo-target" in sys.inputs and sys.inputs.rheo-target == "html"
#let is-rheo-pdf() = "rheo-target" in sys.inputs and sys.inputs.rheo-target == "pdf"

#let rheo_template(doc) = context {
  doc
}
"#,
    );

    if !plugin_library.is_empty() {
        out.push_str(plugin_library);
        out.push_str("\n\n");
    }

    // Documents
    if traced.merge {
        // Merged PDF mode: single #document() wrapper with #include for each file
        let title = traced.title.as_deref().unwrap_or("document");
        out.push_str(&format!("#document(\"{title}.{format}\")[\n"));

        for doc in &traced.documents {
            if doc.is_bundle_entry {
                continue; // Skip the bundle entry itself
            }

            let rel = doc.path.strip_prefix(root).unwrap_or(&doc.path);
            let rel_str = rel.display().to_string().replace('\\', "/");

            // Use #include for each file
            // Note: #set document() directives in source files may cause issues
            // Users should ensure included files don't have conflicting document metadata
            out.push_str(&format!("  #include \"{rel_str}\"\n"));
        }

        out.push_str("]\n");
    } else {
        // Non-merged mode (HTML): separate #document() for each file
        for doc in &traced.documents {
            if doc.is_bundle_entry {
                // For bundle entry, use #include
                let rel = doc.path.strip_prefix(root).unwrap_or(&doc.path);
                let rel_str = rel.display().to_string().replace('\\', "/");
                out.push_str(&format!("#include \"{rel_str}\"\n"));
                continue;
            }

            let rel = doc.path.strip_prefix(root).unwrap_or(&doc.path);
            let rel_str = rel.display().to_string().replace('\\', "/");
            let stem = doc.path.file_stem().unwrap_or_default().to_string_lossy();

            // Use #document with #include for non-merged mode
            // Link transformation removed — users must use #link(<label>) syntax
            out.push_str(&format!(
                "#document(\"{stem}.{format}\")[#include \"{rel_str}\"]\n"
            ));
        }
    }

    // Assets
    if !traced.assets.is_empty() {
        out.push('\n');
        for asset in &traced.assets {
            let filename = asset.file_name().unwrap_or_default().to_string_lossy();
            let rel = asset.strip_prefix(root).unwrap_or(asset);
            let rel_str = rel.display().to_string().replace('\\', "/");
            out.push_str(&format!(
                "#asset(\"{filename}\", read(\"{rel_str}\", encoding: none))\n"
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reticulate::tracer::{SpineDocument, TracedSpine};
    use std::path::PathBuf;

    fn make_traced(
        documents: Vec<SpineDocument>,
        assets: Vec<PathBuf>,
        title: Option<&str>,
        merge: bool,
    ) -> TracedSpine {
        TracedSpine {
            title: title.map(str::to_string),
            documents,
            assets,
            merge,
        }
    }

    fn plain_doc(path: &str) -> SpineDocument {
        SpineDocument {
            path: PathBuf::from(path),
            is_bundle_entry: false,
        }
    }

    fn entry_doc(path: &str) -> SpineDocument {
        SpineDocument {
            path: PathBuf::from(path),
            is_bundle_entry: true,
        }
    }

    #[test]
    fn test_generate_bundle_entry_is_bundle_entry() {
        // is_bundle_entry=true → bare #include, no #document() wrapper
        let root = PathBuf::from("/project");
        let traced = make_traced(vec![entry_doc("/project/index.typ")], vec![], None, false);
        let out = generate_bundle_entry(&traced, &root, "html", "");
        assert!(out.contains("#include \"index.typ\""));
        assert!(!out.contains("#document("));
    }

    #[test]
    fn test_generate_bundle_entry_plain_no_merge() {
        // is_bundle_entry=false, merge=false → #document("{stem}.html")[#include ...]
        let root = PathBuf::from("/project");
        let traced = make_traced(vec![plain_doc("/project/chapter.typ")], vec![], None, false);
        let out = generate_bundle_entry(&traced, &root, "html", "");
        assert!(out.contains("#document(\"chapter.html\")[#include \"chapter.typ\"]"));
    }

    #[test]
    fn test_generate_bundle_entry_merge_with_title() {
        // merge=true with title → single #document("My Book.pdf")[...] around all includes
        let root = PathBuf::from("/project");
        let traced = make_traced(
            vec![plain_doc("/project/ch1.typ"), plain_doc("/project/ch2.typ")],
            vec![],
            Some("My Book"),
            true,
        );
        let out = generate_bundle_entry(&traced, &root, "pdf", "");
        assert!(out.contains("#document(\"My Book.pdf\")[\n"));
        assert!(out.contains("  #include \"ch1.typ\""));
        assert!(out.contains("  #include \"ch2.typ\""));
        assert!(!out.contains("#document(\"ch1.pdf\")")); // no individual wrappers
    }

    #[test]
    fn test_generate_bundle_entry_merge_no_title_fallback() {
        // merge=true, no title → "document.pdf" fallback
        let root = PathBuf::from("/project");
        let traced = make_traced(vec![plain_doc("/project/ch1.typ")], vec![], None, true);
        let out = generate_bundle_entry(&traced, &root, "pdf", "");
        assert!(out.contains("#document(\"document.pdf\")[\n"));
    }

    #[test]
    fn test_generate_bundle_entry_assets() {
        // Assets → #asset("style.css", read("style.css", encoding: none))
        let root = PathBuf::from("/project");
        let traced = make_traced(
            vec![plain_doc("/project/main.typ")],
            vec![PathBuf::from("/project/style.css")],
            None,
            false,
        );
        let out = generate_bundle_entry(&traced, &root, "html", "");
        assert!(out.contains("#asset(\"style.css\", read(\"style.css\", encoding: none))"));
    }

    #[test]
    fn test_generate_bundle_entry_mixed_entry_and_plain() {
        // Self-bundling + plain docs (no merge) → both patterns emitted, order preserved
        let root = PathBuf::from("/project");
        let traced = make_traced(
            vec![
                entry_doc("/project/index.typ"),
                plain_doc("/project/page.typ"),
            ],
            vec![],
            None,
            false,
        );
        let out = generate_bundle_entry(&traced, &root, "html", "");
        // index.typ is bundle entry → bare include
        assert!(out.contains("#include \"index.typ\""));
        // page.typ is plain → wrapped
        assert!(out.contains("#document(\"page.html\")[#include \"page.typ\"]"));
        // entry include appears before the document() wrapper
        let entry_pos = out.find("#include \"index.typ\"").unwrap();
        let doc_pos = out.find("#document(\"page.html\")").unwrap();
        assert!(entry_pos < doc_pos);
    }

    #[test]
    fn test_generate_bundle_entry_preamble_order() {
        // target() polyfill first, then rheo_template content, then plugin_library
        // Note: #show: rheo_template is no longer included in bundle entry
        // because bundle API doesn't allow show rules at top level
        let root = PathBuf::from("/project");
        let traced = make_traced(vec![plain_doc("/project/main.typ")], vec![], None, false);
        let plugin = "#let my_plugin() = {}";
        let out = generate_bundle_entry(&traced, &root, "html", plugin);

        let target_pos = out.find("#let target()").unwrap();
        let rheo_pos = out.find("rheo_template").unwrap(); // appears in rheo.typ content
        let plugin_pos = out.find("#let my_plugin()").unwrap();

        assert!(target_pos < rheo_pos);
        assert!(rheo_pos < plugin_pos);
        // No #show: rheo_template expected in bundle entry anymore
    }
}
