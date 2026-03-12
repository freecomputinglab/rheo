// Simplified link transformer for EPUB (.typ → .xhtml link rewriting)
// Extracted from rheo_core::reticulate::transformer as part of bundle migration.
//
// This performs simple string-based replacement of .typ links to .xhtml links.
// For EPUB, we only need to rewrite relative links to other .typ files.
// Complex AST-based transformation is not needed since EPUB uses its own
// per-file compilation workflow.

use rheo_core::Result;

/// EPUB link transformer that converts .typ links to .xhtml links.
pub struct LinkTransformer;

impl LinkTransformer {
    /// Create a new EPUB link transformer.
    pub fn new() -> Self {
        Self
    }

    /// Transform source code by replacing .typ links with .xhtml links.
    ///
    /// This is a simple string-based replacement suitable for EPUB's use case.
    /// It replaces occurrences of #link("./file.typ") with #link("./file.xhtml").
    pub fn transform_source(
        &self,
        source: &str,
        _current_file: &std::path::Path,
        _project_root: &std::path::Path,
    ) -> Result<String> {
        // Simple regex-based replacement for .typ links in #link() calls
        // This matches: #link("./path/to/file.typ") or #link("../path/to/file.typ")
        use regex::Regex;

        // Match #link("...typ") where ... is any path string
        let re = Regex::new(r#"#link\("([^"]+\.typ)"\)"#)
            .map_err(|e| rheo_core::RheoError::invalid_data(format!("invalid regex: {}", e)))?;

        let transformed = re.replace_all(source, |caps: &regex::Captures| {
            let path = &caps[1];
            // Replace .typ extension with .xhtml
            let new_path = path.replace(".typ", ".xhtml");
            format!(r#"#link("{}")"#, new_path)
        });

        Ok(transformed.to_string())
    }
}
