use crate::compile::RheoCompileOptions;
use crate::config::HtmlOptions;
use crate::formats::postprocess;
use crate::world::RheoWorld;
use crate::{Result, RheoError};
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

    // Transform .typ links to target extension
    let target_ext = if xhtml { ".xhtml" } else { ".html" };
    postprocess::transform_links(&html_string, input, root, target_ext)
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
fn compile_html_impl_fresh(
    input: &Path,
    output: &Path,
    root: &Path,
    repo_root: &Path,
) -> Result<()> {
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
fn compile_html_impl(world: &RheoWorld, input: &Path, output: &Path, root: &Path) -> Result<()> {
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
    let html_string = postprocess::transform_links(&html_string, input, root, ".html")?;

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

/// Compile Typst document to HTML.
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
        Some(world) => compile_html_impl(world, &options.input, &options.output, &options.root),
        // Fresh compilation (create new world)
        None => compile_html_impl_fresh(
            &options.input,
            &options.output,
            &options.root,
            &options.repo_root,
        ),
    }
}

// ============================================================================
// Helper functions
// ============================================================================


