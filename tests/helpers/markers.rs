/// Test marker parser for .typ files
///
/// Parses comment-based test markers that declare files as test cases
/// and provide metadata for test execution.
///
/// Marker syntax:
/// ```typst
/// // @rheo:test
/// // @rheo:formats html,pdf
/// // @rheo:description Tests blog post rendering with footnotes
/// ```
use std::path::Path;

/// Metadata extracted from test markers in .typ files
#[derive(Debug, Clone, PartialEq)]
pub struct TestMetadata {
    /// Output formats to test (html, pdf, epub)
    pub formats: Vec<String>,
    /// Human-readable description of the test
    pub description: Option<String>,
    /// Expected compilation outcome ("error" or "success", None defaults to success)
    pub expect: Option<String>,
    /// Required error patterns to check in stderr (for error cases)
    pub error_patterns: Vec<String>,
}

impl Default for TestMetadata {
    fn default() -> Self {
        Self {
            formats: vec!["html".to_string(), "pdf".to_string()],
            description: None,
            expect: None,
            error_patterns: vec![],
        }
    }
}

/// Checks if a line contains the @rheo:test marker
pub fn is_test_marker(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("//") && trimmed.contains("@rheo:test")
}

/// Parses test metadata from .typ file source
///
/// Returns Some(TestMetadata) if the file contains // @rheo:test marker,
/// otherwise returns None.
pub fn parse_test_metadata(source: &str) -> Option<TestMetadata> {
    let mut has_test_marker = false;
    let mut metadata = TestMetadata::default();

    for line in source.lines() {
        let trimmed = line.trim();

        // Must start with comment
        if !trimmed.starts_with("//") {
            continue;
        }

        let comment = trimmed.trim_start_matches("//").trim();

        // Check for @rheo:test marker
        if comment == "@rheo:test" {
            has_test_marker = true;
            continue;
        }

        // Parse @rheo:formats
        if let Some(formats_str) = comment.strip_prefix("@rheo:formats") {
            let formats_str = formats_str.trim();
            metadata.formats = formats_str
                .split(',')
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect();
            continue;
        }

        // Parse @rheo:description
        if let Some(desc) = comment.strip_prefix("@rheo:description") {
            metadata.description = Some(desc.trim().to_string());
            continue;
        }

        // Parse @rheo:expect
        if let Some(expect_str) = comment.strip_prefix("@rheo:expect") {
            let expect_value = expect_str.trim().to_string();
            if !expect_value.is_empty() {
                metadata.expect = Some(expect_value);
            }
            continue;
        }

        // Parse @rheo:error-patterns
        if let Some(patterns_str) = comment.strip_prefix("@rheo:error-patterns") {
            // Patterns are comma-separated quoted strings
            // Example: @rheo:error-patterns "error", "cannot add", "│"
            let patterns_str = patterns_str.trim();
            metadata.error_patterns = patterns_str
                .split(',')
                .map(|p| p.trim())
                .filter(|p| !p.is_empty())
                .map(|p| p.trim_matches('"').to_string()) // Remove quotes
                .collect();
            continue;
        }
    }

    if has_test_marker {
        Some(metadata)
    } else {
        None
    }
}

