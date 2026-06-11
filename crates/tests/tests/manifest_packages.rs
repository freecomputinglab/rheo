use rheo_core::plugins::detect_manifest_package_assets_in_dirs;
use rheo_tests::helpers::cli::rheo_cli_command;

#[test]
fn detect_manifest_package_assets_reads_tool_rheo_section() {
    let search_root = tempfile::tempdir().unwrap();
    let pkg_dir = search_root.path().join("testns/testpkg/0.1.0");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("typst.toml"),
        r#"
[package]
name = "testpkg"
version = "0.1.0"
entrypoint = "lib.typ"

[tool.rheo.html]
css_stylesheet = "style.css"
js_scripts = "main.js"
"#,
    )
    .unwrap();
    std::fs::write(pkg_dir.join("style.css"), "body { color: red; }").unwrap();
    std::fs::write(pkg_dir.join("main.js"), "console.log('hi');").unwrap();
    std::fs::write(pkg_dir.join("lib.typ"), "").unwrap();

    let imports = vec!["@testns/testpkg:0.1.0".to_string()];
    let blocks = detect_manifest_package_assets_in_dirs(
        &imports,
        "html",
        &[search_root.path().to_path_buf()],
    );

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].assets.dest.as_deref(), Some("testns/testpkg"));
    assert_eq!(
        blocks[0]
            .assets
            .extra
            .get("css_stylesheet")
            .and_then(|v| v.as_str()),
        Some("style.css")
    );
    assert_eq!(
        blocks[0]
            .assets
            .extra
            .get("js_scripts")
            .and_then(|v| v.as_str()),
        Some("main.js")
    );
}

#[test]
fn detect_manifest_skips_packages_without_tool_rheo() {
    let search_root = tempfile::tempdir().unwrap();
    let pkg_dir = search_root.path().join("otherns/pkg/1.0");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("typst.toml"),
        "[package]\nname = \"pkg\"\nversion = \"1.0\"\n",
    )
    .unwrap();

    let imports = vec!["@otherns/pkg:1.0".to_string()];
    let blocks = detect_manifest_package_assets_in_dirs(
        &imports,
        "html",
        &[search_root.path().to_path_buf()],
    );
    assert!(blocks.is_empty());
}

/// E2e test: compile a project that imports a package with [tool.rheo.html]
/// assets, verify CSS/JS appear in the output and are referenced in the HTML.
///
/// Uses XDG_CACHE_HOME to redirect `dirs::cache_dir()` to a tempdir so the
/// fake package is found without polluting the real Typst package cache.
#[test]
fn e2e_auto_detected_manifest_package_assets() {
    let cache_dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let project_path = project_dir.path();

    // Set up fake package in cache
    let pkg_dir = cache_dir.path().join("typst/packages/e2ens/e2epkg/0.1.0");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("typst.toml"),
        r#"
[package]
name = "e2epkg"
version = "0.1.0"
entrypoint = "lib.typ"

[tool.rheo.html]
css_stylesheet = "pkg-style.css"
js_scripts = "pkg-script.js"
"#,
    )
    .unwrap();
    std::fs::write(pkg_dir.join("pkg-style.css"), "body { color: blue; }").unwrap();
    std::fs::write(pkg_dir.join("pkg-script.js"), "console.log('e2e');").unwrap();
    std::fs::write(pkg_dir.join("lib.typ"), "").unwrap();

    // Set up project
    std::fs::write(
        project_path.join("main.typ"),
        r#"#import "@e2ens/e2epkg:0.1.0": *
= Hello
Test document.
"#,
    )
    .unwrap();

    // rheo.toml: no explicit packages, rely on auto-detect
    std::fs::write(
        project_path.join("rheo.toml"),
        format!(
            "version = \"{}\"\nformats = [\"html\"]\n",
            env!("CARGO_PKG_VERSION"),
        ),
    )
    .unwrap();

    let build_dir = project_path.join("build");

    let output = rheo_cli_command()
        .args([
            "compile",
            project_path.to_str().unwrap(),
            "--html",
            "--build-dir",
            build_dir.to_str().unwrap(),
        ])
        .env("TYPST_IGNORE_SYSTEM_FONTS", "1")
        .env("XDG_CACHE_HOME", cache_dir.path())
        .env("XDG_DATA_HOME", cache_dir.path().join("data"))
        .output()
        .expect("Failed to run rheo compile");

    assert!(
        output.status.success(),
        "Compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // CSS/JS assets should be in the output
    assert!(
        build_dir.join("html/e2ens/e2epkg/pkg-style.css").exists(),
        "auto-detected CSS not found at html/e2ens/e2epkg/pkg-style.css"
    );
    assert!(
        build_dir.join("html/e2ens/e2epkg/pkg-script.js").exists(),
        "auto-detected JS not found at html/e2ens/e2epkg/pkg-script.js"
    );

    // HTML output references the assets
    let html = std::fs::read_to_string(build_dir.join("html/main.html"))
        .expect("Failed to read HTML output");
    assert!(
        html.contains("e2ens/e2epkg/pkg-style.css"),
        "HTML should reference auto-detected CSS:\n{}",
        html
    );
    assert!(
        html.contains("e2ens/e2epkg/pkg-script.js"),
        "HTML should reference auto-detected JS:\n{}",
        html
    );
}

