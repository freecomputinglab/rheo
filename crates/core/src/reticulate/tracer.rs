//! Spine tracer: discovers documents and assets via static analysis.
//!
//! The `TracedSpine` struct is populated from two sources:
//! 1. rheo.toml spine configuration (vertebrae glob patterns)
//! 2. Static AST analysis of .typ files for #document() and #asset() calls

use crate::config::Spine;
use crate::path_utils::{collect_all_typst_files, collect_one_typst_file};
use crate::{Result, RheoError};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;
use typst_syntax::ast::{Expr, FuncCall};
use typst_syntax::{SyntaxKind, parse};

/// A single document in the traced spine.
#[derive(Debug, Clone)]
pub struct SpineDocument {
    /// Path to the .typ file (relative to content_dir).
    pub path: PathBuf,

    /// true if file contains top-level #document() or #asset() calls.
    /// Such files are passed through as-is in the bundle entry (not wrapped).
    pub is_bundle_entry: bool,
}

/// Result of tracing a project's spine configuration.
///
/// Contains all documents and assets needed for bundle compilation,
/// populated from both rheo.toml config and static analysis.
#[derive(Debug, Clone)]
pub struct TracedSpine {
    /// Title from spine configuration.
    pub title: Option<String>,

    /// Ordered list of spine documents.
    pub documents: Vec<SpineDocument>,

    /// All asset files (from rheo.toml assets + #asset() calls).
    pub assets: Vec<PathBuf>,

    /// Whether to merge outputs (PDF merged mode).
    pub merge: bool,
}

impl TracedSpine {
    /// Trace the project spine from configuration and static analysis.
    ///
    /// # Arguments
    ///
    /// * `root` - Project root directory
    /// * `content_dir` - Content directory (where .typ files live)
    /// * `spine_config` - Optional spine configuration from rheo.toml
    /// * `assets_config` - Asset glob patterns (global + per-plugin)
    /// * `default_merge` - Plugin's default merge behavior (when config doesn't specify)
    pub fn trace(
        root: &Path,
        content_dir: &Path,
        spine_config: Option<&Spine>,
        assets_config: &[String],
        default_merge: bool,
    ) -> Result<TracedSpine> {
        // Discover documents from vertebrae config or auto-discovery
        let vertebrae_paths = discover_documents(content_dir, spine_config)?;

        // Static analysis: read each .typ file and check for bundle syntax
        let mut documents = Vec::new();
        let mut assets_from_source = Vec::new();

        for path in &vertebrae_paths {
            let source = fs::read_to_string(path).map_err(|e| {
                RheoError::project_config(format!(
                    "failed to read spine file '{}': {}",
                    path.display(),
                    e
                ))
            })?;

            let is_entry = is_bundle_entry(&source);

            // Extract assets from #asset() calls
            extract_assets(&source, path, &mut assets_from_source);

            documents.push(SpineDocument {
                path: path.clone(),
                is_bundle_entry: is_entry,
            });
        }

        // Expand asset glob patterns from config
        let assets_from_config = expand_asset_globs(root, assets_config)?;

        // Merge assets: config assets first, then source assets, deduplicated
        let mut assets = Vec::new();
        let mut seen = HashSet::new();

        // Add config assets first (preserve order)
        for asset in assets_from_config {
            match asset.canonicalize() {
                Ok(canonical) if seen.insert(canonical.clone()) => {
                    assets.push(asset);
                }
                Err(e) => {
                    warn!("Failed to canonicalize asset {:?}: {}", asset, e);
                }
                _ => {}
            }
        }

        // Add source assets
        for asset in assets_from_source {
            match asset.canonicalize() {
                Ok(canonical) if seen.insert(canonical.clone()) => {
                    assets.push(asset);
                }
                Err(e) => {
                    warn!("Failed to canonicalize asset {:?}: {}", asset, e);
                }
                _ => {}
            }
        }

        // Determine title and merge flag
        let title = spine_config.and_then(|s| s.title.clone());
        let merge = spine_config.and_then(|s| s.merge).unwrap_or(default_merge);

        Ok(TracedSpine {
            title,
            documents,
            assets,
            merge,
        })
    }
}

/// Check if source contains top-level #document() or #asset() calls.
///
/// Only checks TOP-LEVEL AST children (root.children()), not nested scopes.
/// This is intentional: bundle-syntax must be at top level, not inside functions.
fn is_bundle_entry(source: &str) -> bool {
    let root = parse(source);
    for node in root.children() {
        if node.kind() == SyntaxKind::FuncCall
            && let Some(call) = node.cast::<FuncCall>()
            && let Expr::Ident(ident) = call.callee()
        {
            let name = ident.get();
            if name == "document" || name == "asset" {
                return true;
            }
        }
    }
    false
}

