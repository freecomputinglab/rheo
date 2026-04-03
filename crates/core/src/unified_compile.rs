/// Unified compilation interface for rheo plugins.
///
/// This module provides consistently-named compilation functions at the
/// rheo_core top level, replacing the scattered html_compile and pdf_compile
/// submodules.
use crate::Result;
use std::path::Path;

// Re-export output types for convenience
pub use typst_html::HtmlDocument;
pub use typst_layout::PagedDocument;

// Output type aliases for clarity
pub type HtmlString = String;
pub type PdfBytes = Vec<u8>;

// ============================================================================
// HTML compilation functions
// ============================================================================

/// Compile a Typst file to an HTML document.
///
/// Creates a new RheoWorld for the given input and compiles to HtmlDocument.
pub fn compile_to_html_document(
    path: &Path,
    root: &Path,
    format_name: &str,
    plugin_library: Option<String>,
) -> Result<HtmlDocument> {
    crate::html_compile::compile_html_to_document(path, root, format_name, plugin_library)
}

/// Compile using an existing RheoWorld to an HTML document.
pub fn compile_to_html_document_with_world(world: &crate::RheoWorld) -> Result<HtmlDocument> {
    crate::html_compile::compile_html_with_world(world)
}

/// Export an HtmlDocument to an HTML string.
pub fn compile_to_html_string(document: &HtmlDocument) -> Result<HtmlString> {
    crate::html_compile::compile_document_to_string(document)
}

// ============================================================================
// PDF compilation functions
// ============================================================================

/// Compile a Typst file to a PDF document.
///
/// Creates a new RheoWorld for the given input and compiles to PagedDocument.
pub fn compile_to_pdf_document(
    path: &Path,
    root: &Path,
    format_name: Option<&str>,
    plugin_library: Option<String>,
) -> Result<PagedDocument> {
    crate::pdf_compile::compile_pdf_to_document(path, root, format_name, plugin_library)
}

/// Compile using an existing RheoWorld to a PDF document.
pub fn compile_to_pdf_document_with_world(world: &crate::RheoWorld) -> Result<PagedDocument> {
    crate::pdf_compile::compile_pdf_with_world(world)
}

/// Export a PagedDocument to PDF bytes.
pub fn compile_to_pdf_bytes(document: &PagedDocument) -> Result<PdfBytes> {
    crate::pdf_compile::document_to_pdf_bytes(document)
}
