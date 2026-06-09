use super::types::{LinkInfo, LinkTransform, RheoVar};
use crate::constants::TYP_EXT;
use crate::pdf_utils::sanitize_label_name;
use crate::plugins::LinkStrategy;
use crate::reticulate::validator::is_relative_typ_link;
use crate::{Result, RheoError};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Output of a single source transformation: the rewritten source plus any
/// `rheo-*` vars harvested from it during the (single) parse.
pub struct TransformOutput {
    pub source: String,
    pub rheo_vars: Vec<RheoVar>,
}

/// Link transformer that converts Typst links to format-specific targets.
pub struct LinkTransformer {
    /// The output format's extension, used solely for the `.typ` -> `.{ext}`
    /// rewrite under [`LinkStrategy::ExtensionRewrite`].
    format_name: String,
    /// How relative `.typ` links are rewritten (extension rewrite vs PDF labels).
    strategy: LinkStrategy,
    spine: Option<Vec<PathBuf>>,
    /// When true, rewrite relative import/include paths to be project-root-relative.
    /// Needed when sources will be compiled from a different directory (e.g. merged into
    /// a single temp file at the project root). Must be false when each source is compiled
    /// from its own directory, or the rewritten paths will be wrong.
    rewrite_imports: bool,
}

impl LinkTransformer {
    /// Create a new LinkTransformer for the specified output format name.
    ///
    /// Defaults to [`LinkStrategy::ExtensionRewrite`]; call
    /// [`with_strategy`](Self::with_strategy) to use PDF labels instead.
    pub(crate) fn new(format_name: &str) -> Self {
        Self {
            format_name: format_name.to_string(),
            strategy: LinkStrategy::ExtensionRewrite,
            spine: None,
            rewrite_imports: false,
        }
    }

