use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::is_single_file_test;

pub fn verify_html_output(test_name: &str, actual_dir: &Path) {
    let ref_dir = get_reference_dir(actual_dir, test_name, "html");
    ensure_reference_exists(&ref_dir, test_name, "HTML");

    validate_html_assets(&ref_dir, actual_dir).expect("HTML asset validation failed");

    for_each_file_with_ext(&ref_dir, "html", |entry| {
        let rel_path = entry.path().strip_prefix(&ref_dir).unwrap();
        let actual_file = actual_dir.join(rel_path);
        compare_html_content(entry.path(), &actual_file, test_name).expect("HTML content mismatch");
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

/// Compute a short hash of a file path for reference directory naming
fn compute_file_hash(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:08x}", hasher.finish())
}

fn get_reference_dir(actual_dir: &Path, test_name: &str, output_type: &str) -> PathBuf {
    // Check if this is a single-file test (test name contains file path components)
    if test_name.contains("_slash")
        && (test_name.contains("_full_stop") || test_name.ends_with("typ"))
    {
        // This is likely a single-file test
        // Extract the original file path from the sanitized test name
        let file_path_guess = test_name
            .replace("_slash", "/")
            .replace("_full_stop", ".")
            .replace("_colon", ":")
            .replace("_minus", "-");

        // Check if this is a single .typ file test
        if is_single_file_test(&file_path_guess) {
            let file_path = Path::new(&file_path_guess);
            let hash = compute_file_hash(file_path);
            let filename = file_path
                .file_stem()
                .unwrap_or(file_path.as_os_str())
                .to_string_lossy();

            return PathBuf::from("tests/ref/files")
                .join(&hash)
                .join(filename.as_ref())
                .join(output_type);
        }
    }

    // Default: project-based references
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

fn compare_html_content(reference: &Path, actual: &Path, test_name: &str) -> Result<(), String> {
    let ref_content =
        fs::read_to_string(reference).map_err(|e| format!("Failed to read reference: {}", e))?;
    let actual_content =
        fs::read_to_string(actual).map_err(|e| format!("Failed to read actual: {}", e))?;

    if ref_content == actual_content {
        Ok(())
    } else {
        let diff = compute_html_diff(&ref_content, &actual_content);

        // Generate test-specific update command
        let test_name_sanitized = test_name
            .replace('/', "_slash")
            .replace('.', "_full_stop")
            .replace(':', "_colon")
            .replace('-', "_minus");
        let update_cmd = format!(
            "UPDATE_REFERENCES=1 cargo test run_test_case_{} -- --nocapture",
            test_name_sanitized
        );

        // Check diff limit for additional suggestion
        let diff_limit = env::var("RHEO_TEST_DIFF_LIMIT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(2000);

        let mut error_msg = format!(
            "HTML content mismatch for {}\n\n{}\n\nTo update this reference, run:\n  {}",
            reference.display(),
            diff,
            update_cmd
        );

        // If diff was truncated, suggest increasing the limit
        if diff.contains("showing first") {
            error_msg.push_str(&format!(
                "\n\nOr to see full diff:\n  RHEO_TEST_DIFF_LIMIT={} cargo test run_test_case_{} -- --nocapture",
                diff_limit * 5,
                test_name_sanitized
            ));
        }

        Err(error_msg)
    }
}

fn compute_html_diff(reference: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(reference, actual);

    // Collect statistics
    let mut insertions = 0;
    let mut deletions = 0;
    let mut unchanged = 0;

    let mut diff_output = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => {
                deletions += 1;
                "-"
            }
            ChangeTag::Insert => {
                insertions += 1;
                "+"
            }
            ChangeTag::Equal => {
                unchanged += 1;
                " "
            }
        };
        diff_output.push_str(&format!("{}{}", sign, change));
    }

    // Get diff limit from environment variable or use default
    let diff_limit = env::var("RHEO_TEST_DIFF_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);

    // Build output with statistics
    let mut output = String::new();
    output.push_str(&format!(
        "Diff: {} insertions(+), {} deletions(-), {} lines unchanged\n\n",
        insertions, deletions, unchanged
    ));

    if diff_output.len() > diff_limit {
        output.push_str(&diff_output[..diff_limit]);
        output.push_str(&format!(
            "\n\n... (showing first {} chars of {} bytes total)",
            diff_limit,
            diff_output.len()
        ));
    } else {
        output.push_str(&diff_output);
    }

    output
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
            errors.push(format!(
                "Missing file: {}\n      Expected in: {}\n      Referenced from: {}",
                ref_file.display(),
                actual_dir.display(),
                reference_dir.display()
            ));
        }
    }

    for metadata_file in &binary_refs {
        validate_binary_file_from_metadata(reference_dir, actual_dir, metadata_file, &mut errors);
    }

    let actual_files = collect_files_by_predicate(actual_dir, |_| true);
    let expected_files = build_expected_files_set(reference_dir, &ref_files, &binary_refs);

    for actual_file in &actual_files {
        if !expected_files.contains(actual_file) {
            errors.push(format!(
                "Unexpected file: {}\n      Found in: {}\n      Not defined in references: {}\n      This file may need to be added to reference or excluded in rheo.toml",
                actual_file.display(),
                actual_dir.join(actual_file).display(),
                reference_dir.display()
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let mut error_msg = String::from("HTML asset validation failed:\n\n");
        for (i, error) in errors.iter().enumerate() {
            error_msg.push_str(&format!("{}. {}\n\n", i + 1, error));
        }
        Err(error_msg)
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EpubMetadata {
    pub filetype: String,
    pub file_size: u64,
    pub title: String,
    pub language: String,
    pub spine_files: Vec<String>,
    pub has_nav: bool,
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

pub fn extract_epub_metadata(epub_path: &Path) -> Result<EpubMetadata, String> {
    use rheo::formats::epub::package::Package;
    use std::io::Read;
    use zip::ZipArchive;

    // Get file size
    let file_size = fs::metadata(epub_path)
        .map_err(|e| format!("Failed to read EPUB metadata: {}", e))?
        .len();

    // Open EPUB as ZIP archive
    let file = fs::File::open(epub_path).map_err(|e| format!("Failed to open EPUB file: {}", e))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read EPUB archive: {}", e))?;

    // Read package.opf
    let opf_contents = {
        let mut opf_file = archive
            .by_name("EPUB/package.opf")
            .map_err(|e| format!("Failed to find package.opf: {}", e))?;
        let mut contents = String::new();
        opf_file
            .read_to_string(&mut contents)
            .map_err(|e| format!("Failed to read package.opf: {}", e))?;
        contents
    };

    // Parse package.opf XML
    let package: Package = serde_xml_rs::from_str(&opf_contents)
        .map_err(|e| format!("Failed to parse package.opf: {}", e))?;

    // Extract metadata
    let title = package.metadata.title.to_string();
    let language = package.metadata.language.to_string();

    // Extract spine files - map idrefs to hrefs via manifest
    let spine_files: Vec<String> = package
        .spine
        .itemref
        .iter()
        .filter_map(|itemref| {
            package
                .manifest
                .items
                .iter()
                .find(|item| item.id == itemref.idref)
                .map(|item| item.href.to_string())
        })
        .collect();

    // Check if nav.xhtml exists (opf_file is dropped, so we can borrow archive again)
    let has_nav = archive.by_name("EPUB/nav.xhtml").is_ok();

    Ok(EpubMetadata {
        filetype: "epub".to_string(),
        file_size,
        title,
        language,
        spine_files,
        has_nav,
    })
}

/// Extract XHTML content files from an EPUB archive
pub fn extract_epub_xhtml(epub_path: &Path) -> Result<HashMap<String, String>, String> {
    use std::io::Read;
    use zip::ZipArchive;

    let file = fs::File::open(epub_path).map_err(|e| format!("Failed to open EPUB file: {}", e))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Failed to read EPUB archive: {}", e))?;

    let mut xhtml_files = HashMap::new();

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read archive entry: {}", e))?;

        let name = file.name().to_string();

        // Extract only XHTML content files (not nav.xhtml)
        if name.starts_with("EPUB/") && name.ends_with(".xhtml") && !name.ends_with("nav.xhtml") {
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(|e| format!("Failed to read XHTML content: {}", e))?;

            // Store with relative name (strip EPUB/ prefix)
            let rel_name = name.strip_prefix("EPUB/").unwrap_or(&name);
            xhtml_files.insert(rel_name.to_string(), contents);
        }
    }

    Ok(xhtml_files)
}

/// Verify EPUB XHTML content against reference files
fn verify_epub_xhtml_content(
    ref_dir: &Path,
    actual_xhtml: &HashMap<String, String>,
    test_name: &str,
) -> Result<(), String> {
    let xhtml_ref_dir = ref_dir.join("xhtml");

    // If no xhtml reference directory exists, skip XHTML verification
    if !xhtml_ref_dir.exists() {
        return Ok(());
    }

    // Check all reference XHTML files exist in actual
    for entry in WalkDir::new(&xhtml_ref_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .map(|ext| ext == "xhtml")
                    .unwrap_or(false)
        })
    {
        let rel_path = entry
            .path()
            .strip_prefix(&xhtml_ref_dir)
            .map_err(|e| format!("Failed to get relative path: {}", e))?;
        let filename = rel_path.to_string_lossy().to_string();

        let ref_content = fs::read_to_string(entry.path())
            .map_err(|e| format!("Failed to read reference XHTML: {}", e))?;

        let actual_content = actual_xhtml
            .get(&filename)
            .ok_or_else(|| format!("Missing XHTML file in EPUB: {}", filename))?;

        if ref_content != *actual_content {
            let diff = compute_html_diff(&ref_content, actual_content);

            let test_name_sanitized = test_name
                .replace('/', "_slash")
                .replace('.', "_full_stop")
                .replace(':', "_colon")
                .replace('-', "_minus");

            return Err(format!(
                "EPUB XHTML content mismatch for {}\n\n{}\n\nTo update, run: UPDATE_REFERENCES=1 cargo test run_test_case_{}",
                filename, diff, test_name_sanitized
            ));
        }
    }

    Ok(())
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

    // Enhanced page count error with more context
    if reference.page_count != actual.page_count {
        let ref_count = reference.page_count.unwrap_or(0);
        let actual_count = actual.page_count.unwrap_or(0);
        let page_diff = (actual_count as i32 - ref_count as i32).abs();
        let change_type = if actual_count > ref_count {
            "added"
        } else {
            "removed"
        };

        errors.push(format!(
            "Page count: expected {}, got {} ({} pages {})",
            ref_count, actual_count, page_diff, change_type
        ));
    }

    // Enhanced file size error with percentage
    let size_diff = (actual.file_size as i64 - reference.file_size as i64).unsigned_abs();
    let tolerance = reference.file_size / 10;

    if size_diff > tolerance {
        let size_percent_diff =
            ((size_diff as f64 / reference.file_size as f64) * 100.0).round() as u32;

        errors.push(format!(
            "File size: expected {} bytes, got {} bytes ({}% diff, beyond 10% tolerance)",
            reference.file_size, actual.file_size, size_percent_diff
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let mut error_msg = String::from("PDF metadata mismatch:\n");
        for error in &errors {
            error_msg.push_str(&format!("  - {}\n", error));
        }
        error_msg.push_str("\nThis may indicate a change in content or formatting.");
        Err(error_msg)
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

fn compare_epub_metadata(reference: &EpubMetadata, actual: &EpubMetadata) -> Result<(), String> {
    let mut errors = Vec::new();

    if reference.filetype != actual.filetype {
        errors.push(format!(
            "Filetype mismatch: expected {}, got {}",
            reference.filetype, actual.filetype
        ));
    }

    if reference.title != actual.title {
        errors.push(format!(
            "Title mismatch: expected '{}', got '{}'",
            reference.title, actual.title
        ));
    }

    if reference.language != actual.language {
        errors.push(format!(
            "Language mismatch: expected '{}', got '{}'",
            reference.language, actual.language
        ));
    }

    if reference.spine_files != actual.spine_files {
        errors.push(format!(
            "Spine order mismatch:\n  Expected: [{}]\n  Got: [{}]",
            reference.spine_files.join(", "),
            actual.spine_files.join(", ")
        ));
    }

    if reference.has_nav != actual.has_nav {
        errors.push(format!(
            "Navigation file: expected {}, got {}",
            if reference.has_nav {
                "present"
            } else {
                "missing"
            },
            if actual.has_nav { "present" } else { "missing" }
        ));
    }

    // File size with 10% tolerance (like PDF)
    let size_diff = (actual.file_size as i64 - reference.file_size as i64).unsigned_abs();
    let tolerance = reference.file_size / 10;

    if size_diff > tolerance {
        let size_percent_diff =
            ((size_diff as f64 / reference.file_size as f64) * 100.0).round() as u32;

        errors.push(format!(
            "File size: expected {} bytes, got {} bytes ({}% diff, beyond 10% tolerance)",
            reference.file_size, actual.file_size, size_percent_diff
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let mut error_msg = String::from("EPUB metadata mismatch:\n");
        for error in &errors {
            error_msg.push_str(&format!("  - {}\n", error));
        }
        Err(error_msg)
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
            errors.push(format!(
                "Missing PDF file: {}\n      Expected in: {}\n      Metadata reference: {}",
                ref_file,
                actual_dir.display(),
                reference_dir
                    .join(format!(
                        "{}.metadata.json",
                        ref_file.strip_suffix(".pdf").unwrap_or(ref_file)
                    ))
                    .display()
            ));
        }
    }

    for actual_file in &actual_files {
        if !ref_files.contains(actual_file) {
            errors.push(format!(
                "Unexpected PDF file: {}\n      Found in: {}\n      Not defined in references: {}\n      Run UPDATE_REFERENCES=1 to add this file to references",
                actual_file,
                actual_dir.join(actual_file).display(),
                reference_dir.display()
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let mut error_msg = String::from("PDF asset validation failed:\n\n");
        for (i, error) in errors.iter().enumerate() {
            error_msg.push_str(&format!("{}. {}\n\n", i + 1, error));
        }
        Err(error_msg)
    }
}

fn validate_epub_assets(reference_dir: &Path, actual_dir: &Path) -> Result<(), String> {
    let ref_files: Vec<String> = collect_files_by_predicate(reference_dir, |e| {
        e.path().extension().and_then(|s| s.to_str()) == Some("json")
    })
    .into_iter()
    .filter_map(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".metadata"))
            .map(|s| format!("{}.epub", s))
    })
    .collect();

    let actual_files: Vec<String> = collect_files_by_predicate(actual_dir, |e| {
        e.path().extension().and_then(|s| s.to_str()) == Some("epub")
    })
    .into_iter()
    .map(|p| p.to_string_lossy().to_string())
    .collect();

    let mut errors = Vec::new();

    for ref_file in &ref_files {
        if !actual_dir.join(ref_file).exists() {
            errors.push(format!(
                "Missing EPUB file: {}\n      Expected in: {}\n      Metadata reference: {}",
                ref_file,
                actual_dir.display(),
                reference_dir
                    .join(format!(
                        "{}.metadata.json",
                        ref_file.strip_suffix(".epub").unwrap_or(ref_file)
                    ))
                    .display()
            ));
        }
    }

    for actual_file in &actual_files {
        if !ref_files.contains(actual_file) {
            errors.push(format!(
                "Unexpected EPUB file: {}\n      Found in: {}\n      Not defined in references: {}\n      Run UPDATE_REFERENCES=1 to add this file to references",
                actual_file,
                actual_dir.join(actual_file).display(),
                reference_dir.display()
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        let mut error_msg = String::from("EPUB asset validation failed:\n\n");
        for (i, error) in errors.iter().enumerate() {
            error_msg.push_str(&format!("{}. {}\n\n", i + 1, error));
        }
        Err(error_msg)
    }
}

pub fn verify_epub_output(test_name: &str, actual_dir: &Path) {
    let ref_dir = get_reference_dir(actual_dir, test_name, "epub");
    ensure_reference_exists(&ref_dir, test_name, "EPUB");

    validate_epub_assets(&ref_dir, actual_dir).expect("EPUB asset validation failed");

    for_each_file_with_ext(actual_dir, "epub", |entry| {
        let rel_path = entry.path().strip_prefix(actual_dir).unwrap();
        let metadata_file = ref_dir.join(format!(
            "{}.metadata.json",
            rel_path.file_stem().unwrap().to_string_lossy()
        ));

        if !metadata_file.exists() {
            panic!(
                "EPUB metadata reference not found: {}. Run with UPDATE_REFERENCES=1",
                metadata_file.display()
            );
        }

        let ref_metadata_json =
            fs::read_to_string(&metadata_file).expect("Failed to read reference metadata");
        let ref_metadata: EpubMetadata =
            serde_json::from_str(&ref_metadata_json).expect("Failed to parse reference metadata");
        let actual_metadata =
            extract_epub_metadata(entry.path()).expect("Failed to extract EPUB metadata");

        compare_epub_metadata(&ref_metadata, &actual_metadata).expect("EPUB metadata mismatch");

        // Verify XHTML content if reference files exist
        let actual_xhtml = extract_epub_xhtml(entry.path()).expect("Failed to extract EPUB XHTML");
        verify_epub_xhtml_content(&ref_dir, &actual_xhtml, test_name)
            .expect("EPUB XHTML content mismatch");
    });
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
