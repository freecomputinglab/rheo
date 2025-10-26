use anyhow::Result;
use std::path::Path;
use tracing::{debug, instrument};

/// Generate EPUB from HTML file using ebook-convert
///
/// Requires Calibre's ebook-convert to be installed and in PATH
#[instrument(skip_all, fields(html = %html_path.display(), epub = %epub_path.display()))]
pub fn generate_epub(html_path: &Path, epub_path: &Path) -> Result<()> {
    // TODO: Implement EPUB generation by shelling out to ebook-convert
    debug!(html = %html_path.display(), epub = %epub_path.display(), "would generate EPUB");
    Ok(())
}
