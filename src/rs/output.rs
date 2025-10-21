use anyhow::Result;
use std::path::PathBuf;

/// Output directory configuration for a project
#[derive(Debug)]
pub struct OutputConfig {
    /// PDF output directory
    pub pdf_dir: PathBuf,

    /// HTML output directory
    pub html_dir: PathBuf,

    /// EPUB output directory
    pub epub_dir: PathBuf,
}

impl OutputConfig {
    /// Create output configuration for a project
    ///
    /// If project_name is empty or "src", outputs to build/{pdf,html,epub}/
    /// Otherwise, outputs to build/{project_name}/{pdf,html,epub}/
    pub fn new(project_name: &str) -> Self {
        let base = if project_name.is_empty() || project_name == "src" {
            PathBuf::from("build")
        } else {
            PathBuf::from("build").join(project_name)
        };

        OutputConfig {
            pdf_dir: base.join("pdf"),
            html_dir: base.join("html"),
            epub_dir: base.join("epub"),
        }
    }

    /// Create all necessary output directories
    pub fn create_dirs(&self) -> Result<()> {
        // TODO: Implement directory creation logic
        println!("Would create directories:");
        println!("  PDF: {:?}", self.pdf_dir);
        println!("  HTML: {:?}", self.html_dir);
        println!("  EPUB: {:?}", self.epub_dir);
        Ok(())
    }
}