/// Extract asset paths from #asset() calls in source.
///
/// Only extracts from TOP-LEVEL AST children (root.children()), not nested scopes.
/// Returns the asset path as the first string argument to #asset() calls.
///
/// # Note
/// Only positional arguments are supported. Named arguments like `#asset(path: "img.png")`
/// are silently ignored. Use positional syntax: `#asset("img.png")`.
fn extract_assets(source: &str, source_path: &Path, assets: &mut Vec<PathBuf>) {
    let root = parse(source);
    for node in root.children() {
        if node.kind() == SyntaxKind::FuncCall
            && let Some(call) = node.cast::<FuncCall>()
            && let Expr::Ident(ident) = call.callee()
            && ident.get() == "asset"
        {
            // Extract the first positional argument (the asset path string)
            for arg in call.args().items() {
                if let typst_syntax::ast::Arg::Pos(Expr::Str(s)) = arg {
                    let asset_path = source_path
                        .parent()
                        .map(|p| p.join(s.get().as_str()))
                        .unwrap_or_else(|| PathBuf::from(s.get().as_str()));
                    assets.push(asset_path);
                    break; // Only take the first argument
                }
            }
        }
    }
}

/// Discover spine documents from vertebrae config or auto-discovery.
fn discover_documents(content_dir: &Path, spine_config: Option<&Spine>) -> Result<Vec<PathBuf>> {
    match spine_config {
        None => {
            // No spine config: single file mode
            collect_one_typst_file(content_dir)
        }
        Some(spine) if spine.vertebrae.is_empty() => {
            // Empty vertebrae: auto-discover all .typ files
            collect_all_typst_files(content_dir)
        }
        Some(spine) => {
            // Expand vertebrae glob patterns
            let mut typst_files = Vec::new();
            for pattern in &spine.vertebrae {
                let glob_pattern = if Path::new(pattern).is_absolute() {
                    pattern.clone()
                } else {
                    content_dir.join(pattern).display().to_string()
                };

                let glob = glob::glob(&glob_pattern).map_err(|e| {
                    RheoError::project_config(format!("invalid glob pattern '{}': {}", pattern, e))
                })?;

                let mut glob_files: Vec<PathBuf> = glob
                    .filter_map(|entry| entry.ok())
                    .filter(|path| path.is_file())
                    .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("typ"))
                    .filter(|path| path.file_name().is_some())
                    .collect();

                // Sort by full path (lexicographic) for consistent ordering
                glob_files.sort();
                typst_files.extend(glob_files);
            }

            if typst_files.is_empty() {
                return Err(RheoError::project_config("spine matched no .typ files"));
            }

            Ok(typst_files)
        }
    }
}

