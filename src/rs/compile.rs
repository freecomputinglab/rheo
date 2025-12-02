use crate::config::{EpubConfig, PdfConfig};
use crate::epub::EpubItem;
use crate::{Result, RheoError, epub};
use regex::Regex;
use std::path::{Path, PathBuf};
use tracing::{info, instrument, warn};

/// Compile a set of Typst documents to EPUB.
pub fn compile_epub(
    config: &EpubConfig,
    epub_path: &Path,
    root: &Path,
    repo_root: &Path,
) -> Result<()> {
    let inner = || -> anyhow::Result<()> {
        let spine = epub::generate_spine(root, config)?;

        let mut items = spine
            .into_iter()
            .map(|path| EpubItem::create(path, root, repo_root))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let nav_xhtml = epub::generate_nav_xhtml(&mut items);
        let package_string = epub::generate_package(&items, config)?;
        epub::zip_epub(epub_path, package_string, nav_xhtml, &items)
    };

    inner().map_err(|e| RheoError::EpubGeneration {
        count: 1,
        errors: e.to_string(),
    })?;

    info!(output = %epub_path.display(), "successfully generated EPUB");
    Ok(())
}

/// Generates the PDF spine as a list of canonicalized paths to .typ files.
///
/// Processes glob patterns from [pdf.merge] config and returns an ordered list.
/// Pattern order is preserved, with lexicographic sorting within each pattern.
///
/// # Errors
/// Returns error if:
/// - No merge config is specified
/// - No .typ files matched the patterns
pub fn generate_pdf_spine(root: &Path, config: &PdfConfig) -> Result<Vec<PathBuf>> {
    crate::spine::generate_spine(root, config.merge.as_ref(), true)
}

