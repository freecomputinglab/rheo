use rheo_core::{FormatPlugin, PluginContext, Result, RheoError, RheoWorld};
use std::path::Path;
use tracing::{debug, info};

pub struct PdfPlugin;

impl FormatPlugin for PdfPlugin {
    fn name(&self) -> &'static str {
        "pdf"
    }

    fn typst_library(&self) -> Option<&'static str> {
        // PDF-specific lemma function for numbered lemmas in academic documents
        Some(
            r#"
#let lemmacount = counter("lemmas")
#let lemma(it) = block(inset: 8pt, [
  #lemmacount.step()
  #strong[Lemma #context lemmacount.display()]: #it
])
"#,
        )
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        compile_pdf_bundle_impl(ctx.options.world, &ctx.options.output, ctx.spine.merge)
    }
}

/// Compile PDF using bundle API.
///
/// Uses typst::compile::<typst_bundle::Bundle>() for multi-file bundle output.
/// For merged mode, produces a single PDF. For non-merged mode, produces one PDF per document.
fn compile_pdf_bundle_impl(world: &RheoWorld, output_path: &Path, merge: bool) -> Result<()> {
    if merge {
        compile_pdf_merged_bundle(world, output_path)
    } else {
        compile_pdf_per_file_bundle(world, output_path)
    }
}

/// Compile a single merged PDF from a bundle.
///
/// The bundle entry produces a single #document() call that wraps all spine files,
/// resulting in one combined PDF output.
fn compile_pdf_merged_bundle(world: &RheoWorld, output_path: &Path) -> Result<()> {
    info!("compiling merged PDF bundle");

    // Compile and export the bundle using the core helper
    let fs = world.export_bundle()?;

    debug!(file_count = fs.len(), "exported PDF bundle");

    // For merged PDF, the bundle produces a single PDF file
    // Export it and write to the output path
    let (_filename, pdf_bytes) = fs
        .into_iter()
        .next()
        .ok_or_else(|| RheoError::invalid_data("bundle produced no output"))?;

    debug!(output = %output_path.display(), "writing merged PDF");
    std::fs::write(output_path, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

    info!(output = %output_path.display(), "successfully compiled merged PDF");
    Ok(())
}

/// Compile multiple PDFs from a bundle (one per document).
///
/// Each #document() in the bundle produces a separate PDF file.
fn compile_pdf_per_file_bundle(world: &RheoWorld, output_dir: &Path) -> Result<()> {
    info!("compiling per-file PDF bundle");

    // Compile and export the bundle using the core helper
    let fs = world.export_bundle()?;

    debug!(file_count = fs.len(), "exported PDF bundle");

    // Each document in the bundle produces a separate PDF.
    // Filter to .pdf files only: bundles that also target HTML will include HTML files
    // and assets in the export; writing those to the PDF output dir would corrupt it.
    let mut file_count = 0;
    for (filename, bytes) in fs {
        if !filename.ends_with(".pdf") {
            continue;
        }
        file_count += 1;
        let out_path = output_dir.join(filename);

        // Ensure parent directory exists
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RheoError::io(e, format!("creating output directory {}", parent.display()))
            })?;
        }

        debug!(output = %out_path.display(), "writing PDF file");
        std::fs::write(&out_path, &bytes)
            .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", out_path)))?;
    }

    info!(output = %output_dir.display(), file_count = file_count, "successfully compiled per-file PDFs");
    Ok(())
}

// Re-export PDF utilities for backwards compatibility
pub use rheo_core::pdf_utils::{DocumentTitle, sanitize_label_name};