    /// Set the link strategy (extension rewrite vs PDF labels).
    pub fn with_strategy(mut self, strategy: LinkStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set the spine for merged PDF compilation.
    pub fn with_spine(mut self, spine: Vec<PathBuf>) -> Self {
        self.spine = Some(spine);
        self
    }

    /// Enable import path rewriting for merged compilation.
    pub fn with_import_rewriting(mut self, rewrite: bool) -> Self {
        self.rewrite_imports = rewrite;
        self
    }

    /// Transform source code by processing all links and rewriting relative
    /// import/include paths so they resolve correctly from the project root
    /// (needed when spine files are merged into a temp file at the root).
    ///
    /// Parses the source exactly once and traverses the AST once to collect
    /// both link info and import info.
    pub fn transform_source(
        &self,
        source: &str,
        current_file: &Path,
        project_root: &Path,
    ) -> Result<String> {
        Ok(self
            .transform_with_vars(source, current_file, project_root)?
            .source)
    }

    /// Like [`transform_source`](Self::transform_source) but also returns the
    /// `rheo-*` vars harvested during the single parse.
    pub fn transform_with_vars(
        &self,
        source: &str,
        current_file: &Path,
        project_root: &Path,
    ) -> Result<TransformOutput> {
        use crate::reticulate::{parser, serializer};

        let source_obj = typst::syntax::Source::detached(source);
        let extracted = parser::extract_nodes(&source_obj);

        for line in &extracted.unresolvable_link_lines {
            warn!(
                file = %current_file.display(),
                line = line,
                "rheo: #link() call has a non-literal URL argument that cannot be statically \
                 transformed. The .typ extension will NOT be rewritten in the output. \
                 To fix: use a string literal directly: #link(\"./file.typ\")[...], \
                 or define the wrapper function in the same file."
            );
        }

        let mut transformations = self.compute_transformations(&extracted.links, current_file)?;

        // Rewrite relative import/include paths to be project-root-relative
        // when sources will be hoisted to a temp file at the project root (merge mode).
        if self.rewrite_imports {
            for import in &extracted.imports {
                if import.is_package || import.path.starts_with('/') {
                    continue;
                }
                let file_dir = current_file.parent().unwrap_or(Path::new(""));
                let absolute = file_dir.join(&import.path);
                let new_path = absolute
                    .strip_prefix(project_root)
                    .map(|p| p.to_str().unwrap().to_owned())
                    .unwrap_or_else(|_| import.path.clone());
                transformations.push((
                    import.byte_range.clone(),
                    LinkTransform::ReplaceUrl { new_url: new_path },
                ));
            }
        }

        let code_ranges = serializer::find_code_block_ranges(&source_obj);
        let transformed = serializer::apply_transformations(source, &transformations, &code_ranges);
        Ok(TransformOutput {
            source: transformed,
            rheo_vars: extracted.rheo_vars,
        })
    }

    /// Compute format-specific transformations for links.
    fn compute_transformations(
        &self,
        links: &[LinkInfo],
        _current_file: &Path,
    ) -> Result<Vec<(Range<usize>, LinkTransform)>> {
        let mut transformations = Vec::new();

        let label_map: HashMap<String, String> = match (self.strategy, &self.spine) {
            (LinkStrategy::PagedLabels, Some(spine)) => build_label_map(spine),
            _ => HashMap::new(),
        };

        for link in links {
            let url = &link.url;
            let filename = extract_filename(url);
            let stem = filename.strip_suffix(TYP_EXT).unwrap_or(filename);

            let transform = if is_relative_typ_link(url) {
                match (self.strategy, &self.spine) {
                    (LinkStrategy::PagedLabels, None) => {
                        // Single PDF: remove links
                        LinkTransform::Remove {
                            body: link.body.clone(),
                        }
                    }
                    (LinkStrategy::PagedLabels, Some(_)) => {
                        // Merged PDF: convert to labels, check if file is in spine
                        if !label_map.contains_key(stem) {
                            return Err(RheoError::project_config(format!(
                                "Link target '{}' not found in spine. Make sure that the file exists in the project and is in the spine in rheo.toml",
                                filename
                            )));
                        }
                        let label = label_map.get(stem).unwrap();
                        LinkTransform::ReplaceUrlWithLabel {
                            new_label: format!("<{}>", label),
                        }
                    }
                    // Generic extension-based replacement: .typ → .{extension}
                    (LinkStrategy::ExtensionRewrite, _) => LinkTransform::ReplaceUrl {
                        new_url: url.replace(TYP_EXT, &format!(".{}", self.format_name)),
                    },
                }
            } else {
                // External URL, fragment, or non-.typ link — always preserve
                LinkTransform::KeepOriginal
            };

            // Wrapper-call links only cover the Str argument, so use
            // ReplaceStringLiteralInPlace regardless of the computed transform.
            let transform = if link.is_wrapper_call {
                match transform {
                    LinkTransform::Remove { .. } => LinkTransform::ReplaceStringLiteralInPlace {
                        new_value: String::new(),
                    },
                    LinkTransform::ReplaceUrl { new_url } => {
                        LinkTransform::ReplaceStringLiteralInPlace { new_value: new_url }
                    }
                    LinkTransform::ReplaceUrlWithLabel { new_label } => {
                        LinkTransform::ReplaceStringLiteralInPlace {
                            new_value: new_label,
                        }
                    }
                    other => other,
                }
            } else {
                transform
            };

            transformations.push((link.byte_range.clone(), transform));
        }

        Ok(transformations)
    }
}

/// Build a map of filename stems to sanitized labels for merged PDF compilation.
fn build_label_map(spine_files: &[PathBuf]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for spine_file in spine_files {
        if let Some(filename) = spine_file.file_name() {
            let filename_str = filename.to_string_lossy();
            let stem = filename_str.strip_suffix(TYP_EXT).unwrap_or(&filename_str);
            let label = sanitize_label_name(stem);
            map.insert(stem.to_string(), label);
        }
    }
    map
}

fn extract_filename(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Span;

    fn make_link(url: &str, body: &str, byte_range: Range<usize>) -> LinkInfo {
        LinkInfo {
            url: url.to_string(),
            body: body.to_string(),
            span: Span::detached(),
            byte_range,
            is_wrapper_call: false,
        }
    }

    #[test]
    fn test_pdf_single_removes_typ_links() {
        let links = vec![make_link("./file.typ", "text", 0..10)];
        let transformer = LinkTransformer::new("pdf").with_strategy(LinkStrategy::PagedLabels);
        let transforms = transformer
            .compute_transformations(&links, Path::new("test.typ"))
            .unwrap();

        assert_eq!(transforms.len(), 1);
        match &transforms[0].1 {
            LinkTransform::Remove { body } => assert_eq!(body, "text"),
            _ => panic!("Expected Remove transform"),
        }
    }

    #[test]
    fn test_pdf_single_preserves_external_urls() {
        let links = vec![
            make_link("https://example.com", "example", 0..10),
            make_link("http://example.com", "example2", 20..30),
            make_link("mailto:test@example.com", "email", 40..50),
        ];
        let transformer = LinkTransformer::new("pdf").with_strategy(LinkStrategy::PagedLabels);
        let transforms = transformer
            .compute_transformations(&links, Path::new("test.typ"))
            .unwrap();

        assert_eq!(transforms.len(), 3);
        for (_range, transform) in transforms {
            assert!(matches!(transform, LinkTransform::KeepOriginal));
        }
    }

    #[test]
    fn test_pdf_merged_converts_to_labels() {
        let links = vec![make_link("./chapter2.typ", "next", 0..10)];
        let spine = vec![PathBuf::from("chapter1.typ"), PathBuf::from("chapter2.typ")];
        let transformer = LinkTransformer::new("pdf")
            .with_strategy(LinkStrategy::PagedLabels)
            .with_spine(spine);
        let transforms = transformer
            .compute_transformations(&links, Path::new("chapter1.typ"))
            .unwrap();

        assert_eq!(transforms.len(), 1);
        match &transforms[0].1 {
            LinkTransform::ReplaceUrlWithLabel { new_label } => {
                assert_eq!(new_label, "<chapter2>")
            }
            _ => panic!("Expected ReplaceUrlWithLabel transform"),
        }
    }

    #[test]
    fn test_pdf_merged_errors_on_missing_spine_file() {
        let links = vec![make_link("./missing.typ", "missing", 0..10)];
        let spine = vec![PathBuf::from("chapter1.typ")];
        let transformer = LinkTransformer::new("pdf")
            .with_strategy(LinkStrategy::PagedLabels)
            .with_spine(spine);
        let result = transformer.compute_transformations(&links, Path::new("chapter1.typ"));

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not found in spine")
        );
    }

