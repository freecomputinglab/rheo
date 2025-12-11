use crate::helpers::comparison::{BinaryFileMetadata, extract_pdf_metadata};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Update HTML reference files from test output
pub fn update_html_references(
    test_name: &str,
    actual_dir: &Path,
    project_path: &Path,
) -> Result<(), String> {
    // Determine reference path based on project location
    let ref_base = if project_path.starts_with("examples/") {
        PathBuf::from("tests/ref/examples")
    } else if project_path.starts_with("tests/cases/") {
        PathBuf::from("tests/ref/cases")
    } else {
        PathBuf::from("tests/ref/examples") // fallback
    };

    let ref_dir = ref_base.join(test_name).join("html");

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

/// Update PDF metadata references from test output
pub fn update_pdf_references(test_name: &str, actual_dir: &Path) -> Result<(), String> {
    // Determine reference path based on actual_dir location
    let ref_base = if actual_dir.starts_with("examples/") {
        PathBuf::from("tests/ref/examples")
    } else if actual_dir.starts_with("tests/cases/") {
        PathBuf::from("tests/ref/cases")
    } else {
        PathBuf::from("tests/ref/examples") // fallback
    };

    let ref_dir = ref_base.join(test_name).join("pdf");

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
        let contents = fs::read(file_path)
            .map_err(|e| format!("Failed to read file contents: {}", e))?;
        let digest = Sha256::digest(&contents);
        Some(format!("{:x}", digest))
    } else {
        None
    };

    // Detect source file location to create repo-relative path
    // For CSS files, use the common source location since they're copied from src/css/
    let repo_relative_path = if filetype == "css" {
        PathBuf::from("src/css").join(rel_path)
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
