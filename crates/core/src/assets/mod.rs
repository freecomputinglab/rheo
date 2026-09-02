//! Asset resolution and copying for format plugins.
//!
//! Provides [`AssetResolver`] which resolves plugin assets (stylesheets, scripts,
//! etc.) from project sources and package dependencies, copies them into the
//! build output directory, and detects path collisions.

pub mod watch;

use crate::config::PluginSection;
use crate::plugins::{Asset, AssetConfig, FormatPlugin, PackageAssets};
use crate::{Result, RheoError};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};
use walkdir::WalkDir;

/// Where an asset override candidate came from. The two precedence rules
/// that used to live only in a comment are now properties of the variant:
/// [`Self::root`] sends `Package` to its own directory rather than the
/// project root, so it can never collide with — or stand in for — a project
/// source; [`Self::warns_on_missing`] is false only for `Package`, since a
/// package-declared file going missing is routine, not misconfiguration.
enum AssetSource<'b> {
    /// A project-authored `[[plugin.assets]]` override.
    User,
    /// Contributed by an `[packages.<ns>]` block, resolved against the
    /// package's own directory.
    Package { source_root: &'b Path, module: bool },
    /// The project-root filename convention, pushed by [`gather_entries`]
    /// only when the project declared no `User` entry of its own.
    ProjectDefault,
}

impl<'b> AssetSource<'b> {
    fn root(&self, project_root: &'b Path) -> &'b Path {
        match self {
            AssetSource::Package { source_root, .. } => source_root,
            AssetSource::User | AssetSource::ProjectDefault => project_root,
        }
    }

    fn module(&self) -> bool {
        matches!(self, AssetSource::Package { module: true, .. })
    }

    /// A package-declared file that's missing stays silent; only a
    /// project-owned entry (user override or the root convention) warns.
    fn warns_on_missing(&self) -> bool {
        !matches!(self, AssetSource::Package { .. })
    }
}

/// One candidate source for a declared asset, gathered from user overrides,
/// package blocks, or the project-root convention.
struct AssetEntry<'b> {
    dest: Option<&'b str>,
    path: &'b str,
    source: AssetSource<'b>,
}

/// Entries sharing an output directory and resolution root, copied together.
struct AssetGroup<'b> {
    dest: Option<&'b str>,
    root: &'b Path,
    entries: Vec<AssetEntry<'b>>,
}

/// Gather every candidate source for `asset_config`: the project's own
/// override blocks, each package's contribution, and — only when the
/// project declared no `AssetSource::User` entry — the project-root
/// convention. A package entry is additive and must never satisfy that
/// last test, which is why it checks the gathered `entries` themselves
/// rather than a separately-tracked "user pairs" list.
fn gather_entries<'b>(
    asset_config: &AssetConfig,
    section: &'b PluginSection,
    package_blocks: &'b [PackageAssets],
) -> Vec<AssetEntry<'b>> {
    let mut entries: Vec<AssetEntry<'b>> = section
        .get_strings_with_block(asset_config.name)
        .into_iter()
        .map(|(block, path)| AssetEntry {
            dest: block.dest.as_deref(),
            path,
            source: AssetSource::User,
        })
        .collect();

    for pkg in package_blocks {
        // A list as well as a bare string: source mode names every
        // unbundled script, where a release names one bundle.
        let paths: Vec<&str> = match pkg.assets.extra.get(asset_config.name) {
            Some(toml::Value::String(s)) => vec![s.as_str()],
            Some(toml::Value::Array(items)) => items.iter().filter_map(|v| v.as_str()).collect(),
            _ => Vec::new(),
        };
        entries.extend(paths.into_iter().map(|path| AssetEntry {
            dest: pkg.assets.dest.as_deref(),
            path,
            source: AssetSource::Package {
                source_root: &pkg.source_root,
                module: pkg.js_module,
            },
        }));
    }

    if !entries
        .iter()
        .any(|e| matches!(e.source, AssetSource::User))
    {
        entries.push(AssetEntry {
            dest: None,
            path: asset_config.default_path,
            source: AssetSource::ProjectDefault,
        });
    }

    entries
}

