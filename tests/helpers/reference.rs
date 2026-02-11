use crate::helpers::comparison::{BinaryFileMetadata, extract_pdf_metadata};
use crate::helpers::is_single_file_test;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Compute a short hash of a file path for reference directory naming
fn compute_file_hash(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:08x}", hasher.finish())
}

/// Update HTML reference files from test output
pub fn update_html_references(
    test_name: &str,
    actual_dir: &Path,
    project_path: &Path,
) -> Result<(), String> {
    // Determine if this is a single-file test
    let ref_dir = if test_name.contains("_slash")
        && (test_name.contains("_full_stop") || test_name.ends_with("typ"))
    {
        // Single-file test - use hash-based path
        let file_path_guess = test_name
            .replace("_slash", "/")
            .replace("_full_stop", ".")
            .replace("_colon", ":")
            .replace("_minus", "-");

        if is_single_file_test(&file_path_guess) {
            let file_path = Path::new(&file_path_guess);
            let hash = compute_file_hash(file_path);
            let filename = file_path
                .file_stem()
                .unwrap_or(file_path.as_os_str())
                .to_string_lossy();

            PathBuf::from("tests/ref/files")
                .join(&hash)
                .join(filename.as_ref())
                .join("html")
        } else {
            // Fallback to project-based path
            get_project_ref_dir(project_path, test_name, "html")
        }
    } else {
        // Project-based test
        get_project_ref_dir(project_path, test_name, "html")
    };

    // Remove existing references
    if ref_dir.exists() {
        fs::remove_dir_all(&ref_dir)
            .map_err(|e| format!("Failed to remove old references: {}", e))?;
    }

    // Copy all files from actual to reference, replacing binary files with .metadata.json
    copy_directory_with_binary_refs(actual_dir, &ref_dir, project_path)?;

    println!(
        "Updated HTML references for {} at {}",
        test_name,
        ref_dir.display()
    );
    Ok(())
}

/// Get project-based reference directory
fn get_project_ref_dir(project_path: &Path, test_name: &str, output_type: &str) -> PathBuf {
    let ref_base = if project_path.starts_with("examples/") {
        PathBuf::from("tests/ref/examples")
    } else if project_path.starts_with("tests/cases/") {
        PathBuf::from("tests/ref/cases")
    } else {
        PathBuf::from("tests/ref/examples") // fallback
    };
    ref_base.join(test_name).join(output_type)
}

/// Update PDF metadata references from test output
pub fn update_pdf_references(test_name: &str, actual_dir: &Path) -> Result<(), String> {
    // Determine if this is a single-file test
    let ref_dir = if test_name.contains("_slash")
        && (test_name.contains("_full_stop") || test_name.ends_with("typ"))
    {
        // Single-file test - use hash-based path
        let file_path_guess = test_name
            .replace("_slash", "/")
            .replace("_full_stop", ".")
            .replace("_colon", ":")
            .replace("_minus", "-");

        if is_single_file_test(&file_path_guess) {
            let file_path = Path::new(&file_path_guess);
            let hash = compute_file_hash(file_path);
            let filename = file_path
                .file_stem()
                .unwrap_or(file_path.as_os_str())
                .to_string_lossy();

            PathBuf::from("tests/ref/files")
                .join(&hash)
                .join(filename.as_ref())
                .join("pdf")
        } else {
            // Fallback to project-based path
            let ref_base = if actual_dir.starts_with("examples/") {
                PathBuf::from("tests/ref/examples")
            } else if actual_dir.starts_with("tests/cases/") {
                PathBuf::from("tests/ref/cases")
            } else {
                PathBuf::from("tests/ref/examples")
            };
            ref_base.join(test_name).join("pdf")
        }
    } else {
        // Project-based test
        let ref_base = if actual_dir.starts_with("examples/") {
            PathBuf::from("tests/ref/examples")
        } else if actual_dir.starts_with("tests/cases/") {
            PathBuf::from("tests/ref/cases")
        } else {
            PathBuf::from("tests/ref/examples")
        };
        ref_base.join(test_name).join("pdf")
    };

    // Remove existing references
    if ref_dir.exists() {
        fs::remove_dir_all(&ref_dir)
            .map_err(|e| format!("Failed to remove old references: {}", e))?;
    }

    // Create reference directory
    fs::create_dir_all(&ref_dir)
        .map_err(|e| format!("Failed to create PDF reference directory: {}", e))?;

    // Find all PDF files in actual output
    for entry in WalkDir::new(actual_dir) {
        if let Ok(entry) = entry
            && entry.file_type().is_file()
            && let Some(ext) = entry.path().extension()
            && ext == "pdf"
        {
            // Extract metadata
            let metadata = extract_pdf_metadata(entry.path())?;

            // Get relative path
            let rel_path = entry
                .path()
                .strip_prefix(actual_dir)
                .map_err(|e| format!("Failed to get relative path: {}", e))?;

            // Save metadata JSON
            let metadata_file = ref_dir.join(format!(
                "{}.metadata.json",
                rel_path.file_stem().unwrap().to_string_lossy()
            ));

            let json = serde_json::to_string_pretty(&metadata)
                .map_err(|e| format!("Failed to serialize metadata: {}", e))?;

            fs::write(&metadata_file, json)
                .map_err(|e| format!("Failed to write metadata: {}", e))?;

            println!("Updated PDF metadata for {}", rel_path.display());
        }
    }

    Ok(())
}

