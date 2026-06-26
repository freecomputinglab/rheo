use crate::diagnostics::{ExportErrorType, handle_export_errors};
use crate::world::RheoWorld;
use std::path::{Path, PathBuf};

/// What a single `compile()` call operates on — the per-file vs merged
/// distinction encoded in the type rather than as nullable `Option` fields.
pub enum CompileUnit {
    /// One source file, compiled through a pre-built world.
    PerFile {
        /// The input `.typ` file.
        input: PathBuf,
        /// The world the file is compiled through. Boxed because `RheoWorld` is
        /// large relative to the `Merged` variant.
        world: Box<RheoWorld>,
    },
    /// All spine files merged into one output. The plugin builds its own worlds,
    /// rooted at `root` (the content directory).
    Merged {
        /// Content root that merged spine sources and worlds resolve against.
        root: PathBuf,
    },
}

impl CompileUnit {
    /// The input `.typ` file in per-file mode, `None` when merged.
    pub fn input(&self) -> Option<&Path> {
        match self {
            CompileUnit::PerFile { input, .. } => Some(input),
            CompileUnit::Merged { .. } => None,
        }
    }
}

/// Common compilation options: the output path plus the [`CompileUnit`] that
/// determines whether this is a per-file or merged compilation.
pub struct RheoCompileOptions {
    /// The output file path.
    pub output: PathBuf,
    /// The unit being compiled (per-file with a world, or merged with a root).
    pub unit: CompileUnit,
}

impl RheoCompileOptions {
    /// Options for compiling a single file through `world`.
    pub fn per_file(
        input: impl Into<PathBuf>,
        output: impl Into<PathBuf>,
        world: RheoWorld,
    ) -> Self {
        Self {
            output: output.into(),
            unit: CompileUnit::PerFile {
                input: input.into(),
                world: Box::new(world),
            },
        }
    }

    /// Options for a merged compilation rooted at `root` (the content directory).
    pub fn merged(output: impl Into<PathBuf>, root: impl Into<PathBuf>) -> Self {
        Self {
            output: output.into(),
            unit: CompileUnit::Merged { root: root.into() },
        }
    }

    /// The input `.typ` file in per-file mode, `None` when merged.
    pub fn input(&self) -> Option<&Path> {
        self.unit.input()
    }
}

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
