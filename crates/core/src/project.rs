//! Project discovery: which directory (or single file) is being compiled, which
//! `rheo.toml` governs it, and what Typst it contains. Reading the filesystem to
//! answer those is this module's job — [`crate::config`] only parses.

use crate::reticulate::SpineScan;
use crate::{Result, RheoConfig, RheoError};
use std::path::{Path, PathBuf};
use tracing::debug;

/// How far up from a single file's directory to look for the `rheo.toml` that
/// governs it. Deep enough for any real project layout, bounded so a file
/// outside a project cannot walk to the filesystem root.
const CONFIG_SEARCH_DEPTH: usize = 10;

/// Mode for project compilation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMode {
    /// Compiling all .typ files in a directory
    Directory,
    /// Compiling a single specified .typ file
    SingleFile,
}

/// Configuration for a Typst project
#[derive(Debug, Clone)]
pub struct ProjectConfig {
    /// Project name (derived from folder basename)
    pub name: String,

    /// Root directory of the project
    pub root: PathBuf,

    /// Rheo configuration from rheo.toml
    pub config: RheoConfig,

    /// List of .typ files in the project
    pub typ_files: Vec<PathBuf>,

    /// Compilation mode (directory or single file)
    pub mode: ProjectMode,

    /// Path to the config file that was loaded
    /// None if using default config (no rheo.toml found)
    pub config_path: Option<PathBuf>,
}

impl ProjectConfig {
    /// Detect project configuration from a path (file or directory).
    pub fn from_path(path: &Path, config_path: Option<&Path>) -> Result<Self> {
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

    fn from_directory(path: &Path, config_path: Option<&Path>) -> Result<Self> {
        let root = crate::util::path::canonicalize_path(path)?;

        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| RheoError::project_config("failed to get project name from directory"))?
            .to_string();

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

        let search_dir = config
            .resolve_content_dir(&root)
            .unwrap_or_else(|| root.clone());
        debug!(search_dir = %search_dir.display(), "searching for .typ files");

        let typ_files = SpineScan::typ_files(&search_dir)?;

        Ok(ProjectConfig {
            name,
            root,
            config,
            typ_files,
            mode: ProjectMode::Directory,
            config_path: loaded_config_path,
        })
    }

    fn from_single_file(file_path: &Path, config_path: Option<&Path>) -> Result<Self> {
        if file_path.extension().and_then(|s| s.to_str()) != Some("typ") {
            return Err(RheoError::path(file_path, "file must have .typ extension"));
        }

        let file_path = crate::util::path::canonicalize_path(file_path)?;

        let file_parent = file_path
            .parent()
            .ok_or_else(|| RheoError::path(&file_path, "file has no parent directory"))?;

        let name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| RheoError::path(&file_path, "invalid filename"))?
            .to_string();

        let (config, loaded_config_path, root) = if let Some(custom_path) = config_path {
            debug!(config = %custom_path.display(), "using custom config in single-file mode");
            let config = RheoConfig::load_from_path(custom_path)?;
            // Use the directory containing the custom config as project root
            let config_root = custom_path
                .parent()
                .ok_or_else(|| RheoError::path(custom_path, "config path has no parent"))?
                .to_path_buf();
            (config, Some(custom_path.to_path_buf()), config_root)
        } else {
            match Self::search_upwards_for_config(file_parent) {
                Some(config_path) => {
                    debug!(
                        config = %config_path.display(),
                        "discovered rheo.toml in single-file mode"
                    );
                    let root = config_path.parent().unwrap_or(file_parent).to_path_buf();
                    (
                        RheoConfig::load_from_path(&config_path)?,
                        Some(config_path),
                        root,
                    )
                }
                None => {
                    debug!("no rheo.toml found in single-file mode, using defaults");
                    (RheoConfig::default(), None, file_parent.to_path_buf())
                }
            }
        };

        let typ_files = vec![file_path.clone()];

        Ok(ProjectConfig {
            name,
            root,
            config,
            typ_files,
            mode: ProjectMode::SingleFile,
            config_path: loaded_config_path,
        })
    }

    /// The nearest `rheo.toml` at or above `dir`, within
    /// [`CONFIG_SEARCH_DEPTH`] levels.
    fn search_upwards_for_config(dir: &Path) -> Option<PathBuf> {
        dir.ancestors()
            .take(CONFIG_SEARCH_DEPTH)
            .map(|dir| dir.join("rheo.toml"))
            .find(|candidate| candidate.exists())
    }
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
    fn test_single_file_with_assets_in_root() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("style.css"), "body {}").unwrap();
        fs::create_dir(temp.path().join("img")).unwrap();
        fs::write(temp.path().join("references.bib"), "@article{}").unwrap();

        let file = temp.path().join("document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        let project = ProjectConfig::from_path(&file, None).unwrap();
        assert_eq!(project.root, temp.path().canonicalize().unwrap());
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

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let result = ProjectConfig::from_path(Path::new("document.typ"), None);
        std::env::set_current_dir(original_dir).unwrap();

        let project = result.unwrap();
        assert_eq!(project.name, "document");
        assert_eq!(project.mode, ProjectMode::SingleFile);
    }

    #[test]
    fn test_no_config_path_when_default() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        let project = ProjectConfig::from_path(&file, None).unwrap();
        assert!(project.config_path.is_none());
    }

    #[test]
    fn test_no_smart_defaults_applied_in_project() {
        // Smart defaults are now applied by plugins in the CLI, not in project loading.
        // The project config should have empty plugin_sections when no rheo.toml exists.
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("my-document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        let project = ProjectConfig::from_path(&file, None).unwrap();
        // No epub section should be present (no rheo.toml)
        assert!(!project.config.plugin_sections.contains_key("epub"));
    }

    #[test]
    fn test_explicit_config_loaded() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("rheo.toml");
        fs::write(
            &config_path,
            format!(
                "version = \"{}\"\n\n[epub.spine]\ntitle = \"Custom Title\"\n",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .unwrap();
        fs::write(temp.path().join("custom.typ"), "content").unwrap();

        let project = ProjectConfig::from_path(temp.path(), None).unwrap();
        let section = project.config.plugin_section("epub");
        let spine = section.spine.unwrap();
        assert_eq!(spine.title.as_deref().unwrap(), "Custom Title");
    }

    #[test]
    fn test_pdf_no_spine_by_default() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("a.typ"), "A").unwrap();
        fs::write(temp.path().join("b.typ"), "B").unwrap();

        let project = ProjectConfig::from_path(temp.path(), None).unwrap();
        assert!(project.config.spine_for_plugin("pdf").is_none());
    }
}