    #[test]
    fn test_html_transforms_typ_to_html() {
        let links = vec![
            make_link("./file.typ", "text", 0..10),
            make_link("https://example.com", "external", 20..30),
        ];
        let transformer = LinkTransformer::new("html");
        let transforms = transformer
            .compute_transformations(&links, Path::new("test.typ"))
            .unwrap();

        assert_eq!(transforms.len(), 2);
        match &transforms[0].1 {
            LinkTransform::ReplaceUrl { new_url } => assert_eq!(new_url, "./file.html"),
            _ => panic!("Expected ReplaceUrl transform for .typ link"),
        }
        assert!(matches!(transforms[1].1, LinkTransform::KeepOriginal));
    }

    #[test]
    fn test_unknown_format_replaces_extension() {
        let links = vec![make_link("./file.typ", "text", 0..10)];
        let transformer = LinkTransformer::new("unknown");
        let transforms = transformer
            .compute_transformations(&links, Path::new("test.typ"))
            .unwrap();
        match &transforms[0].1 {
            LinkTransform::ReplaceUrl { new_url } => assert_eq!(new_url, "./file.unknown"),
            other => panic!("Expected ReplaceUrl transform, got {:?}", other),
        }
    }

    #[test]
    fn test_extract_filename() {
        assert_eq!(extract_filename("./chapter2.typ"), "chapter2.typ");
        assert_eq!(extract_filename("../parent/file.typ"), "file.typ");
        assert_eq!(extract_filename("/absolute/path.typ"), "path.typ");
        assert_eq!(extract_filename("simple.typ"), "simple.typ");
    }

    #[test]
    fn test_unresolvable_link_does_not_error() {
        let source = r#"#link(compute_url())[text]"#;
        let transformer = LinkTransformer::new("html");
        let result =
            transformer.transform_source(source, Path::new("test.typ"), Path::new("/root"));
        assert!(result.is_ok());
        assert!(result.unwrap().contains("#link(compute_url())"));
    }

    #[test]
    fn test_unresolvable_link_passes_through_unchanged() {
        // Dynamic URL expression: should compile fine and leave link untouched
        let source = r#"#link("./ch" + num + ".typ")[Chapter]"#;
        let transformer = LinkTransformer::new("html");
        let result =
            transformer.transform_source(source, Path::new("test.typ"), Path::new("/root"));
        assert!(result.is_ok());
        // The link call passes through unchanged (no transformation applied)
        let output = result.unwrap();
        assert!(output.contains("#link("));
    }
}