/// Reads test metadata from a .typ file
pub fn read_test_metadata(path: &Path) -> Option<TestMetadata> {
    let source = std::fs::read_to_string(path).ok()?;
    parse_test_metadata(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_test_marker() {
        assert!(is_test_marker("// @rheo:test"));
        assert!(is_test_marker("  // @rheo:test  "));
        assert!(!is_test_marker("@rheo:test"));
        assert!(!is_test_marker("// some other comment"));
    }

    #[test]
    fn test_parse_test_metadata_minimal() {
        let source = "// @rheo:test\n= Content";
        let metadata = parse_test_metadata(source).unwrap();
        assert_eq!(metadata.formats, vec!["html", "pdf"]);
        assert_eq!(metadata.description, None);
    }

    #[test]
    fn test_parse_test_metadata_with_formats() {
        let source = "// @rheo:test\n// @rheo:formats html\n= Content";
        let metadata = parse_test_metadata(source).unwrap();
        assert_eq!(metadata.formats, vec!["html"]);
    }

    #[test]
    fn test_parse_test_metadata_with_multiple_formats() {
        let source = "// @rheo:test\n// @rheo:formats html,pdf,epub\n= Content";
        let metadata = parse_test_metadata(source).unwrap();
        assert_eq!(metadata.formats, vec!["html", "pdf", "epub"]);
    }

    #[test]
    fn test_parse_test_metadata_with_description() {
        let source = "// @rheo:test\n// @rheo:description Tests blog post with images\n= Content";
        let metadata = parse_test_metadata(source).unwrap();
        assert_eq!(
            metadata.description,
            Some("Tests blog post with images".to_string())
        );
    }

    #[test]
    fn test_parse_test_metadata_complete() {
        let source = r#"// @rheo:test
// @rheo:formats html,pdf
// @rheo:description Main blog index page with post listings

= Blog Index
"#;
        let metadata = parse_test_metadata(source).unwrap();
        assert_eq!(metadata.formats, vec!["html", "pdf"]);
        assert_eq!(
            metadata.description,
            Some("Main blog index page with post listings".to_string())
        );
    }

    #[test]
    fn test_parse_test_metadata_no_marker() {
        let source = "= Content without markers";
        assert!(parse_test_metadata(source).is_none());
    }

    #[test]
    fn test_parse_test_metadata_ignores_non_comment_lines() {
        let source = r#"// @rheo:test
@rheo:formats html,pdf
// @rheo:formats epub
= Content
"#;
        let metadata = parse_test_metadata(source).unwrap();
        // Should only parse the comment line, not the non-comment @rheo:formats
        assert_eq!(metadata.formats, vec!["epub"]);
    }

    #[test]
    fn test_read_test_metadata_from_file() {
        // Test reading markers from an actual example file
        let path = Path::new("examples/blog_site/content/index.typ");
        let metadata = read_test_metadata(path).unwrap();
        assert_eq!(metadata.formats, vec!["html", "pdf"]);
        assert_eq!(
            metadata.description,
            Some("Main blog index page with post listings".to_string())
        );
    }

    #[test]
    fn test_read_test_metadata_pdf_only() {
        // Test reading PDF-only markers
        let path = Path::new("examples/cover-letter.typ");
        let metadata = read_test_metadata(path).unwrap();
        assert_eq!(metadata.formats, vec!["pdf"]);
        assert_eq!(
            metadata.description,
            Some("Job application cover letter with custom formatting".to_string())
        );
    }

    #[test]
    fn test_parse_test_metadata_with_expect_error() {
        let source = "// @rheo:test\n// @rheo:expect error\n= Content";
        let metadata = parse_test_metadata(source).unwrap();
        assert_eq!(metadata.expect, Some("error".to_string()));
    }

    #[test]
    fn test_parse_test_metadata_with_error_patterns() {
        let source = r#"// @rheo:test
// @rheo:expect error
// @rheo:error-patterns "error", "cannot add", "│"
= Content"#;
        let metadata = parse_test_metadata(source).unwrap();
        assert_eq!(metadata.expect, Some("error".to_string()));
        assert_eq!(
            metadata.error_patterns,
            vec!["error", "cannot add", "│"]
        );
    }

    #[test]
    fn test_parse_test_metadata_error_patterns_with_spaces() {
        let source = r#"// @rheo:test
// @rheo:error-patterns "pattern one", "pattern two"
= Content"#;
        let metadata = parse_test_metadata(source).unwrap();
        assert_eq!(
            metadata.error_patterns,
            vec!["pattern one", "pattern two"]
        );
    }
}
