use anyhow::{Result, Context};
use std::path::Path;
use typst::layout::PagedDocument;
use typst_html::HtmlDocument;
use typst_pdf::PdfOptions;

use crate::world::RheoWorld;

/// Compile a Typst document to PDF
///
/// Uses the typst library with:
/// - Root set to repository root (for imports via World)
/// - Shared resources available in src/typst/ (rheo.typ, style.csl)
pub fn compile_pdf(input: &Path, output: &Path, root: &Path) -> Result<()> {
    // Create the compilation world
    let world = RheoWorld::new(root, input)
        .context("Failed to create compilation world")?;

    // Compile the document
    eprintln!("Compiling {} to PDF...", input.display());
    let result = typst::compile::<PagedDocument>(&world);

    // Print warnings
    for warning in &result.warnings {
        eprintln!("Warning: {}", warning.message);
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for error in &errors {
                eprintln!("Error: {}", error.message);
            }
            anyhow::bail!("Compilation failed with {} error(s)", errors.len());
        }
    };

    // Export to PDF
    eprintln!("Exporting to {}...", output.display());
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|errors| {
            for error in &errors {
                eprintln!("PDF export error: {}", error.message);
            }
            anyhow::anyhow!("PDF export failed with {} error(s)", errors.len())
        })?;

    // Write to file
    std::fs::write(output, pdf_bytes)
        .context("Failed to write PDF file")?;

    eprintln!("Successfully compiled to {}", output.display());
    Ok(())
}

/// Compile a Typst document to HTML
///
/// Uses the typst library with:
/// - Root set to repository root (for imports via World)
/// - Shared resources available in src/typst/ (rheo.typ, style.csl)
pub fn compile_html(input: &Path, output: &Path, root: &Path) -> Result<()> {
    // Create the compilation world
    let world = RheoWorld::new(root, input)
        .context("Failed to create compilation world")?;

    // Compile the document to HtmlDocument
    eprintln!("Compiling {} to HTML...", input.display());
    let result = typst::compile::<HtmlDocument>(&world);

    // Print warnings
    for warning in &result.warnings {
        eprintln!("Warning: {}", warning.message);
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for error in &errors {
                eprintln!("Error: {}", error.message);
            }
            anyhow::bail!("Compilation failed with {} error(s)", errors.len());
        }
    };

    // Export to HTML string
    eprintln!("Exporting to {}...", output.display());
    let html_string = typst_html::html(&document)
        .map_err(|errors| {
            for error in &errors {
                eprintln!("HTML export error: {}", error.message);
            }
            anyhow::anyhow!("HTML export failed with {} error(s)", errors.len())
        })?;

    // Write to file
    std::fs::write(output, html_string)
        .context("Failed to write HTML file")?;

    eprintln!("Successfully compiled to {}", output.display());
    Ok(())
}
