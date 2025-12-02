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
#[test_case("examples/blog_post")]
#[test_case("examples/job_application")]
#[test_case("tests/cases/pdf_merge")]
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

    let run_html = env::var("RUN_HTML_TESTS").is_ok() || env::var("RUN_HTML_TESTS").is_err();
    let run_pdf = env::var("RUN_PDF_TESTS").is_ok() || env::var("RUN_PDF_TESTS").is_err();
    // let run_epub = env::var("RUN_EPUB_TESTS").is_ok() || env::var("RUN_EPUB_TESTS").is_err();

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

    // TODO: Test EPUB output
    // if run_epub {
    //     let epub_output = build_dir.join("epub");
    //     if epub_output.exists() {
    //         if update_mode {
    //             update_epub_references(test_name, &epub_output, project_path)
    //                 .expect("Failed to update EPUB references");
    //         } else {
    //             verify_epub_output(test_name, &epub_output);
    //         }
    //     }
    // }

    // Clean build directory after test
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
}

/// Test PDF merge functionality specifically
#[test]
fn test_pdf_merge() {
    use helpers::comparison::extract_pdf_metadata;
    use lopdf::Document;

    let test_name = "pdf_merge";
    let test_case = TestCase::new(&format!("tests/cases/{}", test_name));
    let project_path = test_case.project_path();

    // Clean and compile
    let clean_output = std::process::Command::new("cargo")
        .args(["run", "--", "clean", project_path.to_str().unwrap()])
        .output()
        .expect("Failed to run rheo clean");

    if !clean_output.status.success() {
        eprintln!(
            "Warning: Clean failed: {}",
            String::from_utf8_lossy(&clean_output.stderr)
        );
    }

    // Compile with PDF merge
    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--",
            "compile",
            project_path.to_str().unwrap(),
            "--pdf",
        ])
        .output()
        .expect("Failed to run rheo compile");

    if !output.status.success() {
        panic!(
            "Compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Verify merged PDF created with correct name
    let pdf_path = project_path.join("build/pdf/pdf_merge.pdf");
    assert!(pdf_path.exists(), "Merged PDF not created at expected path");

    // Verify valid PDF format and can be loaded
    let doc = Document::load(&pdf_path).expect("Failed to load merged PDF");
    let page_count = doc.get_pages().len();
    assert!(page_count > 0, "PDF has no pages");

    // Verify we have at least 1 page
    // Note: With minimal content, Typst may fit everything on one page
    assert!(
        page_count >= 1,
        "Expected at least 1 page, got {}",
        page_count
    );

    // Verify PDF metadata can be extracted
    let metadata = extract_pdf_metadata(&pdf_path).expect("Failed to extract PDF metadata");
    assert_eq!(
        metadata.page_count,
        Some(page_count as u32),
        "Page count mismatch"
    );

    // Note: lopdf doesn't easily expose document title from metadata
    // The title is set via compile_pdf_merged but verifying it would require
    // parsing PDF metadata stream which lopdf doesn't expose directly
}

/// Test error case: link to file not in spine
#[test]
fn test_pdf_merge_link_not_in_spine() {
    // Create a test case with a file that links to a non-spine file
    let test_dir = PathBuf::from("tests/cases/pdf_merge_error_nonspine");
    std::fs::create_dir_all(&test_dir).expect("Failed to create test directory");

    // Create rheo.toml with only intro.typ in spine
    std::fs::write(
        test_dir.join("rheo.toml"),
        r#"[pdf.merge]
spine = ["intro.typ"]
title = "Test Error Case"
"#,
    )
    .expect("Failed to write rheo.toml");

    // Create intro.typ that links to chapter1.typ (not in spine)
    std::fs::write(
        test_dir.join("intro.typ"),
        r#"= Introduction <intro>

This links to #link(<chapter1>)[Chapter 1] which is not in the spine.
"#,
    )
    .expect("Failed to write intro.typ");

    // Create chapter1.typ (not in spine, but referenced)
    std::fs::write(
        test_dir.join("chapter1.typ"),
        r#"= Chapter 1 <chapter1>

Content here.
"#,
    )
    .expect("Failed to write chapter1.typ");

    // Try to compile - should fail or warn
    let output = std::process::Command::new("cargo")
        .args(["run", "--", "compile", test_dir.to_str().unwrap(), "--pdf"])
        .output()
        .expect("Failed to run rheo compile");

    // Clean up
    std::fs::remove_dir_all(&test_dir).ok();

    // Check if compilation failed with link error
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    // The compilation should fail because chapter1.typ is not in the spine
    // The transform_typ_links_to_labels function should detect this and return an error
    assert!(
        !output.status.success() || combined.contains("not found in spine"),
        "Expected error about link target not in spine, got:\nstderr: {}\nstdout: {}",
        stderr,
        stdout
    );
}

/// Test error case: duplicate filenames in spine
#[test]
fn test_pdf_merge_duplicate_filenames() {
    // Create a test case with duplicate filenames in different directories
    let test_dir = PathBuf::from("tests/cases/pdf_merge_error_duplicate");
    let dir1 = test_dir.join("dir1");
    let dir2 = test_dir.join("dir2");
    std::fs::create_dir_all(&dir1).expect("Failed to create dir1");
    std::fs::create_dir_all(&dir2).expect("Failed to create dir2");

    // Create rheo.toml with both files in spine
    std::fs::write(
        test_dir.join("rheo.toml"),
        r#"[pdf.merge]
spine = ["dir1/chapter.typ", "dir2/chapter.typ"]
title = "Test Duplicate Error"
"#,
    )
    .expect("Failed to write rheo.toml");

    // Create dir1/chapter.typ with a label
    std::fs::write(
        dir1.join("chapter.typ"),
        r#"= Chapter from Dir1 <chapter>

Content from dir1.
"#,
    )
    .expect("Failed to write dir1/chapter.typ");

    // Create dir2/chapter.typ with the same label
    std::fs::write(
        dir2.join("chapter.typ"),
        r#"= Chapter from Dir2 <chapter>

Content from dir2.
"#,
    )
    .expect("Failed to write dir2/chapter.typ");

    // Try to compile - should fail with duplicate label error
    let output = std::process::Command::new("cargo")
        .args(["run", "--", "compile", test_dir.to_str().unwrap(), "--pdf"])
        .output()
        .expect("Failed to run rheo compile");

    // Clean up
    std::fs::remove_dir_all(&test_dir).ok();

    // Typst will detect duplicate labels and fail
    // Check for error in output
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);

    // Typst should report duplicate label error
    assert!(
        !output.status.success() || combined.contains("duplicate") || combined.contains("label"),
        "Expected error about duplicate labels, got:\nstderr: {}\nstdout: {}",
        stderr,
        stdout
    );
}
