use crate::{Result, RheoError};
use std::fs;
use std::path::{Path, PathBuf};

/// Output directory configuration for a project
#[derive(Debug)]
pub struct OutputConfig {
    /// Base build directory (e.g. project_root/build)
    pub base: PathBuf,
}

impl OutputConfig {
    /// Create output configuration for a project
    ///
    /// Outputs to {build_dir}/{plugin_name}/ where build_dir defaults to {project_root}/build
    pub fn new(project_root: &Path, build_dir: Option<PathBuf>) -> Self {
        let base = match build_dir {
            Some(custom) => custom,
            None => project_root.join("build"),
        };
        OutputConfig { base }
    }

    /// Get the output directory for a given plugin name
    pub fn dir_for_plugin(&self, name: &str) -> PathBuf {
        self.base.join(name)
    }

    /// Clean this project's build artifacts
    pub fn clean(&self) -> Result<()> {
        if self.base.exists() {
            fs::remove_dir_all(&self.base)
                .map_err(|e| RheoError::io(e, format!("removing directory {:?}", self.base)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_output_config_new() {
        let project_root = PathBuf::from("/home/user/my-book");
        let config = OutputConfig::new(&project_root, None);

        assert_eq!(config.base, PathBuf::from("/home/user/my-book/build"));
        assert_eq!(
            config.dir_for_plugin("pdf"),
            PathBuf::from("/home/user/my-book/build/pdf")
        );
        assert_eq!(
            config.dir_for_plugin("html"),
            PathBuf::from("/home/user/my-book/build/html")
        );
        assert_eq!(
            config.dir_for_plugin("epub"),
            PathBuf::from("/home/user/my-book/build/epub")
        );
    }

    #[test]
    fn test_output_config_custom_build_dir() {
        let project_root = PathBuf::from("/home/user/my-book");
        let custom_build = PathBuf::from("/tmp/rheo-output");
        let config = OutputConfig::new(&project_root, Some(custom_build));

        assert_eq!(config.base, PathBuf::from("/tmp/rheo-output"));
        assert_eq!(
            config.dir_for_plugin("pdf"),
            PathBuf::from("/tmp/rheo-output/pdf")
        );
        assert_eq!(
            config.dir_for_plugin("html"),
            PathBuf::from("/tmp/rheo-output/html")
        );
        assert_eq!(
            config.dir_for_plugin("epub"),
            PathBuf::from("/tmp/rheo-output/epub")
        );
    }

    #[test]
    fn test_clean() {
        let temp_dir = std::env::temp_dir().join("rheo_test_clean");

        // Clean up any previous test runs
        let _ = fs::remove_dir_all(&temp_dir);

        let config = OutputConfig::new(&temp_dir, None);

        // Create directories and some dummy files
        fs::create_dir_all(config.dir_for_plugin("pdf")).expect("Failed to create pdf dir");
        fs::create_dir_all(config.dir_for_plugin("html")).expect("Failed to create html dir");
        fs::write(config.dir_for_plugin("pdf").join("test.pdf"), b"dummy pdf")
            .expect("Failed to write test file");
        fs::write(
            config.dir_for_plugin("html").join("test.html"),
            b"dummy html",
        )
        .expect("Failed to write test file");

        // Verify directories exist
        assert!(config.dir_for_plugin("pdf").exists());
        assert!(config.dir_for_plugin("html").exists());

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
}
