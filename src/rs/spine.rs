use crate::config::Merge;
use crate::{Result, RheoError};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Generates a spine (ordered list of .typ files) based on configuration.
///
/// # Arguments
/// * `root` - Project root directory
/// * `merge_config` - Optional merge configuration with spine patterns
/// * `require_merge` - If true, merge_config must be provided (PDF mode)
///
/// # Behavior
/// - **PDF mode** (`require_merge=true`): merge_config is mandatory
/// - **EPUB mode** (`require_merge=false`): merge_config is optional with fallback to auto-discovery
///
/// # Errors
/// Returns error if:
/// - `require_merge=true` and `merge_config=None`
/// - No .typ files found (fallback mode)
/// - Multiple .typ files found without merge config (fallback mode)
/// - Merge spine matched no .typ files
pub fn generate_spine(
    root: &Path,
    merge_config: Option<&Merge>,
    require_merge: bool,
) -> Result<Vec<PathBuf>> {
    // PDF mode: merge config is required
    if require_merge && merge_config.is_none() {
        return Err(RheoError::project_config(
            "merge configuration required but not provided",
        ));
    }

    match merge_config {
        None => {
            // EPUB fallback: auto-discover .typ files
            let typst_files: Vec<PathBuf> = WalkDir::new(root)
                .into_iter()
                .filter_map(|entry| Some(entry.ok()?.path().to_path_buf()))
                .filter(|entry| {
                    entry
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext == "typ")
                        .unwrap_or(false)
                })
                .collect();

            match typst_files.len() {
                0 => Err(RheoError::project_config("need at least one .typ file")),
                1 => Ok(typst_files),
                _ => Err(RheoError::project_config(
                    "multiple .typ files found, specify spine in merge config",
                )),
            }
        }

        Some(merge) => {
            // Process glob patterns from merge config
            let mut typst_files = Vec::new();
            for pattern in &merge.spine {
                let glob_pattern = root.join(pattern).display().to_string();
                let glob = glob::glob(&glob_pattern).map_err(|e| {
                    RheoError::project_config(format!("invalid glob pattern '{}': {}", pattern, e))
                })?;

                let mut glob_files: Vec<PathBuf> = glob
                    .filter_map(|entry| entry.ok())
                    .filter(|path| path.is_file())
                    .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("typ"))
                    .collect();

                // Sort lexicographically within each pattern
                glob_files.sort_by_cached_key(|p| p.file_name().unwrap().to_os_string());
                typst_files.extend(glob_files);
            }

            if typst_files.is_empty() {
                return Err(RheoError::project_config(
                    "merge spine matched no .typ files",
                ));
            }

            Ok(typst_files)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir_with_files(files: &[&str]) -> TempDir {
        let temp = TempDir::new().unwrap();
        for file in files {
            let path = temp.path().join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, "").unwrap();
        }
        temp
    }

    #[test]
    fn test_generate_spine_requires_merge_mode() {
        let temp = create_test_dir_with_files(&["test.typ"]);
        let result = generate_spine(temp.path(), None, true);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("merge configuration required"));
    }

    #[test]
    fn test_generate_spine_epub_single_file_fallback() {
        let temp = create_test_dir_with_files(&["single.typ"]);
        let result = generate_spine(temp.path(), None, false);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "single.typ");
    }

    #[test]
    fn test_generate_spine_epub_multiple_files_error() {
        let temp = create_test_dir_with_files(&["first.typ", "second.typ"]);
        let result = generate_spine(temp.path(), None, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("multiple .typ files found"));
    }

    #[test]
    fn test_generate_spine_epub_no_files_error() {
        let temp = create_test_dir_with_files(&["readme.md"]);
        let result = generate_spine(temp.path(), None, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("need at least one .typ file"));
    }

    #[test]
    fn test_generate_spine_with_merge_config() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ", "c.typ"]);
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["*.typ".to_string()],
        };
        let result = generate_spine(temp.path(), Some(&merge), false);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_generate_spine_ordered_patterns() {
        let temp = create_test_dir_with_files(&[
            "cover.typ",
            "chapters/ch1.typ",
            "chapters/ch2.typ",
            "appendix.typ",
        ]);
        let merge = Merge {
            title: "Book".to_string(),
            spine: vec![
                "cover.typ".to_string(),
                "chapters/*.typ".to_string(),
                "appendix.typ".to_string(),
            ],
        };
        let result = generate_spine(temp.path(), Some(&merge), true);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 4);
        // Verify pattern order is preserved
        assert_eq!(files[0].file_name().unwrap(), "cover.typ");
        // ch1.typ and ch2.typ should be sorted lexicographically within their pattern
        assert!(files[1].file_name().unwrap().to_str().unwrap().starts_with("ch"));
        assert!(files[2].file_name().unwrap().to_str().unwrap().starts_with("ch"));
        assert_eq!(files[3].file_name().unwrap(), "appendix.typ");
    }

    #[test]
    fn test_generate_spine_merge_no_matches_error() {
        let temp = create_test_dir_with_files(&["readme.md"]);
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["*.typ".to_string()],
        };
        let result = generate_spine(temp.path(), Some(&merge), false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("merge spine matched no .typ files"));
    }
}
