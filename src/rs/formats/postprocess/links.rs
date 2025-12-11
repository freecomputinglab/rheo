//! Link transformation utilities for converting .typ references to output format links.
//!
//! This module provides shared functionality for transforming Typst file references
//! into appropriate output format links (e.g., .typ → .html or .typ → .xhtml).

use crate::{RheoError, Result};
use regex::Regex;
use std::path::Path;
use tracing::debug;

/// Transform .typ file references to target format links.
///
/// This function scans HTML/XHTML content for links to .typ files and transforms them
/// to the specified output format extension.
///
/// # Arguments
/// * `html` - The HTML/XHTML content to transform
/// * `source_file` - The source file path
/// * `root` - The project root directory
/// * `target_ext` - Target file extension (e.g., ".html" or ".xhtml")
///
/// # Returns
/// Transformed HTML with updated links
///
/// # Errors
/// Returns error if:
/// - Linked .typ files don't exist
/// - Linked files are outside the project root
pub fn transform_links(
    html: &str,
    source_file: &Path,
    root: &Path,
    target_ext: &str,
) -> Result<String> {
    // Regex to match href="..." attributes
    // Captures the href value in group 1
    let re = Regex::new(r#"href="([^"]*)""#).expect("invalid regex pattern");

    let source_dir = source_file
        .parent()
        .ok_or_else(|| RheoError::path(source_file, "source file has no parent directory"))?;

    let mut errors = Vec::new();
    let mut result = html.to_string();

    // Find all matches and collect them (to avoid borrowing issues)
    let matches: Vec<_> = re
        .captures_iter(html)
        .map(|cap| cap[1].to_string())
        .collect();

    for href in matches {
        // Skip external URLs
        if href.starts_with("http://")
            || href.starts_with("https://")
            || href.starts_with("mailto:")
            || href.starts_with("//")
        {
            continue;
        }

        // Skip fragment-only links
        if href.starts_with('#') {
            continue;
        }

        // Check if this is a .typ link
        if href.ends_with(".typ") {
            // Resolve the path relative to the source file directory
            let linked_path = if href.starts_with('/') {
                // Absolute path from root
                root.join(href.trim_start_matches('/'))
            } else {
                // Relative path from source file directory
                source_dir.join(&href)
            };

            // Normalize the path
            let linked_path = match linked_path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    // File doesn't exist, record error
                    errors.push(format!(
                        "error: file not found: {}\n  ┌─ {}:1:1\n  │\n  │ link target '{}' does not exist in the project\n  │\n  = help: ensure the file exists or remove the link",
                        href,
                        source_file.display(),
                        href
                    ));
                    continue;
                }
            };

            // Check if the resolved path is within the project
            let root_canonical = root
                .canonicalize()
                .map_err(|e| RheoError::io(e, format!("canonicalizing root path {:?}", root)))?;

            if !linked_path.starts_with(&root_canonical) {
                errors.push(format!(
                    "error: file outside project: {}\n  ┌─ {}:1:1\n  │\n  │ link target '{}' is outside the project root\n  │\n  = help: only link to files within the project",
                    href,
                    source_file.display(),
                    href
                ));
                continue;
            }

            // Transform the link from .typ to target extension
            let new_href = href.replace(".typ", target_ext);
            result = result.replace(
                &format!(r#"href="{}""#, href),
                &format!(r#"href="{}""#, new_href),
            );
            debug!(from = %href, to = %new_href, "transformed link");
        }
    }

    // If there were any errors, return them
    if !errors.is_empty() {
        return Err(RheoError::Compilation {
            count: errors.len(),
            errors: errors.join("\n\n"),
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_transform_links_to_html() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test files
        let source_file = root.join("index.typ");
        let target_file = root.join("about.typ");
        fs::write(&source_file, "").unwrap();
        fs::write(&target_file, "").unwrap();

        let html = r#"<a href="./about.typ">About</a>"#;
        let result = transform_links(html, &source_file, root, ".html").unwrap();

        assert_eq!(result, r#"<a href="./about.html">About</a>"#);
    }

    #[test]
    fn test_transform_links_to_xhtml() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test files
        let source_file = root.join("index.typ");
        let target_file = root.join("about.typ");
        fs::write(&source_file, "").unwrap();
        fs::write(&target_file, "").unwrap();

        let html = r#"<a href="./about.typ">About</a>"#;
        let result = transform_links(html, &source_file, root, ".xhtml").unwrap();

        assert_eq!(result, r#"<a href="./about.xhtml">About</a>"#);
    }

    #[test]
    fn test_transform_links_preserves_external() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r#"<a href="https://example.com">Example</a> <a href="mailto:test@example.com">Email</a>"#;
        let result = transform_links(html, &source_file, root, ".html").unwrap();

        assert_eq!(result, html);
    }

    #[test]
    fn test_transform_links_preserves_fragments() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r##"<a href="#section">Section</a>"##;
        let result = transform_links(html, &source_file, root, ".html").unwrap();

        assert_eq!(result, html);
    }

    #[test]
    fn test_transform_links_missing_file_error() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r#"<a href="./missing.typ">Missing</a>"#;
        let result = transform_links(html, &source_file, root, ".html");

        assert!(result.is_err());
        match result {
            Err(RheoError::Compilation { count, errors }) => {
                assert_eq!(count, 1);
                assert!(errors.contains("file not found"));
                assert!(errors.contains("missing.typ"));
            }
            _ => panic!("expected Compilation error"),
        }
    }
}
