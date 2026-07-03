//! The build orchestrator.
//!
//! [`Build`] owns the full compile pipeline — format selection, smart defaults,
//! asset resolution, spine handling, and per-format compilation — independent of
//! the CLI. The `rheo` binary is a thin wrapper that maps command-line arguments
//! to [`BuildOptions`] and calls [`Build::prepare`] then [`Build::run`].

use crate::assets::AssetResolver;
use crate::compile::export_bundle;
use crate::config::PluginSection;
use crate::output::OutputConfig;
use crate::plugins::{CastVertebra, FormatPlugin, PluginContext, SpineOptions, spine_layout_for};
use crate::project::{ProjectConfig, ProjectMode};
use crate::results::CompilationResults;
use crate::reticulate::spine::VirtualSpine;
use crate::world::RheoWorld;
use crate::{Result, RheoError};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

/// Inputs for preparing a [`Build`], typically mapped from CLI flags and config.
pub struct BuildOptions {
    /// Format names explicitly requested (e.g. from `--html`). Empty means
    /// "fall back to the config `formats` list, then to all plugins".
    pub formats: Vec<String>,
    /// Build output directory override (CLI `--build-dir`); resolved against the
    /// current working directory. `None` falls back to config, then default.
    pub build_dir: Option<PathBuf>,
    /// Additional font directories from `--font-dir`, appended on top of the
    /// autoscan/config-derived directories.
    pub font_dirs: Vec<PathBuf>,
}

/// A prepared, runnable build: a project, the selected plugins, the resolved
/// output location, and the resolved font directories.
///
/// Construct with [`Build::prepare`] (format selection + smart defaults) and
/// execute with [`Build::run`]. This is the library entry point — no CLI is
/// required to compile a project.
pub struct Build {
    project: ProjectConfig,
    plugins: Vec<Box<dyn FormatPlugin>>,
    output: OutputConfig,
    font_dirs: Vec<PathBuf>,
}

impl Build {
    /// Select formats, apply each plugin's smart defaults, and resolve the
    /// output and font directories, producing a ready-to-run [`Build`].
    ///
    /// `all_plugins` is the full set of known plugins; the build keeps only those
    /// whose names are selected by `opts.formats` (or the project config / all).
    pub fn prepare(
        mut project: ProjectConfig,
        all_plugins: Vec<Box<dyn FormatPlugin>>,
        opts: BuildOptions,
    ) -> Result<Self> {
        let formats = determine_formats(opts.formats, &project.config.formats, &all_plugins);
        let plugins = plugins_for_formats(&formats, all_plugins);

        // Apply plugin smart defaults — each plugin fills in only missing values.
        for plugin in &plugins {
            let section = project
                .config
                .plugin_sections
                .entry(plugin.name().to_string())
                .or_default();
            plugin.apply_defaults(section, &project.name);
        }

        let resolved_build_dir = resolve_build_dir(&project, opts.build_dir)?;
        let output = OutputConfig::new(&project.root, resolved_build_dir);
        let font_dirs = resolve_font_dirs(&project, &opts.font_dirs);

        Ok(Self {
            project,
            plugins,
            output,
            font_dirs,
        })
    }

    /// The project being built.
    pub fn project(&self) -> &ProjectConfig {
        &self.project
    }

    /// The format plugins selected for this build.
    pub fn plugins(&self) -> &[Box<dyn FormatPlugin>] {
        &self.plugins
    }

    /// The resolved output configuration (build directory layout).
    pub fn output_config(&self) -> &OutputConfig {
        &self.output
    }

