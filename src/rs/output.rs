use crate::{Result, RheoError};
use std::fs;
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
        fs::create_dir_all(&self.pdf_dir)
            .map_err(|e| RheoError::io(e, format!("creating PDF directory {:?}", self.pdf_dir)))?;

        fs::create_dir_all(&self.html_dir)
            .map_err(|e| RheoError::io(e, format!("creating HTML directory {:?}", self.html_dir)))?;

        fs::create_dir_all(&self.epub_dir)
            .map_err(|e| RheoError::io(e, format!("creating EPUB directory {:?}", self.epub_dir)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_create_dirs() {
        let temp_dir = std::env::temp_dir().join("rheo_test_output");

        // Clean up any previous test runs
        let _ = fs::remove_dir_all(&temp_dir);

        // Create a test configuration in the temp directory
        let config = OutputConfig {
            pdf_dir: temp_dir.join("pdf"),
            html_dir: temp_dir.join("html"),
            epub_dir: temp_dir.join("epub"),
        };

        // Create directories
        config.create_dirs().expect("Failed to create directories");

        // Verify all directories exist
        assert!(config.pdf_dir.exists(), "PDF directory should exist");
        assert!(config.html_dir.exists(), "HTML directory should exist");
        assert!(config.epub_dir.exists(), "EPUB directory should exist");

        // Clean up
        fs::remove_dir_all(&temp_dir).expect("Failed to clean up test directory");
    }

    #[test]
    fn test_output_config_with_project_name() {
        let config = OutputConfig::new("my-book");

        assert_eq!(config.pdf_dir, PathBuf::from("build/my-book/pdf"));
        assert_eq!(config.html_dir, PathBuf::from("build/my-book/html"));
        assert_eq!(config.epub_dir, PathBuf::from("build/my-book/epub"));
    }

    #[test]
    fn test_output_config_with_src() {
        let config = OutputConfig::new("src");

        assert_eq!(config.pdf_dir, PathBuf::from("build/pdf"));
        assert_eq!(config.html_dir, PathBuf::from("build/html"));
        assert_eq!(config.epub_dir, PathBuf::from("build/epub"));
    }

    #[test]
    fn test_output_config_with_empty_name() {
        let config = OutputConfig::new("");

        assert_eq!(config.pdf_dir, PathBuf::from("build/pdf"));
        assert_eq!(config.html_dir, PathBuf::from("build/html"));
        assert_eq!(config.epub_dir, PathBuf::from("build/epub"));
    }
}
