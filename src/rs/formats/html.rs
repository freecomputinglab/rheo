///! HTML compilation using Typst's HtmlDocument.
///!
///! Provides unified compile_html_new() that routes to appropriate implementation
///! based on compilation options (fresh vs incremental).

use crate::compile::RheoCompileOptions;
use crate::config::HtmlOptions;
use crate::world::RheoWorld;
use crate::{Result, RheoError};
use regex::Regex;
use std::path::Path;
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

// ============================================================================
// Single-file HTML compilation (implementation functions)
// ============================================================================

/// Implementation: Compile a Typst document to HTML (fresh compilation)
///
/// Uses the typst library with:
/// - Root set to content_dir or project root (for local file imports across directories)
/// - Shared resources available via repo_root in src/typst/ (rheo.typ)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
fn compile_html_impl_fresh(input: &Path, output: &Path, root: &Path, repo_root: &Path) -> Result<()> {
    let doc = compile_html_to_document(input, root, repo_root)?;
    let html_string = compile_document_to_string(&doc, input, root, false)?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

/// Implementation: Compile a Typst document to HTML (incremental compilation)
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
fn compile_html_impl(
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

// ============================================================================
// Unified public API
// ============================================================================

/// Compile Typst document to HTML (unified API).
///
/// Routes to the appropriate implementation based on options:
/// - Fresh compilation: compile_html_impl_fresh() (when options.world is None)
/// - Incremental compilation: compile_html_impl() (when options.world is Some)
///
/// # Arguments
/// * `options` - Compilation options (input, output, root, repo_root, world)
/// * `_html_options` - HTML-specific options (currently unused but for future extensibility)
///
/// # Returns
/// * `Result<()>` - Success or compilation error
pub fn compile_html_new(options: RheoCompileOptions, _html_options: HtmlOptions) -> Result<()> {
    match options.world {
        // Incremental compilation (reuse existing world)
        Some(world) => {
            compile_html_impl(world, &options.input, &options.output, &options.root)
        }
        // Fresh compilation (create new world)
        None => {
            compile_html_impl_fresh(&options.input, &options.output, &options.root, &options.repo_root)
        }
    }
}

// ============================================================================
// Backward compatibility wrappers (deprecated, for existing call sites)
// ============================================================================

/// Compile a Typst document to HTML (deprecated 4-parameter signature).
///
/// **Deprecated:** Use `compile_html_new()` with `RheoCompileOptions` instead.
///
/// This function is kept for backward compatibility with existing call sites in cli.rs.
#[deprecated(since = "0.1.0", note = "Use compile_html_new() with RheoCompileOptions")]
pub fn compile_html(input: &Path, output: &Path, root: &Path, repo_root: &Path) -> Result<()> {
    compile_html_impl_fresh(input, output, root, repo_root)
}

/// Compile a Typst document to HTML using incremental world (deprecated).
///
/// **Deprecated:** Use `compile_html_new()` with `RheoCompileOptions` instead.
///
/// This is a compatibility shim for existing call sites.
#[deprecated(since = "0.1.0", note = "Use compile_html_new() with RheoCompileOptions")]
pub fn compile_html_incremental(
    world: &RheoWorld,
    input: &Path,
    output: &Path,
    root: &Path,
) -> Result<()> {
    compile_html_impl(world, input, output, root)
}

// ============================================================================
// Helper functions
// ============================================================================

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
}
