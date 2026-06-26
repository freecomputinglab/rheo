use crate::diagnostics::{ExportErrorType, handle_export_errors};

/// Export an HtmlDocument to an HTML string.
pub fn compile_document_to_string(document: &typst_html::HtmlDocument) -> crate::Result<String> {
    use typst_html::HtmlOptions;
    typst_html::html(document, &HtmlOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}

/// Export a PagedDocument to PDF bytes.
pub fn document_to_pdf_bytes(document: &typst_layout::PagedDocument) -> crate::Result<Vec<u8>> {
    use typst_pdf::PdfOptions;
    typst_pdf::pdf(document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))
}

/// Export the compiled spine to its per-file outputs (output-path -> bytes).
///
/// Driven by Typst's bundle target internally; surfaces failures as a
/// format-neutral compilation error so nothing user-facing mentions "bundle".
pub fn export_bundle(bundle: &typst_bundle::Bundle) -> crate::Result<typst_bundle::VirtualFs> {
    typst_bundle::export(bundle, &typst_bundle::BundleOptions::default()).map_err(|errors| {
        for err in &errors {
            tracing::error!(message = %err.message, "spine export error");
        }
        crate::RheoError::Compilation {
            count: errors.len(),
            errors: errors
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    })
}
