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

        fs::create_dir_all(&self.html_dir).map_err(|e| {
            RheoError::io(e, format!("creating HTML directory {:?}", self.html_dir))
        })?;

        fs::create_dir_all(&self.epub_dir).map_err(|e| {
            RheoError::io(e, format!("creating EPUB directory {:?}", self.epub_dir))
        })?;

        Ok(())
    }

    /// Clean this project's build artifacts
    pub fn clean_project(&self) -> Result<()> {
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

    /// Clean all build artifacts (entire build/ directory)
    /// Preserves build/.gitignore if it exists
    pub fn clean_all() -> Result<()> {
        let build_dir = PathBuf::from("build");

        if !build_dir.exists() {
            return Ok(());
        }

        // Read directory contents
        let entries = fs::read_dir(&build_dir)
            .map_err(|e| RheoError::io(e, format!("reading build directory {:?}", build_dir)))?;

        // Remove everything except .gitignore
        for entry in entries {
            let entry = entry.map_err(|e| RheoError::io(e, "reading directory entry"))?;
            let path = entry.path();

            // Skip .gitignore
            if path.file_name().and_then(|n| n.to_str()) == Some(".gitignore") {
                continue;
            }

            // Remove files and directories
            if path.is_dir() {
                fs::remove_dir_all(&path)
                    .map_err(|e| RheoError::io(e, format!("removing directory {:?}", path)))?;
            } else {
                fs::remove_file(&path)
                    .map_err(|e| RheoError::io(e, format!("removing file {:?}", path)))?;
            }
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

    #[test]
    fn test_clean_project() {
        let temp_dir = std::env::temp_dir().join("rheo_test_clean_project");

        // Clean up any previous test runs
        let _ = fs::remove_dir_all(&temp_dir);

        // Create a test configuration in the temp directory
        let config = OutputConfig {
            pdf_dir: temp_dir.join("my-book").join("pdf"),
            html_dir: temp_dir.join("my-book").join("html"),
            epub_dir: temp_dir.join("my-book").join("epub"),
        };

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
        config.clean_project().expect("Failed to clean project");

        // Verify project directory is gone
        assert!(
            !temp_dir.join("my-book").exists(),
            "Project directory should be removed"
        );

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_clean_all() {
        let temp_dir = std::env::temp_dir().join("rheo_test_clean_all");

        // Clean up any previous test runs
        let _ = fs::remove_dir_all(&temp_dir);

        // Create temp directory
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        // Change to temp directory
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&temp_dir).expect("Failed to change dir");

        // Create multiple project directories
        let config1 = OutputConfig::new("project1");
        let config2 = OutputConfig::new("project2");

        config1
            .create_dirs()
            .expect("Failed to create project1 dirs");
        config2
            .create_dirs()
            .expect("Failed to create project2 dirs");

        fs::write(config1.pdf_dir.join("test.pdf"), b"dummy").expect("Failed to write");
        fs::write(config2.pdf_dir.join("test.pdf"), b"dummy").expect("Failed to write");

        // Create a .gitignore file in build/
        let build_dir = PathBuf::from("build");
        fs::write(build_dir.join(".gitignore"), b"**/*\n!.gitignore\n")
            .expect("Failed to write .gitignore");

        // Verify build directory exists
        assert!(build_dir.exists());
        assert!(build_dir.join(".gitignore").exists());

        // Clean all
        OutputConfig::clean_all().expect("Failed to clean all");

        // Verify build directory still exists
        assert!(build_dir.exists(), "Build directory should still exist");

        // Verify .gitignore is preserved
        assert!(
            build_dir.join(".gitignore").exists(),
            ".gitignore should be preserved"
        );

        // Verify project directories are gone
        assert!(
            !build_dir.join("project1").exists(),
            "project1 should be removed"
        );
        assert!(
            !build_dir.join("project2").exists(),
            "project2 should be removed"
        );

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_clean_nonexistent_directory() {
        let config = OutputConfig {
            pdf_dir: PathBuf::from("/tmp/rheo_nonexistent_test_xyz/pdf"),
            html_dir: PathBuf::from("/tmp/rheo_nonexistent_test_xyz/html"),
            epub_dir: PathBuf::from("/tmp/rheo_nonexistent_test_xyz/epub"),
        };

        // Should not error when cleaning non-existent directory
        assert!(config.clean_project().is_ok());
    }
}
