use rheo_core::compile::RheoCompileOptions;
use rheo_core::config::PdfConfig;
use rheo_core::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use rheo_core::reticulate::spine::RheoSpine;
use rheo_core::world::RheoWorld;
use rheo_core::{FormatPlugin, OpenHandle, OutputFormat, PluginContext, Result, RheoError};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
use tracing::{debug, info};
use typst::layout::PagedDocument;
use typst_pdf::PdfOptions;

pub struct PdfPlugin;

impl FormatPlugin for PdfPlugin {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn open(&self, output_dir: &Path, _format_name: &str) -> Result<OpenHandle> {
        rheo_core::open_all_files_in_folder(output_dir.to_path_buf(), "pdf")?;
        Ok(OpenHandle::Direct)
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        let pdf_config = if ctx.plugin_config.spine.merge {
            Some(&ctx.project.config.pdf)
        } else {
            None
        };
        compile_pdf_new(ctx.options, pdf_config)
    }
}

// ============================================================================
// Single-file PDF compilation (implementation functions)
// ============================================================================

/// Implementation: Compile a single Typst document to PDF.
///
/// Uses format-aware RheoWorld for automatic link transformation (removes .typ links).
/// Transformations happen on-demand during Typst compilation (including imports).
/// The engine provides the World, handling fresh vs incremental compilation.
///
/// Pipeline: Compile (with transformations) → Export → Write
fn compile_pdf_single_impl(world: &RheoWorld, output: &Path) -> Result<()> {
    // Compile the document
    info!("compiling to PDF");
    let result = typst::compile::<PagedDocument>(world);
    let document = unwrap_compilation_result(Some(world), result, None::<fn(&_) -> bool>)?;

    // Export to PDF
    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;

    // Write to file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}

// ============================================================================
// Merged PDF compilation (implementation functions)
// ============================================================================

/// Implementation: Compile multiple Typst files into a single merged PDF.
///
/// Generates a spine from the PDF spine configuration, concatenates all sources
/// with labels and transformed links, then compiles to a single PDF document.
/// The engine provides the World, handling fresh vs incremental compilation.
fn compile_pdf_merged_impl(
    config: &PdfConfig,
    output_path: &Path,
    root: &Path,
) -> Result<()> {
    let merge = config.spine.as_ref().ok_or_else(|| {
        RheoError::project_config("PDF spine configuration required for merged compilation")
    })?;

    // Build RheoSpine with AST-transformed sources (links → labels, metadata headings injected)
    let spine_config: &dyn rheo_core::config::SpineConfig = merge;
    let rheo_spine = RheoSpine::build(root, Some(spine_config), OutputFormat::Pdf)?;

    debug!(file_count = rheo_spine.source.len(), "built PDF spine");

    // Extract concatenated source (already merged into single source)
    let concatenated_source = &rheo_spine.source[0];
    debug!(
        source_length = concatenated_source.len(),
        "concatenated sources"
    );

    // Create temporary file with concatenated source in the root directory
    // (Typst compiler requires main file to be within root for imports)
    let mut temp_file = NamedTempFile::new_in(root)
        .map_err(|e| RheoError::io(e, "creating temporary file for merged PDF"))?;
    temp_file
        .write_all(concatenated_source.as_bytes())
        .map_err(|e| RheoError::io(e, "writing concatenated source to temporary file"))?;
    temp_file
        .flush()
        .map_err(|e| RheoError::io(e, "flushing temporary file"))?;

    let temp_path = temp_file.path();
    debug!(temp_path = %temp_path.display(), "created temporary file");

    // Create RheoWorld with temp file as main
    // output_format=None because links already transformed to labels by RheoSpine
    let world = RheoWorld::new(root, temp_path, None)?;

    // Compile to PagedDocument
    info!(output = %output_path.display(), "compiling merged PDF");
    let result = typst::compile::<PagedDocument>(&world);
    let document = unwrap_compilation_result(Some(&world), result, None::<fn(&_) -> bool>)?;

    // Export PDF bytes
    // Note: PDF title is set via document metadata in Typst source, not PdfOptions
    debug!(output = %output_path.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;

    // Write to output file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output_path, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

    info!(output = %output_path.display(), "successfully compiled merged PDF");
    Ok(())
}

// ============================================================================
// Unified public API
// ============================================================================

/// Compile Typst document(s) to PDF.
///
/// The engine provides the World, handling fresh vs incremental compilation.
/// This is a single code path - the engine creates and manages the World lifecycle.
///
/// # Arguments
/// * `options` - Compilation options (input, output, root, world)
/// * `pdf_config` - Optional PDF spine configuration (None for single-file)
///
/// # Returns
/// * `Result<()>` - Success or compilation error
pub fn compile_pdf_new(options: RheoCompileOptions, pdf_config: Option<&PdfConfig>) -> Result<()> {
    // Check if this is merged PDF compilation (spine with merge = true)
    let is_merged = pdf_config
        .and_then(|c| c.spine.as_ref())
        .and_then(|s| s.merge)
        .unwrap_or(false);

    if is_merged {
        let config = pdf_config.ok_or_else(|| {
            RheoError::project_config("PDF config required for merged compilation")
        })?;
        compile_pdf_merged_impl(config, &options.output, &options.root)
    } else {
        compile_pdf_single_impl(options.world, &options.output)
    }
}

// Re-export PDF utilities for backwards compatibility
pub use rheo_core::pdf_utils::{DocumentTitle, sanitize_label_name};
