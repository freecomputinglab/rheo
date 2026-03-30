use crate::Result;
use crate::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::world::RheoWorld;
use std::path::Path;
use tracing::info;
use typst::diag::SourceDiagnostic;
use typst_html::HtmlDocument;

pub fn compile_html_to_document(
    input: &Path,
    root: &Path,
    _format_name: &str,
    plugin_library: Option<String>,
) -> Result<HtmlDocument> {
    compile_html_to_document_with_polyfill(input, root, plugin_library, false)
}

/// Compile to HTML document with optional EPUB polyfill mode.
pub fn compile_html_to_document_with_polyfill(
    input: &Path,
    root: &Path,
    plugin_library: Option<String>,
    epub_polyfill_mode: bool,
) -> Result<HtmlDocument> {
    let mut world = RheoWorld::new(root, input, plugin_library)?;
    world.epub_polyfill_mode = epub_polyfill_mode;
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    let html_filter = |w: &SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    unwrap_compilation_result(Some(&world), result, Some(html_filter))
}

pub fn compile_document_to_string(document: &HtmlDocument) -> Result<String> {
    typst_html::html(document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}
