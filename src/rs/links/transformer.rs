use super::types::{LinkInfo, LinkTransform};
use crate::constants::TYP_EXT;
use crate::formats::pdf::sanitize_label_name;
use crate::links::validator::is_relative_typ_link;
use crate::{HTML_EXT, OutputFormat, Result, RheoError, XHTML_EXT};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Compute format-specific transformations for links
///
/// Returns a vector of (byte_range, transformation) tuples where:
/// - byte_range: Location of the link in the source
/// - transformation: What to do with the link
///
/// # Arguments
/// * `links` - Links extracted from the source via AST parsing
/// * `format` - Target output format determining transformation behavior
/// * `spine` - For PdfMerged only: list of files in the spine
/// * `current_file` - Current file being transformed (for error messages)
pub fn compute_transformations(
    links: &[LinkInfo],
    format: OutputFormat,
    spine: Option<&[PathBuf]>,
    _current_file: &Path,
) -> Result<Vec<(Range<usize>, LinkTransform)>> {
    let mut transformations = Vec::new();

    // For Pdf (merged mode), build a map of filename stems to labels
    let label_map = if format == OutputFormat::Pdf && spine.is_some() {
        build_label_map(spine.unwrap_or(&[]))
    } else {
        HashMap::new()
    };

    for link in links {
        let url = &link.url;
        let filename = extract_filename(url);
        let stem = filename.strip_suffix(TYP_EXT).unwrap_or(filename);

        // Determine transformation based on format and link type
        let transform = if is_relative_typ_link(url) {
            // Relative .typ link transformation according to format
            match format {
                OutputFormat::Pdf if spine.is_none() => {
                    // Single PDF: remove links
                    LinkTransform::Remove {
                        body: link.body.clone(),
                    }
                }
                OutputFormat::Pdf => {
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
                OutputFormat::Html => {
                    // HTML: convert .typ to .html
                    LinkTransform::ReplaceUrl {
                        new_url: url.replace(TYP_EXT, HTML_EXT),
                    }
                }
                OutputFormat::Epub => {
                    // EPUB: convert .typ to .xhtml
                    LinkTransform::ReplaceUrl {
                        new_url: url.replace(TYP_EXT, XHTML_EXT),
                    }
                }
            }
        } else {
            // External URL, fragment, or non-.typ link - always preserve
            LinkTransform::KeepOriginal
        };

        transformations.push((link.byte_range.clone(), transform));
    }

    Ok(transformations)
}

/// Build a map of filename stems to sanitized labels for merged PDF compilation
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

/// Extract the filename from a path (handles both relative and absolute paths)
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
        // PDF without spine = single file mode (removes links)
        let transforms =
            compute_transformations(&links, OutputFormat::Pdf, None, Path::new("test.typ"))
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
        // PDF without spine = single file mode
        let transforms =
            compute_transformations(&links, OutputFormat::Pdf, None, Path::new("test.typ"))
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
        // PDF with spine = merged mode (converts to labels)
        let transforms = compute_transformations(
            &links,
            OutputFormat::Pdf,
            Some(&spine),
            Path::new("chapter1.typ"),
        )
        .unwrap();

        assert_eq!(transforms.len(), 1);
        match &transforms[0].1 {
            LinkTransform::ReplaceUrlWithLabel { new_label } => assert_eq!(new_label, "<chapter2>"),
            _ => panic!("Expected ReplaceUrlWithLabel transform"),
        }
    }

    #[test]
    fn test_pdf_merged_errors_on_missing_spine_file() {
        let links = vec![make_link("./missing.typ", "missing", 0..10)];
        let spine = vec![PathBuf::from("chapter1.typ")];
        // PDF with spine = merged mode
        let result = compute_transformations(
            &links,
            OutputFormat::Pdf,
            Some(&spine),
            Path::new("chapter1.typ"),
        );

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
        let transforms =
            compute_transformations(&links, OutputFormat::Html, None, Path::new("test.typ"))
                .unwrap();

        assert_eq!(transforms.len(), 2);
        // First link (.typ) should be transformed to .html
        match &transforms[0].1 {
            LinkTransform::ReplaceUrl { new_url } => assert_eq!(new_url, "./file.html"),
            _ => panic!("Expected ReplaceUrl transform for .typ link"),
        }
        // Second link (external) should be kept as-is
        assert!(matches!(transforms[1].1, LinkTransform::KeepOriginal));
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
