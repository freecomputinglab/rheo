use crate::{Result, RheoError};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

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
    /// Outputs to {build_dir}/{pdf,html,epub}/ where build_dir defaults to {project_root}/build
    pub fn new(project_root: &Path, build_dir: Option<PathBuf>) -> Self {
        let base = match build_dir {
            Some(custom) => custom,
            None => project_root.join("build"),
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

        fs::create_dir_all(&self.html_dir).map_err(|e| {
            RheoError::io(e, format!("creating HTML directory {:?}", self.html_dir))
        })?;

        fs::create_dir_all(&self.epub_dir).map_err(|e| {
            RheoError::io(e, format!("creating EPUB directory {:?}", self.epub_dir))
        })?;

        Ok(())
    }

    /// Clean this project's build artifacts
    pub fn clean(&self) -> Result<()> {
        // Determine the project's root build directory (parent of pdf/html/epub dirs)
        let project_build_dir = self
            .pdf_dir
            .parent()
            .ok_or_else(|| RheoError::path(&self.pdf_dir, "no parent directory"))?;

        if project_build_dir.exists() {
            fs::remove_dir_all(project_build_dir).map_err(|e| {
                RheoError::io(e, format!("removing directory {:?}", project_build_dir))
            })?;
        }

        Ok(())
    }

    /// Copy style.css to HTML output directory
    ///
    /// Priority:
    /// 1. If project has style.css in its root, use that (project-specific override)
    /// 2. Otherwise, use bundled default style.css
    ///
    /// # Arguments
    /// * `project_style_css` - Optional path to project-specific style.css
    ///
    /// # Returns
    /// * `Ok(())` if style.css was successfully copied
    /// * `Err` if copying failed
    pub fn copy_html_assets(&self, project_style_css: Option<&Path>) -> Result<()> {
        // Default bundled CSS (embedded at compile time)
        const DEFAULT_CSS: &str = include_str!("../css/style.css");

        let dest_path = self.html_dir.join("style.css");

        if let Some(project_css) = project_style_css {
            // Copy project-specific style.css
            fs::copy(project_css, &dest_path).map_err(|e| {
                RheoError::io(
                    e,
                    format!(
                        "copying project style.css from {:?} to {:?}",
                        project_css, dest_path
                    ),
                )
            })?;
            debug!(source = %project_css.display(), dest = %dest_path.display(), "copied project-specific style.css");
        } else {
            // Write bundled default CSS
            fs::write(&dest_path, DEFAULT_CSS).map_err(|e| {
                RheoError::io(e, format!("writing default style.css to {:?}", dest_path))
            })?;
            debug!(dest = %dest_path.display(), "copied default style.css");
        }

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
    fn test_output_config_new() {
        let project_root = PathBuf::from("/home/user/my-book");
        let config = OutputConfig::new(&project_root, None);

        assert_eq!(
            config.pdf_dir,
            PathBuf::from("/home/user/my-book/build/pdf")
        );
        assert_eq!(
            config.html_dir,
            PathBuf::from("/home/user/my-book/build/html")
        );
        assert_eq!(
            config.epub_dir,
            PathBuf::from("/home/user/my-book/build/epub")
        );
    }

    #[test]
    fn test_clean() {
        let temp_dir = std::env::temp_dir().join("rheo_test_clean");

        // Clean up any previous test runs
        let _ = fs::remove_dir_all(&temp_dir);

        // Create a test configuration
        let config = OutputConfig::new(&temp_dir, None);

        // Create directories and some dummy files
        config.create_dirs().expect("Failed to create directories");
        fs::write(config.pdf_dir.join("test.pdf"), b"dummy pdf")
            .expect("Failed to write test file");
        fs::write(config.html_dir.join("test.html"), b"dummy html")
            .expect("Failed to write test file");

        // Verify directories exist
        assert!(config.pdf_dir.exists());
        assert!(config.html_dir.exists());

        // Clean project
        config.clean().expect("Failed to clean project");

        // Verify build directory is gone
        assert!(
            !temp_dir.join("build").exists(),
            "Build directory should be removed"
        );

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_clean_nonexistent_directory() {
        let nonexistent = PathBuf::from("/tmp/rheo_nonexistent_test_xyz");
        let config = OutputConfig::new(&nonexistent, None);

        // Should not error when cleaning non-existent directory
        assert!(config.clean().is_ok());
    }

    #[test]
    fn test_output_config_custom_build_dir() {
        let project_root = PathBuf::from("/home/user/my-book");
        let custom_build = PathBuf::from("/tmp/rheo-output");
        let config = OutputConfig::new(&project_root, Some(custom_build));

        assert_eq!(config.pdf_dir, PathBuf::from("/tmp/rheo-output/pdf"));
        assert_eq!(config.html_dir, PathBuf::from("/tmp/rheo-output/html"));
        assert_eq!(config.epub_dir, PathBuf::from("/tmp/rheo-output/epub"));
    }
}
