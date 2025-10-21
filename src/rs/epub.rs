use anyhow::Result;
use std::path::Path;

/// Generate EPUB from HTML file using ebook-convert
///
/// Requires Calibre's ebook-convert to be installed and in PATH
pub fn generate_epub(html_path: &Path, epub_path: &Path) -> Result<()> {
    // TODO: Implement EPUB generation by shelling out to ebook-convert
    println!("Would generate EPUB from {:?} to {:?}", html_path, epub_path);
    Ok(())
}
