use crate::config::Merge;
use crate::formats::pdf::filename_to_title;
use crate::{Result, RheoConfig, RheoError};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use walkdir::WalkDir;

/// Mode for project compilation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMode {
    /// Compiling all .typ files in a directory
    Directory,
    /// Compiling a single specified .typ file
    SingleFile,
}

/// Configuration for a Typst project
#[derive(Debug)]
pub struct ProjectConfig {
    /// Project name (derived from folder basename)
    pub name: String,

    /// Root directory of the project
    pub root: PathBuf,

    /// Rheo configuration from rheo.toml
    pub config: RheoConfig,

    /// List of .typ files in the project
    pub typ_files: Vec<PathBuf>,

    /// Project-specific style.css (for HTML export) if it exists
    pub style_css: Option<PathBuf>,

    /// Compilation mode (directory or single file)
    pub mode: ProjectMode,

    /// Path to the config file that was loaded
    /// None if using default config (single-file mode without --config)
    pub config_path: Option<PathBuf>,
}

impl ProjectConfig {
    /// Detect project configuration from a path (file or directory)
    ///
    /// # Arguments
    /// * `path` - Path to project directory or single .typ file
    /// * `config_path` - Optional path to custom rheo.toml config file
    pub fn from_path(path: &Path, config_path: Option<&Path>) -> Result<Self> {
        // Check if path exists and determine if it's a file or directory
        let metadata = path
            .metadata()
            .map_err(|e| RheoError::path(path, format!("path does not exist: {}", e)))?;

        if metadata.is_file() {
            Self::from_single_file(path, config_path)
        } else if metadata.is_dir() {
            Self::from_directory(path, config_path)
        } else {
            Err(RheoError::path(path, "path must be a file or directory"))
        }
    }

    /// Detect project configuration from a directory path
    fn from_directory(path: &Path, config_path: Option<&Path>) -> Result<Self> {
        // Canonicalize the root path for consistent path handling
        let root = path.canonicalize().map_err(|e| {
            RheoError::path(
                path,
                format!("failed to canonicalize project directory: {}", e),
            )
        })?;

        // Extract project name from directory basename
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| RheoError::project_config("failed to get project name from directory"))?
            .to_string();

        // Load config: custom path takes precedence
        let (config, loaded_config_path) = if let Some(custom_path) = config_path {
            debug!(config = %custom_path.display(), "loading custom config");
            let config = RheoConfig::load_from_path(custom_path)?;
            (config, Some(custom_path.to_path_buf()))
        } else {
            let config = RheoConfig::load(&root)?;
            let default_path = root.join("rheo.toml");
            let loaded_path = if default_path.exists() {
                Some(default_path)
            } else {
                None
            };
            (config, loaded_path)
        };

        // Apply smart defaults if no config file was loaded
        let config = if loaded_config_path.is_none() {
            apply_smart_defaults(config, &name, ProjectMode::Directory)
        } else {
            config
        };

        // Determine search directory: content_dir if configured, otherwise project root
        let search_dir = config
            .resolve_content_dir(&root)
            .unwrap_or_else(|| root.clone());
        debug!(search_dir = %search_dir.display(), "searching for .typ files");

        // Find all .typ files in the search directory (recursive walk)
        let typ_files: Vec<PathBuf> = WalkDir::new(&search_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("typ"))
            .map(|e| e.path().to_path_buf())
            .collect();

        // Detect optional project-specific resources
        let style_css = root.join("style.css");
        let style_css = if style_css.is_file() {
            Some(style_css)
        } else {
            None
        };

        Ok(ProjectConfig {
            name,
            root,
            config,
            typ_files,
            style_css,
            mode: ProjectMode::Directory,
            config_path: loaded_config_path,
        })
    }

    /// Detect project configuration from a single .typ file
    fn from_single_file(file_path: &Path, config_path: Option<&Path>) -> Result<Self> {
        // Validate .typ extension
        if file_path.extension().and_then(|s| s.to_str()) != Some("typ") {
            return Err(RheoError::path(file_path, "file must have .typ extension"));
        }

        // Canonicalize the file path first (resolves relative to absolute)
        let file_path = file_path.canonicalize().map_err(|e| {
            RheoError::path(
                file_path,
                format!("failed to canonicalize file path: {}", e),
            )
        })?;

        // Root = parent directory (now guaranteed to be absolute)
        let root = file_path
            .parent()
            .ok_or_else(|| RheoError::path(&file_path, "file has no parent directory"))?
            .to_path_buf();

        // Project name = file stem
        let name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| RheoError::path(&file_path, "invalid filename"))?
            .to_string();

        // Load config if --config provided, otherwise use default
        let (config, loaded_config_path) = if let Some(custom_path) = config_path {
            debug!(config = %custom_path.display(), "using custom config in single-file mode");
            let config = RheoConfig::load_from_path(custom_path)?;
            (config, Some(custom_path.to_path_buf()))
        } else {
            (RheoConfig::default(), None)
        };

        // Apply smart defaults if no config file was loaded
        let config = if loaded_config_path.is_none() {
            apply_smart_defaults(config, &name, ProjectMode::SingleFile)
        } else {
            config
        };

        // Single file in typ_files list
        let typ_files = vec![file_path.clone()];

        // Check for optional resources in root directory
        let style_css = root.join("style.css");
        let style_css = if style_css.is_file() {
            Some(style_css)
        } else {
            None
        };

        Ok(ProjectConfig {
            name,
            root,
            config,
            typ_files,
            style_css,
            mode: ProjectMode::SingleFile,
            config_path: loaded_config_path,
        })
    }
}

