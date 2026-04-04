use crate::Result;
use crate::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::world::RheoWorld;
use std::path::Path;
use tracing::info;
use typst::diag::SourceDiagnostic;
use typst_html::HtmlDocument;

fn is_not_html_incomplete_warning(w: &SourceDiagnostic) -> bool {
    !w.message
        .contains("html export is under active development and incomplete")
}

pub fn compile_html_to_document(
    input: &Path,
    root: &Path,
    format_name: &str,
    plugin_library: Option<String>,
) -> Result<HtmlDocument> {
    let world = RheoWorld::new(root, input, Some(format_name), plugin_library)?;
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);
    unwrap_compilation_result(Some(&world), result, Some(is_not_html_incomplete_warning))
}

/// Compile using an existing RheoWorld to an HTML document.
///
/// This function uses a pre-configured RheoWorld (with main file already set)
/// and compiles it to an HtmlDocument. Useful for per-file compilation where
/// the world is shared across multiple files.
pub fn compile_html_with_world(world: &RheoWorld) -> Result<HtmlDocument> {
    info!("compiling to HTML");
    let result = typst::compile::<HtmlDocument>(world);
    unwrap_compilation_result(Some(world), result, Some(is_not_html_incomplete_warning))
}

pub fn compile_document_to_string(document: &HtmlDocument) -> Result<String> {
    typst_html::html(document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}
