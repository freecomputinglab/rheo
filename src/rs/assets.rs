use crate::FilterPatterns;
use crate::{Result, RheoError};
use std::fs;
use std::path::Path;
use tracing::{debug, info};
use walkdir::WalkDir;

/// Copy files to HTML output directory based on unified filter patterns
///
/// Walks the search directory and copies all files that pass the filter.
/// Applies unified exclude/include logic to all file types (.typ and others).
///
/// # Arguments
/// * `project_dir` - Project root directory
/// * `output_dir` - HTML output directory
/// * `filter` - Unified filter patterns (include/exclude logic)
/// * `content_dir` - Optional content subdirectory (patterns relative to this)
///
/// # Examples
///
/// Include only .typ files and images:
/// ```no_run
/// # use rheo::FilterPatterns;
/// # use rheo::assets::copy_html_assets;
/// # use std::path::Path;
/// # let project_dir = Path::new(".");
/// # let output_dir = Path::new(".");
/// let filter = FilterPatterns::from_patterns(&["!**/*.typ".to_string(), "!img/**".to_string()])?;
/// copy_html_assets(&project_dir, &output_dir, &filter, Some(Path::new("content")))?;
/// # Ok::<(), rheo::RheoError>(())
/// ```
pub fn copy_html_assets(
    project_dir: &Path,
    output_dir: &Path,
    filter: &FilterPatterns,
    content_dir: Option<&Path>,
) -> Result<()> {
    // Determine search root
    let search_root = if let Some(content_dir) = content_dir {
        project_dir.join(content_dir)
    } else {
        project_dir.to_path_buf()
    };

    let mut copied_count = 0;

    // Walk directory and apply filter
    for entry in WalkDir::new(&search_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        // Get relative path for matching
        let relative_path = match path.strip_prefix(&search_root) {
            Ok(rel) => rel,
            Err(_) => continue,
        };

        // Apply unified filter
        if filter.should_include(relative_path) {
            let dest_path = output_dir.join(relative_path);

            // Create parent directories
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| RheoError::io(e, format!("creating directory {:?}", parent)))?;
            }

            // Copy file
            fs::copy(path, &dest_path).map_err(|e| RheoError::AssetCopy {
                source: path.to_path_buf(),
                dest: dest_path.clone(),
                error: e,
            })?;

            debug!(file = %relative_path.display(), "copied HTML asset");
            copied_count += 1;
        }
    }

    if copied_count > 0 {
        info!(count = copied_count, "copied HTML assets");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_copy_html_assets_include_only_typ() {
        let temp_dir = std::env::temp_dir().join("rheo_test_html_assets_typ");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");
        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Create .typ and other files
        fs::write(project_dir.join("doc.typ"), "content").unwrap();
        fs::write(project_dir.join("image.png"), "data").unwrap();
        fs::write(project_dir.join("data.json"), "{}").unwrap();

        // Include only .typ files
        let filter = FilterPatterns::from_patterns(&["!**/*.typ".to_string()]).unwrap();
        copy_html_assets(&project_dir, &output_dir, &filter, None).unwrap();

        assert!(output_dir.join("doc.typ").exists());
        assert!(!output_dir.join("image.png").exists());
        assert!(!output_dir.join("data.json").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_html_assets_include_typ_and_images() {
        let temp_dir = std::env::temp_dir().join("rheo_test_html_assets_both");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");
        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        let img_dir = project_dir.join("img");
        fs::create_dir_all(&img_dir).unwrap();

        fs::write(project_dir.join("doc.typ"), "content").unwrap();
        fs::write(img_dir.join("photo.jpg"), "data").unwrap();
        fs::write(project_dir.join("data.json"), "{}").unwrap();

        // Include .typ and img/**
        let filter =
            FilterPatterns::from_patterns(&["!**/*.typ".to_string(), "!img/**".to_string()])
                .unwrap();
        copy_html_assets(&project_dir, &output_dir, &filter, None).unwrap();

        assert!(output_dir.join("doc.typ").exists());
        assert!(output_dir.join("img/photo.jpg").exists());
        assert!(!output_dir.join("data.json").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_html_assets_exclude_temps() {
        let temp_dir = std::env::temp_dir().join("rheo_test_html_assets_exclude");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");
        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        fs::write(project_dir.join("doc.txt"), "content").unwrap();
        fs::write(project_dir.join("temp.tmp"), "temp").unwrap();

        // Exclude .tmp files
        let filter = FilterPatterns::from_patterns(&["*.tmp".to_string()]).unwrap();
        copy_html_assets(&project_dir, &output_dir, &filter, None).unwrap();

        assert!(output_dir.join("doc.txt").exists());
        assert!(!output_dir.join("temp.tmp").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_html_assets_with_content_dir() {
        let temp_dir = std::env::temp_dir().join("rheo_test_html_assets_content");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let content_dir = project_dir.join("content");
        let img_dir = content_dir.join("img");
        let output_dir = temp_dir.join("output");

        fs::create_dir_all(&img_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        fs::write(img_dir.join("photo.jpg"), "data").unwrap();

        let filter = FilterPatterns::from_patterns(&["!img/**".to_string()]).unwrap();
        copy_html_assets(
            &project_dir,
            &output_dir,
            &filter,
            Some(Path::new("content")),
        )
        .unwrap();

        // Should copy to output/img/ (not output/content/img/)
        assert!(output_dir.join("img/photo.jpg").exists());
        assert!(!output_dir.join("content").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_html_assets_mixed_include_exclude() {
        let temp_dir = std::env::temp_dir().join("rheo_test_html_assets_mixed");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");
        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        let img_dir = project_dir.join("img");
        fs::create_dir_all(&img_dir).unwrap();

        fs::write(project_dir.join("doc.typ"), "content").unwrap();
        fs::write(img_dir.join("photo.jpg"), "data").unwrap();
        fs::write(img_dir.join("temp.tmp"), "temp").unwrap();
        fs::write(project_dir.join("data.json"), "{}").unwrap();

        // Include .typ and img, but exclude .tmp
        let filter = FilterPatterns::from_patterns(&[
            "!**/*.typ".to_string(),
            "!img/**".to_string(),
            "*.tmp".to_string(),
        ])
        .unwrap();
        copy_html_assets(&project_dir, &output_dir, &filter, None).unwrap();

        assert!(output_dir.join("doc.typ").exists());
        assert!(output_dir.join("img/photo.jpg").exists());
        assert!(!output_dir.join("img/temp.tmp").exists()); // Excluded
        assert!(!output_dir.join("data.json").exists()); // Not included

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_html_assets_empty_filter_includes_all() {
        let temp_dir = std::env::temp_dir().join("rheo_test_html_assets_empty");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");
        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        fs::write(project_dir.join("doc.typ"), "content").unwrap();
        fs::write(project_dir.join("image.png"), "data").unwrap();

        // Empty filter should include everything
        let filter = FilterPatterns::from_patterns(&[]).unwrap();
        copy_html_assets(&project_dir, &output_dir, &filter, None).unwrap();

        assert!(output_dir.join("doc.typ").exists());
        assert!(output_dir.join("image.png").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_html_assets_nested_directories() {
        let temp_dir = std::env::temp_dir().join("rheo_test_html_assets_nested");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");
        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        let img_dir = project_dir.join("img");
        let icons_dir = img_dir.join("icons");
        fs::create_dir_all(&icons_dir).unwrap();

        fs::write(img_dir.join("photo.jpg"), "data").unwrap();
        fs::write(icons_dir.join("star.svg"), "svg").unwrap();

        let filter = FilterPatterns::from_patterns(&["!img/**".to_string()]).unwrap();
        copy_html_assets(&project_dir, &output_dir, &filter, None).unwrap();

        assert!(output_dir.join("img/photo.jpg").exists());
        assert!(output_dir.join("img/icons/star.svg").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