/// Expand asset glob patterns relative to root.
fn expand_asset_globs(root: &Path, patterns: &[String]) -> Result<Vec<PathBuf>> {
    let mut assets = Vec::new();

    for pattern in patterns {
        let glob_pattern = if Path::new(pattern).is_absolute() {
            pattern.clone()
        } else {
            root.join(pattern).display().to_string()
        };

        let glob = glob::glob(&glob_pattern).map_err(|e| {
            RheoError::project_config(format!("invalid asset glob pattern '{}': {}", pattern, e))
        })?;

        let matched: Vec<PathBuf> = glob
            .filter_map(|entry| entry.ok())
            .filter(|path| path.is_file())
            .collect();

        assets.extend(matched);
    }

    // Sort for consistent ordering
    assets.sort();
    Ok(assets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir_with_files(files: &[&str]) -> TempDir {
        let temp = TempDir::new().unwrap();
        for file in files {
            let path = temp.path().join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, "").unwrap();
        }
        temp
    }

    #[test]
    fn test_is_bundle_entry_plain_file() {
        let source = r#"
            #set page(width: 210pt, height: 297pt)

            = Hello
            This is a plain file.
        "#;
        assert!(!is_bundle_entry(source));
    }

    #[test]
    fn test_is_bundle_entry_with_document() {
        let source = r#"
            #document("index.html", title: "Home")[
              = Home
              Welcome!
            ]
        "#;
        assert!(is_bundle_entry(source));
    }

    #[test]
    fn test_is_bundle_entry_with_asset() {
        let source = r#"
            #asset("styles.css", read("styles.css", encoding: none))

            = Hello
        "#;
        assert!(is_bundle_entry(source));
    }

    #[test]
    fn test_is_bundle_entry_nested_not_counted() {
        // Nested #document() inside a function should NOT count
        let source = r#"
            #let myfunc() = {
              #document("nested.html")[Nested]
            }

            = Hello
        "#;
        // root.children() only checks top-level, so nested is not found
        assert!(!is_bundle_entry(source));
    }

    #[test]
    fn test_is_bundle_entry_multiple_calls() {
        let source = r#"
            #document("a.html")[A]
            #document("b.html")[B]
        "#;
        assert!(is_bundle_entry(source));
    }

    #[test]
    fn test_collect_one_typst_file_single() {
        let temp = create_test_dir_with_files(&["test.typ"]);
        let result = collect_one_typst_file(temp.path());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "test.typ");
    }

    #[test]
    fn test_collect_one_typst_file_multiple_error() {
        let temp = create_test_dir_with_files(&["first.typ", "second.typ"]);
        let result = collect_one_typst_file(temp.path());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("multiple .typ files found")
        );
    }

    #[test]
    fn test_collect_one_typst_file_no_files_error() {
        let temp = create_test_dir_with_files(&["readme.md"]);
        let result = collect_one_typst_file(temp.path());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("need at least one .typ file")
        );
    }

    #[test]
    fn test_collect_all_typst_files() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ", "c.typ"]);
        let result = collect_all_typst_files(temp.path());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_collect_all_typst_files_sorted() {
        let temp = create_test_dir_with_files(&["c.typ", "a.typ", "b.typ"]);
        let result = collect_all_typst_files(temp.path());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 3);
        // Should be sorted lexicographically
        assert!(files[0].file_name().unwrap() < files[1].file_name().unwrap());
        assert!(files[1].file_name().unwrap() < files[2].file_name().unwrap());
    }

    #[test]
    fn test_discover_documents_with_vertebrae() {
        let temp =
            create_test_dir_with_files(&["cover.typ", "chapters/ch1.typ", "chapters/ch2.typ"]);
        let spine = Spine {
            title: Some("Test".to_string()),
            vertebrae: vec!["cover.typ".to_string(), "chapters/*.typ".to_string()],
            merge: Some(false),
        };
        let result = discover_documents(temp.path(), Some(&spine));
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_discover_documents_recursive_glob() {
        // Test recursive glob pattern `**/*.typ` for nested directories
        let temp = create_test_dir_with_files(&[
            "cover.typ",
            "chapters/ch1.typ",
            "chapters/nested/ch2.typ",
            "appendix/notes/a.typ",
        ]);
        let spine = Spine {
            title: Some("Test".to_string()),
            vertebrae: vec!["**/*.typ".to_string()],
            merge: Some(false),
        };
        let result = discover_documents(temp.path(), Some(&spine));
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 4);
        // Verify lexicographic ordering within the glob pattern
        assert!(files[0].file_name().unwrap() < files[1].file_name().unwrap());
        assert!(files[1].file_name().unwrap() < files[2].file_name().unwrap());
        assert!(files[2].file_name().unwrap() < files[3].file_name().unwrap());
    }

    #[test]
    fn test_traced_spine_auto_discovery_single_file() {
        // Test TracedSpine::trace() with no spine config (single .typ file mode)
        let temp = create_test_dir_with_files(&["main.typ"]);
        let result = TracedSpine::trace(temp.path(), temp.path(), None, &[], false);
        assert!(result.is_ok());
        let traced = result.unwrap();
        assert_eq!(traced.documents.len(), 1);
        assert_eq!(traced.documents[0].path.file_name().unwrap(), "main.typ");
        assert!(!traced.documents[0].is_bundle_entry); // plain file, no #document() calls
        assert!(traced.title.is_none());
        assert!(!traced.merge);
    }

    #[test]
    fn test_traced_spine_auto_discovery_multiple_files_error() {
        // Test that auto-discovery with multiple .typ files returns an error
        let temp = create_test_dir_with_files(&["first.typ", "second.typ"]);
        let result = TracedSpine::trace(temp.path(), temp.path(), None, &[], false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("multiple .typ files found")
        );
    }

    #[test]
    fn test_traced_spine_empty_vertebrae_discovers_all() {
        // Test that empty vertebrae list auto-discovers all .typ files
        let temp = create_test_dir_with_files(&["a.typ", "b.typ", "c.typ"]);
        let spine = Spine {
            title: Some("All Files".to_string()),
            vertebrae: vec![],
            merge: Some(false),
        };
        let result = TracedSpine::trace(temp.path(), temp.path(), Some(&spine), &[], false);
        assert!(result.is_ok());
        let traced = result.unwrap();
        assert_eq!(traced.documents.len(), 3);
        // Verify lexicographic ordering
        assert!(traced.documents[0].path < traced.documents[1].path);
        assert!(traced.documents[1].path < traced.documents[2].path);
        assert_eq!(traced.title.as_ref().unwrap(), "All Files");
    }

    #[test]
    fn test_traced_spine_filters_non_typ_files() {
        // Test that .md and other files are filtered out
        let temp = create_test_dir_with_files(&["README.md", "a.typ", "b.typ", "script.sh"]);
        let spine = Spine {
            title: None,
            vertebrae: vec![],
            merge: Some(false),
        };
        let result = TracedSpine::trace(temp.path(), temp.path(), Some(&spine), &[], false);
        assert!(result.is_ok());
        let traced = result.unwrap();
        // Only .typ files should be discovered
        assert_eq!(traced.documents.len(), 2);
        assert!(traced.documents[0].path.ends_with("a.typ"));
        assert!(traced.documents[1].path.ends_with("b.typ"));
    }

    #[test]
    fn test_traced_spine_asset_deduplication() {
        // Test that duplicate assets (from config + source) are deduplicated
        let temp = TempDir::new().unwrap();

        // Create an asset file
        let asset_path = temp.path().join("style.css");
        fs::write(&asset_path, "body { color: red; }").unwrap();

        // Create a .typ file with an #asset() call for the same CSS file
        let typ_path = temp.path().join("main.typ");
        fs::write(
            &typ_path,
            r#"#asset("style.css", read("style.css", encoding: none))

= Hello"#,
        )
        .unwrap();

        // Trace with asset config that includes the same file
        let spine = Spine {
            title: None,
            vertebrae: vec![],
            merge: Some(false),
        };
        let result = TracedSpine::trace(
            temp.path(),
            temp.path(),
            Some(&spine),
            &["style.css".to_string()],
            false,
        );
        assert!(result.is_ok());
        let traced = result.unwrap();

        // Asset should appear only once (deduplicated)
        let css_count = traced
            .assets
            .iter()
            .filter(|p| p.ends_with("style.css"))
            .count();
        assert_eq!(css_count, 1);
    }

    #[test]
    fn test_traced_spine_merge_flag_from_config() {
        let temp = create_test_dir_with_files(&["a.typ"]);
        let spine = Spine {
            title: None,
            vertebrae: vec![],
            merge: Some(true),
        };
        let result = TracedSpine::trace(temp.path(), temp.path(), Some(&spine), &[], false);
        assert!(result.is_ok());
        let traced = result.unwrap();
        assert!(traced.merge);
    }

    #[test]
    fn test_extract_assets_from_typst() {
        // Test extracting asset paths from #asset() calls
        let source = r#"
            #asset("style.css", read("style.css", encoding: none))
            #asset("images/logo.png")
        "#;
        let source_path = Path::new("/project/content/main.typ");
        let mut assets = Vec::new();

        extract_assets(source, source_path, &mut assets);

        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0], PathBuf::from("/project/content/style.css"));
        assert_eq!(assets[1], PathBuf::from("/project/content/images/logo.png"));
    }

    #[test]
    fn test_extract_assets_nested_not_extracted() {
        // Test that nested #asset() calls are NOT extracted (top-level only)
        let source = r#"
            #let myfunc() = {
              #asset("nested.css")
            }
        "#;
        let source_path = Path::new("/project/content/main.typ");
        let mut assets = Vec::new();

        extract_assets(source, source_path, &mut assets);

        // No assets should be extracted since #asset() is nested
        assert_eq!(assets.len(), 0);
    }

    #[test]
    fn test_extract_assets_empty() {
        // Test source with no #asset() calls
        let source = "= Hello World\nNo assets here.";
        let source_path = Path::new("/project/content/main.typ");
        let mut assets = Vec::new();

        extract_assets(source, source_path, &mut assets);

        assert_eq!(assets.len(), 0);
    }

    #[test]
    fn test_traced_spine_vertebrae_order_preserved() {
        // Test that vertebrae order from config is preserved
        let temp = create_test_dir_with_files(&["z.typ", "a.typ", "m.typ"]);
        let spine = Spine {
            title: None,
            // Reverse alphabetical order
            vertebrae: vec![
                "z.typ".to_string(),
                "m.typ".to_string(),
                "a.typ".to_string(),
            ],
            merge: Some(false),
        };
        let result = TracedSpine::trace(temp.path(), temp.path(), Some(&spine), &[], false);
        assert!(result.is_ok());
        let traced = result.unwrap();
        assert_eq!(traced.documents.len(), 3);
        // Order should match config order, not lexicographic
        assert_eq!(traced.documents[0].path.file_name().unwrap(), "z.typ");
        assert_eq!(traced.documents[1].path.file_name().unwrap(), "m.typ");
        assert_eq!(traced.documents[2].path.file_name().unwrap(), "a.typ");
    }
}
