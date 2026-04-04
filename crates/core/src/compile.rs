use crate::diagnostics::{ExportErrorType, handle_export_errors};
use crate::world::RheoWorld;
use std::path::PathBuf;

/// Common compilation options used across all output formats.
///
/// This struct encapsulates the core parameters needed for any compilation:
/// - Input file (the .typ file to compile, or `None` for merged/spine compilation)
/// - Output file (where to write the result)
/// - Root directory (for resolving imports)
/// - RheoWorld (`Some` in single-file mode, `None` in merged/spine mode)
///
/// # Merged mode contract
/// For merged plugins (e.g. PDF spine, EPUB), `input` is `None` and `world` is
/// `None`. Use `ctx.spine` to locate the files to compile; the plugin creates
/// its own worlds per spine file.
pub struct RheoCompileOptions<'a> {
    /// The input .typ file to compile, or `None` in merged/spine mode.
    pub input: Option<PathBuf>,
    /// The output file path
    pub output: PathBuf,
    /// Root directory for resolving imports
    pub root: PathBuf,
    /// RheoWorld for compilation. `Some` in single-file mode; `None` in merged/spine mode.
    pub world: Option<&'a mut RheoWorld>,
}

impl<'a> RheoCompileOptions<'a> {
    /// Create compilation options.
    ///
    /// # Arguments
    /// * `input` - The input .typ file, or `None` for merged/spine compilation
    /// * `output` - The output file path
    /// * `root` - Root directory for resolving imports
    /// * `world` - `Some` with the RheoWorld in single-file mode, `None` in merged/spine mode
    pub fn new(
        input: Option<impl Into<PathBuf>>,
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        world: Option<&'a mut RheoWorld>,
    ) -> Self {
        Self {
            input: input.map(Into::into),
            output: output.into(),
            root: root.into(),
            world,
        }
    }
}

/// Export an HtmlDocument to an HTML string.
pub fn compile_document_to_string(document: &typst_html::HtmlDocument) -> crate::Result<String> {
    typst_html::html(document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}

/// Export a PagedDocument to PDF bytes.
pub fn document_to_pdf_bytes(document: &typst::layout::PagedDocument) -> crate::Result<Vec<u8>> {
    use typst_pdf::PdfOptions;
    typst_pdf::pdf(document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))
}
