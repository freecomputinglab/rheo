//! Proves `rheo_core::Build` can compile a project from a `ProjectConfig` plus
//! plugins, with no CLI involvement — the library entry point introduced when the
//! orchestrator moved out of `crates/cli`.

use rheo_core::manifest_version;
use rheo_core::project::ProjectConfig;
use rheo_core::{Build, BuildOptions};
use std::fs;
use tempfile::TempDir;

/// Write a minimal one-page rheo project into a fresh temp dir and return it.
fn scaffold_project() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    fs::write(
        root.join("rheo.toml"),
        format!("version = \"{}\"\n", manifest_version::CURRENT),
    )
    .unwrap();

    let content = root.join("content");
    fs::create_dir_all(&content).unwrap();
    fs::write(content.join("index.typ"), "= Hello\n\nA paragraph.\n").unwrap();

    temp
}

#[test]
fn build_compiles_html_without_cli() {
    // SAFETY: tests in this binary run single-threaded for this env var; mirrors
    // the harness which sets the same flag for deterministic font behaviour.
    unsafe {
        std::env::set_var("TYPST_IGNORE_SYSTEM_FONTS", "1");
    }

    let temp = scaffold_project();
    let build_dir = temp.path().join("build");

    let project = ProjectConfig::from_path(temp.path(), None).expect("load project");

    let mut build = Build::prepare(
        project,
        vec![Box::new(rheo_html::HtmlPlugin)],
        BuildOptions {
            formats: vec!["html".to_string()],
            build_dir: Some(build_dir.clone()),
            font_dirs: vec![],
        },
    )
    .expect("prepare build");

    // Only the requested format is selected.
    let names: Vec<&str> = build.plugins().iter().map(|p| p.name()).collect();
    assert_eq!(names, vec!["html"]);

    let results = build.run().expect("run build");
    assert_eq!(results.get("html").succeeded, 1);
    assert_eq!(results.get("html").failed, 0);

    let html = build_dir.join("html").join("index.html");
    assert!(html.is_file(), "expected {} to exist", html.display());
    let body = fs::read_to_string(&html).unwrap();
    assert!(body.contains("Hello"), "compiled HTML missing heading text");
}
