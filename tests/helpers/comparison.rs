use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn verify_html_output(test_name: &str, actual_dir: &PathBuf) {
    // Determine reference path based on test location
    // actual_dir is like: examples/blog_site/build/html or tests/cases/pdf_merge/build/html
    let ref_base = if actual_dir.starts_with("examples/") {
        PathBuf::from("tests/ref/examples")
    } else if actual_dir.starts_with("tests/cases/") {
        PathBuf::from("tests/ref/cases")
    } else {
        PathBuf::from("tests/ref/examples") // fallback
    };

    let ref_dir = ref_base.join(test_name).join("html");

    if !ref_dir.exists() {
        panic!(
            "HTML reference not found for {}. Run with UPDATE_REFERENCES=1 to generate.",
            test_name
        );
    }

    // Verify exclusion patterns for blog_site
    if test_name == "blog_site" {
        // HTML should include only .typ files and img/** per rheo.toml
        verify_included_files_present(actual_dir, &[".typ", "img/"])
            .expect("HTML inclusion pattern validation failed");
    }

    // Validate assets
    validate_html_assets(&ref_dir, actual_dir).expect("HTML asset validation failed");

    // Compare HTML files
    for entry in WalkDir::new(&ref_dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "html" {
                        let rel_path = entry.path().strip_prefix(&ref_dir).unwrap();
                        let actual_file = actual_dir.join(rel_path);

                        compare_html_content(entry.path(), &actual_file)
                            .expect("HTML content mismatch");
                    }
                }
            }
        }
    }
}

pub fn verify_pdf_output(test_name: &str, actual_dir: &PathBuf) {
    // Determine reference path based on test location
    // actual_dir is like: examples/blog_site/build/pdf or tests/cases/pdf_merge/build/pdf
    let ref_base = if actual_dir.starts_with("examples/") {
        PathBuf::from("tests/ref/examples")
    } else if actual_dir.starts_with("tests/cases/") {
        PathBuf::from("tests/ref/cases")
    } else {
        PathBuf::from("tests/ref/examples") // fallback
    };

    let ref_dir = ref_base.join(test_name).join("pdf");

    if !ref_dir.exists() {
        panic!(
            "PDF reference not found for {}. Run with UPDATE_REFERENCES=1 to generate.",
            test_name
        );
    }

    // Validate PDF assets match reference
    validate_pdf_assets(&ref_dir, actual_dir).expect("PDF asset validation failed");

    // Compare PDF metadata
    for entry in WalkDir::new(actual_dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "pdf" {
                        let rel_path = entry.path().strip_prefix(actual_dir).unwrap();
                        let metadata_file = ref_dir.join(format!(
                            "{}.metadata.json",
                            rel_path.file_stem().unwrap().to_string_lossy()
                        ));

                        if !metadata_file.exists() {
                            panic!(
                                "PDF metadata reference not found: {}. Run with UPDATE_REFERENCES=1",
                                metadata_file.display()
                            );
                        }

                        // Load reference metadata
                        let ref_metadata_json = std::fs::read_to_string(&metadata_file)
                            .expect("Failed to read reference metadata");
                        let ref_metadata = serde_json::from_str(&ref_metadata_json)
                            .expect("Failed to parse reference metadata");

                        // Extract actual metadata
                        let actual_metadata = extract_pdf_metadata(entry.path())
                            .expect("Failed to extract PDF metadata");

                        // Compare
                        compare_pdf_metadata(&ref_metadata, &actual_metadata)
                            .expect("PDF metadata mismatch");
                    }
                }
            }
        }
    }
}

/// Extract build-relative path from repo-relative metadata path
///
/// Converts paths like:
/// - "examples/blog_site/content/img/foo.jpg" → "img/foo.jpg"
/// - "examples/academic_book/img/bar.png" → "img/bar.png"
///
/// Logic:
/// 1. Strip "examples/<project>/" prefix
/// 2. Strip "content/" if present
/// 3. Return remaining path
fn extract_build_relative_path(repo_relative_path: &str) -> PathBuf {
    let path = PathBuf::from(repo_relative_path);

    // Strip examples/<project>/ prefix (first two components)
    let after_project = path
        .components()
        .skip(2) // Skip "examples" and "<project_name>"
        .collect::<PathBuf>();

    // Strip content/ if it's the first component
    if let Ok(stripped) = after_project.strip_prefix("content") {
        stripped.to_path_buf()
    } else {
        after_project
    }
}

