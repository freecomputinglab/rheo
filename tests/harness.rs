mod helpers;

use helpers::{
    comparison::{
        compare_html_content, compare_pdf_metadata, extract_pdf_metadata, validate_html_assets,
        validate_pdf_assets, verify_included_files_present,
    },
    fixtures::TestCase,
    reference::{update_html_references, update_pdf_references},
};
use ntest::test_case;
use rheo::project::ProjectConfig;
use std::env;
use std::path::PathBuf;
use walkdir::WalkDir;

#[test_case("examples/blog_site")]
#[test_case("examples/academic_book")]
fn run_test_case(name: &str) {
    let test_case = TestCase::new(name);
    let update_mode = env::var("UPDATE_REFERENCES").is_ok();
    let test_name = test_case.name();

    // Set up test environment
    let test_store = PathBuf::from("tests/store").join(test_name);
    std::fs::create_dir_all(&test_store).expect("Failed to create test store");

    // Load project
    let project_path = test_case.project_path();
    let _project = ProjectConfig::from_path(project_path, None).expect("Failed to load project");

    // Get build directory
    let build_dir = project_path.join("build");

    // Clean build directory before test to avoid stale artifacts
    let clean_output = std::process::Command::new("cargo")
        .args(["run", "--", "clean", project_path.to_str().unwrap()])
        .output()
        .expect("Failed to run rheo clean");

    if !clean_output.status.success() {
        eprintln!(
            "Warning: Clean failed for {}: {}",
            test_name,
            String::from_utf8_lossy(&clean_output.stderr)
        );
    }

    // Compile the project using rheo CLI logic
    // We'll invoke the rheo binary directly for simplicity
    let output = std::process::Command::new("cargo")
        .args(["run", "--", "compile", project_path.to_str().unwrap()])
        .output()
        .expect("Failed to run rheo compile");

    if !output.status.success() {
        panic!(
            "Compilation failed for {}: {}",
            test_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Check if we should run HTML tests
    let run_html = env::var("RUN_HTML_TESTS").is_ok() || env::var("RUN_HTML_TESTS").is_err();

    // Check if we should run PDF tests
    let run_pdf = env::var("RUN_PDF_TESTS").is_ok() || env::var("RUN_PDF_TESTS").is_err();

    // Test HTML output
    if run_html {
        let html_output = build_dir.join("html");
        if html_output.exists() {
            if update_mode {
                update_html_references(test_name, &html_output, project_path)
                    .expect("Failed to update HTML references");
            } else {
                verify_html_output(test_name, &html_output);
            }
        }
    }

    // Test PDF output
    if run_pdf {
        let pdf_output = build_dir.join("pdf");
        if pdf_output.exists() {
            if update_mode {
                update_pdf_references(test_name, &pdf_output)
                    .expect("Failed to update PDF references");
            } else {
                verify_pdf_output(test_name, &pdf_output);
            }
        }
    }
}

fn verify_html_output(test_name: &str, actual_dir: &PathBuf) {
    let ref_dir = PathBuf::from("tests/ref")
        .join("examples")
        .join(test_name)
        .join("html");

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

fn verify_pdf_output(test_name: &str, actual_dir: &PathBuf) {
    let ref_dir = PathBuf::from("tests/ref")
        .join("examples")
        .join(test_name)
        .join("pdf");

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
