use rheo_core::{
    FormatPlugin, PluginContext, Result, RheoError, RheoWorld, diagnostics::print_diagnostics,
};
use std::path::Path;
use tracing::{debug, info};
use typst::diag::Warned;
use typst_pdf::PdfOptions;

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

    // Compile the bundle using the world (which has the synthetic bundle entry as main)
    let Warned { output, warnings } = typst::compile::<typst_bundle::Bundle>(world);

    // Print warnings (ignore errors from diagnostic printing)
    let _ = print_diagnostics(world, &[], &warnings);

    let bundle = output.map_err(|errors| {
        // Print errors to stderr with proper formatting
        let _ = print_diagnostics(world, &errors, &[]);
        // Return error for error handling
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::project_config(format!(
            "bundle compilation had errors: {}",
            error_messages.join(", ")
        ))
    })?;

    // Export the bundle to get PDF files
    let bundle_options = typst_bundle::BundleOptions {
        pixel_per_pt: 144.0,
        pdf: PdfOptions::default(),
    };

    let fs = typst_bundle::export(&bundle, &bundle_options)
        .map_err(|e| RheoError::project_config(format!("bundle export failed: {:?}", e)))?;

    debug!(file_count = fs.len(), "exported PDF bundle");

    // For merged PDF, the bundle produces a single PDF file
    // Export it and write to the output path
    let (_vpath, pdf_bytes) = fs
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

    // Compile the bundle using the world
    let Warned { output, warnings } = typst::compile::<typst_bundle::Bundle>(world);

    // Print warnings (ignore errors from diagnostic printing)
    let _ = print_diagnostics(world, &[], &warnings);

    let bundle = output.map_err(|errors| {
        // Print errors to stderr with proper formatting
        let _ = print_diagnostics(world, &errors, &[]);
        // Return error for error handling
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::project_config(format!(
            "bundle compilation had errors: {}",
            error_messages.join(", ")
        ))
    })?;

    // Export the bundle to get PDF files
    let bundle_options = typst_bundle::BundleOptions {
        pixel_per_pt: 144.0,
        pdf: PdfOptions::default(),
    };

    let fs = typst_bundle::export(&bundle, &bundle_options)
        .map_err(|e| RheoError::project_config(format!("bundle export failed: {:?}", e)))?;

    debug!(file_count = fs.len(), "exported PDF bundle");

    // Each document in the bundle produces a separate PDF
    let mut file_count = 0;
    for (vpath, pdf_bytes) in fs {
        file_count += 1;
        // Get the filename from the virtual path
        let filename = vpath.get_without_slash();
        let out_path = output_dir.join(filename);

        // Ensure parent directory exists
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RheoError::io(e, format!("creating output directory {}", parent.display()))
            })?;
        }

        debug!(output = %out_path.display(), "writing PDF file");
        std::fs::write(&out_path, &pdf_bytes)
            .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", out_path)))?;
    }

    info!(output = %output_dir.display(), file_count = file_count, "successfully compiled per-file PDFs");
    Ok(())
}

// Re-export PDF utilities for backwards compatibility
pub use rheo_core::pdf_utils::{DocumentTitle, sanitize_label_name};
