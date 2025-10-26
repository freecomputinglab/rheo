use crate::{Result, RheoError};
use std::path::{Path, PathBuf};
use std::fs;
use walkdir::WalkDir;

/// Configuration for a Typst project
#[derive(Debug)]
pub struct ProjectConfig {
    /// Project name (derived from folder basename)
    pub name: String,

    /// Root directory of the project
    pub root: PathBuf,

    /// List of .typ files in the project
    pub typ_files: Vec<PathBuf>,

    /// Project-specific style.css if it exists
    pub style_css: Option<PathBuf>,

    /// Project img/ directory if it exists
    pub img_dir: Option<PathBuf>,

    /// Project references.bib if it exists
    pub references_bib: Option<PathBuf>,
}

impl ProjectConfig {
    /// Detect project configuration from a directory path
    pub fn from_path(path: &Path) -> Result<Self> {
        // Canonicalize the root path for consistent path handling
        let root = path.canonicalize()
            .map_err(|e| RheoError::path(path, format!("failed to canonicalize project directory: {}", e)))?;

        // Extract project name from directory basename
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| RheoError::project_config("failed to get project name from directory"))?
            .to_string();

        // Find all .typ files in the directory (recursive walk)
        let typ_files: Vec<PathBuf> = WalkDir::new(&root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension()
                    .and_then(|s| s.to_str())
                    == Some("typ")
            })
            .map(|e| e.path().to_path_buf())
            .collect();

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
            typ_files,
            style_css,
            img_dir,
            references_bib,
        })
    }
}
