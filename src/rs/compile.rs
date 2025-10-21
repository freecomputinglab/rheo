use anyhow::{Result, Context};
use std::path::Path;
use typst::layout::PagedDocument;
use typst_pdf::PdfOptions;

use crate::world::RheoWorld;

/// Compile a Typst document to PDF
///
/// Uses the typst library with:
/// - Root set to repository root (for imports via World)
/// - Shared resources available in src/typst/ (bookutils.typ, style.csl)
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
/// - --root set to repository root (for imports)
/// - --features html enabled
/// - --format html
/// - Shared resources available in src/typst/ (bookutils.typ, style.csl)
pub fn compile_html(input: &Path, output: &Path) -> Result<()> {
    // TODO: Implement HTML compilation using typst library
    // Set root to "." so typst can find src/typst/bookutils.typ
    println!("Would compile {:?} to HTML at {:?}", input, output);
    Ok(())
}
