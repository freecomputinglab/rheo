use crate::config::{EpubConfig, PdfConfig};
use crate::epub::EpubItem;
use crate::world::RheoWorld;
use crate::{Result, RheoError, epub};
use regex::Regex;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, instrument, warn};
use typst_html::HtmlDocument;

pub fn compile_html_to_document(
    input: &Path,
    root: &Path,
    repo_root: &Path,
) -> Result<HtmlDocument> {
    // Create the compilation world
    // For HTML compilation, keep .typ links so we can transform them to .html
    let world = RheoWorld::new(root, input, repo_root, false)?;

    // Compile the document to HtmlDocument
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    // Print warnings (filter out known Typst HTML development warning)
    for warning in &result.warnings {
        // Skip the "html export is under active development" warning from Typst
        if warning
            .message
            .contains("html export is under active development and incomplete")
        {
            continue;
        }
        warn!(message = %warning.message, "compilation warning");
    }

    // Get the document or return errors
    result.output.map_err(|errors| {
        for err in &errors {
            error!(message = %err.message, "compilation error");
        }
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::Compilation {
            count: errors.len(),
            errors: error_messages.join("\n"),
        }
    })
}

pub fn compile_document_to_string(
    document: &HtmlDocument,
    input: &Path,
    root: &Path,
    xhtml: bool,
) -> Result<String> {
    // Export to HTML string
    let html_string = typst_html::html(document).map_err(|errors| {
        for err in &errors {
            error!(message = %err.message, "HTML export error");
        }
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::HtmlGeneration {
            count: errors.len(),
            errors: error_messages.join("\n"),
        }
    })?;

    // Transform .typ links to .html links
    transform_html_links(&html_string, input, root, xhtml)
}

/// Compile a Typst document to HTML
///
/// Uses the typst library with:
/// - Root set to content_dir or project root (for local file imports across directories)
/// - Shared resources available via repo_root in src/typst/ (rheo.typ)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
pub fn compile_html(input: &Path, output: &Path, root: &Path, repo_root: &Path) -> Result<()> {
    let doc = compile_html_to_document(input, root, repo_root)?;
    let html_string = compile_document_to_string(&doc, input, root, false)?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

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

/// Compile a Typst document to HTML using an existing World (for watch mode).
///
/// This function reuses an existing RheoWorld instance, enabling incremental
/// compilation through Typst's comemo caching system. The World should have
/// its main file set via `set_main()` and `reset()` called before compilation.
///
/// # Arguments
/// * `world` - Existing RheoWorld instance with main file already set
/// * `input` - Path to the source .typ file (for link transformation)
/// * `output` - Path where the HTML should be written
/// * `root` - Project root path (for link validation)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
pub fn compile_html_incremental(
    world: &RheoWorld,
    input: &Path,
    output: &Path,
    root: &Path,
) -> Result<()> {
    // Compile the document to HtmlDocument
    info!("compiling to HTML");
    let result = typst::compile::<HtmlDocument>(world);

    // Print warnings (filter out known Typst HTML development warning)
    for warning in &result.warnings {
        // Skip the "html export is under active development" warning from Typst
        if warning
            .message
            .contains("html export is under active development and incomplete")
        {
            continue;
        }
        warn!(message = %warning.message, "compilation warning");
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for err in &errors {
                error!(message = %err.message, "compilation error");
            }
            let error_messages: Vec<String> =
                errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export to HTML string
    debug!(output = %output.display(), "exporting to HTML");
    let html_string = typst_html::html(&document).map_err(|errors| {
        for err in &errors {
            error!(message = %err.message, "HTML export error");
        }
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::HtmlGeneration {
            count: errors.len(),
            errors: error_messages.join("\n"),
        }
    })?;

    // Transform .typ links to .html links
    let html_string = transform_html_links(&html_string, input, root, false)?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
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

/// Transform relative .typ links to .html links in HTML output,
/// validating that the linked files exist.
///
/// # Arguments
/// * `html` - The HTML string to transform
/// * `source_file` - Path to the source .typ file (for resolving relative links)
/// * `root` - Project root path (for validating file existence)
///
/// # Returns
/// * `Ok(String)` - Transformed HTML with .typ links changed to .html
/// * `Err(RheoError)` - If a linked .typ file doesn't exist
#[instrument(skip(html), fields(source = %source_file.display()))]
pub fn transform_html_links(
    html: &str,
    source_file: &Path,
    root: &Path,
    xhtml: bool,
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

            // Transform the link from .typ to .html
            let new_ext = if xhtml { ".xhtml" } else { ".html" };
            let new_href = href.replace(".typ", new_ext);
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
    fn test_transform_html_links_basic() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test files
        let source_file = root.join("index.typ");
        let target_file = root.join("about.typ");
        fs::write(&source_file, "").unwrap();
        fs::write(&target_file, "").unwrap();

        let html = r#"<a href="./about.typ">About</a>"#;
        let result = transform_html_links(html, &source_file, root, false).unwrap();

        assert_eq!(result, r#"<a href="./about.html">About</a>"#);
    }

    #[test]
    fn test_transform_html_links_preserves_external() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r#"<a href="https://example.com">Example</a> <a href="mailto:test@example.com">Email</a>"#;
        let result = transform_html_links(html, &source_file, root, false).unwrap();

        assert_eq!(result, html);
    }

    #[test]
    fn test_transform_html_links_preserves_fragments() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r##"<a href="#section">Section</a>"##;
        let result = transform_html_links(html, &source_file, root, false).unwrap();

        assert_eq!(result, html);
    }

    #[test]
    fn test_transform_html_links_missing_file_error() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r#"<a href="./missing.typ">Missing</a>"#;
        let result = transform_html_links(html, &source_file, root, false);

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
