use crate::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::world::RheoWorld;
use crate::Result;
use std::path::Path;
use tracing::info;
use typst_html::HtmlDocument;

pub fn compile_html_to_document(input: &Path, root: &Path, format_name: &str) -> Result<HtmlDocument> {
    let world = RheoWorld::new(root, input, Some(format_name))?;
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    unwrap_compilation_result(Some(&world), result, Some(html_filter))
}

pub fn compile_document_to_string(document: &HtmlDocument) -> Result<String> {
    typst_html::html(document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}