/// Update EPUB metadata references from test output
pub fn update_epub_references(test_name: &str, actual_dir: &Path) -> Result<(), String> {
    use crate::helpers::comparison::{extract_epub_metadata, extract_epub_xhtml};

    // Determine if this is a single-file test
    let ref_dir = if test_name.contains("_slash")
        && (test_name.contains("_full_stop") || test_name.ends_with("typ"))
    {
        // Single-file test - use hash-based path
        let file_path_guess = test_name
            .replace("_slash", "/")
            .replace("_full_stop", ".")
            .replace("_colon", ":")
            .replace("_minus", "-");

        if is_single_file_test(&file_path_guess) {
            let file_path = Path::new(&file_path_guess);
            let hash = compute_file_hash(file_path);
            let filename = file_path
                .file_stem()
                .unwrap_or(file_path.as_os_str())
                .to_string_lossy();

            PathBuf::from("tests/ref/files")
                .join(&hash)
                .join(filename.as_ref())
                .join("epub")
        } else {
            // Fallback to project-based path
            let ref_base = if actual_dir.starts_with("examples/") {
                PathBuf::from("tests/ref/examples")
            } else if actual_dir.starts_with("tests/cases/") {
                PathBuf::from("tests/ref/cases")
            } else {
                PathBuf::from("tests/ref/examples")
            };
            ref_base.join(test_name).join("epub")
        }
    } else {
        // Project-based test
        let ref_base = if actual_dir.starts_with("examples/") {
            PathBuf::from("tests/ref/examples")
        } else if actual_dir.starts_with("tests/cases/") {
            PathBuf::from("tests/ref/cases")
        } else {
            PathBuf::from("tests/ref/examples")
        };
        ref_base.join(test_name).join("epub")
    };

    // Remove existing references
    if ref_dir.exists() {
        fs::remove_dir_all(&ref_dir)
            .map_err(|e| format!("Failed to remove old references: {}", e))?;
    }

    // Create reference directory
    fs::create_dir_all(&ref_dir)
        .map_err(|e| format!("Failed to create EPUB reference directory: {}", e))?;

    // Find all EPUB files in actual output
    for entry in WalkDir::new(actual_dir) {
        if let Ok(entry) = entry
            && entry.file_type().is_file()
            && let Some(ext) = entry.path().extension()
            && ext == "epub"
        {
            // Extract metadata
            let metadata = extract_epub_metadata(entry.path())?;

            // Get relative path
            let rel_path = entry
                .path()
                .strip_prefix(actual_dir)
                .map_err(|e| format!("Failed to get relative path: {}", e))?;

            // Save metadata JSON
            let metadata_file = ref_dir.join(format!(
                "{}.metadata.json",
                rel_path.file_stem().unwrap().to_string_lossy()
            ));

            let json = serde_json::to_string_pretty(&metadata)
                .map_err(|e| format!("Failed to serialize metadata: {}", e))?;

            fs::write(&metadata_file, json)
                .map_err(|e| format!("Failed to write metadata: {}", e))?;

            println!("Updated EPUB metadata for {}", rel_path.display());

            // Extract and save XHTML content files
            let xhtml_content = extract_epub_xhtml(entry.path())?;
            let xhtml_dir = ref_dir.join("xhtml");
            fs::create_dir_all(&xhtml_dir)
                .map_err(|e| format!("Failed to create XHTML directory: {}", e))?;

            for (filename, content) in xhtml_content {
                let xhtml_path = xhtml_dir.join(&filename);
                // Create parent dirs if nested (e.g., chapters/ch1.xhtml)
                if let Some(parent) = xhtml_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                fs::write(&xhtml_path, &content)
                    .map_err(|e| format!("Failed to write XHTML: {}", e))?;
                println!("Updated EPUB XHTML: {}", filename);
            }
        }
    }

    println!(
        "Updated EPUB references for {} at {}",
        test_name,
        ref_dir.display()
    );
    Ok(())
}