    /// Derive the set of asset sources the watcher should treat as relevant.
    ///
    /// Mirrors the asset-resolution pass in [`run`](Self::run) across every
    /// enabled plugin — collecting each resolved asset's source path, the
    /// project/plugin/package `copy` glob patterns, and the source roots of any
    /// packages that declare assets — so watch coverage is driven by what the
    /// plugins actually declare rather than a hard-coded extension list.
    ///
    /// Only packages that declare rheo assets in their `typst.toml` contribute a
    /// source root here, so the watched set stays tight (immutable `@preview`
    /// deps that declare nothing are never watched).
    pub fn watch_asset_spec(&self) -> crate::watch::WatchAssetSpec {
        let default_section = PluginSection::default();
        let package_imports = crate::plugins::scan_project_package_imports(&self.project.typ_files);

        let mut asset_paths: Vec<PathBuf> = Vec::new();
        let mut copy_globs: Vec<(PathBuf, Vec<String>)> = Vec::new();
        let mut package_roots: Vec<PathBuf> = Vec::new();

        for plugin in &self.plugins {
            let plugin_output_dir = self.output.dir_for_plugin(plugin.name());
            let plugin_section: &PluginSection = self
                .project
                .config
                .plugin_sections
                .get(plugin.name())
                .unwrap_or(&default_section);

            let manifest_blocks = if plugin_section.auto_detect_packages_enabled() {
                crate::plugins::detect_manifest_package_assets(&package_imports, plugin.name())
            } else {
                vec![]
            };

            let resolver = AssetResolver::new(&self.project.root, &plugin_output_dir);
            if let Ok(resolved) =
                resolver.resolve(plugin.as_ref(), plugin_section, &manifest_blocks)
            {
                for assets in resolved.values() {
                    for asset in assets {
                        asset_paths.push(asset.source_path.clone());
                    }
                }
            }

            // Copy globs: project-level, per-package, and per-plugin asset blocks.
            if !self.project.config.copy.is_empty() {
                copy_globs.push((self.project.root.clone(), self.project.config.copy.clone()));
            }
            for block in &manifest_blocks {
                if !block.assets.copy.is_empty() {
                    copy_globs.push((block.source_root.clone(), block.assets.copy.clone()));
                }
                package_roots.push(block.source_root.clone());
            }
            for block in plugin_section.asset_blocks() {
                if !block.copy.is_empty() {
                    copy_globs.push((self.project.root.clone(), block.copy.clone()));
                }
            }
        }

        crate::watch::WatchAssetSpec::new(asset_paths, copy_globs, package_roots)
    }

    /// Compile HTML to an in-memory VirtualFs for the dev server.
    ///
    /// Resolves assets (CSS/JS), copies them to the plugin output dir so the
    /// dev server can fall back to disk for them, compiles the Typst bundle, and
    /// injects CSS/JS `<link>`/`<script>` tags into each HTML entry before
    /// returning the VirtualFs. Typst's comemo layer caches unchanged sources, so
    /// this second compile after `run()` is near-instant on unchanged content.
    ///
    /// Returns `None` when the HTML plugin is not selected.
    pub fn compile_for_watch(&mut self) -> Result<Option<typst_bundle::VirtualFs>> {
        if self.project.typ_files.is_empty() {
            return Err(RheoError::project_config("no .typ files found in project"));
        }

        let html_plugin = match self.plugins.iter().find(|p| p.name() == "html") {
            Some(p) => p,
            None => return Ok(None),
        };

        let plugin_output_dir = self.output.dir_for_plugin(html_plugin.name());
        std::fs::create_dir_all(&plugin_output_dir).map_err(|e| {
            RheoError::io(
                e,
                format!("creating output directory for {}", html_plugin.name()),
            )
        })?;

        let default_section = PluginSection::default();

        let content_dir = resolve_effective_content_dir(&self.project);

        let plugin_section: &PluginSection = self
            .project
            .config
            .plugin_sections
            .get(html_plugin.name())
            .unwrap_or(&default_section);

        // Resolve assets — copies CSS/JS to disk so the dev server can serve them
        // as fallback for requests not satisfied by the VirtualFs.
        //
        // Mirror `run()`: detect package-provided assets (e.g. a sidebar
        // package's stylesheet and script declared in its `typst.toml`) so the
        // in-memory entry page injects the same head links as the on-disk build.
        // Without this the VirtualFs index lacks package CSS/JS and renders
        // unstyled.
        let package_imports = crate::plugins::scan_project_package_imports(&self.project.typ_files);
        let manifest_blocks = if plugin_section.auto_detect_packages_enabled() {
            crate::plugins::prewarm_packages(&package_imports);
            crate::plugins::detect_manifest_package_assets(&package_imports, html_plugin.name())
        } else {
            vec![]
        };

        let resolver = AssetResolver::new(&self.project.root, &plugin_output_dir);
        let resolved_assets = resolver
            .resolve(html_plugin.as_ref(), plugin_section, &manifest_blocks)
            .unwrap_or_default();

        let css_paths: Vec<String> = resolved_assets
            .get("css_stylesheet")
            .map(|v| v.iter().map(|a| a.built_relative_path.clone()).collect())
            .unwrap_or_default();
        let js_paths: Vec<String> = resolved_assets
            .get("js_scripts")
            .map(|v| v.iter().map(|a| a.built_relative_path.clone()).collect())
            .unwrap_or_default();

        // Resolve spine options.
        let spine_cfg = plugin_section.spine.as_ref();
        let spine = SpineOptions {
            title: spine_cfg.and_then(|s| s.title.clone()),
            vertebrae: spine_cfg.map(|s| s.vertebrae.clone()).unwrap_or_default(),
        };

        // Build VirtualSpine + compile.
        let layout = spine_layout_for(
            html_plugin.spine_layout_kind(),
            html_plugin.as_ref(),
            &self.project.name,
        );
        let spine_files = match self.project.mode {
            ProjectMode::SingleFile => vec![self.project.typ_files[0].clone()],
            ProjectMode::Directory => {
                let explicit_content_dir =
                    self.project.config.resolve_content_dir(&self.project.root);
                let generate_root = explicit_content_dir
                    .as_deref()
                    .unwrap_or(&self.project.root);
                spine.generate(generate_root)?
            }
        };

        debug!(
            plugin = html_plugin.name(),
            files = spine_files.len(),
            "building virtual spine for watch mode"
        );

        let virtual_spine =
            VirtualSpine::build(&spine_files, &content_dir, &self.project.root, layout)?;
        virtual_spine.check_output_collisions()?;

        let spine_source = virtual_spine.source();

        let world = RheoWorld::new_for_bundle(
            &self.project.root,
            spine_source,
            html_plugin.rheo_target(),
            self.font_dirs.clone(),
        )?;
        let bundle = world.compile_bundle()?;
        let virtual_fs = export_bundle(&bundle)?;

        // Inject CSS/JS link tags into each HTML entry in memory.
        let needs_injection = !css_paths.is_empty() || !js_paths.is_empty();
        if needs_injection {
            let css_refs: Vec<&str> = css_paths.iter().map(|s| s.as_str()).collect();
            let js_refs: Vec<&str> = js_paths.iter().map(|s| s.as_str()).collect();
            let injected: Result<typst_bundle::VirtualFs> = virtual_fs
                .into_iter()
                .map(|(vpath, bytes)| {
                    let path_str = vpath.get_with_slash().trim_start_matches('/').to_string();
                    if path_str.ends_with(".html") {
                        let html = String::from_utf8_lossy(&bytes);
                        let mut dom = crate::html_utils::HtmlDom::parse(&html)?;
                        dom.inject_head_links(&[], &css_refs, &js_refs)?;
                        let modified = dom.serialize()?;
                        Ok((vpath, typst::foundations::Bytes::new(modified.into_bytes())))
                    } else {
                        Ok((vpath, bytes))
                    }
                })
                .collect();
            Ok(Some(injected?))
        } else {
            Ok(Some(virtual_fs))
        }
    }

