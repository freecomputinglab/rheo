use crate::compile::RheoCompileOptions;
use crate::config::PdfConfig;
use crate::constants::TYPST_LABEL_PATTERN;
use crate::formats::common::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::links::spine::RheoSpine;
use crate::world::RheoWorld;
use crate::{OutputFormat, Result, RheoError};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
use tracing::{debug, info};
use typst::layout::PagedDocument;
use typst_pdf::PdfOptions;

// ============================================================================
// Single-file PDF compilation (implementation functions)
// ============================================================================

/// Implementation: Compile a single Typst document to PDF (fresh compilation)
///
/// Uses format-aware RheoWorld for automatic link transformation (removes .typ links).
/// Transformations happen on-demand during Typst compilation (including imports).
///
/// Pipeline: Compile (with transformations) → Export → Write
fn compile_pdf_single_impl_fresh(
    input: &Path,
    output: &Path,
    root: &Path,
) -> Result<()> {
    // Create format-aware world (handles link removal on import)
    let world = RheoWorld::new(root, input, Some(OutputFormat::Pdf))?;

    // Compile the document
    info!(input = %input.display(), "compiling to PDF");
    let result = typst::compile::<PagedDocument>(&world);
    let document = unwrap_compilation_result(Some(&world), result, None::<fn(&_) -> bool>)?;

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

/// Implementation: Compile a single Typst document to PDF (incremental compilation)
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
// Helper functions for merged PDF compilation
// ============================================================================

/// Sanitize a filename to create a valid Typst label name.
///
/// Replaces non-alphanumeric characters (except hyphens and underscores) with underscores.
pub fn sanitize_label_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Convert filename to readable title.
///
/// Transforms a filename stem into a human-readable title by replacing
/// separators with spaces and capitalizing words.
pub fn filename_to_title(filename: &str) -> String {
    filename
        .replace(['-', '_'], " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Strip basic Typst markup to get plain text.
///
/// Removes common Typst markup patterns like #emph[...], #strong[...],
/// and italic markers (_) to extract plain text from formatted content.
fn strip_typst_markup(text: &str) -> String {
    // Remove #emph[...], #strong[...], etc.
    let result = TYPST_LABEL_PATTERN.replace_all(text, "$1");

    // Remove underscores (italic markers)
    let result = result.replace('_', "");

    result.trim().to_string()
}

/// Extract title from Typst document source.
///
/// Searches for `#set document(title: [...])` and extracts the content.
/// Falls back to filename if no title is found. The extracted title is
/// cleaned of basic Typst markup.
pub fn extract_document_title(source: &str, filename: &str) -> String {
    // Find the start of the title parameter
    if let Some(title_start) = source.find("#set document(") {
        let after_doc = &source[title_start..];
        if let Some(title_pos) = after_doc.find("title:") {
            let after_title = &after_doc[title_pos + 6..]; // Skip "title:"

            // Find the opening bracket for the title
            if let Some(bracket_start) = after_title.find('[') {
                let title_content = &after_title[bracket_start + 1..];

                // Count brackets to find the matching closing bracket
                let mut depth = 1;
                let mut end_pos = 0;

                for (i, ch) in title_content.chars().enumerate() {
                    if ch == '[' {
                        depth += 1;
                    } else if ch == ']' {
                        depth -= 1;
                        if depth == 0 {
                            end_pos = i;
                            break;
                        }
                    }
                }

                if end_pos > 0 {
                    let title = &title_content[..end_pos];
                    // Strip Typst markup for plain text
                    let cleaned = strip_typst_markup(title);
                    if !cleaned.trim().is_empty() {
                        return cleaned;
                    }
                }
            }
        }
    }

    // Fallback: use filename, convert to title case
    filename_to_title(filename)
}

// ============================================================================
// Merged PDF compilation (implementation functions)
// ============================================================================

/// Implementation: Compile multiple Typst files into a single merged PDF (fresh compilation)
///
/// Generates a spine from the PDF merge configuration, concatenates all sources
/// with labels and transformed links, then compiles to a single PDF document.
fn compile_pdf_merged_impl_fresh(
    config: &PdfConfig,
    output_path: &Path,
    root: &Path,
) -> Result<()> {
    let merge = config.merge.as_ref().ok_or_else(|| {
        RheoError::project_config("PDF merge configuration required for merged compilation")
    })?;

    // Build RheoSpine with AST-transformed sources (links → labels, metadata headings injected)
    let rheo_spine = RheoSpine::build(root, Some(merge), crate::OutputFormat::Pdf, &merge.title)?;

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

/// Implementation: Compile multiple Typst files into a single merged PDF (incremental compilation)
///
/// Same as compile_pdf_merged_impl_fresh() but reuses an existing RheoWorld for faster
/// recompilation in watch mode.
fn compile_pdf_merged_impl(
    world: &mut RheoWorld,
    config: &PdfConfig,
    output_path: &Path,
    root: &Path,
) -> Result<()> {
    let merge = config.merge.as_ref().ok_or_else(|| {
        RheoError::project_config("PDF merge configuration required for merged compilation")
    })?;

    // Build RheoSpine with AST-transformed sources (links → labels, metadata headings injected)
    let rheo_spine = RheoSpine::build(root, Some(merge), crate::OutputFormat::Pdf, &merge.title)?;

    debug!(file_count = rheo_spine.source.len(), "built PDF spine");

    // Extract concatenated source (already merged into single source)
    let concatenated_source = &rheo_spine.source[0];
    debug!(
        source_length = concatenated_source.len(),
        "concatenated sources"
    );

    // Create temporary file with concatenated source in the root directory
    // (Typst compiler requires main file to be within root)
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

    // Set main file in existing world
    world.set_main(temp_path)?;

    // Compile to PagedDocument
    info!("compiling merged PDF");
    let result = typst::compile::<PagedDocument>(world);
    let document = unwrap_compilation_result(Some(world), result, None::<fn(&_) -> bool>)?;

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
/// Routes to the appropriate implementation based on options:
/// - Single file, fresh: compile_pdf_single_impl_fresh()
/// - Single file, incremental: compile_pdf_single_impl()
/// - Merged PDF, fresh: compile_pdf_merged_impl_fresh()
/// - Merged PDF, incremental: compile_pdf_merged_impl()
///
/// # Arguments
/// * `options` - Compilation options (input, output, root, repo_root, world)
/// * `pdf_config` - Optional PDF merge configuration (None for single-file)
///
/// # Returns
/// * `Result<()>` - Success or compilation error
pub fn compile_pdf_new(options: RheoCompileOptions, pdf_config: Option<&PdfConfig>) -> Result<()> {
    // Check if this is merged PDF compilation
    let is_merged = pdf_config.and_then(|c| c.merge.as_ref()).is_some();

    match (is_merged, options.world) {
        // Merged PDF, incremental
        (true, Some(world)) => {
            let config = pdf_config.ok_or_else(|| {
                RheoError::project_config("PDF config required for merged compilation")
            })?;
            compile_pdf_merged_impl(world, config, &options.output, &options.root)
        }
        // Merged PDF, fresh
        (true, None) => {
            let config = pdf_config.ok_or_else(|| {
                RheoError::project_config("PDF config required for merged compilation")
            })?;
            compile_pdf_merged_impl_fresh(
                config,
                &options.output,
                &options.root,
            )
        }
        // Single file, incremental
        (false, Some(world)) => compile_pdf_single_impl(world, &options.output),
        // Single file, fresh
        (false, None) => compile_pdf_single_impl_fresh(
            &options.input,
            &options.output,
            &options.root,
        ),
    }
}
