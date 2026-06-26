use crate::config::PluginAssets;
use crate::plugins::{PackageAssets, ResolvedPackage, parse_package_spec};
use crate::reticulate::parser::extract_package_imports;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::warn;
use typst::syntax::Source;
use typst::syntax::package::PackageSpec;
use typst_kit::downloader::SystemDownloader;
use typst_kit::packages::SystemPackages;
use crate::rheo_packages::RheoPackages;

/// Build the standard Typst package search directories:
/// `XDG_DATA_HOME/typst/packages`, `XDG_CACHE_HOME/typst/packages`,
/// plus an optional extra directory (e.g. a caller-supplied cache dir).
pub fn typst_package_search_dirs(extra: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [
        dirs::data_dir().map(|d| d.join("typst/packages")),
        dirs::cache_dir().map(|d| d.join("typst/packages")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if let Some(extra_dir) = extra {
        dirs.push(extra_dir.to_path_buf());
    }
    dirs
}

/// Scans project .typ files for package imports (those starting with '@').
/// Returns deduplicated import path strings in encounter order.
/// Unreadable files are logged via `tracing::warn!` and skipped.
pub fn scan_project_package_imports(typ_files: &[PathBuf]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for file in typ_files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                warn!(path = %file.display(), error = %e, "could not read .typ for package import scan");
                continue;
            }
        };
        let source = Source::detached(content);
        for path in extract_package_imports(&source) {
            if seen.insert(path.clone()) {
                result.push(path);
            }
        }
    }
    result
}

/// Probe `search_dirs` (in order) for `{namespace}/{name}/{version}/`.
/// Returns the resolved package directory the first time it's found.
pub fn find_package_in_dirs(spec: &str, search_dirs: &[PathBuf]) -> Option<ResolvedPackage> {
    let (namespace, name, version) = parse_package_spec(spec)?;
    let rel = Path::new(namespace).join(name).join(version);
    let source_root = search_dirs
        .iter()
        .map(|d| d.join(&rel))
        .find(|p| p.is_dir())?;
    Some(ResolvedPackage {
        name: name.to_string(),
        source_root,
        namespace: Some(namespace.to_string()),
        version: Some(version.to_string()),
    })
}

/// Reads `{pkg.source_root}/typst.toml` and returns a `PackageAssets` for
/// `format_name` if `[tool.rheo.{format_name}]` exists and is non-empty.
/// Returns `None` otherwise. IO and parse errors are logged via warn!.
pub fn manifest_package_assets(pkg: &ResolvedPackage, format_name: &str) -> Option<PackageAssets> {
    let manifest_path = pkg.source_root.join("typst.toml");
    if !manifest_path.is_file() {
        return None;
    }
    let content = match std::fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(path = %manifest_path.display(), error = %e, "could not read typst.toml for auto-detect");
            return None;
        }
    };
    let toml: toml::Value = match toml::from_str(&content) {
        Ok(t) => t,
        Err(e) => {
            warn!(path = %manifest_path.display(), error = %e, "could not parse typst.toml for auto-detect");
            return None;
        }
    };
    let section = toml
        .get("tool")?
        .get("rheo")?
        .get(format_name)?
        .as_table()?;
    if section.is_empty() {
        return None;
    }
    let copy = section
        .get("copy")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let extra = section.clone();
    let namespace = pkg.namespace.as_deref().unwrap_or("");
    let dest = if namespace.is_empty() {
        pkg.name.clone()
    } else {
        format!("{}/{}", namespace, pkg.name)
    };
    Some(PackageAssets {
        assets: PluginAssets {
            copy,
            dest: Some(dest),
            extra,
        },
        source_root: pkg.source_root.clone(),
    })
}

/// Scans `import_paths`, locates each package via `find_package_in_dirs`,
/// reads its manifest, and returns `PackageAssets` blocks for `format_name`.
/// Silently skips packages not present locally or with no matching section.
pub fn detect_manifest_package_assets_in_dirs(
    import_paths: &[String],
    format_name: &str,
    search_dirs: &[PathBuf],
) -> Vec<PackageAssets> {
    import_paths
        .iter()
        .filter_map(|p| find_package_in_dirs(p, search_dirs))
        .filter_map(|pkg| manifest_package_assets(&pkg, format_name))
        .collect()
}

/// Production wrapper using Typst's system data/cache dirs.
pub fn detect_manifest_package_assets(
    import_paths: &[String],
    format_name: &str,
) -> Vec<PackageAssets> {
    let dirs = typst_package_search_dirs(None);
    detect_manifest_package_assets_in_dirs(import_paths, format_name, &dirs)
}

