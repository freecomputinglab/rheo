use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn verify_html_output(test_name: &str, actual_dir: &Path) {
    let ref_dir = get_reference_dir(actual_dir, test_name, "html");
    ensure_reference_exists(&ref_dir, test_name, "HTML");

    validate_html_assets(&ref_dir, actual_dir).expect("HTML asset validation failed");

    for_each_file_with_ext(&ref_dir, "html", |entry| {
        let rel_path = entry.path().strip_prefix(&ref_dir).unwrap();
        let actual_file = actual_dir.join(rel_path);
        compare_html_content(entry.path(), &actual_file).expect("HTML content mismatch");
    });
}

pub fn verify_pdf_output(test_name: &str, actual_dir: &Path) {
    let ref_dir = get_reference_dir(actual_dir, test_name, "pdf");
    ensure_reference_exists(&ref_dir, test_name, "PDF");

    validate_pdf_assets(&ref_dir, actual_dir).expect("PDF asset validation failed");

    for_each_file_with_ext(actual_dir, "pdf", |entry| {
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

        let ref_metadata_json =
            fs::read_to_string(&metadata_file).expect("Failed to read reference metadata");
        let ref_metadata =
            serde_json::from_str(&ref_metadata_json).expect("Failed to parse reference metadata");
        let actual_metadata =
            extract_pdf_metadata(entry.path()).expect("Failed to extract PDF metadata");

        compare_pdf_metadata(&ref_metadata, &actual_metadata).expect("PDF metadata mismatch");
    });
}

fn get_reference_dir(actual_dir: &Path, test_name: &str, output_type: &str) -> PathBuf {
    let ref_base = if actual_dir.starts_with("examples/") {
        PathBuf::from("tests/ref/examples")
    } else if actual_dir.starts_with("tests/cases/") {
        PathBuf::from("tests/ref/cases")
    } else {
        PathBuf::from("tests/ref/examples")
    };
    ref_base.join(test_name).join(output_type)
}

fn ensure_reference_exists(ref_dir: &Path, test_name: &str, output_type: &str) {
    if !ref_dir.exists() {
        panic!(
            "{} reference not found for {}. Run with UPDATE_REFERENCES=1 to generate.",
            output_type, test_name
        );
    }
}

fn for_each_file_with_ext<F>(dir: &Path, extension: &str, mut callback: F)
where
    F: FnMut(&walkdir::DirEntry),
{
    for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|s| s.to_str()) == Some(extension)
        {
            callback(&entry);
        }
    }
}

fn extract_build_relative_path(repo_relative_path: &str) -> PathBuf {
    let path = PathBuf::from(repo_relative_path);
    let after_project = path.components().skip(2).collect::<PathBuf>();
    after_project
        .strip_prefix("content")
        .unwrap_or(&after_project)
        .to_path_buf()
}

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

fn collect_files_by_predicate<F>(dir: &Path, predicate: F) -> Vec<PathBuf>
where
    F: Fn(&walkdir::DirEntry) -> bool,
{
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && predicate(e))
        .filter_map(|e| e.path().strip_prefix(dir).ok().map(|p| p.to_path_buf()))
        .collect()
}

