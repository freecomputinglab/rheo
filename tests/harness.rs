mod helpers;

use helpers::{
    comparison::{verify_epub_output, verify_html_output, verify_pdf_output},
    fixtures::TestCase,
    reference::{update_epub_references, update_html_references, update_pdf_references},
    test_store::copy_project_to_test_store,
};
use ntest::test_case;
use rheo::{OutputFormat, RheoConfig, project::ProjectConfig};
use std::env;
use std::path::PathBuf;

#[test_case("examples/blog_site")]
#[test_case("examples/blog_post")]
#[test_case("examples/cover-letter.typ")]
#[test_case("examples/blog_site/content/index.typ")]
#[test_case("examples/blog_site/content/severance-ep-1.typ")]
#[test_case("examples/blog_post/portable_epubs.typ")]
#[test_case("tests/cases/code_blocks_with_links")]
#[test_case("tests/cases/cross_directory_links")]
#[test_case("tests/cases/epub_inferred_spine")]
#[test_case("tests/cases/link_path_edge_cases")]
#[test_case("tests/cases/link_transformation")]
#[test_case("tests/cases/links_with_fragments")]
#[test_case("tests/cases/multiple_links_inline.typ")]
#[test_case("tests/cases/pdf_individual")]
#[test_case("tests/cases/relative_path_links")]
#[test_case("tests/cases/error_formatting/type_error.typ")]
#[test_case("tests/cases/error_formatting/undefined_var.typ")]
#[test_case("tests/cases/error_formatting/syntax_error.typ")]
#[test_case("tests/cases/error_formatting/function_arg_error.typ")]
#[test_case("tests/cases/error_formatting/import_error.typ")]
#[test_case("tests/cases/error_formatting/unknown_function.typ")]
fn run_test_case(name: &str) {
    let test_case = TestCase::new(name);
    let update_mode = env::var("UPDATE_REFERENCES").is_ok();
    let test_name = test_case.name();
    let original_project_path = test_case.project_path();

    // Create isolated test store
    let test_store = PathBuf::from("tests/store").join(test_name);

    // Clean previous test artifacts
    if test_store.exists() {
        std::fs::remove_dir_all(&test_store).expect("Failed to clean test store");
    }
    std::fs::create_dir_all(&test_store).expect("Failed to create test store");

    // Copy project to test store for isolation
    if test_case.is_single_file() {
        // For single-file tests, copy just the file and its parent directory structure
        let parent = original_project_path
            .parent()
            .expect("Single file should have parent");
        copy_project_to_test_store(parent, &test_store)
            .expect("Failed to copy project to test store");
    } else {
        // For directory tests, copy the whole project
        copy_project_to_test_store(original_project_path, &test_store)
            .expect("Failed to copy project to test store");
    }

    // Use test store as project path
    let project_path = if test_case.is_single_file() {
        let rel_path = original_project_path
            .strip_prefix(
                original_project_path
                    .parent()
                    .expect("Single file should have parent"),
            )
            .expect("Failed to get relative path");
        test_store.join(rel_path)
    } else {
        test_store.clone()
    };

    // Load project from isolated copy
    let project = ProjectConfig::from_path(&project_path, None).expect("Failed to load project");
    let config = RheoConfig::load(&project.root);

    // Get declared formats from test case (respects markers for single-file tests)
    let declared_formats = test_case.formats();

    // Check environment variables for format filtering
    let env_html = env::var("RUN_HTML_TESTS").is_ok();
    let env_pdf = env::var("RUN_PDF_TESTS").is_ok();
    let env_epub = env::var("RUN_EPUB_TESTS").is_ok();

    // If no env vars set, run all declared formats
    let run_all = !env_html && !env_pdf && !env_epub;

    // Compute which formats to actually run
    // For single-file tests: use declared formats (config check optional, markers are authoritative)
    // For directory tests: require config support (preserve existing behavior)
    let run_html = declared_formats.contains(&OutputFormat::Html)
        && (run_all || env_html)
        && (test_case.is_single_file() || config.as_ref().is_ok_and(|cfg| cfg.has_html()));
    let run_pdf = declared_formats.contains(&OutputFormat::Pdf)
        && (run_all || env_pdf)
        && (test_case.is_single_file() || config.as_ref().is_ok_and(|cfg| cfg.has_pdf()));
    let run_epub = declared_formats.contains(&OutputFormat::Epub)
        && (run_all || env_epub)
        && (test_case.is_single_file() || config.as_ref().is_ok_and(|cfg| cfg.has_epub()));

    // Get build directory in test store
    let build_dir = test_store.join("build");

    // Build compile command with format flags
    let mut compile_args = vec!["run", "--", "compile", project_path.to_str().unwrap()];

    // Use isolated build directory
    compile_args.push("--build-dir");
    compile_args.push(build_dir.to_str().unwrap());

    // For single-file tests, add explicit format flags based on declared formats
    // For directory tests, let rheo use config/defaults (no flags = backward compatible)
    if test_case.is_single_file() {
        if run_html {
            compile_args.push("--html");
        }
        if run_pdf {
            compile_args.push("--pdf");
        }
        if run_epub {
            compile_args.push("--epub");
        }
    }

    // Compile the project using rheo CLI logic
    let output = std::process::Command::new("cargo")
        .args(&compile_args)
        .env("TYPST_IGNORE_SYSTEM_FONTS", "1")
        .output()
        .expect("Failed to run rheo compile");

    // Check if test expects compilation error
    let expects_error = test_case
        .metadata()
        .and_then(|m| m.expect.as_ref())
        .map(|e| e == "error")
        .unwrap_or(false);

    if expects_error {
        // Test expects compilation to fail
        assert!(
            !output.status.success(),
            "Expected compilation to fail for {}, but it succeeded",
            test_name
        );

        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check all required error patterns
        if let Some(metadata) = test_case.metadata() {
            for pattern in &metadata.error_patterns {
                assert!(
                    stderr.contains(pattern),
                    "Expected error output to contain pattern '{}', but it was not found.\nFull stderr:\n{}",
                    pattern,
                    stderr
                );
            }
        }

        // For error cases, skip reference comparison and return early
        // Clean test store before returning
        if test_store.exists() {
            std::fs::remove_dir_all(&test_store).ok();
        }
        return;
    }

    // For success cases, continue with existing logic
    if !output.status.success() {
        panic!(
            "Compilation failed for {}: {}",
            test_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // let run_epub = env::var("RUN_EPUB_TESTS").is_ok() || env::var("RUN_EPUB_TESTS").is_err();

    // Test HTML output
    if run_html {
        let html_output = build_dir.join("html");
        if html_output.exists() {
            if update_mode {
                update_html_references(test_name, &html_output, &project_path)
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

    // Test EPUB output
    if run_epub {
        let epub_output = build_dir.join("epub");
        if epub_output.exists() {
            if update_mode {
                update_epub_references(test_name, &epub_output)
                    .expect("Failed to update EPUB references");
            } else {
                verify_epub_output(test_name, &epub_output);
            }
        }
    }

    // Clean test store after test
    if test_store.exists() {
        std::fs::remove_dir_all(&test_store).ok();
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
        .env("TYPST_IGNORE_SYSTEM_FONTS", "1")
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
        .env("TYPST_IGNORE_SYSTEM_FONTS", "1")
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
        .env("TYPST_IGNORE_SYSTEM_FONTS", "1")
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

/// Test HTML post-processing: CSS link injection
#[test]
fn test_html_css_link_injection() {
    let test_case = TestCase::new("examples/blog_site");
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

    let output = std::process::Command::new("cargo")
        .args([
            "run",
            "--",
            "compile",
            project_path.to_str().unwrap(),
            "--html",
        ])
        .env("TYPST_IGNORE_SYSTEM_FONTS", "1")
        .output()
        .expect("Failed to run rheo compile");

    if !output.status.success() {
        panic!(
            "Compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Read compiled HTML
    let html_path = project_path.join("build/html/index.html");
    let html = std::fs::read_to_string(&html_path).expect("Failed to read HTML file");

    // Test 1: CSS stylesheet link is present in head
    assert!(
        html.contains(r#"<link rel="stylesheet" href="style.css">"#),
        "Should have stylesheet link in HTML"
    );

    // Test 3: Links are in the <head> section
    let head_start = html.find("<head>").expect("HTML should have <head> tag");
    let head_end = html.find("</head>").expect("HTML should have </head> tag");
    let head = &html[head_start..head_end];

    assert!(
        head.contains("style.css"),
        "CSS link should be in head section"
    );

    // Test 4: NO JavaScript DOM manipulation hack
    assert!(
        !html.contains("document.createElement"),
        "Should not have JavaScript DOM manipulation"
    );
    assert!(
        !html.contains("var cssLink"),
        "Should not have old JavaScript hack"
    );
    assert!(
        !html.contains("console.log(\"CSS and font inserted.\")"),
        "Should not have JavaScript console.log from old hack"
    );

    // Test 5: Existing head content is preserved
    assert!(
        html.contains(r#"<meta charset="utf-8">"#),
        "Should preserve meta charset"
    );
    assert!(
        html.contains(r#"<meta name="viewport""#),
        "Should preserve viewport meta tag"
    );

    // Clean up
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
}

/// Test warning formatting with codespan-reporting
#[test]
fn test_warning_formatting() {
    // Use blog_post which has a known warning (block in paragraph)
    let test_dir = PathBuf::from("examples/blog_post");

    // Clean first
    let _ = std::process::Command::new("cargo")
        .args(["run", "--", "clean", test_dir.to_str().unwrap()])
        .output();

    // Compile - should succeed with warnings
    let output = std::process::Command::new("cargo")
        .args(["run", "--", "compile", test_dir.to_str().unwrap(), "--pdf"])
        .env("TYPST_IGNORE_SYSTEM_FONTS", "1")
        .output()
        .expect("Failed to run rheo compile");

    // Should succeed despite warnings
    assert!(
        output.status.success(),
        "Compilation should succeed with warnings"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify warning formatting
    assert!(
        stderr.contains("warning"),
        "Output should contain warning marker"
    );

    // Check for codespan-reporting style formatting
    assert!(
        stderr.contains("│") || stderr.contains("|"),
        "Warning should use codespan-style formatting"
    );

    // Clean up
    let _ = std::process::Command::new("cargo")
        .args(["run", "--", "clean", test_dir.to_str().unwrap()])
        .output();
}