    /// Compile the project across all selected plugins.
    ///
    /// Returns the per-format [`CompilationResults`] on full success. If any
    /// format fails, the failure is logged and an error is returned (the CLI maps
    /// this to a non-zero exit).
    pub fn run(&mut self) -> Result<CompilationResults> {
        if self.project.typ_files.is_empty() {
            return Err(RheoError::project_config("no .typ files found in project"));
        }

        let mut results = CompilationResults::new();
        let default_section = PluginSection::default();

        // Scan .typ files for package imports once — shared across all plugins.
        let package_imports = crate::plugins::scan_project_package_imports(&self.project.typ_files);

        let content_dir = resolve_effective_content_dir(&self.project);

        for plugin in &self.plugins {
            let plugin_output_dir = self.output.dir_for_plugin(plugin.name());
            std::fs::create_dir_all(&plugin_output_dir).map_err(|e| {
                RheoError::io(
                    e,
                    format!("creating output directory for {}", plugin.name()),
                )
            })?;

            let plugin_section: &PluginSection = self
                .project
                .config
                .plugin_sections
                .get(plugin.name())
                .unwrap_or(&default_section);

            // Pre-warm and auto-detect manifest package assets.
            let manifest_blocks = if plugin_section.auto_detect_packages_enabled() {
                crate::plugins::prewarm_packages(&package_imports);
                crate::plugins::detect_manifest_package_assets(&package_imports, plugin.name())
            } else {
                vec![]
            };

            let resolver = AssetResolver::new(&self.project.root, &plugin_output_dir);
            let resolved_assets =
                resolver.resolve(plugin.as_ref(), plugin_section, &manifest_blocks)?;

            // Resolve spine options.
            let spine_cfg = plugin_section.spine.as_ref();
            let spine = SpineOptions {
                title: spine_cfg.and_then(|s| s.title.clone()),
                vertebrae: spine_cfg.map(|s| s.vertebrae.clone()).unwrap_or_default(),
            };

            let ctx = PluginContext {
                project: &self.project,
                output_dir: &plugin_output_dir,
                spine: &spine,
                config: plugin_section,
                assets: &resolved_assets,
                font_dirs: &self.font_dirs,
            };

            // Build VirtualSpine from plugin's declared layout + project context.
            let layout = spine_layout_for(
                plugin.spine_layout_kind(),
                plugin.as_ref(),
                &self.project.name,
            );
            let spine_files = match self.project.mode {
                ProjectMode::SingleFile => vec![self.project.typ_files[0].clone()],
                ProjectMode::Directory => {
                    // When content_dir is explicit in config, vertebrae patterns are
                    // relative to it. When auto-detected or absent, vertebrae are
                    // project-root-relative (users write "content/**" explicitly).
                    let explicit_content_dir =
                        self.project.config.resolve_content_dir(&self.project.root);
                    let generate_root = explicit_content_dir
                        .as_deref()
                        .unwrap_or(&self.project.root);
                    spine.generate(generate_root)?
                }
            };

            debug!(
                plugin = plugin.name(),
                files = spine_files.len(),
                "building virtual spine"
            );

            let virtual_spine =
                VirtualSpine::build(&spine_files, &content_dir, &self.project.root, layout)?;
            virtual_spine.check_output_collisions()?;

            let spine_source = virtual_spine.source();
            debug!(plugin = plugin.name(), "created virtual spine source");

            // Single Typst bundle compile for this plugin.
            let world = RheoWorld::new_for_bundle(
                &self.project.root,
                spine_source,
                plugin.rheo_target(),
                self.font_dirs.clone(),
            )?;
            let bundle = world.compile_bundle()?;
            let virtual_fs = export_bundle(&bundle)?;

            // Flatten VirtualFs entries into plugin-facing CastVertebra list.
            // VirtualPath::get_with_slash() gives the path string (e.g. "/intro.html").
            // Strip the leading "/" to produce a relative filename.
            // Match each output back to its Vertebra to include harvested rheo-vars.
            let outputs: Vec<CastVertebra> = virtual_fs
                .into_iter()
                .map(|(vpath, bytes)| {
                    let output_path = vpath.get_with_slash().trim_start_matches('/').to_string();
                    // Find the corresponding Vertebra to get its vars.
                    let vars = virtual_spine
                        .vertebrae
                        .iter()
                        .find(|v| v.output_path == output_path)
                        .map(|v| v.vars.clone())
                        .unwrap_or_default();
                    CastVertebra {
                        output_path,
                        bytes,
                        format: plugin.typst_format(),
                        vars,
                    }
                })
                .collect();

            debug!(
                plugin = plugin.name(),
                outputs = outputs.len(),
                "spine compile produced outputs"
            );

            match plugin.compile(ctx, &outputs) {
                Ok(_) => {
                    // Apply copy globs after bundle output is written so that
                    // explicit copy patterns win over any colliding bundle output.
                    resolver.copy_globs(
                        &self.project.config.copy,
                        &self.project.root,
                        None,
                        true,
                    )?;
                    for block in &manifest_blocks {
                        resolver.copy_globs(
                            &block.assets.copy,
                            &block.source_root,
                            block.assets.dest.as_deref(),
                            true,
                        )?;
                    }
                    for block in plugin_section.asset_blocks() {
                        resolver.copy_globs(
                            &block.copy,
                            &self.project.root,
                            block.dest.as_deref(),
                            true,
                        )?;
                    }
                    results.record_success(plugin.name());
                    info!(plugin = plugin.name(), "compilation succeeded");
                }
                Err(e) => {
                    error!(error = %e, "{} generation failed", plugin.name());
                    results.record_failure(plugin.name());
                }
            }
        }

        let names: Vec<&str> = self.plugins.iter().map(|p| p.name()).collect();
        results.log_summary(&names);

        if results.has_failures() {
            if names.iter().any(|name| results.get(name).succeeded > 0) {
                return Err(RheoError::project_config(
                    "some formats failed to compile".to_string(),
                ));
            }
            return Err(RheoError::project_config(
                "all formats failed or no files were compiled".to_string(),
            ));
        }

        info!("compilation complete");
        Ok(results)
    }
}

