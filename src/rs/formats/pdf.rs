///! PDF compilation using Typst's PagedDocument.
///!
///! Provides compile_pdf() for single-file compilation and
///! compile_pdf_incremental() for watch mode.

use crate::world::RheoWorld;
use crate::{Result, RheoError};
use std::path::Path;
use tracing::{debug, error, info, instrument, warn};
use typst::layout::PagedDocument;
use typst_pdf::PdfOptions;

/// Compile a Typst document to PDF
///
/// Uses the typst library with:
/// - Root set to content_dir or project root (for local file imports across directories)
/// - Shared resources available via repo_root in src/typst/ (rheo.typ)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
pub fn compile_pdf(input: &Path, output: &Path, root: &Path, repo_root: &Path) -> Result<()> {
    // Create the compilation world
    // For standalone PDF compilation, remove relative .typ links from source
    let world = RheoWorld::new(root, input, repo_root, true)?;

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
            let error_messages: Vec<String> =
                errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export to PDF
    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default()).map_err(|errors| {
        for err in &errors {
            error!(message = %err.message, "PDF export error");
        }
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::PdfGeneration {
            count: errors.len(),
            errors: error_messages.join("\n"),
        }
    })?;

    // Write to file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}

pub fn compile_pdf_incremental(world: &RheoWorld, output: &Path) -> Result<()> {
    // Compile the document
    info!("compiling to PDF");
    let result = typst::compile::<PagedDocument>(world);

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
            let error_messages: Vec<String> =
                errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export to PDF
    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default()).map_err(|errors| {
        for err in &errors {
            error!(message = %err.message, "PDF export error");
        }
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::PdfGeneration {
            count: errors.len(),
            errors: error_messages.join("\n"),
        }
    })?;

    // Write to file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}