/// Group entries by (dest, resolution root), preserving first-seen order.
/// An index `HashMap` gives each key's position in `groups` in O(1), so
/// grouping stays linear despite the ordered output — `indexmap` would do
/// this directly, but it isn't a dependency of rheo-core and this task's
/// edits are scoped to this file, so it's a plain `Vec` plus an index map
/// instead of a new external dependency.
fn group_entries<'b>(entries: Vec<AssetEntry<'b>>, project_root: &'b Path) -> Vec<AssetGroup<'b>> {
    let mut index: HashMap<(Option<&'b str>, &'b Path), usize> = HashMap::new();
    let mut groups: Vec<AssetGroup<'b>> = Vec::new();
    for entry in entries {
        let root = entry.source.root(project_root);
        let key = (entry.dest, root);
        let idx = *index.entry(key).or_insert_with(|| {
            groups.push(AssetGroup {
                dest: entry.dest,
                root,
                entries: Vec::new(),
            });
            groups.len() - 1
        });
        groups[idx].entries.push(entry);
    }
    groups
}

/// Resolves plugin assets and copies source files into the build output directory.
///
/// Construct with a project root and plugin output directory, then call
/// [`resolve`](Self::resolve) to gather assets and [`copy_globs`](Self::copy_globs)
/// to expand glob-based copy patterns.
pub struct AssetResolver<'a> {
    project_root: &'a Path,
    plugin_output_dir: &'a Path,
}

impl<'a> AssetResolver<'a> {
    /// Create a new resolver for the given project root and plugin output directory.
    pub fn new(project_root: &'a Path, plugin_output_dir: &'a Path) -> Self {
        Self {
            project_root,
            plugin_output_dir,
        }
    }

