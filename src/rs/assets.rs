use crate::{Result, RheoError};
use globset::{Glob, GlobSetBuilder};
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

/// Default CSS embedded at compile time
const DEFAULT_CSS: &str = include_str!("../css/style.css");

/// Copy CSS files to output directory
///
/// Looks for style.css in the project directory first,
/// falls back to embedded default CSS if not found
pub fn copy_css(project_dir: &Path, output_dir: &Path) -> Result<()> {
    let project_css = project_dir.join("style.css");

    let css_content = if project_css.exists() {
        // User provided custom CSS - read from file
        fs::read_to_string(&project_css)
            .map_err(|e| RheoError::io(e, format!("reading project CSS {:?}", project_css)))?
    } else {
        // Use embedded default CSS
        DEFAULT_CSS.to_string()
    };

    let dest_css = output_dir.join("style.css");

    fs::write(&dest_css, css_content)
        .map_err(|e| RheoError::io(e, format!("writing CSS to {:?}", dest_css)))?;

    Ok(())
}

/// Copy img/ directory to output directory if it exists
pub fn copy_images(project_dir: &Path, output_dir: &Path) -> Result<()> {
    let source_img = project_dir.join("img");

    // If img/ directory doesn't exist, that's OK - not all projects have images
    if !source_img.exists() {
        return Ok(());
    }

    let dest_img = output_dir.join("img");

    // Create destination img/ directory
    fs::create_dir_all(&dest_img)
        .map_err(|e| RheoError::io(e, format!("creating image directory {:?}", dest_img)))?;

    // Recursively copy all files from source_img to dest_img
    copy_dir_recursive(&source_img, &dest_img)?;

    Ok(())
}

/// Copy static files matching glob patterns from project to output directory
///
/// If content_dir is provided, glob patterns are evaluated relative to project_dir/content_dir
/// and files are searched only within that directory. Otherwise, patterns are relative to project_dir.
///
/// Files maintain their relative directory structure in the output.
pub fn copy_static_files(
    project_dir: &Path,
    output_dir: &Path,
    patterns: &[String],
    content_dir: Option<&Path>,
) -> Result<()> {
    if patterns.is_empty() {
        debug!("no static file patterns configured");
        return Ok(());
    }

    // Determine the root directory to search
    let search_root = if let Some(content_dir) = content_dir {
        project_dir.join(content_dir)
    } else {
        project_dir.to_path_buf()
    };

    // Build glob set from patterns
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match Glob::new(pattern) {
            Ok(glob) => {
                builder.add(glob);
                debug!(pattern = %pattern, "added static file pattern");
            }
            Err(e) => {
                warn!(pattern = %pattern, error = %e, "invalid glob pattern, skipping");
            }
        }
    }

    let globset = builder
        .build()
        .map_err(|e| RheoError::project_config(format!("failed to build static file patterns: {}", e)))?;

    let mut copied_count = 0;

    // Walk search root directory and copy matching files
    for entry in WalkDir::new(&search_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        // Get relative path for glob matching (relative to search_root)
        let relative_path = match path.strip_prefix(&search_root) {
            Ok(rel) => rel,
            Err(_) => continue,
        };

        // Check if file matches any pattern
        if globset.is_match(relative_path) {
            let dest_path = output_dir.join(relative_path);

            // Create parent directory if needed
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| RheoError::io(e, format!("creating directory {:?}", parent)))?;
            }

            // Copy file
            fs::copy(path, &dest_path)
                .map_err(|e| RheoError::AssetCopy {
                    source: path.to_path_buf(),
                    dest: dest_path.clone(),
                    error: e,
                })?;

            debug!(file = %relative_path.display(), "copied static file");
            copied_count += 1;
        }
    }

    if copied_count > 0 {
        info!(count = copied_count, "copied static files");
    }

    Ok(())
}

