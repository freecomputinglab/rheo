use anyhow::Result;
use std::path::Path;

/// Compile a Typst document to PDF
///
/// Uses the typst library with:
/// - --root set to repository root (for imports)
/// - --features html enabled
/// - Shared resources available in src/typst/ (bookutils.typ, style.csl)
pub fn compile_pdf(input: &Path, output: &Path) -> Result<()> {
    // TODO: Implement PDF compilation using typst library
    // Set root to "." so typst can find src/typst/bookutils.typ
    println!("Would compile {:?} to PDF at {:?}", input, output);
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
