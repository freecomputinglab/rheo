//! Asset resolution and copying for format plugins.
//!
//! Provides [`AssetResolver`] which resolves plugin assets (stylesheets, scripts,
//! etc.) from project sources and package dependencies, copies them into the
//! build output directory, and detects path collisions.

pub mod watch;

use crate::config::PluginSection;
use crate::plugins::{Asset, FormatPlugin, PackageAssets};
use crate::{Result, RheoError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

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
            // Gather pairs from user-declared asset blocks and package blocks.
            struct AssetEntry<'b> {
                dest: Option<&'b str>,
                root: &'b Path,
                path: &'b str,
                is_pkg: bool,
            }

            let mut all_pairs: Vec<AssetEntry<'_>> = Vec::new();

            // User-declared pairs resolve against project_root.
            let user_pairs = section.get_strings_with_block(asset_config.name);
            for (block, path) in &user_pairs {
                all_pairs.push(AssetEntry {
                    dest: block.dest.as_deref(),
                    root: self.project_root,
                    path,
                    is_pkg: false,
                });
            }

            // Package-derived pairs resolve against their own source_root. These
            // are additive to the project's own configuration: a package asset
            // lives in a different scope and must never stand in for it.
            for pkg in package_blocks {
                if let Some(val) = pkg.assets.extra.get(asset_config.name)
                    && let Some(s) = val.as_str()
                {
                    all_pairs.push(AssetEntry {
                        dest: pkg.assets.dest.as_deref(),
                        root: &pkg.source_root,
                        path: s,
                        is_pkg: true,
                    });
                }
            }

            // The project-root convention fires whenever the project itself has
            // no override, independent of what packages contribute — package
            // pairs must not satisfy the project's own emptiness test.
            if user_pairs.is_empty() {
                all_pairs.push(AssetEntry {
                    dest: None,
                    root: self.project_root,
                    path: asset_config.default_path,
                    is_pkg: false,
                });
            }

            // Group sources by (dest, resolution_root), preserving insertion order.
            struct AssetGroup<'b> {
                dest: Option<&'b str>,
                root: &'b Path,
                entries: Vec<(&'b str, bool)>,
            }
            let mut groups: Vec<AssetGroup<'_>> = Vec::new();
            for entry in &all_pairs {
                if let Some(group) = groups
                    .iter_mut()
                    .find(|g| g.dest == entry.dest && g.root.as_os_str() == entry.root.as_os_str())
                {
                    group.entries.push((entry.path, entry.is_pkg));
                } else {
                    groups.push(AssetGroup {
                        dest: entry.dest,
                        root: entry.root,
                        entries: vec![(entry.path, entry.is_pkg)],
                    });
                }
            }

            let mut all_assets: Vec<Asset> = Vec::new();
            let mut any_source_found = false;

            for group in &groups {
                let out_dir = match group.dest {
                    Some(d) => self.plugin_output_dir.join(d),
                    None => self.plugin_output_dir.to_path_buf(),
                };

                let mut sources: Vec<PathBuf> = Vec::new();
                let mut missing: Vec<(&str, bool)> = Vec::new();
                for (path, is_pkg) in &group.entries {
                    let abs = group.root.join(path);
                    if abs.is_file() {
                        sources.push(abs);
                    } else {
                        missing.push((*path, *is_pkg));
                    }
                }

                if sources.is_empty() {
                    continue;
                }
                any_source_found = true;

                for (m, is_pkg) in &missing {
                    if !is_pkg {
                        warn!(
                            plugin = plugin.name(),
                            asset = asset_config.name,
                            path = %m,
                            "asset override path not found, skipping"
                        );
                    }
                }

                let outputs: Vec<PathBuf> =
                    copy_each(&sources, group.root, &out_dir, group.dest.is_some())?;

                let assets: Vec<Asset> = outputs
                    .into_iter()
                    .zip(sources.iter())
                    .map(|(abs, src)| {
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
                            source_path: src.clone(),
                            resolved_path: abs,
                            built_relative_path: rel,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                all_assets.extend(assets);
            }

            if !any_source_found {
                if asset_config.required {
                    let paths: Vec<&str> = all_pairs.iter().map(|e| e.path).collect();
                    return Err(RheoError::project_config(format!(
                        "plugin '{}' requires input '{}' but no source was found (tried: {})",
                        plugin.name(),
                        asset_config.name,
                        paths.join(", ")
                    )));
                }
                // No on-disk source: fall back to the plugin's embedded default
                // (if any), written as a real file so it is copied + linked like
                // any other asset instead of being inlined.
                if let Some(embedded) = asset_config.default_content {
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
                    resolved.insert(
                        asset_config.name,
                        vec![Asset {
                            config: asset_config.clone(),
                            source_path: dest.clone(),
                            resolved_path: dest.clone(),
                            built_relative_path: rel,
                        }],
                    );
                }
                continue;
            }

            resolved.insert(asset_config.name, all_assets);
        }
        Ok(resolved)
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

/// Expand glob patterns against `source_root` and copy matching files into
/// `plugin_output_dir` (optionally under `dest_prefix`).
///
/// When `warn_on_overwrite` is true, logs a warning for each destination file
/// that already exists before it is overwritten.
fn copy_glob_patterns(
    patterns: &[String],
    source_root: &Path,
    plugin_output_dir: &Path,
    dest_prefix: Option<&str>,
    warn_on_overwrite: bool,
) -> Result<()> {
    for pattern in patterns {
        let abs_pattern = source_root.join(pattern).display().to_string();
        let entries = glob::glob(&abs_pattern).map_err(|e| {
            RheoError::project_config(format!("invalid copy pattern '{}': {}", pattern, e))
        })?;
        let mut matched = false;
        for entry in entries.filter_map(|e| e.ok()).filter(|p| p.is_file()) {
            matched = true;
            let rel = entry.strip_prefix(source_root).unwrap_or(entry.as_path());
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
                    src = %entry.display(),
                    dest = %dest.display(),
                    "copy glob overwrites existing bundle output"
                );
            }
            std::fs::copy(&entry, &dest).map_err(|e| RheoError::AssetCopy {
                source: entry.clone(),
                dest: dest.clone(),
                error: e,
            })?;
            debug!(src = %entry.display(), dest = %dest.display(), "copied file");
        }
        if !matched {
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