/// Recursively copy a directory and all its contents
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src)
        .map_err(|e| RheoError::io(e, format!("reading directory {:?}", src)))?
    {
        let entry = entry
            .map_err(|e| RheoError::io(e, "reading directory entry"))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dst.join(file_name);

        if path.is_dir() {
            fs::create_dir_all(&dest_path)
                .map_err(|e| RheoError::io(e, format!("creating directory {:?}", dest_path)))?;
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path)
                .map_err(|e| RheoError::AssetCopy {
                    source: path.clone(),
                    dest: dest_path.clone(),
                    error: e,
                })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_copy_css_with_project_css() {
        let temp_dir = std::env::temp_dir().join("rheo_test_assets_css_project");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");

        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Create a project-specific CSS file
        let project_css_content = "/* project specific */";
        fs::write(project_dir.join("style.css"), project_css_content).unwrap();

        // Copy CSS
        copy_css(&project_dir, &output_dir).expect("Failed to copy CSS");

        // Verify the project CSS was copied, not the fallback
        let copied_content = fs::read_to_string(output_dir.join("style.css")).unwrap();
        assert_eq!(copied_content, project_css_content);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_css_with_fallback() {
        let temp_dir = std::env::temp_dir().join("rheo_test_assets_css_fallback");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");

        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Don't create project CSS, so it should use embedded default

        // Copy CSS
        copy_css(&project_dir, &output_dir).expect("Failed to copy CSS");

        // Verify CSS file was created
        assert!(output_dir.join("style.css").exists());

        // Verify it contains the embedded default CSS (should match DEFAULT_CSS constant)
        let copied_content = fs::read_to_string(output_dir.join("style.css")).unwrap();
        assert_eq!(copied_content, DEFAULT_CSS);

        // Sanity check: embedded CSS should contain some expected content
        assert!(copied_content.contains(":root"));
        assert!(copied_content.contains("--max-width"));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_images_with_directory() {
        let temp_dir = std::env::temp_dir().join("rheo_test_assets_images");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");

        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Create img/ directory with test files
        let img_dir = project_dir.join("img");
        fs::create_dir_all(&img_dir).unwrap();
        fs::write(img_dir.join("test.png"), b"fake png data").unwrap();
        fs::write(img_dir.join("test.jpg"), b"fake jpg data").unwrap();

        // Create nested subdirectory
        let subdir = img_dir.join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("nested.gif"), b"fake gif data").unwrap();

        // Copy images
        copy_images(&project_dir, &output_dir).expect("Failed to copy images");

        // Verify all files were copied
        assert!(output_dir.join("img/test.png").exists());
        assert!(output_dir.join("img/test.jpg").exists());
        assert!(output_dir.join("img/subdir/nested.gif").exists());

        // Verify content
        let png_content = fs::read(output_dir.join("img/test.png")).unwrap();
        assert_eq!(png_content, b"fake png data");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_images_no_directory() {
        let temp_dir = std::env::temp_dir().join("rheo_test_assets_no_images");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");

        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Don't create img/ directory

        // Copy images should succeed but do nothing
        copy_images(&project_dir, &output_dir).expect("Should succeed even with no img/ directory");

        // Verify no img/ directory was created in output
        assert!(!output_dir.join("img").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_static_files_with_patterns() {
        let temp_dir = std::env::temp_dir().join("rheo_test_static_files");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");

        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Create test files in various directories
        let css_dir = project_dir.join("css");
        fs::create_dir_all(&css_dir).unwrap();
        fs::write(css_dir.join("style.css"), b"/* css */").unwrap();
        fs::write(css_dir.join("theme.css"), b"/* theme */").unwrap();

        let img_dir = project_dir.join("img");
        fs::create_dir_all(&img_dir).unwrap();
        fs::write(img_dir.join("photo.jpg"), b"jpeg data").unwrap();

        let data_dir = project_dir.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("config.json"), b"{}").unwrap();
        fs::write(data_dir.join("readme.txt"), b"text file").unwrap();

        // Copy with glob patterns
        let patterns = vec![
            "css/**".to_string(),
            "img/**".to_string(),
            "data/*.json".to_string(),
        ];

        copy_static_files(&project_dir, &output_dir, &patterns, None)
            .expect("Failed to copy static files");

        // Verify CSS files were copied
        assert!(output_dir.join("css/style.css").exists());
        assert!(output_dir.join("css/theme.css").exists());

        // Verify image was copied
        assert!(output_dir.join("img/photo.jpg").exists());

        // Verify only .json was copied from data/
        assert!(output_dir.join("data/config.json").exists());
        assert!(!output_dir.join("data/readme.txt").exists());

        // Verify content
        let css_content = fs::read(output_dir.join("css/style.css")).unwrap();
        assert_eq!(css_content, b"/* css */");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_static_files_empty_patterns() {
        let temp_dir = std::env::temp_dir().join("rheo_test_static_empty");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");

        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Create a test file
        fs::write(project_dir.join("test.txt"), b"test").unwrap();

        // Copy with empty patterns
        let patterns: Vec<String> = vec![];
        copy_static_files(&project_dir, &output_dir, &patterns, None)
            .expect("Should succeed with empty patterns");

        // Verify no files were copied
        assert!(!output_dir.join("test.txt").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_copy_static_files_with_content_dir() {
        let temp_dir = std::env::temp_dir().join("rheo_test_static_content_dir");
        let _ = fs::remove_dir_all(&temp_dir);

        let project_dir = temp_dir.join("project");
        let output_dir = temp_dir.join("output");

        fs::create_dir_all(&project_dir).unwrap();
        fs::create_dir_all(&output_dir).unwrap();

        // Create content directory structure
        let content_dir = project_dir.join("content");
        let img_dir = content_dir.join("img");
        fs::create_dir_all(&img_dir).unwrap();
        fs::write(img_dir.join("photo.jpg"), b"jpeg data").unwrap();
        fs::write(img_dir.join("logo.png"), b"png data").unwrap();

        // Create subdirectory in img
        let subdir = img_dir.join("icons");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("star.svg"), b"svg data").unwrap();

        // Copy with content_dir - patterns are relative to content_dir
        let patterns = vec!["img/**".to_string()];
        copy_static_files(&project_dir, &output_dir, &patterns, Some(Path::new("content")))
            .expect("Failed to copy static files");

        // Verify files were copied to output_dir/img/ (NOT output_dir/content/img/)
        assert!(output_dir.join("img/photo.jpg").exists());
        assert!(output_dir.join("img/logo.png").exists());
        assert!(output_dir.join("img/icons/star.svg").exists());

        // Verify content_dir is NOT in the output path
        assert!(!output_dir.join("content").exists());

        // Verify content
        let jpg_content = fs::read(output_dir.join("img/photo.jpg")).unwrap();
        assert_eq!(jpg_content, b"jpeg data");

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
