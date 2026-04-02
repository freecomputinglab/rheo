use super::types::{LinkInfo, LinkTransform};
use crate::constants::TYP_EXT;
use crate::pdf_utils::sanitize_label_name;
use crate::reticulate::validator::is_relative_typ_link;
use crate::{HTML_EXT, Result, RheoError, XHTML_EXT};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Link transformer that converts Typst links to format-specific targets.
pub struct LinkTransformer {
    // TODO: whether or not we build labels shouldn't be done based on a format name, but based on
    // a flag set in the FormatPlugin indicating whether `merge = true` produces a PDF...
    format_name: String,
    spine: Option<Vec<PathBuf>>,
}

impl LinkTransformer {
    /// Create a new LinkTransformer for the specified output format name.
    pub(crate) fn new(format_name: &str) -> Self {
        Self {
            format_name: format_name.to_string(),
            spine: None,
        }
    }

    /// Set the spine for merged PDF compilation.
    pub fn with_spine(mut self, spine: Vec<PathBuf>) -> Self {
        self.spine = Some(spine);
        self
    }

    /// Transform source code by processing all links.
    pub fn transform_source(
        &self,
        source: &str,
        current_file: &Path,
        _project_root: &Path,
    ) -> Result<String> {
        use crate::reticulate::{parser, serializer};

        let source_obj = typst::syntax::Source::detached(source);
        let links = parser::extract_links(&source_obj);
        let transformations = self.compute_transformations(&links, current_file)?;
        let code_ranges = serializer::find_code_block_ranges(&source_obj);
        Ok(serializer::apply_transformations(
            source,
            &transformations,
            &code_ranges,
        ))
    }

    /// Compute format-specific transformations for links.
    fn compute_transformations(
        &self,
        links: &[LinkInfo],
        _current_file: &Path,
    ) -> Result<Vec<(Range<usize>, LinkTransform)>> {
        let mut transformations = Vec::new();

        let label_map: HashMap<String, String> = match (self.format_name.as_str(), &self.spine) {
            ("pdf", Some(spine)) => build_label_map(spine),
            _ => HashMap::new(),
        };

        for link in links {
            let url = &link.url;
            let filename = extract_filename(url);
            let stem = filename.strip_suffix(TYP_EXT).unwrap_or(filename);

            let transform = if is_relative_typ_link(url) {
                match (self.format_name.as_str(), &self.spine) {
                    ("pdf", None) => {
                        // Single PDF: remove links
                        LinkTransform::Remove {
                            body: link.body.clone(),
                        }
                    }
                    ("pdf", Some(_)) => {
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
                    ("html", _) => LinkTransform::ReplaceUrl {
                        new_url: url.replace(TYP_EXT, HTML_EXT),
                    },
                    ("epub", _) => LinkTransform::ReplaceUrl {
                        new_url: url.replace(TYP_EXT, XHTML_EXT),
                    },
                    // Unknown formats: passthrough
                    _ => LinkTransform::KeepOriginal,
                }
            } else {
                // External URL, fragment, or non-.typ link — always preserve
                LinkTransform::KeepOriginal
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
        }
    }

    #[test]
    fn test_pdf_single_removes_typ_links() {
        let links = vec![make_link("./file.typ", "text", 0..10)];
        let transformer = LinkTransformer::new("pdf");
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
        let transformer = LinkTransformer::new("pdf");
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
        let transformer = LinkTransformer::new("pdf").with_spine(spine);
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
        let transformer = LinkTransformer::new("pdf").with_spine(spine);
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
    fn test_unknown_format_passthrough() {
        let links = vec![make_link("./file.typ", "text", 0..10)];
        let transformer = LinkTransformer::new("unknown");
        let transforms = transformer
            .compute_transformations(&links, Path::new("test.typ"))
            .unwrap();
        assert!(matches!(transforms[0].1, LinkTransform::KeepOriginal));
    }

    #[test]
    fn test_sanitize_label_name() {
        assert_eq!(sanitize_label_name("chapter 01"), "chapter_01");
        assert_eq!(sanitize_label_name("severance-01"), "severance-01");
        assert_eq!(sanitize_label_name("my_file!@#"), "my_file___");
        assert_eq!(sanitize_label_name("test.typ"), "test_typ");
    }

    #[test]
    fn test_extract_filename() {
        assert_eq!(extract_filename("./chapter2.typ"), "chapter2.typ");
        assert_eq!(extract_filename("../parent/file.typ"), "file.typ");
        assert_eq!(extract_filename("/absolute/path.typ"), "path.typ");
        assert_eq!(extract_filename("simple.typ"), "simple.typ");
    }
}