    /// Resolve plugin assets, collecting overrides across all `[[plugin.assets]]` blocks
    /// and package blocks, then copying each source verbatim into the plugin output dir.
    ///
    /// When a block has `dest` set, named assets are placed under that subdirectory
    /// with their directory components stripped (basename only).
    pub fn resolve(
        &self,
        plugin: &dyn FormatPlugin,
        section: &PluginSection,
        package_blocks: &[PackageAssets],
    ) -> Result<HashMap<&'static str, Vec<Asset>>> {
        let mut resolved = HashMap::new();
        let mut seen_relative_paths: HashMap<String, PathBuf> = HashMap::new();
        for asset_config in plugin.assets() {
            if let Some(assets) = self.resolve_asset_config(
                plugin,
                &asset_config,
                section,
                package_blocks,
                &mut seen_relative_paths,
            )? {
                resolved.insert(asset_config.name, assets);
            }
        }
        Ok(resolved)
    }

    /// Resolve one declared asset: gather its candidates, copy each group,
    /// and — when nothing on disk resolved — fall back to the plugin's
    /// embedded default. `Ok(None)` means an optional asset with no source
    /// at all.
    fn resolve_asset_config(
        &self,
        plugin: &dyn FormatPlugin,
        asset_config: &AssetConfig,
        section: &PluginSection,
        package_blocks: &[PackageAssets],
        seen_relative_paths: &mut HashMap<String, PathBuf>,
    ) -> Result<Option<Vec<Asset>>> {
        let entries = gather_entries(asset_config, section, package_blocks);
        let tried_paths: Vec<&str> = entries.iter().map(|e| e.path).collect();

        let mut all_assets = Vec::new();
        for group in group_entries(entries, self.project_root) {
            all_assets.extend(self.copy_group(group, asset_config, plugin, seen_relative_paths)?);
        }
        if !all_assets.is_empty() {
            return Ok(Some(all_assets));
        }

        if asset_config.required {
            return Err(RheoError::project_config(format!(
                "plugin '{}' requires input '{}' but no source was found (tried: {})",
                plugin.name(),
                asset_config.name,
                tried_paths.join(", ")
            )));
        }

        Ok(self
            .embedded_fallback(asset_config, seen_relative_paths)?
            .map(|asset| vec![asset]))
    }

    /// Copy every on-disk source in `group`, registering each result's
    /// build-relative path and erroring on a collision. A missing source is
    /// skipped; only a project-owned entry warns (see
    /// [`AssetSource::warns_on_missing`]).
    fn copy_group(
        &self,
        group: AssetGroup<'_>,
        asset_config: &AssetConfig,
        plugin: &dyn FormatPlugin,
        seen_relative_paths: &mut HashMap<String, PathBuf>,
    ) -> Result<Vec<Asset>> {
        let out_dir = match group.dest {
            Some(d) => self.plugin_output_dir.join(d),
            None => self.plugin_output_dir.to_path_buf(),
        };

        let mut sources: Vec<PathBuf> = Vec::new();
        let mut modules: Vec<bool> = Vec::new();
        for entry in &group.entries {
            let abs = group.root.join(entry.path);
            if abs.is_file() {
                sources.push(abs);
                modules.push(entry.source.module());
            } else if entry.source.warns_on_missing() {
                warn!(
                    plugin = plugin.name(),
                    asset = asset_config.name,
                    path = %entry.path,
                    "asset override path not found, skipping"
                );
            }
        }
        if sources.is_empty() {
            return Ok(Vec::new());
        }

        let outputs = copy_each(&sources, group.root, &out_dir, group.dest.is_some())?;
        outputs
            .into_iter()
            .zip(sources.iter())
            .zip(modules.iter())
            .map(|((abs, src), module)| {
                let rel = abs
                    .strip_prefix(self.plugin_output_dir)
                    .expect("copy_each output is always under plugin_output_dir")
                    .to_string_lossy()
                    .into_owned();
                if let Some(prev) = seen_relative_paths.get(&rel) {
                    return Err(RheoError::project_config(format!(
                        "asset path collision: output '{}' would be written by both '{}' and '{}'",
                        rel,
                        prev.display(),
                        src.display()
                    )));
                }
                seen_relative_paths.insert(rel.clone(), src.clone());
                Ok(Asset {
                    config: asset_config.clone(),
                    module: *module,
                    source_path: src.clone(),
                    resolved_path: abs,
                    built_relative_path: rel,
                })
            })
            .collect()
    }

    /// Write the plugin's embedded default asset when an optional asset had
    /// no on-disk source at all. `Ok(None)` when the asset config declares
    /// no fallback. Written as a real file so it is copied and linked like
    /// any other asset instead of being inlined.
    fn embedded_fallback(
        &self,
        asset_config: &AssetConfig,
        seen_relative_paths: &mut HashMap<String, PathBuf>,
    ) -> Result<Option<Asset>> {
        let Some(embedded) = asset_config.default_content else {
            return Ok(None);
        };
        let rel = embedded.name.to_string();
        if let Some(prev) = seen_relative_paths.get(&rel) {
            return Err(RheoError::project_config(format!(
                "asset path collision: output '{}' would be written by both '{}' and the embedded default for '{}'",
                rel,
                prev.display(),
                asset_config.name
            )));
        }
        let dest = self.plugin_output_dir.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RheoError::io(
                    e,
                    format!("creating directory for embedded default '{}'", rel),
                )
            })?;
        }
        std::fs::write(&dest, embedded.content).map_err(|e| {
            RheoError::io(e, format!("writing embedded default asset to {:?}", dest))
        })?;
        seen_relative_paths.insert(rel.clone(), dest.clone());
        Ok(Some(Asset {
            config: asset_config.clone(),
            module: false,
            source_path: dest.clone(),
            resolved_path: dest,
            built_relative_path: rel,
        }))
    }

    /// Expand glob patterns against `source_root` and copy matching files into
    /// the plugin output directory (optionally under `dest_prefix`).
    ///
    /// When `warn_on_overwrite` is true, logs a warning for each destination file
    /// that already exists (meaning a bundle output is being overwritten by a copy glob).
    pub fn copy_globs(
        &self,
        patterns: &[String],
        source_root: &Path,
        dest_prefix: Option<&str>,
        warn_on_overwrite: bool,
    ) -> Result<()> {
        copy_glob_patterns(
            patterns,
            source_root,
            self.plugin_output_dir,
            dest_prefix,
            warn_on_overwrite,
        )
    }
}

/// Copy each source file verbatim into the build dir.
/// When `strip_to_basename` is true, only the filename is used (for dest-prefixed dirs).
/// Otherwise the project-root-relative path is preserved.
fn copy_each(
    sources: &[PathBuf],
    source_root: &Path,
    build_dir: &Path,
    strip_to_basename: bool,
) -> Result<Vec<PathBuf>> {
    let mut out = Vec::with_capacity(sources.len());
    for src in sources {
        let rel = src.strip_prefix(source_root).map_err(|_| {
            RheoError::project_config(format!(
                "asset override path '{}' is absolute or outside the source root; paths must be relative to the source root",
                src.display()
            ))
        })?;
        let dest = if strip_to_basename {
            build_dir.join(src.file_name().expect("source must have a filename"))
        } else {
            build_dir.join(rel)
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RheoError::io(
                    e,
                    format!("creating directory for asset '{}'", rel.display()),
                )
            })?;
        }
        std::fs::copy(src, &dest).map_err(|e| RheoError::AssetCopy {
            source: src.clone(),
            dest: dest.clone(),
            error: e,
        })?;
        out.push(dest);
    }
    Ok(out)
}