/// Ensure each `@preview/name:version` import is present in the local
/// Typst package cache, downloading if necessary. Non-`@preview` namespaces
/// are skipped — they are either local packages (already on disk) or not
/// downloadable via the Typst registry. No-op for already-cached packages.
/// Errors are logged and swallowed — pre-warm failure is not fatal; the
/// downstream scan or compile will surface real problems.
///
/// Call this before `detect_manifest_package_assets` so that scan can see
/// packages Typst would otherwise only download during compile.
///
/// Call this before `detect_manifest_package_assets` so that scan can see
/// packages Typst would otherwise only download during compile.
pub fn prewarm_packages(import_paths: &[String]) {
    if import_paths.is_empty() {
        return;
    }
    let user_agent = concat!("rheo/", env!("CARGO_PKG_VERSION"));
    let preview_storage = SystemPackages::new(SystemDownloader::new(user_agent));
    let rheo_storage = RheoPackages::new(SystemDownloader::new(user_agent));
    for spec_str in import_paths {
        let spec = match PackageSpec::from_str(spec_str) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let result = match spec.namespace.as_str() {
            "preview" => preview_storage.obtain(&spec).map(|_| ()),
            "rheo" => rheo_storage.obtain(&spec).map(|_| ()),
            _ => continue,
        };
        if let Err(e) = result {
            warn!(
                spec = %spec_str,
                error = ?e,
                "package pre-warm failed; auto-detect may miss assets"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_import_detected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.typ");
        std::fs::write(&file, r#"#import "@preview/tablex:0.0.6": tablex"#).unwrap();
        let result = scan_project_package_imports(&[file]);
        assert_eq!(result, vec!["@preview/tablex:0.0.6"]);
    }

    #[test]
    fn non_package_imports_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.typ");
        std::fs::write(&file, r#"#import "./utils.typ": *"#).unwrap();
        let result = scan_project_package_imports(&[file]);
        assert!(result.is_empty());
    }

    #[test]
    fn duplicates_deduplicated_across_files() {
        let dir = tempfile::tempdir().unwrap();
        let f1 = dir.path().join("a.typ");
        let f2 = dir.path().join("b.typ");
        std::fs::write(&f1, r#"#import "@preview/tablex:0.0.6": tablex"#).unwrap();
        std::fs::write(&f2, r#"#import "@preview/tablex:0.0.6": *"#).unwrap();
        let result = scan_project_package_imports(&[f1, f2]);
        assert_eq!(result, vec!["@preview/tablex:0.0.6"]);
    }

    #[test]
    fn non_preview_namespace_imports_detected() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.typ");
        std::fs::write(
            &file,
            r#"#import "@rheo/slides:0.1.0": slide, template
#import "@rheo/tooltip:0.1.0": tooltip"#,
        )
        .unwrap();
        let result = scan_project_package_imports(&[file]);
        assert_eq!(result, vec!["@rheo/slides:0.1.0", "@rheo/tooltip:0.1.0"]);
    }

    #[test]
    fn unreadable_files_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.typ");
        let bad = dir.path().join("nonexistent.typ");
        std::fs::write(&good, r#"#import "@preview/tablex:0.0.6": *"#).unwrap();
        let result = scan_project_package_imports(&[bad.clone(), good]);
        assert_eq!(result, vec!["@preview/tablex:0.0.6"]);
    }

    #[test]
    fn parse_package_spec_valid() {
        assert_eq!(
            parse_package_spec("@rheo/slides:0.1.0"),
            Some(("rheo", "slides", "0.1.0"))
        );
    }

    #[test]
    fn parse_package_spec_malformed() {
        assert_eq!(parse_package_spec("no-at-sign"), None);
        assert_eq!(parse_package_spec("@noslash:1.0"), None);
        assert_eq!(parse_package_spec("@ns/ncolon"), None);
        assert_eq!(parse_package_spec("@//:"), None);
    }

    #[test]
    fn find_package_in_dirs_returns_first_match() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        let pkg = dir1.path().join("myns").join("mypkg").join("1.0");
        std::fs::create_dir_all(&pkg).unwrap();
        let search = vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()];
        let result = find_package_in_dirs("@myns/mypkg:1.0", &search);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.name, "mypkg");
        assert_eq!(r.namespace.as_deref(), Some("myns"));
        assert_eq!(r.version.as_deref(), Some("1.0"));
        assert_eq!(r.source_root, pkg);
    }

    #[test]
    fn find_package_in_dirs_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let search = vec![dir.path().to_path_buf()];
        assert_eq!(find_package_in_dirs("@ns/pkg:1.0", &search), None);
    }

    fn make_pkg_dir(base: &std::path::Path, ns: &str, name: &str, version: &str) -> PathBuf {
        let dir = base.join(ns).join(name).join(version);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_resolved(
        dir: &std::path::Path,
        ns: &str,
        name: &str,
        version: &str,
    ) -> ResolvedPackage {
        ResolvedPackage {
            name: name.to_string(),
            source_root: dir.to_path_buf(),
            namespace: Some(ns.to_string()),
            version: Some(version.to_string()),
        }
    }

    #[test]
    fn manifest_reads_tool_rheo_section() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "testns", "testpkg", "0.1.0");
        std::fs::write(
            pkg_dir.join("typst.toml"),
            r#"[tool.rheo.html]
css_stylesheet = "style.css"
"#,
        )
        .unwrap();
        let pkg = make_resolved(&pkg_dir, "testns", "testpkg", "0.1.0");
        let result = manifest_package_assets(&pkg, "html").unwrap();
        assert_eq!(result.assets.dest.as_deref(), Some("testns/testpkg"));
        assert_eq!(
            result.assets.extra.get("css_stylesheet").unwrap().as_str(),
            Some("style.css")
        );
        assert!(result.assets.copy.is_empty());
    }

    #[test]
    fn manifest_extracts_copy_patterns() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(
            pkg_dir.join("typst.toml"),
            r#"[tool.rheo.html]
copy = ["**/*.css", "fonts/*"]
css_stylesheet = "style.css"
"#,
        )
        .unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        let result = manifest_package_assets(&pkg, "html").unwrap();
        assert_eq!(
            result.assets.copy,
            vec!["**/*.css".to_string(), "fonts/*".to_string()]
        );
    }

    #[test]
    fn manifest_copy_absent_defaults_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(
            pkg_dir.join("typst.toml"),
            r#"[tool.rheo.html]
css_stylesheet = "style.css"
"#,
        )
        .unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        let result = manifest_package_assets(&pkg, "html").unwrap();
        assert!(result.assets.copy.is_empty());
    }

    #[test]
    fn manifest_no_tool_rheo_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(pkg_dir.join("typst.toml"), "[package]\nname = \"pkg\"\n").unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        assert_eq!(manifest_package_assets(&pkg, "html"), None);
    }

    #[test]
    fn manifest_missing_toml_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        assert_eq!(manifest_package_assets(&pkg, "html"), None);
    }

    #[test]
    fn manifest_empty_section_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(pkg_dir.join("typst.toml"), "[tool.rheo.html]\n").unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        assert_eq!(manifest_package_assets(&pkg, "html"), None);
    }

    #[test]
    fn manifest_malformed_toml_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(pkg_dir.join("typst.toml"), "{{invalid toml!!").unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        assert_eq!(manifest_package_assets(&pkg, "html"), None);
    }

    #[test]
    fn different_namespaces_no_dest_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = make_pkg_dir(tmp.path(), "ns_a", "slides", "1.0");
        let dir_b = make_pkg_dir(tmp.path(), "ns_b", "slides", "1.0");
        std::fs::write(
            dir_a.join("typst.toml"),
            "[tool.rheo.html]\ncss_stylesheet = \"a.css\"\n",
        )
        .unwrap();
        std::fs::write(
            dir_b.join("typst.toml"),
            "[tool.rheo.html]\ncss_stylesheet = \"b.css\"\n",
        )
        .unwrap();

        let search = vec![tmp.path().to_path_buf()];
        let paths = vec![
            "@ns_a/slides:1.0".to_string(),
            "@ns_b/slides:1.0".to_string(),
        ];
        let results = detect_manifest_package_assets_in_dirs(&paths, "html", &search);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].assets.dest.as_deref(), Some("ns_a/slides"));
        assert_eq!(results[1].assets.dest.as_deref(), Some("ns_b/slides"));
    }

    #[test]
    fn prewarm_empty_is_noop() {
        prewarm_packages(&[]);
    }

    #[test]
    fn prewarm_malformed_spec_does_not_panic() {
        prewarm_packages(&["not-a-valid-spec".to_string()]);
    }

    #[test]
    fn prewarm_skips_non_preview_namespace() {
        // Non-preview namespaces are not downloadable via the Typst registry;
        // the filter must skip them without attempting a network call.
        prewarm_packages(&[
            "@local/foo:0.1.0".to_string(),
            "@myns/bar:2.0.0".to_string(),
        ]);
    }
}
