use anyhow::Result;
use std::path::{Path, PathBuf};

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
        // TODO: Implement project detection logic
        println!("Would detect project at {:?}", path);

        Ok(ProjectConfig {
            name: path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            root: path.to_path_buf(),
            typ_files: Vec::new(),
            style_css: None,
            img_dir: None,
            references_bib: None,
        })
    }
}
