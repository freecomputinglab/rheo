use crate::Result;
/// PDF compilation wrappers for rheo plugins.
///
/// These functions encapsulate Typst PDF compilation, allowing plugin crates
/// to compile PDFs without directly importing typst crates.
use crate::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::world::RheoWorld;
use std::path::Path;
use tracing::info;
use typst_layout::PagedDocument;

/// Compile a Typst source file to a PDF document.
///
/// This function creates a new RheoWorld for the given input and compiles
/// it to a PagedDocument. The result can be exported to PDF bytes using
/// `document_to_pdf_bytes`.
///
/// # Arguments
/// * `input` - Path to the .typ file to compile
/// * `root` - Project root directory for resolving imports
/// * `format_name` - Output format name for link transformations (e.g., "pdf", None)
/// * `plugin_library` - Optional plugin-contributed Typst library code to inject
///
/// # Returns
/// A PagedDocument ready for PDF export
///
/// # Example
/// ```ignore
/// let document = compile_pdf_to_document(&input_path, &project_root, Some("pdf"), None)?;
/// let pdf_bytes = document_to_pdf_bytes(&document)?;
/// std::fs::write("output.pdf", &pdf_bytes)?;
/// ```
pub fn compile_pdf_to_document(
    input: &Path,
    root: &Path,
    _format_name: Option<&str>,
    plugin_library: Option<String>,
) -> Result<PagedDocument> {
    let world = RheoWorld::new(root, input, plugin_library)?;
    info!(input = %input.display(), "compiling to PDF");
    let result = typst::compile::<PagedDocument>(&world);
    unwrap_compilation_result(Some(&world), result, None::<fn(&_) -> bool>)
}

/// Compile using an existing RheoWorld to a PDF document.
///
/// This function uses a pre-configured RheoWorld (with main file already set)
/// and compiles it to a PagedDocument. Useful for per-file compilation where
/// the world is shared across multiple files.
///
/// # Arguments
/// * `world` - A configured RheoWorld with the main file set
///
/// # Returns
/// A PagedDocument ready for PDF export
///
/// # Example
/// ```ignore
/// let document = compile_pdf_with_world(&world)?;
/// let pdf_bytes = document_to_pdf_bytes(&document)?;
/// std::fs::write("output.pdf", &pdf_bytes)?;
/// ```
pub fn compile_pdf_with_world(world: &RheoWorld) -> Result<PagedDocument> {
    info!("compiling to PDF");
    let result = typst::compile::<PagedDocument>(world);
    unwrap_compilation_result(Some(world), result, None::<fn(&_) -> bool>)
}

/// Export a PagedDocument to PDF bytes.
///
/// Converts a compiled PagedDocument into its PDF representation as bytes.
/// The resulting bytes can be written directly to a file.
///
/// # Arguments
/// * `document` - The compiled PagedDocument to export
///
/// # Returns
/// PDF file content as a byte vector
///
/// # Example
/// ```ignore
/// let document = compile_pdf_to_document(&input_path, &root, Some("pdf"))?;
/// let pdf_bytes = document_to_pdf_bytes(&document)?;
/// std::fs::write("output.pdf", &pdf_bytes)?;
/// ```
pub fn document_to_pdf_bytes(document: &PagedDocument) -> Result<Vec<u8>> {
    use typst_pdf::PdfOptions;
    typst_pdf::pdf(document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))
}