/// Determine which format names to compile.
///
/// Priority: explicit `enabled` list → config `formats` → all plugins.
fn determine_formats(
    enabled: Vec<String>,
    config_defaults: &[String],
    all: &[Box<dyn FormatPlugin>],
) -> Vec<String> {
    if !enabled.is_empty() {
        return enabled;
    }
    if !config_defaults.is_empty() {
        return config_defaults.to_vec();
    }
    all.iter().map(|p| p.name().to_string()).collect()
}

/// Keep only the plugins whose names appear in `formats`.
fn plugins_for_formats(
    formats: &[String],
    all: Vec<Box<dyn FormatPlugin>>,
) -> Vec<Box<dyn FormatPlugin>> {
    all.into_iter()
        .filter(|p| formats.iter().any(|f| f == p.name()))
        .collect()
}

/// Resolve a path relative to a base directory (absolute paths pass through).
fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// Resolve the build directory with priority: CLI arg > config > default (`None`).
pub fn resolve_build_dir(
    project: &ProjectConfig,
    cli_build_dir: Option<PathBuf>,
) -> Result<Option<PathBuf>> {
    if let Some(cli_path) = cli_build_dir {
        let cwd =
            std::env::current_dir().map_err(|e| RheoError::io(e, "getting current directory"))?;
        debug!(dir = %cli_path.display(), "build directory");
        Ok(Some(resolve_path(&cwd, &cli_path)))
    } else if let Some(config_path) = &project.config.build_dir {
        let resolved = resolve_path(&project.root, Path::new(config_path));
        debug!(dir = %resolved.display(), "build directory");
        Ok(Some(resolved))
    } else {
        Ok(None)
    }
}

