use anyhow::{Result, Context};
use std::path::Path;
use tracing::{debug, error, info, instrument, warn};
use typst::layout::PagedDocument;
use typst_html::HtmlDocument;
use typst_pdf::PdfOptions;

use crate::world::RheoWorld;

/// Compile a Typst document to PDF
///
/// Uses the typst library with:
/// - Root set to repository root (for imports via World)
/// - Shared resources available in src/typst/ (rheo.typ, style.csl)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
pub fn compile_pdf(input: &Path, output: &Path, root: &Path) -> Result<()> {
    // Create the compilation world
    let world = RheoWorld::new(root, input)
        .context("Failed to create compilation world")?;

    // Compile the document
    info!(input = %input.display(), "compiling to PDF");
    let result = typst::compile::<PagedDocument>(&world);

    // Print warnings
    for warning in &result.warnings {
        warn!(message = %warning.message, "compilation warning");
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for err in &errors {
                error!(message = %err.message, "compilation error");
            }
            anyhow::bail!("Compilation failed with {} error(s)", errors.len());
        }
    };

    // Export to PDF
    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|errors| {
            for err in &errors {
                error!(message = %err.message, "PDF export error");
            }
            anyhow::anyhow!("PDF export failed with {} error(s)", errors.len())
        })?;

    // Write to file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, pdf_bytes)
        .context("Failed to write PDF file")?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}

/// Compile a Typst document to HTML
///
/// Uses the typst library with:
/// - Root set to repository root (for imports via World)
/// - Shared resources available in src/typst/ (rheo.typ, style.csl)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
pub fn compile_html(input: &Path, output: &Path, root: &Path) -> Result<()> {
    // Create the compilation world
    let world = RheoWorld::new(root, input)
        .context("Failed to create compilation world")?;

    // Compile the document to HtmlDocument
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    // Print warnings
    for warning in &result.warnings {
        warn!(message = %warning.message, "compilation warning");
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for err in &errors {
                error!(message = %err.message, "compilation error");
            }
            anyhow::bail!("Compilation failed with {} error(s)", errors.len());
        }
    };

    // Export to HTML string
    debug!(output = %output.display(), "exporting to HTML");
    let html_string = typst_html::html(&document)
        .map_err(|errors| {
            for err in &errors {
                error!(message = %err.message, "HTML export error");
            }
            anyhow::anyhow!("HTML export failed with {} error(s)", errors.len())
        })?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, html_string)
        .context("Failed to write HTML file")?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}
