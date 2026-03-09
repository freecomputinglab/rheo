use rheo_core::config::UniversalSpine;
use rheo_core::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use rheo_core::reticulate::spine::RheoSpine;
use rheo_core::world::RheoWorld;
use rheo_core::{FormatPlugin, PluginContext, Result, RheoError};
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

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        if ctx.spine.merge {
            let spine = UniversalSpine {
                title: ctx.spine.title.clone(),
                vertebrae: ctx.spine.vertebrae.clone(),
                merge: Some(true),
            };
            compile_pdf_merged_impl(&spine, &ctx.options.output, &ctx.options.root)
        } else {
            let world = ctx
                .options
                .world
                .expect("PDF single-file compile requires a world");
            compile_pdf_single_impl(world, &ctx.options.output)
        }
    }
}

fn compile_pdf_single_impl(world: &RheoWorld, output: &Path) -> Result<()> {
    info!("compiling to PDF");
    let result = typst::compile::<PagedDocument>(world);
    let document = unwrap_compilation_result(Some(world), result, None::<fn(&_) -> bool>)?;

    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;

    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}

fn compile_pdf_merged_impl(
    spine_config: &UniversalSpine,
    output_path: &Path,
    root: &Path,
) -> Result<()> {
    // Build RheoSpine with AST-transformed sources (links → labels, metadata headings injected)
    let rheo_spine = RheoSpine::build(root, Some(spine_config), "pdf")?;

    debug!(file_count = rheo_spine.source.len(), "built PDF spine");

    let concatenated_source = &rheo_spine.source[0];
    debug!(
        source_length = concatenated_source.len(),
        "concatenated sources"
    );

    // Create temporary file with concatenated source in root directory
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

    // output_format=None because links already transformed to labels by RheoSpine
    let world = RheoWorld::new(root, temp_path, None)?;

    info!(output = %output_path.display(), "compiling merged PDF");
    let result = typst::compile::<PagedDocument>(&world);
    let document = unwrap_compilation_result(Some(&world), result, None::<fn(&_) -> bool>)?;

    debug!(output = %output_path.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;

    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output_path, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

    info!(output = %output_path.display(), "successfully compiled merged PDF");
    Ok(())
}

// Re-export PDF utilities for backwards compatibility
pub use rheo_core::pdf_utils::{DocumentTitle, sanitize_label_name};
