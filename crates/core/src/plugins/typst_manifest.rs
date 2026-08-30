use crate::config::PluginAssets;
use crate::packages::PackageResolver;
use crate::parser::ImportInfo;
use crate::plugins::{PackageAssets, ResolvedPackage, parse_package_spec};
use crate::{Result, RheoError};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::{debug, warn};
use typst::syntax::Source;
use typst::syntax::package::PackageSpec;

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
        for path in ImportInfo::package_paths(&source) {
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

/// A TOML value read as a list of strings, accepting a bare string as a
/// one-element list — `js_scripts` has always allowed either spelling.
fn toml_strings(value: &toml::Value) -> Vec<String> {
    match value {
        toml::Value::String(s) => vec![s.clone()],
        toml::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

/// Locate one package: through `resolver` when its namespace is configured,
/// else by probing `search_dirs`.
fn resolve_package(
    spec: &str,
    resolver: &PackageResolver,
    search_dirs: &[PathBuf],
) -> Option<ResolvedPackage> {
    let (namespace, name, version) = parse_package_spec(spec)?;
    if !resolver.is_configured(namespace) {
        return find_package_in_dirs(spec, search_dirs);
    }
    let parsed = PackageSpec::from_str(spec).ok()?;
    match resolver.obtain(&parsed) {
        Ok(root) => Some(ResolvedPackage {
            name: name.to_string(),
            source_root: root.path().to_path_buf(),
            namespace: Some(namespace.to_string()),
            version: Some(version.to_string()),
        }),
        Err(e) => {
            warn!(spec = %spec, error = ?e, "could not resolve a configured namespace");
            None
        }
    }
}

/// A resolved package's parsed `typst.toml`, the sole read of that file.
pub struct PackageManifest {
    pkg: ResolvedPackage,
    toml: toml::Value,
}

impl PackageManifest {
    /// Reads and parses `{pkg.source_root}/typst.toml`. Missing, unreadable, or
    /// unparseable manifests are logged via warn! and return `None` — a
    /// malformed manifest in someone else's package must never break a build.
    pub fn load(pkg: &ResolvedPackage) -> Option<Self> {
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
        Some(Self {
            pkg: pkg.clone(),
            toml,
        })
    }

    /// The `[tool.rheo.{format_name}]` block, if present and non-empty.
    pub fn package_assets(&self, format_name: &str) -> Option<PackageAssets> {
        self.assets_block(format_name, false)
    }

    /// The asset block for this package, preferring the source-mode block when
    /// the package came from a repository ref.
    ///
    /// A ref carries no `dist/`, so the ordinary block's bundle is absent there;
    /// `[tool.rheo.source.{format}]` names the unbundled sources instead. When a
    /// package declares no source block the ordinary one still applies — that is
    /// the correct answer for a package with nothing to build.
    pub fn assets_for(&self, format_name: &str, source_mode: bool) -> Option<PackageAssets> {
        if source_mode && let Some(block) = self.assets_block(format_name, true) {
            debug!(
                package = %self.pkg.name,
                format = format_name,
                "using the package's source-mode asset block",
            );
            return Some(block);
        }
        self.assets_block(format_name, false)
    }

    /// Declared `js_scripts` paths that are not present under the package root.
    ///
    /// Scans every `[tool.rheo.<format>]` subtable, since which formats a
    /// package declares is up to the package.
    fn missing_declared_scripts(&self) -> Vec<String> {
        let Some(rheo) = self.toml.get("tool").and_then(|t| t.get("rheo")) else {
            return Vec::new();
        };
        let Some(table) = rheo.as_table() else {
            return Vec::new();
        };
        let mut missing = Vec::new();
        for (key, value) in table {
            if key == "source" {
                continue;
            }
            let Some(section) = value.as_table() else {
                continue;
            };
            let Some(scripts) = section.get("js_scripts") else {
                continue;
            };
            for path in toml_strings(scripts) {
                if !self.pkg.source_root.join(&path).exists() {
                    missing.push(path);
                }
            }
        }
        missing.sort();
        missing.dedup();
        missing
    }

    /// Whether this package declares a `[tool.rheo.source.*]` block at all.
    fn has_source_block(&self) -> bool {
        self.toml
            .get("tool")
            .and_then(|t| t.get("rheo"))
            .and_then(|r| r.get("source"))
            .and_then(|s| s.as_table())
            .is_some_and(|t| !t.is_empty())
    }

    fn assets_block(&self, format_name: &str, source: bool) -> Option<PackageAssets> {
        let rheo = self.toml.get("tool")?.get("rheo")?;
        let section = if source {
            rheo.get("source")?.get(format_name)?.as_table()?
        } else {
            rheo.get(format_name)?.as_table()?
        };
        if section.is_empty() {
            return None;
        }
        let js_module = section
            .get("js_module")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
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
        let namespace = self.pkg.namespace.as_deref().unwrap_or("");
        let dest = if namespace.is_empty() {
            self.pkg.name.clone()
        } else {
            format!("{}/{}", namespace, self.pkg.name)
        };
        Some(PackageAssets {
            assets: PluginAssets {
                copy,
                dest: Some(dest),
                extra,
            },
            source_root: self.pkg.source_root.clone(),
            js_module,
        })
    }

    /// The package's declared `[tool.rheo] min_version` floor, if present and valid semver.
    pub fn min_version(&self) -> Option<semver::Version> {
        let raw = self
            .toml
            .get("tool")?
            .get("rheo")?
            .get("min_version")?
            .as_str()?;
        match semver::Version::parse(raw) {
            Ok(v) => Some(v),
            Err(e) => {
                warn!(version = raw, error = %e, "invalid min_version in typst.toml");
                None
            }
        }
    }
}

/// Reads `{pkg.source_root}/typst.toml` and returns a `PackageAssets` for
/// `format_name` if `[tool.rheo.{format_name}]` exists and is non-empty.
/// Returns `None` otherwise. IO and parse errors are logged via warn!.
pub fn manifest_package_assets(pkg: &ResolvedPackage, format_name: &str) -> Option<PackageAssets> {
    PackageManifest::load(pkg)?.package_assets(format_name)
}

/// The imported packages resolvable from a set of Typst package search
/// directories, each probed and its `typst.toml` parsed exactly once.
///
/// Resolution is deliberately forgiving: a package that is not present locally,
/// ships no manifest, or ships a malformed one is simply absent from the index
/// rather than an error, since a broken manifest in someone else's package must
/// never break a build.
/// One import spec, resolved.
struct IndexEntry {
    /// Retained because [`PackageIndex::check_min_versions`] names it in errors.
    spec: String,
    pkg: ResolvedPackage,
    manifest: Option<PackageManifest>,
    /// Resolved from a repository ref, so any `dist/` build output is absent and
    /// the source-mode asset block applies.
    source_mode: bool,
}

pub struct PackageIndex {
    resolved: Vec<IndexEntry>,
}

impl PackageIndex {
    /// Resolve `import_paths` against `search_dirs`, in the given order.
    pub fn new(import_paths: &[String], search_dirs: &[PathBuf]) -> Self {
        let resolved = import_paths
            .iter()
            .filter_map(|spec| {
                let pkg = find_package_in_dirs(spec, search_dirs)?;
                let manifest = PackageManifest::load(&pkg);
                Some(IndexEntry {
                    spec: spec.clone(),
                    pkg,
                    manifest,
                    source_mode: false,
                })
            })
            .collect();
        Self { resolved }
    }

    /// Resolve `import_paths` against Typst's own data/cache directories.
    pub fn system(import_paths: &[String]) -> Self {
        Self::new(import_paths, &typst_package_search_dirs(None))
    }

    /// Resolve `import_paths`, routing a configured namespace through
    /// `resolver` and probing Typst's own directories for everything else.
    ///
    /// A repository-backed package lives at a sha-keyed path bearing no relation
    /// to the `{namespace}/{name}/{version}` layout a directory probe looks for,
    /// so probing alone finds nothing, the package contributes no assets, and
    /// the build still succeeds — with an unstyled site as the only symptom.
    pub fn resolved(import_paths: &[String], resolver: &PackageResolver) -> Self {
        let search_dirs = typst_package_search_dirs(None);
        let resolved = import_paths
            .iter()
            .filter_map(|spec| {
                let pkg = resolve_package(spec, resolver, &search_dirs)?;
                let manifest = PackageManifest::load(&pkg);
                let source_mode = pkg
                    .namespace
                    .as_deref()
                    .is_some_and(|ns| resolver.is_repo_backed(ns));
                Some(IndexEntry {
                    spec: spec.clone(),
                    pkg,
                    manifest,
                    source_mode,
                })
            })
            .collect();
        Self { resolved }
    }

    /// Each package's asset block for this format, in import order — the
    /// source-mode block for a package that came from a repository ref.
    pub fn manifest_assets(&self, format_name: &str) -> Vec<PackageAssets> {
        self.resolved
            .iter()
            .filter_map(|entry| {
                entry
                    .manifest
                    .as_ref()?
                    .assets_for(format_name, entry.source_mode)
            })
            .collect()
    }

    /// Reject a package that came from a ref, declares no source-mode block, and
    /// whose declared scripts are the `dist/` output that no ref carries.
    ///
    /// Without this the failure is silent: the file simply is not there, so the
    /// page loads with no behaviour and nothing says why.
    pub fn check_source_availability(&self) -> Result<()> {
        let mut lines: Vec<String> = Vec::new();
        for entry in &self.resolved {
            let Some(manifest) = &entry.manifest else {
                continue;
            };
            if !entry.source_mode || manifest.has_source_block() {
                continue;
            }
            for missing in manifest.missing_declared_scripts() {
                lines.push(format!(
                    "{spec} declares `js_scripts = \"{missing}\"`, which this ref does not \
                     carry: the package is built, and a repository ref has no build output. \
                     Add a `[tool.rheo.source.<format>]` block naming its `src/` scripts, or \
                     consume the package from a release instead",
                    spec = entry.spec,
                ));
            }
        }
        if lines.is_empty() {
            return Ok(());
        }
        Err(crate::RheoError::project_config(lines.join("\n")))
    }

    /// Errors naming every package whose declared `[tool.rheo] min_version`
    /// exceeds this build — one line per offender, so a project importing
    /// several stale packages learns all of them from a single build.
    pub fn check_min_versions(&self) -> Result<()> {
        let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("CARGO_PKG_VERSION must be valid semver");
        let mut lines: Vec<String> = self
            .resolved
            .iter()
            .filter_map(|entry| {
                let (spec, manifest) = (&entry.spec, &entry.manifest);
                let min = manifest.as_ref()?.min_version()?;
                (min > current)
                    .then(|| format!("{spec} needs rheo >= {min}, but this is rheo {current}"))
            })
            .collect();
        if lines.is_empty() {
            return Ok(());
        }
        lines.push("Upgrade rheo: https://rheo.ohrg.org".to_string());
        Err(RheoError::invalid_data(lines.join("\n")))
    }

    /// Every package's epilogue marrow (`.marrow.typ`), in import order.
    pub fn marrow(&self) -> Vec<String> {
        self.read_marrow(crate::MARROW_FILE)
    }

    /// Every package's prologue marrow (`.marrow-prologue.typ`), in import order.
    pub fn marrow_prologue(&self) -> Vec<String> {
        self.read_marrow(crate::MARROW_PROLOGUE_FILE)
    }

    fn read_marrow(&self, filename: &str) -> Vec<String> {
        self.resolved
            .iter()
            .filter_map(|entry| package_marrow_file(&entry.pkg, filename))
            .collect()
    }
}

/// Reads a package's marrow file at `filename`, if it ships one under that name.
///
/// A package contributes to the bundle root exactly the way a project does —
/// by shipping a marrow file whose text is inlined verbatim — so there is one
/// concept to learn rather than a separate package-only mechanism. Position
/// (before vs. after the documents) is chosen by which filename it ships:
/// [`crate::MARROW_FILE`] (epilogue, default) or
/// [`crate::MARROW_PROLOGUE_FILE`] (prologue). A package may ship either or both.
///
/// The text is spliced into the synthesized main, so paths inside it resolve
/// against the project root, not the package directory: a package's marrow must
/// reach its own code through its package spec (`@ns/name:version`), never a
/// relative import. Files the package imports that way may use relative paths
/// among themselves as usual.
fn package_marrow_file(pkg: &ResolvedPackage, filename: &str) -> Option<String> {
    let marrow_path = pkg.source_root.join(filename);
    if !marrow_path.is_file() {
        return None;
    }
    match std::fs::read_to_string(&marrow_path) {
        Ok(text) => Some(text),
        Err(e) => {
            warn!(path = %marrow_path.display(), error = %e, "could not read package marrow file");
            None
        }
    }
}

/// Ensure each import rheo knows how to fetch is present on disk, downloading
/// or checking out if necessary. A namespace rheo cannot resolve is skipped —
/// it is either a local package (already on disk) or not fetchable at all.
/// No-op for anything already cached. Errors are logged and swallowed — pre-warm
/// failure is not fatal; the downstream scan or compile will surface real
/// problems.
///
/// Call this before `detect_manifest_package_assets` so that scan can see
/// packages that would otherwise only be fetched during compile. Skipping a
/// namespace here does NOT fail the build: it produces a build that succeeds
/// and a site with no stylesheet, because asset detection runs before the
/// compile-time fetch. That is why `resolver` is threaded in rather than
/// rebuilt here — this function has to route exactly as `path_for_id` does.
pub fn prewarm_packages(import_paths: &[String], resolver: &PackageResolver) {
    if import_paths.is_empty() {
        return;
    }
    for spec_str in import_paths {
        let spec = match PackageSpec::from_str(spec_str) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // A configured namespace is checked FIRST, mirroring `path_for_id`, so
        // `[packages.rheo]` overrides the built-in `@rheo` in both places. A
        // divergence between them is a package that compiles from one source and
        // prewarms from another.
        if !resolver.is_prewarmable(&spec.namespace) {
            continue;
        }
        if let Err(e) = resolver.obtain(&spec) {
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
        let results = PackageIndex::new(&paths, &search).manifest_assets("html");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].assets.dest.as_deref(), Some("ns_a/slides"));
        assert_eq!(results[1].assets.dest.as_deref(), Some("ns_b/slides"));
    }

    #[test]
    fn min_version_absent_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(pkg_dir.join("typst.toml"), "[tool.rheo.html]\n").unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        assert_eq!(PackageManifest::load(&pkg).unwrap().min_version(), None);
    }

    #[test]
    fn min_version_invalid_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(
            pkg_dir.join("typst.toml"),
            "[tool.rheo]\nmin_version = \"not-semver\"\n",
        )
        .unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        assert_eq!(PackageManifest::load(&pkg).unwrap().min_version(), None);
    }

    #[test]
    fn min_version_alongside_format_subtable() {
        // The real trap: `[tool.rheo] min_version` coexisting with a sibling
        // `[tool.rheo.<format>]` asset subtable must not shadow either field.
        // `[tool.rheo.source.<format>]` joins the same namespace and must not
        // disturb it either.
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(
            pkg_dir.join("typst.toml"),
            "[tool.rheo]\nmin_version = \"0.5.0\"\n\n[tool.rheo.html]\ncss_stylesheet = \"a.css\"\n\
             js_scripts = \"dist/lib.js\"\n\n[tool.rheo.source.html]\n\
             js_scripts = [\"src/a.js\", \"src/b.js\"]\njs_module = true\n",
        )
        .unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        let manifest = PackageManifest::load(&pkg).unwrap();
        assert_eq!(
            manifest.min_version(),
            Some(semver::Version::parse("0.5.0").unwrap())
        );
        assert!(manifest.package_assets("html").is_some());
        assert!(manifest.has_source_block());

        // Release mode keeps the bundle and its classic tag.
        let release = manifest.assets_for("html", false).unwrap();
        assert!(!release.js_module);
        assert_eq!(
            release.assets.extra.get("js_scripts").unwrap().as_str(),
            Some("dist/lib.js"),
        );

        // Source mode takes the unbundled list and asks for modules.
        let source = manifest.assets_for("html", true).unwrap();
        assert!(source.js_module);
        assert_eq!(
            source
                .assets
                .extra
                .get("js_scripts")
                .and_then(|v| v.as_array())
                .map(Vec::len),
            Some(2),
        );
    }

    /// A package with nothing to build declares no source block, and must still
    /// get its ordinary block when served from a ref.
    #[test]
    fn source_mode_falls_back_to_the_ordinary_block() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(
            pkg_dir.join("typst.toml"),
            "[tool.rheo.html]\ncss_stylesheet = \"src/a.css\"\n",
        )
        .unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        let manifest = PackageManifest::load(&pkg).unwrap();
        assert!(!manifest.has_source_block());
        assert!(manifest.assets_for("html", true).is_some());
        assert!(manifest.missing_declared_scripts().is_empty());
    }

    /// The scripts a ref cannot carry are named, so the failure is not a silent
    /// absence of behaviour on the page.
    #[test]
    fn missing_declared_scripts_names_the_absent_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(
            pkg_dir.join("typst.toml"),
            "[tool.rheo.html]\njs_scripts = \"dist/lib.js\"\n",
        )
        .unwrap();
        let pkg = make_resolved(&pkg_dir, "ns", "pkg", "1.0");
        let manifest = PackageManifest::load(&pkg).unwrap();
        assert_eq!(manifest.missing_declared_scripts(), vec!["dist/lib.js"]);
    }

    fn write_min_version(dir: &std::path::Path, version: &str) {
        std::fs::write(
            dir.join("typst.toml"),
            format!("[tool.rheo]\nmin_version = \"{version}\"\n"),
        )
        .unwrap();
    }

    #[test]
    fn check_min_versions_rejects_package_above_floor() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        write_min_version(&pkg_dir, "99.0.0");
        let search = vec![tmp.path().to_path_buf()];
        let err = PackageIndex::new(&["@ns/pkg:1.0".to_string()], &search)
            .check_min_versions()
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("99.0.0"));
        assert!(message.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn check_min_versions_accepts_package_below_floor() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        write_min_version(&pkg_dir, "0.1.0");
        let search = vec![tmp.path().to_path_buf()];
        assert!(
            PackageIndex::new(&["@ns/pkg:1.0".to_string()], &search)
                .check_min_versions()
                .is_ok()
        );
    }

    #[test]
    fn check_min_versions_accepts_package_with_no_floor() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "ns", "pkg", "1.0");
        std::fs::write(pkg_dir.join("typst.toml"), "[package]\nname = \"pkg\"\n").unwrap();
        let search = vec![tmp.path().to_path_buf()];
        assert!(
            PackageIndex::new(&["@ns/pkg:1.0".to_string()], &search)
                .check_min_versions()
                .is_ok()
        );
    }

    #[test]
    fn check_min_versions_names_all_offenders() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = make_pkg_dir(tmp.path(), "ns", "a", "1.0");
        let dir_b = make_pkg_dir(tmp.path(), "ns", "b", "1.0");
        write_min_version(&dir_a, "99.0.0");
        write_min_version(&dir_b, "98.0.0");
        let search = vec![tmp.path().to_path_buf()];
        let err = PackageIndex::new(&["@ns/a:1.0".to_string(), "@ns/b:1.0".to_string()], &search)
            .check_min_versions()
            .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("@ns/a:1.0"));
        assert!(message.contains("@ns/b:1.0"));
        assert!(message.contains("99.0.0"));
        assert!(message.contains("98.0.0"));
    }

    fn empty_resolver() -> PackageResolver {
        PackageResolver::new(&std::collections::HashMap::new())
    }

    #[test]
    fn prewarm_empty_is_noop() {
        prewarm_packages(&[], &empty_resolver());
    }

    #[test]
    fn prewarm_malformed_spec_does_not_panic() {
        prewarm_packages(&["not-a-valid-spec".to_string()], &empty_resolver());
    }

    #[test]
    fn prewarm_skips_non_preview_namespace() {
        // Namespaces rheo cannot fetch are skipped without a network call.
        prewarm_packages(
            &[
                "@local/foo:0.1.0".to_string(),
                "@myns/bar:2.0.0".to_string(),
            ],
            &empty_resolver(),
        );
    }

    /// The trap this whole path exists to avoid: a CONFIGURED namespace must not
    /// be skipped by pre-warm. Skipping it does not fail the build — it ships a
    /// site with no stylesheet, because asset detection runs before the
    /// compile-time fetch.
    #[test]
    fn prewarm_does_not_skip_a_configured_namespace() {
        use crate::config::{GitRef, NamespaceSource, RepoSource};

        let mut sources = std::collections::HashMap::new();
        sources.insert(
            "rookery".to_string(),
            NamespaceSource::Repo(RepoSource {
                url: "https://example.invalid/rookery".to_string(),
                git_ref: GitRef::Branch("main".to_string()),
                subdir: String::new(),
            }),
        );
        let resolver = PackageResolver::new(&sources);

        assert!(
            resolver.is_prewarmable("rookery"),
            "a configured namespace must be warmed"
        );
        assert!(resolver.is_prewarmable("preview"));
        assert!(resolver.is_prewarmable("rheo"));
        assert!(
            !resolver.is_prewarmable("local"),
            "an unconfigured namespace is still skipped"
        );
    }

    /// `[packages.rheo]` overrides the built-in `@rheo` rather than losing to
    /// it — the ordering `path_for_id` and pre-warm must agree on.
    #[test]
    fn a_configured_rheo_namespace_overrides_the_built_in() {
        use crate::config::{NamespaceSource, ReleasesSource};

        let mut sources = std::collections::HashMap::new();
        sources.insert(
            "rheo".to_string(),
            NamespaceSource::Releases(ReleasesSource::Base("https://example.invalid".to_string())),
        );
        let resolver = PackageResolver::new(&sources);
        assert!(resolver.is_configured("rheo"));
        assert!(resolver.is_prewarmable("rheo"));
    }

    /// An index over one package living at `<base>/testns/testpkg/0.1.0`.
    fn index_for(base: &std::path::Path) -> PackageIndex {
        PackageIndex::new(
            &["@testns/testpkg:0.1.0".to_string()],
            &[base.to_path_buf()],
        )
    }

    #[test]
    fn package_prologue_marrow_is_read_from_its_own_filename() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "testns", "testpkg", "0.1.0");
        std::fs::write(
            pkg_dir.join(crate::MARROW_PROLOGUE_FILE),
            "#show strong: it => it",
        )
        .unwrap();

        let index = index_for(tmp.path());
        assert_eq!(
            index.marrow_prologue(),
            vec!["#show strong: it => it".to_string()]
        );
        assert!(index.marrow().is_empty(), "no .marrow.typ shipped");
    }

    #[test]
    fn package_may_ship_both_marrow_positions() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg_dir = make_pkg_dir(tmp.path(), "testns", "testpkg", "0.1.0");
        std::fs::write(pkg_dir.join(crate::MARROW_FILE), "epilogue").unwrap();
        std::fs::write(pkg_dir.join(crate::MARROW_PROLOGUE_FILE), "prologue").unwrap();

        let index = index_for(tmp.path());
        assert_eq!(index.marrow(), vec!["epilogue".to_string()]);
        assert_eq!(index.marrow_prologue(), vec!["prologue".to_string()]);
    }

    #[test]
    fn detect_package_marrow_prologue_in_dirs_collects_in_import_order() {
        let dir = tempfile::tempdir().unwrap();
        let a = make_pkg_dir(dir.path(), "ns", "a", "1.0");
        std::fs::write(a.join(crate::MARROW_PROLOGUE_FILE), "a-prologue").unwrap();
        let b = make_pkg_dir(dir.path(), "ns", "b", "1.0");
        std::fs::write(b.join(crate::MARROW_PROLOGUE_FILE), "b-prologue").unwrap();

        let result = PackageIndex::new(
            &["@ns/a:1.0".to_string(), "@ns/b:1.0".to_string()],
            &[dir.path().to_path_buf()],
        )
        .marrow_prologue();
        assert_eq!(
            result,
            vec!["a-prologue".to_string(), "b-prologue".to_string()]
        );
    }
}
