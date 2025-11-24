use crate::{Result, RheoError};
use crate::world::RheoWorld;
use regex::Regex;
use std::path::Path;
use tracing::{debug, error, info, instrument, warn};
use typst::layout::PagedDocument;
use typst_html::HtmlDocument;
use typst_pdf::PdfOptions;

/// Compile a Typst document to PDF
///
/// Uses the typst library with:
/// - Root set to content_dir or project root (for local file imports across directories)
/// - Shared resources available via repo_root in src/typst/ (rheo.typ, style.csl)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
pub fn compile_pdf(input: &Path, output: &Path, root: &Path, repo_root: &Path) -> Result<()> {
    // Create the compilation world
    let world = RheoWorld::new(root, input, repo_root)?;

    // Compile the document
    info!(input = %input.display(), "compiling to PDF");
    let result = typst::compile::<PagedDocument>(&world);

    // Print warnings
    for warning in &result.warnings {
        warn!(message = %warning.message, "compilation warning");
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for err in &errors {
                error!(message = %err.message, "compilation error");
            }
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export to PDF
    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|errors| {
            for err in &errors {
                error!(message = %err.message, "PDF export error");
            }
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            RheoError::PdfExport {
                count: errors.len(),
                errors: error_messages.join("\n"),
            }
        })?;

    // Write to file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}

/// Compile a Typst document to HTML
///
/// Uses the typst library with:
/// - Root set to content_dir or project root (for local file imports across directories)
/// - Shared resources available via repo_root in src/typst/ (rheo.typ, style.csl)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
pub fn compile_html(input: &Path, output: &Path, root: &Path, repo_root: &Path) -> Result<()> {
    // Create the compilation world
    let world = RheoWorld::new(root, input, repo_root)?;

    // Compile the document to HtmlDocument
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    // Print warnings (filter out known Typst HTML development warning)
    for warning in &result.warnings {
        // Skip the "html export is under active development" warning from Typst
        if warning.message.contains("html export is under active development and incomplete") {
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
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export to HTML string
    debug!(output = %output.display(), "exporting to HTML");
    let html_string = typst_html::html(&document)
        .map_err(|errors| {
            for err in &errors {
                error!(message = %err.message, "HTML export error");
            }
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            RheoError::HtmlExport {
                count: errors.len(),
                errors: error_messages.join("\n"),
            }
        })?;

    // Transform .typ links to .html links
    let html_string = transform_html_links(&html_string, input, root)?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

/// Compile a Typst document to PDF using an existing World (for watch mode).
///
/// This function reuses an existing RheoWorld instance, enabling incremental
/// compilation through Typst's comemo caching system. The World should have
/// its main file set via `set_main()` and `reset()` called before compilation.
///
/// # Arguments
/// * `world` - Existing RheoWorld instance with main file already set
/// * `output` - Path where the PDF should be written
#[instrument(skip_all, fields(output = %output.display()))]
pub fn compile_pdf_incremental(world: &RheoWorld, output: &Path) -> Result<()> {
    // Compile the document
    info!("compiling to PDF");
    let result = typst::compile::<PagedDocument>(world);

    // Print warnings
    for warning in &result.warnings {
        warn!(message = %warning.message, "compilation warning");
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for err in &errors {
                error!(message = %err.message, "compilation error");
            }
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export to PDF
    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|errors| {
            for err in &errors {
                error!(message = %err.message, "PDF export error");
            }
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            RheoError::PdfExport {
                count: errors.len(),
                errors: error_messages.join("\n"),
            }
        })?;

    // Write to file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
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
pub fn compile_html_incremental(world: &RheoWorld, input: &Path, output: &Path, root: &Path) -> Result<()> {
    // Compile the document to HtmlDocument
    info!("compiling to HTML");
    let result = typst::compile::<HtmlDocument>(world);

    // Print warnings (filter out known Typst HTML development warning)
    for warning in &result.warnings {
        // Skip the "html export is under active development" warning from Typst
        if warning.message.contains("html export is under active development and incomplete") {
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
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export to HTML string
    debug!(output = %output.display(), "exporting to HTML");
    let html_string = typst_html::html(&document)
        .map_err(|errors| {
            for err in &errors {
                error!(message = %err.message, "HTML export error");
            }
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            RheoError::HtmlExport {
                count: errors.len(),
                errors: error_messages.join("\n"),
            }
        })?;

    // Transform .typ links to .html links
    let html_string = transform_html_links(&html_string, input, root)?;

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

    let re = Regex::new(r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"#)
        .expect("invalid regex pattern");

    let result = re.replace_all(source, |caps: &regex::Captures| {
        let url = &caps[1];
        let body = &caps[2];

        // Check if this is a relative .typ link
        let is_relative_typ = url.ends_with(".typ") &&
            !url.starts_with("http://") &&
            !url.starts_with("https://") &&
            !url.starts_with("mailto:");

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
pub fn transform_html_links(html: &str, source_file: &Path, root: &Path) -> Result<String> {
    // Regex to match href="..." attributes
    // Captures the href value in group 1
    let re = Regex::new(r#"href="([^"]*)""#)
        .expect("invalid regex pattern");

    let source_dir = source_file.parent()
        .ok_or_else(|| RheoError::path(source_file, "source file has no parent directory"))?;

    let mut errors = Vec::new();
    let mut result = html.to_string();

    // Find all matches and collect them (to avoid borrowing issues)
    let matches: Vec<_> = re.captures_iter(html)
        .map(|cap| cap[1].to_string())
        .collect();

    for href in matches {
        // Skip external URLs
        if href.starts_with("http://")
            || href.starts_with("https://")
            || href.starts_with("mailto:")
            || href.starts_with("//") {
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
            let root_canonical = root.canonicalize()
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
            let new_href = href.replace(".typ", ".html");
            result = result.replace(&format!(r#"href="{}""#, href), &format!(r#"href="{}""#, new_href));
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
        let result = transform_html_links(html, &source_file, root).unwrap();

        assert_eq!(result, r#"<a href="./about.html">About</a>"#);
    }

    #[test]
    fn test_transform_html_links_preserves_external() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r#"<a href="https://example.com">Example</a> <a href="mailto:test@example.com">Email</a>"#;
        let result = transform_html_links(html, &source_file, root).unwrap();

        assert_eq!(result, html);
    }

    #[test]
    fn test_transform_html_links_preserves_fragments() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r##"<a href="#section">Section</a>"##;
        let result = transform_html_links(html, &source_file, root).unwrap();

        assert_eq!(result, html);
    }

    #[test]
    fn test_transform_html_links_missing_file_error() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let source_file = root.join("index.typ");
        fs::write(&source_file, "").unwrap();

        let html = r#"<a href="./missing.typ">Missing</a>"#;
        let result = transform_html_links(html, &source_file, root);

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
        let source = r#"See #link("./local.typ")[local] and #link("https://example.com")[external]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"See [local] and #link("https://example.com")[external]."#);
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
}