/// A `copy` pattern list compiled once against a base directory, shared by the
/// copy step ([`copy_glob_patterns`]) and [`crate::assets::watch::WatchAssetSpec`]
/// so both agree on exactly the same file set — unifying what used to be two
/// drifting engines (the `glob` crate here, `globset` in the watcher), where
/// only the latter understood brace alternation (`*.{png,jpg}`).
///
/// `literal_separator` is enabled so `*` does not cross `/` while `**` still
/// descends, matching both the old `glob`-crate copy behaviour and the
/// exclude/include globs in [`crate::reticulate::spine`].
#[derive(Debug)]
pub struct CopyGlobs {
    set: GlobSet,
    /// Successfully compiled patterns, in the same order as `set`'s indices —
    /// used only to report a pattern that matched nothing.
    patterns: Vec<String>,
}

impl CopyGlobs {
    /// Compile `patterns` (relative to `base`) into an absolute-path glob set.
    /// A pattern that fails to compile is warned about and skipped rather than
    /// failing the whole build. Returns `None` when nothing compiled.
    pub fn compile(base: &Path, patterns: &[String]) -> Option<Self> {
        let mut builder = GlobSetBuilder::new();
        let mut compiled = Vec::new();
        for pattern in patterns {
            let abs = base.join(pattern).display().to_string();
            match GlobBuilder::new(&abs).literal_separator(true).build() {
                Ok(glob) => {
                    builder.add(glob);
                    compiled.push(pattern.clone());
                }
                Err(e) => warn!(pattern = %pattern, error = %e, "invalid copy pattern, skipping"),
            }
        }
        if compiled.is_empty() {
            return None;
        }
        builder.build().ok().map(|set| Self {
            set,
            patterns: compiled,
        })
    }

    /// True if `path` matches any compiled pattern.
    pub fn is_match(&self, path: &Path) -> bool {
        self.set.is_match(path)
    }

    /// Indices into `self.patterns` that match `path`.
    fn matches(&self, path: &Path) -> Vec<usize> {
        self.set.matches(path)
    }
}

