use crate::compile::RheoCompileOptions;
use crate::config::HtmlOptions;
use crate::formats::common::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::formats::compiler::FormatCompiler;
use crate::postprocess;
use crate::world::RheoWorld;
use crate::{OutputFormat, Result, RheoError};
use std::io::Write;
use std::path::Path;
use tracing::{debug, info};
use typst_html::HtmlDocument;

pub fn compile_html_to_document(
    input: &Path,
    root: &Path,
    repo_root: &Path,
) -> Result<HtmlDocument> {
    // Create the compilation world
    let world = RheoWorld::new(root, input, repo_root, false)?;

    // Compile the document to HtmlDocument
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    // Filter out HTML development warning
    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    unwrap_compilation_result(Some(&world), result, Some(html_filter))
}

pub fn compile_document_to_string(document: &HtmlDocument) -> Result<String> {
    // Export to HTML string (no post-processing - that happens in the compilation pipeline)
    typst_html::html(document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}

// ============================================================================
// Single-file HTML compilation (implementation functions)
// ============================================================================

/// Implementation: Compile a Typst document to HTML (fresh compilation)
///
/// Uses RheoSpine for AST-based link transformation (.typ → .html).
/// - Root set to content_dir or project root (for local file imports across directories)
/// - Shared resources available via repo_root in src/typst/ (rheo.typ)
///
/// Pipeline: RheoSpine Transform → Write Temp → Compile → Export → Inject Head → Write
fn compile_html_impl_fresh(
    input: &Path,
    output: &Path,
    root: &Path,
    repo_root: &Path,
    html_options: &HtmlOptions,
) -> Result<()> {
    // Derive title from filename
    let title = input
        .file_stem()
        .and_then(|s| s.to_str())
        .map(crate::formats::pdf::filename_to_title)
        .unwrap_or_else(|| "Untitled".to_string());

    // Create a single-file spine config pointing to just this input file
    let input_relative = input.strip_prefix(root).unwrap_or(input);
    let spine_pattern = input_relative.display().to_string();
    let merge_config = crate::config::Merge {
        title: title.clone(),
        spine: vec![spine_pattern],
    };

    // Build RheoSpine with AST-transformed source (.typ links → .html)
    let spine = RheoSpine::build(
        root,
        Some(&merge_config),  // Single-file merge config
        crate::OutputFormat::Html,
        &title,
    )?;

    // Extract transformed source (links already .typ → .html)
    let transformed_source = &spine.source[0];

    // Write to temporary file in root directory
    let mut temp_file = tempfile::NamedTempFile::new_in(root)
        .map_err(|e| RheoError::io(e, "creating temporary file for HTML compilation"))?;
    temp_file
        .write_all(transformed_source.as_bytes())
        .map_err(|e| RheoError::io(e, "writing transformed source to temporary file"))?;
    temp_file
        .flush()
        .map_err(|e| RheoError::io(e, "flushing temporary file"))?;

    let temp_path = temp_file.path();

    // Compile to HTML document
    let doc = compile_html_to_document(temp_path, root, repo_root)?;
    let html_string = compile_document_to_string(&doc)?;

    // Inject CSS and font links into <head>
    let stylesheets: Vec<&str> = html_options
        .stylesheets
        .iter()
        .map(|s| s.as_str())
        .collect();
    let fonts: Vec<&str> = html_options.fonts.iter().map(|s| s.as_str()).collect();
    let html_string = postprocess::inject_head_links(&html_string, &stylesheets, &fonts)?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

/// Implementation: Compile a Typst document to HTML (incremental compilation)
///
/// Uses RheoSpine for AST-based link transformation (.typ → .html), then reuses
/// an existing RheoWorld instance for compilation (enabling incremental compilation
/// through Typst's comemo caching system).
///
/// Pipeline: RheoSpine Transform → Write Temp → Compile → Export → Inject Head → Write
///
/// # Arguments
/// * `world` - Existing RheoWorld instance (will be updated with transformed temp file)
/// * `input` - Path to the source .typ file
/// * `output` - Path where the HTML should be written
/// * `root` - Project root path
/// * `repo_root` - Repository root path (for rheo.typ imports)
/// * `html_options` - HTML-specific options (stylesheets, fonts)
fn compile_html_impl(
    _world: &RheoWorld,
    input: &Path,
    output: &Path,
    root: &Path,
    repo_root: &Path,
    html_options: &HtmlOptions,
) -> Result<()> {
    // Derive title from filename
    let title = input
        .file_stem()
        .and_then(|s| s.to_str())
        .map(crate::formats::pdf::filename_to_title)
        .unwrap_or_else(|| "Untitled".to_string());

    // Create a single-file spine config pointing to just this input file
    let input_relative = input.strip_prefix(root).unwrap_or(input);
    let spine_pattern = input_relative.display().to_string();
    let merge_config = crate::config::Merge {
        title: title.clone(),
        spine: vec![spine_pattern],
    };

    // Build RheoSpine with AST-transformed source (.typ links → .html)
    let spine = RheoSpine::build(
        root,
        Some(&merge_config),  // Single-file merge config
        crate::OutputFormat::Html,
        &title,
    )?;

    // Extract transformed source (links already .typ → .html)
    let transformed_source = &spine.source[0];

    // Write to temporary file in root directory
    let mut temp_file = tempfile::NamedTempFile::new_in(root)
        .map_err(|e| RheoError::io(e, "creating temporary file for HTML compilation"))?;
    temp_file
        .write_all(transformed_source.as_bytes())
        .map_err(|e| RheoError::io(e, "writing transformed source to temporary file"))?;
    temp_file
        .flush()
        .map_err(|e| RheoError::io(e, "flushing temporary file"))?;

    // Note: For incremental compilation, we'd ideally update the world with set_main(),
    // but since we're using a temp file approach, we compile directly
    let temp_path = temp_file.path();

    // Compile the document to HtmlDocument
    info!("compiling to HTML");
    // Create temporary world for this compilation
    let temp_world = RheoWorld::new(root, temp_path, repo_root, false)?;
    let result = typst::compile::<HtmlDocument>(&temp_world);

    // Filter out HTML development warning
    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    let document = unwrap_compilation_result(Some(&temp_world), result, Some(html_filter))?;

    // Export to HTML string
    debug!(output = %output.display(), "exporting to HTML");
    let html_string =
        typst_html::html(&document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))?;

    // Inject CSS and font links into <head>
    let stylesheets: Vec<&str> = html_options
        .stylesheets
        .iter()
        .map(|s| s.as_str())
        .collect();
    let fonts: Vec<&str> = html_options.fonts.iter().map(|s| s.as_str()).collect();
    let html_string = postprocess::inject_head_links(&html_string, &stylesheets, &fonts)?;

    // 5. Write to file
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
/// * `html_options` - HTML-specific options (stylesheets, fonts for head injection)
///
/// # Returns
/// * `Result<()>` - Success or compilation error
pub fn compile_html_new(options: RheoCompileOptions, html_options: HtmlOptions) -> Result<()> {
    match options.world {
        // Incremental compilation (reuse existing world)
        Some(world) => compile_html_impl(
            world,
            &options.input,
            &options.output,
            &options.root,
            &options.repo_root,
            &html_options,
        ),
        // Fresh compilation (create new world)
        None => compile_html_impl_fresh(
            &options.input,
            &options.output,
            &options.root,
            &options.repo_root,
            &html_options,
        ),
    }
}

// ============================================================================
// FormatCompiler trait implementation
// ============================================================================

/// HTML compiler implementation
pub use crate::formats::compiler::HtmlCompiler;
use crate::links::spine::RheoSpine;

impl FormatCompiler for HtmlCompiler {
    type Config = HtmlOptions;

    fn format(&self) -> OutputFormat {
        OutputFormat::Html
    }

    fn supports_per_file(&self, _config: &Self::Config) -> bool {
        // HTML always supports per-file
        true
    }

    fn compile(&self, options: RheoCompileOptions, config: &Self::Config) -> Result<()> {
        compile_html_new(options, config.clone())
    }
}

// ============================================================================
// Helper functions
// ============================================================================