/// Compare HTML content byte-for-byte
fn compare_html_content(reference: &Path, actual: &Path) -> Result<(), String> {
    let ref_content =
        fs::read_to_string(reference).map_err(|e| format!("Failed to read reference: {}", e))?;
    let actual_content =
        fs::read_to_string(actual).map_err(|e| format!("Failed to read actual: {}", e))?;

    if ref_content == actual_content {
        Ok(())
    } else {
        let diff = compute_html_diff(&ref_content, &actual_content);
        Err(format!(
            "HTML content mismatch for {}\n{}",
            reference.display(),
            diff
        ))
    }
}

/// Compute unified diff for HTML content
fn compute_html_diff(reference: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(reference, actual);
    let mut output = String::new();

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        output.push_str(&format!("{}{}", sign, change));
    }

    // Limit output size for readability
    if output.len() > 2000 {
        format!(
            "{}... (truncated, {} bytes total)",
            &output[..2000],
            output.len()
        )
    } else {
        output
    }
}

/// Validate that expected HTML assets are present
fn validate_html_assets(reference_dir: &Path, actual_dir: &Path) -> Result<(), String> {
    let mut errors = Vec::new();

    // Collect all files in reference directory
    let mut ref_files = Vec::new();
    let mut binary_refs = Vec::new(); // Track .metadata.json files

    for entry in WalkDir::new(reference_dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Ok(rel_path) = entry.path().strip_prefix(reference_dir) {
                    // Check if this is a .metadata.json file
                    if rel_path.extension().and_then(|s| s.to_str()) == Some("json")
                        && rel_path.to_string_lossy().ends_with(".metadata.json")
                    {
                        binary_refs.push(rel_path.to_path_buf());
                    } else {
                        ref_files.push(rel_path.to_path_buf());
                    }
                }
            }
        }
    }

    // Check that all reference files exist in actual output
    for ref_file in &ref_files {
        let actual_file = actual_dir.join(ref_file);
        if !actual_file.exists() {
            errors.push(format!("Missing file: {}", ref_file.display()));
        }
    }

    // Check that binary files referenced by .metadata.json exist
    for metadata_file in &binary_refs {
        // Read the metadata to get the actual file path
        let metadata_path = reference_dir.join(metadata_file);
        if let Ok(json_str) = fs::read_to_string(&metadata_path) {
            if let Ok(metadata) = serde_json::from_str::<BinaryFileMetadata>(&json_str) {
                // Check if actual file exists based on metadata path
                if let Some(ref path) = metadata.path {
                    // Convert repo-relative path to build-relative path
                    let build_relative_path = extract_build_relative_path(path);
                    let actual_file = actual_dir.join(&build_relative_path);
                    if !actual_file.exists() {
                        errors.push(format!(
                            "Missing binary file: {} (expected at {})",
                            path,
                            build_relative_path.display()
                        ));
                    }
                } else {
                    // If no path in metadata, derive from .metadata.json filename
                    let file_str = metadata_file.to_string_lossy();
                    let binary_name = file_str.strip_suffix(".metadata.json").unwrap_or(&file_str);
                    let actual_file = actual_dir.join(binary_name);
                    if !actual_file.exists() {
                        errors.push(format!("Missing binary file: {}", binary_name));
                    }
                }
            }
        }
    }

    // Check for unexpected files in actual output
    let mut actual_files = Vec::new();
    for entry in WalkDir::new(actual_dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Ok(rel_path) = entry.path().strip_prefix(actual_dir) {
                    actual_files.push(rel_path.to_path_buf());
                }
            }
        }
    }

    // Build expected files set (includes both text files and binary files from metadata)
    let mut expected_files = std::collections::HashSet::new();
    for ref_file in &ref_files {
        expected_files.insert(ref_file.clone());
    }
    for metadata_file in &binary_refs {
        // Read metadata to get the actual binary file path
        let metadata_path = reference_dir.join(metadata_file);
        if let Ok(json_str) = fs::read_to_string(&metadata_path) {
            if let Ok(metadata) = serde_json::from_str::<BinaryFileMetadata>(&json_str) {
                if let Some(path) = metadata.path {
                    // Convert repo-relative path to build-relative path
                    let build_relative_path = extract_build_relative_path(&path);
                    expected_files.insert(build_relative_path);
                } else {
                    // Derive from .metadata.json filename
                    let file_str = metadata_file.to_string_lossy();
                    let binary_name = file_str
                        .strip_suffix(".metadata.json")
                        .unwrap_or(&file_str)
                        .to_string();
                    expected_files.insert(PathBuf::from(binary_name));
                }
            }
        }
    }

    for actual_file in &actual_files {
        if !expected_files.contains(actual_file) {
            errors.push(format!("Unexpected file: {}", actual_file.display()));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// Binary file metadata for comparison
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BinaryFileMetadata {
    #[serde(default = "default_filetype")]
    pub filetype: String,
    pub file_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<u32>,
}

fn default_filetype() -> String {
    "pdf".to_string()
}

/// Extract PDF metadata (page count, file size)
pub fn extract_pdf_metadata(pdf_path: &Path) -> Result<BinaryFileMetadata, String> {
    use lopdf::Document;

    // Get file size
    let file_size = fs::metadata(pdf_path)
        .map_err(|e| format!("Failed to read PDF metadata: {}", e))?
        .len();

    // Load PDF and count pages
    let doc = Document::load(pdf_path).map_err(|e| format!("Failed to load PDF: {}", e))?;
    let page_count = doc.get_pages().len() as u32;

    Ok(BinaryFileMetadata {
        filetype: "pdf".to_string(),
        file_size,
        path: None,
        page_count: Some(page_count),
    })
}

/// Compare binary file metadata with tolerance
fn compare_pdf_metadata(
    reference: &BinaryFileMetadata,
    actual: &BinaryFileMetadata,
) -> Result<(), String> {
    let mut errors = Vec::new();

    // Filetype must match exactly
    if reference.filetype != actual.filetype {
        errors.push(format!(
            "Filetype mismatch: expected {}, got {}",
            reference.filetype, actual.filetype
        ));
    }

    // Page count must match exactly (if present)
    if reference.page_count != actual.page_count {
        errors.push(format!(
            "Page count mismatch: expected {:?}, got {:?}",
            reference.page_count, actual.page_count
        ));
    }

    // File size tolerance: 10%
    let size_diff = (actual.file_size as i64 - reference.file_size as i64).abs() as u64;
    let tolerance = reference.file_size / 10; // 10%

    if size_diff > tolerance {
        errors.push(format!(
            "File size mismatch beyond 10% tolerance: expected {}, got {} (diff: {})",
            reference.file_size, actual.file_size, size_diff
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// Validate that expected PDF metadata files are present
fn validate_pdf_assets(reference_dir: &Path, actual_dir: &Path) -> Result<(), String> {
    let mut errors = Vec::new();

    // Collect all metadata files in reference directory
    let mut ref_files = Vec::new();
    for entry in WalkDir::new(reference_dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "json" {
                        if let Ok(_rel_path) = entry.path().strip_prefix(reference_dir) {
                            // Convert .metadata.json to .pdf
                            let file_stem = entry
                                .path()
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .and_then(|s| s.strip_suffix(".metadata"))
                                .unwrap_or("");
                            ref_files.push(format!("{}.pdf", file_stem));
                        }
                    }
                }
            }
        }
    }

    // Check that all reference PDFs exist in actual output
    for ref_file in &ref_files {
        let actual_file = actual_dir.join(ref_file);
        if !actual_file.exists() {
            errors.push(format!("Missing file: {}", ref_file));
        }
    }

    // Check for unexpected PDFs in actual output
    let mut actual_files = Vec::new();
    for entry in WalkDir::new(actual_dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "pdf" {
                        if let Ok(rel_path) = entry.path().strip_prefix(actual_dir) {
                            actual_files.push(rel_path.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    for actual_file in &actual_files {
        if !ref_files.contains(actual_file) {
            errors.push(format!("Unexpected file: {}", actual_file));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// Verify that files matching the pattern are present in the output
fn verify_included_files_present(output_dir: &Path, patterns: &[&str]) -> Result<(), String> {
    let mut found = vec![false; patterns.len()];

    for entry in WalkDir::new(output_dir) {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Ok(rel_path) = entry.path().strip_prefix(output_dir) {
                    let path_str = rel_path.to_string_lossy();
                    for (i, pattern) in patterns.iter().enumerate() {
                        if path_str.contains(*pattern) {
                            found[i] = true;
                        }
                    }
                }
            }
        }
    }

    let missing: Vec<_> = patterns
        .iter()
        .enumerate()
        .filter(|(i, _)| !found[*i])
        .map(|(_, p)| *p)
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Expected files not found in output:\n{}",
            missing
                .iter()
                .map(|p| format!("  - {}", p))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}
