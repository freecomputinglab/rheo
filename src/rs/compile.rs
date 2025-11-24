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
/// * `output` - Path where the HTML should be written
#[instrument(skip_all, fields(output = %output.display()))]
pub fn compile_html_incremental(world: &RheoWorld, output: &Path) -> Result<()> {
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

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
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
fn transform_html_links(html: &str, source_file: &Path, root: &Path) -> Result<String> {
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
}