/// Apply smart defaults when no rheo.toml exists.
///
/// This generates sensible merge configurations for EPUB based on the project
/// mode and name. PDF is not modified to maintain backwards compatibility
/// (users expect per-file PDFs by default).
fn apply_smart_defaults(
    mut config: RheoConfig,
    project_name: &str,
    mode: ProjectMode,
) -> RheoConfig {
    // Generate human-readable title from project/file name
    let title = filename_to_title(project_name);

    // Apply EPUB defaults if merge not configured
    if config.epub.merge.is_none() {
        let spine = match mode {
            ProjectMode::SingleFile => vec![], // Empty: will auto-discover single file
            ProjectMode::Directory => vec!["**/*.typ".to_string()], // All files
        };
        config.epub.merge = Some(Merge { title, spine });
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_single_file_basic() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        let project = ProjectConfig::from_path(&file, None).unwrap();

        assert_eq!(project.name, "document");
        assert_eq!(project.mode, ProjectMode::SingleFile);
        assert_eq!(project.typ_files.len(), 1);
        assert_eq!(project.root, temp.path().canonicalize().unwrap());
    }

    #[test]
    fn test_single_file_non_typ_extension_fails() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("document.txt");
        fs::write(&file, "test").unwrap();

        let result = ProjectConfig::from_path(&file, None);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(".typ extension"));
    }

    #[test]
    fn test_single_file_nonexistent_fails() {
        let path = PathBuf::from("/tmp/nonexistent_file_12345_rheo_test.typ");
        let result = ProjectConfig::from_path(&path, None);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("does not exist"));
    }

    #[test]
    fn test_single_file_discovers_assets() {
        let temp = TempDir::new().unwrap();

        // Create assets in parent directory
        fs::write(temp.path().join("style.css"), "body {}").unwrap();
        fs::create_dir(temp.path().join("img")).unwrap();
        fs::write(temp.path().join("references.bib"), "@article{}").unwrap();

        let file = temp.path().join("document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        let project = ProjectConfig::from_path(&file, None).unwrap();

        assert!(project.style_css.is_some());
    }

    #[test]
    fn test_directory_mode_unchanged() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("doc1.typ"), "#heading[1]").unwrap();
        fs::write(temp.path().join("doc2.typ"), "#heading[2]").unwrap();

        let project = ProjectConfig::from_path(temp.path(), None).unwrap();

        assert_eq!(project.mode, ProjectMode::Directory);
        assert_eq!(project.typ_files.len(), 2);
    }

    #[test]
    fn test_single_file_with_relative_path() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        // Save original directory and change to temp directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        // Use relative path (no directory component)
        let result = ProjectConfig::from_path(Path::new("document.typ"), None);

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();

        // Verify it succeeded
        let project = result.unwrap();
        assert_eq!(project.name, "document");
        assert_eq!(project.mode, ProjectMode::SingleFile);
        assert_eq!(project.typ_files.len(), 1);
        assert_eq!(project.root, temp.path().canonicalize().unwrap());
    }

    #[test]
    fn test_no_config_path_when_default() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        let project = ProjectConfig::from_path(&file, None).unwrap();

        // Single-file mode without custom config should have None
        assert!(project.config_path.is_none());
    }

    #[test]
    fn test_single_file_epub_default_merge() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("my-document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        let project = ProjectConfig::from_path(&file, None).unwrap();

        // Should have default merge config for EPUB
        assert!(project.config.epub.merge.is_some());
        let merge = project.config.epub.merge.as_ref().unwrap();
        assert_eq!(merge.title, "My Document");
        assert!(merge.spine.is_empty());
    }

    #[test]
    fn test_directory_epub_default_merge() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.typ"), "A").unwrap();
        fs::write(temp.path().join("b.typ"), "B").unwrap();

        let project = ProjectConfig::from_path(temp.path(), None).unwrap();

        // Should have default merge config for EPUB
        assert!(project.config.epub.merge.is_some());
        let merge = project.config.epub.merge.as_ref().unwrap();
        assert_eq!(merge.spine, vec!["**/*.typ"]);
        // Title should be based on temp directory name (will vary)
        assert!(!merge.title.is_empty());
    }

    #[test]
    fn test_explicit_config_not_modified() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("rheo.toml");
        fs::write(
            &config_path,
            r#"
[epub.merge]
title = "Custom Title"
spine = ["custom.typ"]
"#,
        )
        .unwrap();
        fs::write(temp.path().join("custom.typ"), "content").unwrap();

        let project = ProjectConfig::from_path(temp.path(), None).unwrap();

        // Should preserve explicit config
        let merge = project.config.epub.merge.as_ref().unwrap();
        assert_eq!(merge.title, "Custom Title");
        assert_eq!(merge.spine, vec!["custom.typ"]);
    }

    #[test]
    fn test_pdf_not_auto_merged() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.typ"), "A").unwrap();
        fs::write(temp.path().join("b.typ"), "B").unwrap();

        let project = ProjectConfig::from_path(temp.path(), None).unwrap();

        // PDF should not get default merge config (backwards compatibility)
        assert!(project.config.pdf.merge.is_none());
    }
}
