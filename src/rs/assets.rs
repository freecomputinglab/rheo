use crate::{Result, RheoError};
use std::fs;
use std::path::Path;

/// Path to the shared Typst resources directory
pub const TYPST_RESOURCES_DIR: &str = "src/typst";

/// Copy CSS files to output directory
///
/// Looks for style.css in the project directory first,
/// falls back to src/typst/style.css if not found
pub fn copy_css(project_dir: &Path, output_dir: &Path) -> Result<()> {
    let project_css = project_dir.join("style.css");
    let fallback_css = Path::new(TYPST_RESOURCES_DIR).join("style.css");

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
        // The fallback CSS should exist in the repo at src/typst/style.css

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
}