fn stage_package_in_data_dir(data_dir: &std::path::Path) {
    let pkg_dir = data_dir.join("typst/packages/testns/testpkg/0.1.0");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    std::fs::write(
        pkg_dir.join("typst.toml"),
        r#"
[package]
name = "testpkg"
version = "0.1.0"
entrypoint = "lib.typ"

[tool.rheo.html]
css_stylesheet = "style.css"
"#,
    )
    .unwrap();
    std::fs::write(pkg_dir.join("style.css"), "body { color: green; }").unwrap();
    std::fs::write(pkg_dir.join("lib.typ"), "").unwrap();
}

fn run_rheo_compile(
    project_path: &std::path::Path,
    build_dir: &std::path::Path,
    env_extra: Vec<(&str, &std::path::Path)>,
) -> std::process::Output {
    let mut cmd = rheo_cli_command();
    cmd.args([
        "compile",
        project_path.to_str().unwrap(),
        "--html",
        "--build-dir",
        build_dir.to_str().unwrap(),
    ])
    .env("TYPST_IGNORE_SYSTEM_FONTS", "1");
    for (key, path) in &env_extra {
        cmd.env(key, path);
    }
    cmd.output().expect("Failed to run rheo compile")
}

/// Setting `auto_detect_packages = false` suppresses import-driven asset
/// injection, even when the .typ file imports the package.
#[test]
fn auto_detect_packages_false_skips_detection() {
    let data_dir = tempfile::tempdir().unwrap();
    let cache_dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let project_path = project_dir.path();

    stage_package_in_data_dir(data_dir.path());

    std::fs::write(
        project_path.join("main.typ"),
        r#"#import "@testns/testpkg:0.1.0": *
= Hello
Opt-out test.
"#,
    )
    .unwrap();
    std::fs::write(
        project_path.join("rheo.toml"),
        format!(
            "version = \"{}\"\nformats = [\"html\"]\n\n[html]\nauto_detect_packages = false\n",
            env!("CARGO_PKG_VERSION"),
        ),
    )
    .unwrap();

    let build_dir = project_path.join("build");

    let output = run_rheo_compile(
        project_path,
        &build_dir,
        vec![
            ("XDG_DATA_HOME", data_dir.path()),
            ("XDG_CACHE_HOME", cache_dir.path()),
        ],
    );

    assert!(
        output.status.success(),
        "Compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The package assets should NOT be copied
    assert!(
        !build_dir.join("html/testns/testpkg/style.css").exists(),
        "auto-detect opted out but assets were still copied"
    );

    let html = std::fs::read_to_string(build_dir.join("html/main.html")).expect("read HTML output");
    assert!(
        !html.contains("testns/testpkg/style.css"),
        "HTML should not reference auto-detected CSS when opted out:\n{}",
        html
    );
}

/// Auto-detect works for non-preview namespaces: package is in XDG_DATA_HOME.
/// Pre-warm skips non-preview packages, so auto-detect scans the data dir directly.
#[test]
fn auto_detects_non_preview_package_assets_from_data_dir() {
    let data_dir = tempfile::tempdir().unwrap();
    let cache_dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let project_path = project_dir.path();

    stage_package_in_data_dir(data_dir.path());

    // No explicit packages in rheo.toml; auto_detect_packages defaults to true.
    std::fs::write(
        project_path.join("main.typ"),
        r#"#import "@testns/testpkg:0.1.0": *
= Hello
Non-preview auto-detect test.
"#,
    )
    .unwrap();
    std::fs::write(
        project_path.join("rheo.toml"),
        format!(
            "version = \"{}\"\nformats = [\"html\"]\n",
            env!("CARGO_PKG_VERSION"),
        ),
    )
    .unwrap();

    let build_dir = project_path.join("build");

    let output = run_rheo_compile(
        project_path,
        &build_dir,
        vec![
            ("XDG_DATA_HOME", data_dir.path()),
            ("XDG_CACHE_HOME", cache_dir.path()),
        ],
    );

    assert!(
        output.status.success(),
        "Compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        build_dir.join("html/testns/testpkg/style.css").exists(),
        "auto-detected CSS not found at html/testns/testpkg/style.css"
    );

    let html = std::fs::read_to_string(build_dir.join("html/main.html")).expect("read HTML output");
    assert!(
        html.contains("testns/testpkg/style.css"),
        "HTML should reference auto-detected CSS:\n{}",
        html
    );
}

/// Manifest `copy` patterns cause matched files to be copied into the output.
#[test]
fn manifest_copy_patterns_copied_to_output() {
    let data_dir = tempfile::tempdir().unwrap();
    let cache_dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let project_path = project_dir.path();

    // Set up package with copy patterns and some asset files
    let pkg_dir = data_dir.path().join("typst/packages/testns/copypkg/0.1.0");
    std::fs::create_dir_all(pkg_dir.join("img")).unwrap();
    std::fs::write(
        pkg_dir.join("typst.toml"),
        r#"[package]
name = "copypkg"
version = "0.1.0"
entrypoint = "lib.typ"

[tool.rheo.html]
copy = ["img/*.png"]
css_stylesheet = "pkg-style.css"
"#,
    )
    .unwrap();
    std::fs::write(pkg_dir.join("pkg-style.css"), "body { color: green; }").unwrap();
    std::fs::write(pkg_dir.join("img/logo.png"), "fake-png-data").unwrap();
    std::fs::write(pkg_dir.join("img/ignored.txt"), "not-matched").unwrap();
    std::fs::write(pkg_dir.join("lib.typ"), "").unwrap();

    std::fs::write(
        project_path.join("main.typ"),
        r#"#import "@testns/copypkg:0.1.0": *
= Hello
Copy pattern test.
"#,
    )
    .unwrap();
    std::fs::write(
        project_path.join("rheo.toml"),
        format!(
            "version = \"{}\"\nformats = [\"html\"]\n",
            env!("CARGO_PKG_VERSION"),
        ),
    )
    .unwrap();

    let build_dir = project_path.join("build");

    let output = run_rheo_compile(
        project_path,
        &build_dir,
        vec![
            ("XDG_DATA_HOME", data_dir.path()),
            ("XDG_CACHE_HOME", cache_dir.path()),
        ],
    );

    assert!(
        output.status.success(),
        "Compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The PNG matched by copy pattern should be in the output
    assert!(
        build_dir.join("html/testns/copypkg/img/logo.png").exists(),
        "copy-matched PNG not found at html/testns/copypkg/img/logo.png"
    );
    // The .txt file should NOT be copied (not matched by pattern)
    assert!(
        !build_dir
            .join("html/testns/copypkg/img/ignored.txt")
            .exists(),
        "non-matched file should not be copied"
    );
}