/// Copy directory recursively, replacing binary files with .metadata.json references
pub fn copy_directory_with_binary_refs(
    src: &Path,
    dst: &Path,
    project_path: &Path,
) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("Failed to create directory: {}", e))?;

    for entry in WalkDir::new(src) {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let rel_path = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| format!("Failed to get relative path: {}", e))?;
        let dst_path = dst.join(rel_path);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&dst_path)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        } else if is_binary_file(entry.path()) {
            // For binary files, create .metadata.json instead of copying
            let metadata = create_binary_metadata(entry.path(), rel_path, project_path)?;

            let metadata_path = dst_path.with_extension("metadata.json");
            let json = serde_json::to_string_pretty(&metadata)
                .map_err(|e| format!("Failed to serialize metadata: {}", e))?;

            fs::write(&metadata_path, json)
                .map_err(|e| format!("Failed to write metadata: {}", e))?;
        } else {
            // Copy text files normally
            fs::copy(entry.path(), &dst_path).map_err(|e| format!("Failed to copy file: {}", e))?;
        }
    }

    Ok(())
}

/// Check if a file is a binary file based on extension
fn is_binary_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();
        matches!(
            ext_str.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "mp4" | "webm" | "pdf" | "css"
        )
    } else {
        false
    }
}

/// Create metadata for a binary file
fn create_binary_metadata(
    file_path: &Path,
    rel_path: &Path,
    project_path: &Path,
) -> Result<BinaryFileMetadata, String> {
    let file_size = fs::metadata(file_path)
        .map_err(|e| format!("Failed to read file metadata: {}", e))?
        .len();

    let filetype = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown")
        .to_lowercase();

    // Compute hash for CSS files
    let hash = if filetype == "css" {
        use sha2::{Digest, Sha256};
        let contents =
            fs::read(file_path).map_err(|e| format!("Failed to read file contents: {}", e))?;
        let digest = Sha256::digest(&contents);
        Some(format!("{:x}", digest))
    } else {
        None
    };

    // Detect source file location to create repo-relative path
    // For CSS files, store the build-relative path directly (CSS is at the html root)
    let repo_relative_path = if filetype == "css" {
        PathBuf::from(rel_path)
    } else if project_path.join("content").join(rel_path).exists() {
        project_path.join("content").join(rel_path)
    } else if project_path.join(rel_path).exists() {
        project_path.join(rel_path)
    } else {
        // Fallback: just prepend project path to rel_path
        project_path.join(rel_path)
    };

    Ok(BinaryFileMetadata {
        filetype,
        file_size,
        path: Some(repo_relative_path.to_string_lossy().to_string()),
        page_count: None,
        hash,
    })
}
