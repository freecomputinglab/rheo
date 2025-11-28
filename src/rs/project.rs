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

    /// Project-specific style.css if it exists
    pub style_css: Option<PathBuf>,

    /// Project img/ directory if it exists
    pub img_dir: Option<PathBuf>,

    /// Project references.bib if it exists
    pub references_bib: Option<PathBuf>,

    /// Compilation mode (directory or single file)
    pub mode: ProjectMode,
}

impl ProjectConfig {
    /// Detect project configuration from a path (file or directory)
    pub fn from_path(path: &Path) -> Result<Self> {
        // Check if path exists and determine if it's a file or directory
        let metadata = path
            .metadata()
            .map_err(|e| RheoError::path(path, format!("path does not exist: {}", e)))?;

        if metadata.is_file() {
            Self::from_single_file(path)
        } else if metadata.is_dir() {
            Self::from_directory(path)
        } else {
            Err(RheoError::path(path, "path must be a file or directory"))
        }
    }

    /// Detect project configuration from a directory path
    fn from_directory(path: &Path) -> Result<Self> {
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

        // Load configuration and build exclusion set
        let config = RheoConfig::load(&root)?;
        let exclusions = config.build_exclusion_set()?;

        // Determine search directory: content_dir if configured, otherwise project root
        let search_dir = config
            .resolve_content_dir(&root)
            .unwrap_or_else(|| root.clone());
        debug!(search_dir = %search_dir.display(), "searching for .typ files");

        // Find all .typ files in the search directory (recursive walk)
        let all_typ_files: Vec<PathBuf> = WalkDir::new(&search_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("typ"))
            .map(|e| e.path().to_path_buf())
            .collect();

        let total_count = all_typ_files.len();

        // Apply exclusions - filter out files matching exclusion patterns
        let typ_files: Vec<PathBuf> = all_typ_files
            .into_iter()
            .filter(|path| {
                // Make path relative to search_dir for glob matching
                let relative_path = match path.strip_prefix(&search_dir) {
                    Ok(rel) => rel,
                    Err(_) => return true, // Keep file if we can't make it relative
                };

                let is_excluded = exclusions.is_match(relative_path);
                if is_excluded {
                    debug!(file = %relative_path.display(), "excluding file from compilation");
                }
                !is_excluded
            })
            .collect();

        let excluded_count = total_count - typ_files.len();
        if excluded_count > 0 {
            info!(
                excluded = excluded_count,
                included = typ_files.len(),
                "applied exclusion filters"
            );
        }

        // Detect optional project-specific resources
        let style_css = root.join("style.css");
        let style_css = if style_css.is_file() {
            Some(style_css)
        } else {
            None
        };

        let img_dir = root.join("img");
        let img_dir = if img_dir.is_dir() {
            Some(img_dir)
        } else {
            None
        };

        let references_bib = root.join("references.bib");
        let references_bib = if references_bib.is_file() {
            Some(references_bib)
        } else {
            None
        };

        Ok(ProjectConfig {
            name,
            root,
            config,
            typ_files,
            style_css,
            img_dir,
            references_bib,
            mode: ProjectMode::Directory,
        })
    }

    /// Detect project configuration from a single .typ file
    fn from_single_file(file_path: &Path) -> Result<Self> {
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

        // Use default config (skip rheo.toml entirely)
        let config = RheoConfig::default();

        // Single file in typ_files list
        let typ_files = vec![file_path.clone()];

        // Check for optional resources in root directory
        let style_css = root.join("style.css");
        let style_css = if style_css.is_file() {
            Some(style_css)
        } else {
            None
        };

        let img_dir = root.join("img");
        let img_dir = if img_dir.is_dir() {
            Some(img_dir)
        } else {
            None
        };

        let references_bib = root.join("references.bib");
        let references_bib = if references_bib.is_file() {
            Some(references_bib)
        } else {
            None
        };

        Ok(ProjectConfig {
            name,
            root,
            config,
            typ_files,
            style_css,
            img_dir,
            references_bib,
            mode: ProjectMode::SingleFile,
        })
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

        let project = ProjectConfig::from_path(&file).unwrap();

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

        let result = ProjectConfig::from_path(&file);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains(".typ extension"));
    }

    #[test]
    fn test_single_file_nonexistent_fails() {
        let path = PathBuf::from("/tmp/nonexistent_file_12345_rheo_test.typ");
        let result = ProjectConfig::from_path(&path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("does not exist"));
    }

    #[test]
    fn test_single_file_uses_default_config() {
        let temp = TempDir::new().unwrap();

        // Create rheo.toml in parent directory with custom exclusions
        let config_content = r#"
[compile]
exclude = ["*.typ"]
"#;
        fs::write(temp.path().join("rheo.toml"), config_content).unwrap();

        let file = temp.path().join("document.typ");
        fs::write(&file, "#heading[Test]").unwrap();

        let project = ProjectConfig::from_path(&file).unwrap();

        // Should use default config, not load rheo.toml
        // Default exclusion is "lib/**/*.typ"
        assert_eq!(project.config.compile.exclude, vec!["lib/**/*.typ"]);
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

        let project = ProjectConfig::from_path(&file).unwrap();

        assert!(project.style_css.is_some());
        assert!(project.img_dir.is_some());
        assert!(project.references_bib.is_some());
    }

    #[test]
    fn test_directory_mode_unchanged() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("doc1.typ"), "#heading[1]").unwrap();
        fs::write(temp.path().join("doc2.typ"), "#heading[2]").unwrap();

        let project = ProjectConfig::from_path(temp.path()).unwrap();

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
        let result = ProjectConfig::from_path(Path::new("document.typ"));

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();

        // Verify it succeeded
        let project = result.unwrap();
        assert_eq!(project.name, "document");
        assert_eq!(project.mode, ProjectMode::SingleFile);
        assert_eq!(project.typ_files.len(), 1);
        assert_eq!(project.root, temp.path().canonicalize().unwrap());
    }
}
