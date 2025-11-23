use crate::{Result, RheoConfig, RheoError};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
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

        // Load configuration and build exclusion set
        let config = RheoConfig::load(&root)?;
        let exclusions = config.build_exclusion_set()?;

        // Determine search directory: content_dir if configured, otherwise project root
        let search_dir = config.resolve_content_dir(&root).unwrap_or_else(|| root.clone());
        debug!(search_dir = %search_dir.display(), "searching for .typ files");

        // Find all .typ files in the search directory (recursive walk)
        let all_typ_files: Vec<PathBuf> = WalkDir::new(&search_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension()
                    .and_then(|s| s.to_str())
                    == Some("typ")
            })
            .map(|e| e.path().to_path_buf())
            .collect();

        let total_count = all_typ_files.len();

        // Apply exclusions - filter out files matching exclusion patterns
        let typ_files: Vec<PathBuf> = all_typ_files
            .into_iter()
            .filter(|path| {
                // Make path relative to root for glob matching
                let relative_path = match path.strip_prefix(&root) {
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
            info!(excluded = excluded_count, included = typ_files.len(), "applied exclusion filters");
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
            typ_files,
            style_css,
            img_dir,
            references_bib,
        })
    }
}
