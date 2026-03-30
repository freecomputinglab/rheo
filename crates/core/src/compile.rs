use crate::Result;
use crate::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::world::RheoWorld;
use std::path::{Path, PathBuf};
use tracing::info;
use typst::diag::SourceDiagnostic;
use typst_html::HtmlDocument;
use typst_layout::PagedDocument;

/// Common compilation options used across all output formats.
///
/// This struct encapsulates the core parameters needed for any compilation:
/// - Output file (where to write the result)
/// - Root directory (for resolving imports)
/// - RheoWorld (always present for bundle mode)
///
/// # Bundle mode contract
///
/// In bundle mode, the bundle entry is a virtual file pre-populated in
/// `world.slots` (not a real path on disk). Every plugin receives a world
/// configured with the bundle entry as main. HTML and PDF plugins call
/// `typst::compile::<Bundle>(&world)` for multi-file output.
///
/// EPUB is out of scope for bundle compilation (typst-bundle has no EPUB
/// variant). The EPUB plugin creates its own per-file RheoWorld instances
/// internally and ignores `ctx.options.world`.
pub struct RheoCompileOptions<'a> {
    /// The output file path
    pub output: PathBuf,
    /// Root directory for resolving imports
    pub root: PathBuf,
    /// RheoWorld for compilation. Always present in bundle mode.
    pub world: &'a mut RheoWorld,
}

impl<'a> RheoCompileOptions<'a> {
    /// Create compilation options.
    pub fn new(
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        world: &'a mut RheoWorld,
    ) -> Self {
        Self {
            output: output.into(),
            root: root.into(),
            world,
        }
    }
}

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

pub fn document_to_pdf_bytes(document: &PagedDocument) -> Result<Vec<u8>> {
    use typst_pdf::PdfOptions;
    typst_pdf::pdf(document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))
}
