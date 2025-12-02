mod helpers;

use helpers::{
    comparison::{verify_html_output, verify_pdf_output},
    fixtures::TestCase,
    reference::{update_html_references, update_pdf_references},
};
use ntest::test_case;
use rheo::project::ProjectConfig;
use std::env;
use std::path::PathBuf;

#[test_case("examples/blog_site")]
#[test_case("examples/web_book")]
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