/// Walk `source_root` and copy every file matching a compiled copy-glob into
/// `plugin_output_dir` (optionally under `dest_prefix`).
///
/// When `warn_on_overwrite` is true, logs a warning for each destination file
/// that already exists before it is overwritten. A directory-walk error (e.g.
/// a permission-denied subdirectory) is warned about and skipped rather than
/// silently dropped.
fn copy_glob_patterns(
    patterns: &[String],
    source_root: &Path,
    plugin_output_dir: &Path,
    dest_prefix: Option<&str>,
    warn_on_overwrite: bool,
) -> Result<()> {
    let Some(globs) = CopyGlobs::compile(source_root, patterns) else {
        return Ok(());
    };
    let mut matched = vec![false; globs.patterns.len()];
    for entry in WalkDir::new(source_root) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "error walking source tree for copy globs");
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let idxs = globs.matches(path);
        if idxs.is_empty() {
            continue;
        }
        for i in idxs {
            matched[i] = true;
        }
        let rel = path.strip_prefix(source_root).unwrap_or(path);
        let dest = match dest_prefix {
            Some(d) => plugin_output_dir.join(d).join(rel),
            None => plugin_output_dir.join(rel),
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RheoError::io(
                    e,
                    format!("creating directory for copy of {}", rel.display()),
                )
            })?;
        }
        if warn_on_overwrite && dest.exists() {
            warn!(
                src = %path.display(),
                dest = %dest.display(),
                "copy glob overwrites existing bundle output"
            );
        }
        std::fs::copy(path, &dest).map_err(|e| RheoError::AssetCopy {
            source: path.to_path_buf(),
            dest: dest.clone(),
            error: e,
        })?;
        debug!(src = %path.display(), dest = %dest.display(), "copied file");
    }
    for (pattern, was_matched) in globs.patterns.iter().zip(matched.iter()) {
        if !was_matched {
            debug!(pattern = %pattern, "copy pattern matched no files");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AssetsField, PluginAssets};
    use crate::plugins::PluginContext;
    use crate::{AssetConfig, EmbeddedDefault, FormatPlugin, Result};

    struct MockPlugin {
        plugin_name: &'static str,
        declared_assets: Vec<AssetConfig>,
    }

    impl FormatPlugin for MockPlugin {
        fn name(&self) -> &'static str {
            self.plugin_name
        }
        fn assets(&self) -> Vec<AssetConfig> {
            self.declared_assets.clone()
        }
        fn compile(
            &self,
            _ctx: PluginContext<'_>,
            _outputs: &[crate::plugins::CastVertebra],
        ) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_resolve_assets_default_path_when_no_override() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::write(project_root.join("style.css"), "body {}").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };
        let section = PluginSection::default();

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        assert_eq!(
            resolved.get("css_stylesheet").unwrap()[0].built_relative_path,
            "style.css"
        );
    }

    #[test]
    fn test_resolve_assets_override_path_when_configured() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::write(project_root.join("custom.css"), "body {}").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };
        let mut asset_extra = toml::map::Map::new();
        asset_extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("custom.css".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Single(PluginAssets {
                extra: asset_extra,
                ..Default::default()
            })),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        assert_eq!(
            resolved.get("css_stylesheet").unwrap()[0].built_relative_path,
            "custom.css"
        );
    }

    #[test]
    fn test_resolve_assets_required_missing_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "missing_asset",
                default_path: "nonexistent.css",
                required: true,
                default_content: None,
            }],
        };
        let section = PluginSection::default();

        let resolver = AssetResolver::new(project_root, &output_dir);
        let result = resolver.resolve(&plugin, &section, &[]);
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("requires input"),
            "expected 'requires input' in error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_resolve_assets_optional_missing_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "optional_asset",
                default_path: "nonexistent.css",
                required: false,
                default_content: None,
            }],
        };
        let section = PluginSection::default();

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        assert!(
            !resolved.contains_key("optional_asset"),
            "optional missing asset should not be in resolved map"
        );
    }

    #[test]
    fn test_resolve_assets_subdirectory_in_override_path() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        let styles_dir = project_root.join("styles");
        std::fs::create_dir_all(&styles_dir).unwrap();
        std::fs::write(styles_dir.join("custom.css"), "body {}").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };
        let mut asset_extra = toml::map::Map::new();
        asset_extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("styles/custom.css".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Single(PluginAssets {
                extra: asset_extra,
                ..Default::default()
            })),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        assert_eq!(
            resolved.get("css_stylesheet").unwrap()[0].built_relative_path,
            "styles/custom.css"
        );
        assert!(
            output_dir.join("styles/custom.css").exists(),
            "subdirectory asset should be copied to output"
        );
    }

    #[test]
    fn test_resolve_assets_multiple_blocks_copy_each() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::write(project_root.join("one.css"), "/* one */").unwrap();
        std::fs::write(project_root.join("two.css"), "/* two */").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };

        let mut extra1 = toml::map::Map::new();
        extra1.insert(
            "css_stylesheet".into(),
            toml::Value::String("one.css".into()),
        );
        let mut extra2 = toml::map::Map::new();
        extra2.insert(
            "css_stylesheet".into(),
            toml::Value::String("two.css".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Multiple(vec![
                PluginAssets {
                    extra: extra1,
                    ..Default::default()
                },
                PluginAssets {
                    extra: extra2,
                    ..Default::default()
                },
            ])),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        let assets = resolved.get("css_stylesheet").unwrap();
        assert_eq!(assets.len(), 2);
        assert!(output_dir.join("one.css").exists());
        assert!(output_dir.join("two.css").exists());
    }

    #[test]
    fn test_resolve_assets_required_all_missing_errors() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "missing_asset",
                default_path: "nonexistent.css",
                required: true,
                default_content: None,
            }],
        };
        let section = PluginSection::default();

        let resolver = AssetResolver::new(project_root, &output_dir);
        let result = resolver.resolve(&plugin, &section, &[]);
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("requires input"),
            "expected 'requires input' in error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_resolve_assets_required_some_missing_warns_but_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::write(project_root.join("exists.css"), "body {}").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: true,
                default_content: None,
            }],
        };

        let mut extra1 = toml::map::Map::new();
        extra1.insert(
            "css_stylesheet".into(),
            toml::Value::String("exists.css".into()),
        );
        let mut extra2 = toml::map::Map::new();
        extra2.insert(
            "css_stylesheet".into(),
            toml::Value::String("missing.css".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Multiple(vec![
                PluginAssets {
                    extra: extra1,
                    ..Default::default()
                },
                PluginAssets {
                    extra: extra2,
                    ..Default::default()
                },
            ])),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        let assets = resolved.get("css_stylesheet").unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].built_relative_path, "exists.css");
    }

    #[test]
    fn test_resolve_assets_collision_across_blocks_errors() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        // Both blocks point at the same file via different paths isn't a collision,
        // but two blocks with the same *relative* dest path is. Create two files
        // in different dirs that both strip to "style.css" in the output dir.
        std::fs::write(project_root.join("a.css"), "/* a */").unwrap();
        std::fs::write(project_root.join("b.css"), "/* b */").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };

        // Both overrides resolve to the same built_relative_path "style.css"
        // because they're at the project root. This means copy_each would
        // overwrite — detect this as a collision.
        // Actually, both "a.css" and "b.css" are at the root, so their dest
        // paths differ ("a.css" vs "b.css"). To create a real collision we
        // need two blocks that both set css_stylesheet = "same.css".
        std::fs::write(project_root.join("same.css"), "/* same */").unwrap();

        let mut extra1 = toml::map::Map::new();
        extra1.insert(
            "css_stylesheet".into(),
            toml::Value::String("same.css".into()),
        );
        let mut extra2 = toml::map::Map::new();
        extra2.insert(
            "css_stylesheet".into(),
            toml::Value::String("same.css".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Multiple(vec![
                PluginAssets {
                    extra: extra1,
                    ..Default::default()
                },
                PluginAssets {
                    extra: extra2,
                    ..Default::default()
                },
            ])),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let result = resolver.resolve(&plugin, &section, &[]);
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("asset path collision"),
            "expected collision error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_resolve_assets_absolute_override_errors() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        // Create a file at an absolute path outside the project root
        let abs_css = std::env::temp_dir().join("rheo_test_absolute.css");
        std::fs::write(&abs_css, "body {}").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };

        let mut asset_extra = toml::map::Map::new();
        asset_extra.insert(
            "css_stylesheet".into(),
            toml::Value::String(abs_css.to_str().unwrap().into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Single(PluginAssets {
                extra: asset_extra,
                ..Default::default()
            })),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let result = resolver.resolve(&plugin, &section, &[]);
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("absolute or outside the source root"),
            "expected absolute path error, got: {}",
            err_msg
        );

        // Clean up
        let _ = std::fs::remove_file(&abs_css);
    }

    #[test]
    fn test_resolve_assets_dest_places_in_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::write(project_root.join("custom.css"), "/* custom */").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };

        let mut extra = toml::map::Map::new();
        extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("custom.css".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Multiple(vec![PluginAssets {
                extra,
                dest: Some("subdir".into()),
                ..Default::default()
            }])),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        let assets = resolved.get("css_stylesheet").unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].built_relative_path, "subdir/custom.css");
        assert!(output_dir.join("subdir/custom.css").exists());
    }

    #[test]
    fn test_resolve_assets_mixed_dest_and_no_dest() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::write(project_root.join("root.css"), "/* root */").unwrap();
        std::fs::write(project_root.join("dest.css"), "/* dest */").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };

        let mut extra1 = toml::map::Map::new();
        extra1.insert(
            "css_stylesheet".into(),
            toml::Value::String("root.css".into()),
        );
        let mut extra2 = toml::map::Map::new();
        extra2.insert(
            "css_stylesheet".into(),
            toml::Value::String("dest.css".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Multiple(vec![
                PluginAssets {
                    extra: extra1,
                    ..Default::default()
                },
                PluginAssets {
                    extra: extra2,
                    dest: Some("assets".into()),
                    ..Default::default()
                },
            ])),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        let assets = resolved.get("css_stylesheet").unwrap();
        assert_eq!(assets.len(), 2);
        assert!(output_dir.join("root.css").exists());
        assert!(output_dir.join("assets/dest.css").exists());
    }

    #[test]
    fn test_resolve_assets_dest_strips_to_basename() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::create_dir_all(project_root.join("dist")).unwrap();
        std::fs::write(project_root.join("dist/index.js"), "// js").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "js_scripts",
                default_path: "script.js",
                required: false,
                default_content: None,
            }],
        };

        let mut extra = toml::map::Map::new();
        extra.insert(
            "js_scripts".into(),
            toml::Value::String("dist/index.js".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Multiple(vec![PluginAssets {
                extra,
                dest: Some("allassets".into()),
                ..Default::default()
            }])),
            ..Default::default()
        };

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver.resolve(&plugin, &section, &[]).unwrap();
        let assets = resolved.get("js_scripts").unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].built_relative_path, "allassets/index.js");
        assert!(output_dir.join("allassets/index.js").exists());
        assert!(
            !output_dir.join("allassets/dist/index.js").exists(),
            "source directory components should be stripped under dest"
        );
    }

    #[test]
    fn test_resolve_assets_package_block_css_override() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        let pkg_dir = dir.path().join("pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("index.css"), "body { color: red; }").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };
        let section = PluginSection::default();
        let mut extra = toml::map::Map::new();
        extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("index.css".into()),
        );
        let package_blocks = vec![PackageAssets {
            assets: PluginAssets {
                copy: vec![],
                dest: Some("pkg".into()),
                extra,
            },
            js_module: false,
            source_root: pkg_dir.clone(),
        }];

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver
            .resolve(&plugin, &section, &package_blocks)
            .unwrap();
        let assets = resolved.get("css_stylesheet").unwrap();
        assert_eq!(assets[0].built_relative_path, "pkg/index.css");
    }

    #[test]
    fn test_resolve_assets_package_block_optional_missing_skips() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        let pkg_dir = dir.path().join("pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        // No index.css on disk

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };
        let section = PluginSection::default();
        let mut extra = toml::map::Map::new();
        extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("index.css".into()),
        );
        let package_blocks = vec![PackageAssets {
            assets: PluginAssets {
                copy: vec![],
                dest: Some("pkg".into()),
                extra,
            },
            js_module: false,
            source_root: pkg_dir,
        }];

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver
            .resolve(&plugin, &section, &package_blocks)
            .unwrap();
        assert!(
            !resolved.contains_key("css_stylesheet"),
            "missing package default should be silently skipped"
        );
    }

    #[test]
    fn test_resolve_assets_user_and_package_collision() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        let pkg_dir = dir.path().join("pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("x.css"), "/* pkg */").unwrap();
        std::fs::write(project_root.join("x.css"), "/* user */").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };
        let mut user_extra = toml::map::Map::new();
        user_extra.insert("css_stylesheet".into(), toml::Value::String("x.css".into()));
        let section = PluginSection {
            assets: Some(AssetsField::Single(PluginAssets {
                dest: Some("pkg".into()),
                extra: user_extra,
                ..Default::default()
            })),
            ..Default::default()
        };
        let mut pkg_extra = toml::map::Map::new();
        pkg_extra.insert("css_stylesheet".into(), toml::Value::String("x.css".into()));
        let package_blocks = vec![PackageAssets {
            assets: PluginAssets {
                copy: vec![],
                dest: Some("pkg".into()),
                extra: pkg_extra,
            },
            js_module: false,
            source_root: pkg_dir,
        }];

        let resolver = AssetResolver::new(project_root, &output_dir);
        let result = resolver.resolve(&plugin, &section, &package_blocks);
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("asset path collision"),
            "expected collision error, got: {}",
            err
        );
    }

    #[test]
    fn test_resolve_assets_user_and_package_stack_css() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        let pkg_dir = dir.path().join("pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("index.css"), "/* pkg default */").unwrap();
        std::fs::write(project_root.join("custom.css"), "/* user */").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: None,
            }],
        };
        // User declares custom.css for dest "pkg"
        let mut user_extra = toml::map::Map::new();
        user_extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("custom.css".into()),
        );
        let section = PluginSection {
            assets: Some(AssetsField::Single(PluginAssets {
                dest: Some("pkg".into()),
                extra: user_extra,
                ..Default::default()
            })),
            ..Default::default()
        };
        // Package contributes index.css for dest "pkg"
        let mut pkg_extra = toml::map::Map::new();
        pkg_extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("index.css".into()),
        );
        let package_blocks = vec![PackageAssets {
            assets: PluginAssets {
                copy: vec![],
                dest: Some("pkg".into()),
                extra: pkg_extra,
            },
            js_module: false,
            source_root: pkg_dir,
        }];

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver
            .resolve(&plugin, &section, &package_blocks)
            .unwrap();
        let assets = resolved.get("css_stylesheet").unwrap();
        let paths: Vec<&str> = assets
            .iter()
            .map(|a| a.built_relative_path.as_str())
            .collect();
        assert!(
            paths.contains(&"pkg/custom.css"),
            "expected user css in output, got: {:?}",
            paths
        );
        assert!(
            paths.contains(&"pkg/index.css"),
            "expected package default css in output, got: {:?}",
            paths
        );
    }

    /// rheo-965: a package contributing `css_stylesheet` must not suppress the
    /// project's own conventional `style.css` — the two are additive, resolved
    /// against different roots, and the project declares no override block at all.
    #[test]
    fn test_resolve_assets_project_default_survives_package_presence() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();
        std::fs::write(project_root.join("style.css"), "/* project */").unwrap();

        let pkg_dir = dir.path().join("pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("index.css"), "/* pkg */").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: Some(EmbeddedDefault {
                    name: "rheo-default.css",
                    content: "/* fallback */",
                }),
            }],
        };
        let mut pkg_extra = toml::map::Map::new();
        pkg_extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("index.css".into()),
        );
        let package_blocks = vec![PackageAssets {
            assets: PluginAssets {
                copy: vec![],
                dest: Some("pkg".into()),
                extra: pkg_extra,
            },
            js_module: false,
            source_root: pkg_dir,
        }];

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver
            .resolve(&plugin, &PluginSection::default(), &package_blocks)
            .unwrap();
        let assets = resolved.get("css_stylesheet").unwrap();
        let paths: Vec<&str> = assets
            .iter()
            .map(|a| a.built_relative_path.as_str())
            .collect();
        assert!(
            paths.contains(&"style.css"),
            "project's own style.css must survive an unrelated package block, got: {:?}",
            paths
        );
        assert!(
            paths.contains(&"pkg/index.css"),
            "package css must still be included, got: {:?}",
            paths
        );
        assert!(
            !paths.contains(&"rheo-default.css"),
            "embedded fallback must not fire when the project has its own style.css, got: {:?}",
            paths
        );
    }

    /// rheo-965: with no project-root style.css at all, a package's CSS is linked
    /// but must suppress the embedded `rheo-default.css` fallback — the project
    /// already has real styling via the package, so it does not also want rheo's
    /// default chrome under it.
    #[test]
    fn test_resolve_assets_package_only_suppresses_embedded_default() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();
        // No style.css at project root.

        let pkg_dir = dir.path().join("pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("index.css"), "/* pkg */").unwrap();

        let plugin = MockPlugin {
            plugin_name: "html",
            declared_assets: vec![AssetConfig {
                name: "css_stylesheet",
                default_path: "style.css",
                required: false,
                default_content: Some(EmbeddedDefault {
                    name: "rheo-default.css",
                    content: "/* fallback */",
                }),
            }],
        };
        let mut pkg_extra = toml::map::Map::new();
        pkg_extra.insert(
            "css_stylesheet".into(),
            toml::Value::String("index.css".into()),
        );
        let package_blocks = vec![PackageAssets {
            assets: PluginAssets {
                copy: vec![],
                dest: Some("pkg".into()),
                extra: pkg_extra,
            },
            js_module: false,
            source_root: pkg_dir,
        }];

        let resolver = AssetResolver::new(project_root, &output_dir);
        let resolved = resolver
            .resolve(&plugin, &PluginSection::default(), &package_blocks)
            .unwrap();
        let assets = resolved.get("css_stylesheet").unwrap();
        let paths: Vec<&str> = assets
            .iter()
            .map(|a| a.built_relative_path.as_str())
            .collect();
        assert_eq!(
            paths,
            vec!["pkg/index.css"],
            "only the package css should resolve, with no embedded fallback: {:?}",
            paths
        );
    }

    #[test]
    fn test_copy_globs_wins_over_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        // Simulate a bundle output already written to output_dir.
        let bundle_content = b"bundle-output";
        std::fs::write(output_dir.join("logo.png"), bundle_content).unwrap();

        // User's source file with different content — should win.
        let copy_content = b"copy-wins";
        std::fs::write(project_root.join("logo.png"), copy_content).unwrap();

        let resolver = AssetResolver::new(project_root, &output_dir);
        resolver
            .copy_globs(&["logo.png".into()], project_root, None, true)
            .unwrap();

        let written = std::fs::read(output_dir.join("logo.png")).unwrap();
        assert_eq!(
            written, copy_content,
            "copy glob should overwrite bundle output"
        );
    }

    #[test]
    fn test_copy_globs_no_warn_when_dest_absent() {
        let dir = tempfile::tempdir().unwrap();
        let project_root = dir.path();
        let output_dir = dir.path().join("build/html");
        std::fs::create_dir_all(&output_dir).unwrap();

        std::fs::write(project_root.join("style.css"), b"body {}").unwrap();

        let resolver = AssetResolver::new(project_root, &output_dir);
        // Should succeed without panicking even with warn_on_overwrite=true.
        resolver
            .copy_globs(&["style.css".into()], project_root, None, true)
            .unwrap();

        assert!(output_dir.join("style.css").exists());
    }
}
