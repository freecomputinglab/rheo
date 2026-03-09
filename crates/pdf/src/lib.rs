use rheo_core::{
    BuiltSpine, FormatPlugin, PluginContext, Result, RheoError, RheoWorld, SpineOptions,
    compile_pdf_to_document, compile_pdf_with_world, document_to_pdf_bytes,
};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
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
        if ctx.spine.merge {
            compile_pdf_merged_impl(&ctx.spine, &ctx.options.output, &ctx.options.root)
        } else {
            let world = ctx.options.world.ok_or_else(|| {
                RheoError::project_config(
                    "PDF per-file compile requires a world; this is a rheo bug (internal invariant violation)",
                )
            })?;
            compile_pdf_single_impl(world, &ctx.options.output)
        }
    }
}

fn compile_pdf_single_impl(world: &RheoWorld, output: &Path) -> Result<()> {
    let document = compile_pdf_with_world(world)?;

    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = document_to_pdf_bytes(&document)?;

    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}

fn compile_pdf_merged_impl(spine_config: &SpineOptions, output_path: &Path, root: &Path) -> Result<()> {
    // Build RheoSpine with AST-transformed sources (links → labels, metadata headings injected)
    let merge = spine_config.merge;
    let rheo_spine = BuiltSpine::build(root, Some(spine_config), "pdf", merge)?;

    debug!(file_count = rheo_spine.source.len(), "built PDF spine");

    let concatenated_source = &rheo_spine.source[0];
    debug!(
        source_length = concatenated_source.len(),
        "concatenated sources"
    );

    // Create temporary file with concatenated source in root directory
    let mut temp_file = NamedTempFile::new_in(root)
        .map_err(|e| RheoError::io(e, "creating temporary file for merged PDF"))?;
    temp_file
        .write_all(concatenated_source.as_bytes())
        .map_err(|e| RheoError::io(e, "writing concatenated source to temporary file"))?;
    temp_file
        .flush()
        .map_err(|e| RheoError::io(e, "flushing temporary file"))?;

    let temp_path = temp_file.path();
    debug!(temp_path = %temp_path.display(), "created temporary file");

    // output_format=None because links already transformed to labels by RheoSpine
    let plugin_library = PdfPlugin.typst_library().map(|s| s.to_string());
    let document = compile_pdf_to_document(temp_path, root, None, plugin_library)?;

    debug!(output = %output_path.display(), "exporting to PDF");
    let pdf_bytes = document_to_pdf_bytes(&document)?;

    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output_path, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

    info!(output = %output_path.display(), "successfully compiled merged PDF");
    Ok(())
}

// Re-export PDF utilities for backwards compatibility
pub use rheo_core::pdf_utils::{DocumentTitle, sanitize_label_name};