fn validate_html_assets(reference_dir: &Path, actual_dir: &Path) -> Result<(), String> {
    let mut errors = Vec::new();

    let ref_files = collect_files_by_predicate(reference_dir, |e| {
        !e.path().to_string_lossy().ends_with(".metadata.json")
    });

    let binary_refs = collect_files_by_predicate(reference_dir, |e| {
        e.path().extension().and_then(|s| s.to_str()) == Some("json")
            && e.path().to_string_lossy().ends_with(".metadata.json")
    });

    for ref_file in &ref_files {
        if !actual_dir.join(ref_file).exists() {
            errors.push(format!("Missing file: {}", ref_file.display()));
        }
    }

    for metadata_file in &binary_refs {
        validate_binary_file_from_metadata(reference_dir, actual_dir, metadata_file, &mut errors);
    }

    let actual_files = collect_files_by_predicate(actual_dir, |_| true);
    let expected_files = build_expected_files_set(reference_dir, &ref_files, &binary_refs);

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

fn validate_binary_file_from_metadata(
    reference_dir: &Path,
    actual_dir: &Path,
    metadata_file: &Path,
    errors: &mut Vec<String>,
) {
    let metadata_path = reference_dir.join(metadata_file);
    if let Ok(json_str) = fs::read_to_string(&metadata_path)
        && let Ok(metadata) = serde_json::from_str::<BinaryFileMetadata>(&json_str)
    {
        let build_relative_path = metadata
            .path
            .as_ref()
            .map(|p| extract_build_relative_path(p))
            .unwrap_or_else(|| {
                let file_str = metadata_file.to_string_lossy();
                PathBuf::from(file_str.strip_suffix(".metadata.json").unwrap_or(&file_str))
            });

        let actual_file_path = actual_dir.join(&build_relative_path);

        if !actual_file_path.exists() {
            errors.push(format!(
                "Missing binary file: {} (expected at {})",
                metadata.path.as_deref().unwrap_or(""),
                build_relative_path.display()
            ));
            return;
        }

        // Validate CSS metadata
        if metadata.filetype == "css" {
            match extract_css_metadata(&actual_file_path) {
                Ok(actual_metadata) => {
                    if let Err(e) = compare_css_metadata(&metadata, &actual_metadata) {
                        errors.push(format!(
                            "CSS validation failed for {}: {}",
                            build_relative_path.display(),
                            e
                        ));
                    }
                }
                Err(e) => {
                    errors.push(format!(
                        "Failed to extract CSS metadata for {}: {}",
                        build_relative_path.display(),
                        e
                    ));
                }
            }
        }
    }
}

fn build_expected_files_set(
    reference_dir: &Path,
    ref_files: &[PathBuf],
    binary_refs: &[PathBuf],
) -> std::collections::HashSet<PathBuf> {
    let mut expected_files = ref_files
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();

    for metadata_file in binary_refs {
        let metadata_path = reference_dir.join(metadata_file);
        if let Ok(json_str) = fs::read_to_string(&metadata_path)
            && let Ok(metadata) = serde_json::from_str::<BinaryFileMetadata>(&json_str)
        {
            let build_relative_path = metadata
                .path
                .as_ref()
                .map(|p| extract_build_relative_path(p))
                .unwrap_or_else(|| {
                    let file_str = metadata_file.to_string_lossy();
                    PathBuf::from(file_str.strip_suffix(".metadata.json").unwrap_or(&file_str))
                });
            expected_files.insert(build_relative_path);
        }
    }

    expected_files
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BinaryFileMetadata {
    #[serde(default = "default_filetype")]
    pub filetype: String,
    pub file_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

fn default_filetype() -> String {
    "pdf".to_string()
}

pub fn extract_pdf_metadata(pdf_path: &Path) -> Result<BinaryFileMetadata, String> {
    use lopdf::Document;

    let file_size = fs::metadata(pdf_path)
        .map_err(|e| format!("Failed to read PDF metadata: {}", e))?
        .len();

    let doc = Document::load(pdf_path).map_err(|e| format!("Failed to load PDF: {}", e))?;
    let page_count = doc.get_pages().len() as u32;

    Ok(BinaryFileMetadata {
        filetype: "pdf".to_string(),
        file_size,
        path: None,
        page_count: Some(page_count),
        hash: None,
    })
}

pub fn extract_css_metadata(css_path: &Path) -> Result<BinaryFileMetadata, String> {
    use sha2::{Digest, Sha256};

    let file_size = fs::metadata(css_path)
        .map_err(|e| format!("Failed to read CSS metadata: {}", e))?
        .len();

    let contents = fs::read(css_path).map_err(|e| format!("Failed to read CSS contents: {}", e))?;

    let hash_bytes = Sha256::digest(&contents);
    let hash = format!("{:x}", hash_bytes);

    Ok(BinaryFileMetadata {
        filetype: "css".to_string(),
        file_size,
        path: None,
        page_count: None,
        hash: Some(hash),
    })
}

fn compare_pdf_metadata(
    reference: &BinaryFileMetadata,
    actual: &BinaryFileMetadata,
) -> Result<(), String> {
    let mut errors = Vec::new();

    if reference.filetype != actual.filetype {
        errors.push(format!(
            "Filetype mismatch: expected {}, got {}",
            reference.filetype, actual.filetype
        ));
    }

    if reference.page_count != actual.page_count {
        errors.push(format!(
            "Page count mismatch: expected {:?}, got {:?}",
            reference.page_count, actual.page_count
        ));
    }

    let size_diff = (actual.file_size as i64 - reference.file_size as i64).unsigned_abs();
    let tolerance = reference.file_size / 10;

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

fn compare_css_metadata(
    reference: &BinaryFileMetadata,
    actual: &BinaryFileMetadata,
) -> Result<(), String> {
    let mut errors = Vec::new();

    if reference.filetype != actual.filetype {
        errors.push(format!(
            "Filetype mismatch: expected {}, got {}",
            reference.filetype, actual.filetype
        ));
    }

    if reference.hash != actual.hash {
        errors.push(format!(
            "Hash mismatch: expected {:?}, got {:?}",
            reference.hash, actual.hash
        ));
    }

    if reference.file_size != actual.file_size {
        errors.push(format!(
            "File size mismatch: expected {}, got {}",
            reference.file_size, actual.file_size
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn validate_pdf_assets(reference_dir: &Path, actual_dir: &Path) -> Result<(), String> {
    let ref_files: Vec<String> = collect_files_by_predicate(reference_dir, |e| {
        e.path().extension().and_then(|s| s.to_str()) == Some("json")
    })
    .into_iter()
    .filter_map(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".metadata"))
            .map(|s| format!("{}.pdf", s))
    })
    .collect();

    let actual_files: Vec<String> = collect_files_by_predicate(actual_dir, |e| {
        e.path().extension().and_then(|s| s.to_str()) == Some("pdf")
    })
    .into_iter()
    .map(|p| p.to_string_lossy().to_string())
    .collect();

    let mut errors = Vec::new();

    for ref_file in &ref_files {
        if !actual_dir.join(ref_file).exists() {
            errors.push(format!("Missing file: {}", ref_file));
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

#[allow(unused)]
fn verify_included_files_present(output_dir: &Path, patterns: &[&str]) -> Result<(), String> {
    let mut found = vec![false; patterns.len()];

    for_each_file_with_ext(output_dir, "", |entry| {
        if let Ok(rel_path) = entry.path().strip_prefix(output_dir) {
            let path_str = rel_path.to_string_lossy();
            for (i, pattern) in patterns.iter().enumerate() {
                if path_str.contains(*pattern) {
                    found[i] = true;
                }
            }
        }
    });

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