/// Resolve the effective content directory for a project.
///
/// If `content_dir` is set in config, use it. Otherwise, if `project.root/content/`
/// exists and all `.typ` files in the project are under it, treat it as the implicit
/// content root so that file stems are relative to `content/` (not the project root).
pub fn resolve_effective_content_dir(project: &ProjectConfig) -> PathBuf {
    if let Some(dir) = project.config.resolve_content_dir(&project.root) {
        return dir;
    }
    let candidate = project.root.join("content");
    if candidate.is_dir()
        && !project.typ_files.is_empty()
        && project.typ_files.iter().all(|f| f.starts_with(&candidate))
    {
        debug!(content_dir = %candidate.display(), "auto-detected content directory");
        return candidate;
    }
    project.root.clone()
}

/// Resolve font directories with autoscan, config, and CLI precedence.
fn resolve_font_dirs(project: &ProjectConfig, cli_font_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if project.config.font_dirs.is_empty() {
        let autoscan_dir = project.root.join("fonts");
        if autoscan_dir.is_dir() {
            debug!(dir = %autoscan_dir.display(), "auto-discovered font directory");
            dirs.push(autoscan_dir);
        }
    } else {
        dirs.extend(project.config.resolve_font_dirs(&project.root));
    }

    let cwd = std::env::current_dir().unwrap();
    for dir in cli_font_dirs {
        dirs.push(resolve_path(&cwd, dir));
    }

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_names(all: &[Box<dyn FormatPlugin>]) -> Vec<String> {
        all.iter().map(|p| p.name().to_string()).collect()
    }

    struct FakePlugin(&'static str);
    impl FormatPlugin for FakePlugin {
        fn name(&self) -> &'static str {
            self.0
        }
        fn compile(&self, _ctx: PluginContext<'_>, _outputs: &[CastVertebra]) -> crate::Result<()> {
            Ok(())
        }
    }

    fn fake_all() -> Vec<Box<dyn FormatPlugin>> {
        vec![
            Box::new(FakePlugin("html")),
            Box::new(FakePlugin("pdf")),
            Box::new(FakePlugin("epub")),
        ]
    }

    #[test]
    fn test_determine_formats_cli_flags_override_config() {
        let all = fake_all();
        let formats = determine_formats(vec!["pdf".into()], &["html".into()], &all);
        assert_eq!(formats, vec!["pdf".to_string()]);
    }

    #[test]
    fn test_determine_formats_uses_config_defaults_when_no_flags() {
        let all = fake_all();
        let formats = determine_formats(vec![], &["html".into()], &all);
        assert_eq!(formats, vec!["html".to_string()]);
    }

    #[test]
    fn test_determine_formats_falls_back_to_all_when_empty() {
        let all = fake_all();
        let formats = determine_formats(vec![], &[], &all);
        assert_eq!(formats, plugin_names(&all));
    }

    #[test]
    fn test_plugins_for_formats_filters() {
        let all = fake_all();
        let selected = plugins_for_formats(&["pdf".into(), "epub".into()], all);
        let names: Vec<&str> = selected.iter().map(|p| p.name()).collect();
        assert_eq!(names, vec!["pdf", "epub"]);
    }
}