/// Remove relative .typ links from Typst source code for PDF/EPUB compilation.
///
/// For PDF and EPUB outputs, relative links to other .typ files don't make sense
/// (yet - in the future they may become document anchors for multi-file PDFs).
/// This function removes those links while preserving the link text.
///
/// # Arguments
/// * `source` - The Typst source code
///
/// # Returns
/// * `String` - Source code with relative .typ links removed
///
/// # Examples
/// ```
/// # use rheo::compile::remove_relative_typ_links;
/// let source = r#"See #link("./other.typ")[the other page] for details."#;
/// let result = remove_relative_typ_links(source);
/// assert_eq!(result, r#"See [the other page] for details."#);
/// ```
///
/// # Note
/// External URLs (http://, https://, etc.) are preserved unchanged.
///
/// # TODO
/// When multi-file PDF compilation is implemented, relative links should
/// become document anchors instead of being removed.
#[instrument(skip(source))]
pub fn remove_relative_typ_links(source: &str) -> String {
    // Regex to match Typst link function calls
    // Captures: #link("url")[body] or #link("url", body)
    // We need to handle:
    // 1. #link("./file.typ")[text] -> [text]
    // 2. #link("../dir/file.typ")[text] -> [text]
    // 3. #link("/abs/path.typ")[text] -> [text]
    // 4. #link("https://example.com")[text] -> preserve

    let re =
        Regex::new(r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"#).expect("invalid regex pattern");

    let result = re.replace_all(source, |caps: &regex::Captures| {
        let url = &caps[1];
        let body = &caps[2];

        // Check if this is a relative .typ link
        let is_relative_typ = url.ends_with(".typ")
            && !url.starts_with("http://")
            && !url.starts_with("https://")
            && !url.starts_with("mailto:");

        if is_relative_typ {
            // Remove the link, keep just the body
            if body.starts_with('[') {
                // #link("url")[body] -> [body]
                body.to_string()
            } else {
                // #link("url", body) -> body (without comma)
                body.trim_start_matches(',').trim().to_string()
            }
        } else {
            // Preserve the full link for external URLs
            format!("#link(\"{}\"){}", url, body)
        }
    });

    result.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_relative_typ_links_basic() {
        let source = r#"See #link("./other.typ")[the other page] for details."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"See [the other page] for details."#);
    }

    #[test]
    fn test_remove_relative_typ_links_parent_dir() {
        let source = r#"Check #link("../parent/file.typ")[parent file]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"Check [parent file]."#);
    }

    #[test]
    fn test_remove_relative_typ_links_absolute() {
        let source = r#"See #link("/absolute/path.typ")[absolute]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"See [absolute]."#);
    }

    #[test]
    fn test_remove_relative_typ_links_preserves_external() {
        let source = r#"Visit #link("https://example.com")[our website] or #link("mailto:test@example.com")[email us]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, source); // Should be unchanged
    }

    #[test]
    fn test_remove_relative_typ_links_mixed() {
        let source =
            r#"See #link("./local.typ")[local] and #link("https://example.com")[external]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(
            result,
            r#"See [local] and #link("https://example.com")[external]."#
        );
    }

    #[test]
    fn test_remove_relative_typ_links_multiple() {
        let source = r#"#link("./one.typ")[First], #link("./two.typ")[Second], and #link("./three.typ")[Third]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"[First], [Second], and [Third]."#);
    }

    #[test]
    fn test_remove_relative_typ_links_preserves_non_typ() {
        let source = r#"Download #link("./file.pdf")[the PDF] here."#;
        let result = remove_relative_typ_links(source);
        // .pdf files should be preserved since they're not .typ files
        assert_eq!(result, source);
    }

    #[test]
    fn test_generate_pdf_spine_basic() {
        use crate::config::{Merge, PdfConfig};

        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test files
        fs::write(root.join("chapter1.typ"), "content").unwrap();
        fs::write(root.join("chapter2.typ"), "content").unwrap();
        fs::write(root.join("chapter3.typ"), "content").unwrap();

        let config = PdfConfig {
            merge: Some(Merge {
                title: "Test".to_string(),
                spine: vec!["chapter*.typ".to_string()],
            }),
        };

        let result = generate_pdf_spine(root, &config).unwrap();
        assert_eq!(result.len(), 3);

        // Check lexicographic ordering
        assert!(result[0].ends_with("chapter1.typ"));
        assert!(result[1].ends_with("chapter2.typ"));
        assert!(result[2].ends_with("chapter3.typ"));
    }

    #[test]
    fn test_generate_pdf_spine_ordered_patterns() {
        use crate::config::{Merge, PdfConfig};

        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test files
        fs::write(root.join("intro.typ"), "content").unwrap();
        fs::write(root.join("chapter1.typ"), "content").unwrap();
        fs::write(root.join("chapter2.typ"), "content").unwrap();
        fs::write(root.join("conclusion.typ"), "content").unwrap();

        let config = PdfConfig {
            merge: Some(Merge {
                title: "Test".to_string(),
                spine: vec![
                    "intro.typ".to_string(),
                    "chapter*.typ".to_string(),
                    "conclusion.typ".to_string(),
                ],
            }),
        };

        let result = generate_pdf_spine(root, &config).unwrap();
        assert_eq!(result.len(), 4);

        // Check pattern order is preserved
        assert!(result[0].ends_with("intro.typ"));
        assert!(result[1].ends_with("chapter1.typ"));
        assert!(result[2].ends_with("chapter2.typ"));
        assert!(result[3].ends_with("conclusion.typ"));
    }

    #[test]
    fn test_generate_pdf_spine_no_merge_config() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let config = PdfConfig { merge: None };

        let result = generate_pdf_spine(root, &config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("merge configuration required"));
    }

    #[test]
    fn test_generate_pdf_spine_empty() {
        use crate::config::{Merge, PdfConfig};

        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create non-typ file
        fs::write(root.join("readme.md"), "content").unwrap();

        let config = PdfConfig {
            merge: Some(Merge {
                title: "Test".to_string(),
                spine: vec!["*.typ".to_string()],
            }),
        };

        let result = generate_pdf_spine(root, &config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("merge spine matched no .typ files"));
    }
}
