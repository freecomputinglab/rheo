use crate::{Result, RheoError};
use globset::{Glob, GlobSetBuilder};
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

/// Path to the CSS resources directory
pub const CSS_RESOURCES_DIR: &str = "src/css";

/// Copy CSS files to output directory
///
/// Looks for style.css in the project directory first,
/// falls back to src/css/style.css if not found
pub fn copy_css(project_dir: &Path, output_dir: &Path) -> Result<()> {
    let project_css = project_dir.join("style.css");
    let fallback_css = Path::new(CSS_RESOURCES_DIR).join("style.css");

    let source_css = if project_css.exists() {
        project_css
    } else {
        fallback_css
    };

    let dest_css = output_dir.join("style.css");

    fs::copy(&source_css, &dest_css)
        .map_err(|e| RheoError::AssetCopy {
            source: source_css,
            dest: dest_css,
            error: e,
        })?;

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
/// Glob patterns are relative to project_dir
/// Files maintain their relative directory structure in the output
///
/// If content_dir is provided, it will be stripped from the output path for files
/// that are matched inside that directory. This prevents the content_dir from
/// appearing in the output structure.
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

    // Walk project directory and copy matching files
    for entry in WalkDir::new(project_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        // Get relative path for glob matching
        let relative_path = match path.strip_prefix(project_dir) {
            Ok(rel) => rel,
            Err(_) => continue,
        };

        // Check if file matches any pattern
        if globset.is_match(relative_path) {
            // Strip content_dir prefix if the file is inside it
            let output_relative_path = if let Some(content_dir) = content_dir {
                relative_path.strip_prefix(content_dir).unwrap_or(relative_path)
            } else {
                relative_path
            };

            let dest_path = output_dir.join(output_relative_path);

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

        // Don't create project CSS, so it should use fallback
        // The fallback CSS should exist in the repo at src/css/style.css

        // Copy CSS
        copy_css(&project_dir, &output_dir).expect("Failed to copy CSS");

        // Verify some CSS was copied
        assert!(output_dir.join("style.css").exists());

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
}
