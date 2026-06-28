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
use crate::plugins::{FormatPlugin, PluginContext, SpineOptions, SpineOutput, spine_layout_for};
use crate::project::ProjectConfig;
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

    /// Compile the project and return VirtualFs for watch mode (HTML only).
    ///
    /// This is a specialized method for watch mode that returns the raw VirtualFs
    /// from bundle compilation instead of writing files to disk. The VirtualFs
    /// can be served directly by the dev server for faster live reload.
    ///
    /// Only supports HTML format currently.
    pub fn compile_for_watch(&mut self) -> Result<Option<typst_bundle::VirtualFs>> {
        if self.project.typ_files.is_empty() {
            return Err(RheoError::project_config("no .typ files found in project"));
        }

        // Only HTML plugin for now
        let html_plugin = self
            .plugins
            .iter()
            .find(|p| p.name() == "html")
            .ok_or_else(|| RheoError::project_config("HTML plugin not enabled"))?;

        let plugin_output_dir = self.output.dir_for_plugin(html_plugin.name());
        std::fs::create_dir_all(&plugin_output_dir).map_err(|e| {
            RheoError::io(
                e,
                format!("creating output directory for {}", html_plugin.name()),
            )
        })?;

        let default_section = PluginSection::default();

        // Scan .typ files for package imports once — shared across all plugins.
        let _package_imports = crate::plugins::scan_project_package_imports(&self.project.typ_files);

        let content_dir = self
            .project
            .config
            .resolve_content_dir(&self.project.root)
            .unwrap_or_else(|| self.project.root.clone());

        let plugin_section: &PluginSection = self
            .project
            .config
            .plugin_sections
            .get(html_plugin.name())
            .unwrap_or(&default_section);

        // Resolve spine options.
        let spine_cfg = plugin_section.spine.as_ref();
        let spine = SpineOptions {
            title: spine_cfg.and_then(|s| s.title.clone()),
            vertebrae: spine_cfg.map(|s| s.vertebrae.clone()).unwrap_or_default(),
            merge: spine_cfg.and_then(|s| s.merge).unwrap_or(false),
        };

        let _ctx = PluginContext {
            project: &self.project,
            output_config: &self.output,
            output_dir: &plugin_output_dir,
            spine: &spine,
            config: plugin_section,
            assets: &Default::default(), // Unused in watch mode
            font_dirs: &self.font_dirs,
        };

        // Build VirtualSpine from plugin's declared layout + project context.
        let layout = spine_layout_for(
            html_plugin.spine_layout_kind(),
            html_plugin.as_ref(),
            &self.project.name,
        );
        let spine_files = spine.generate(&content_dir)?;

        debug!(
            plugin = html_plugin.name(),
            files = spine_files.len(),
            "building virtual spine for watch mode"
        );

        let virtual_spine =
            VirtualSpine::build(&spine_files, &content_dir, &self.project.root, layout)?;
        virtual_spine.check_output_collisions()?;

        let spine_source = virtual_spine.source();
        debug!(plugin = html_plugin.name(), "created virtual spine source");

        // Single Typst bundle compile for this plugin.
        let world =
            RheoWorld::new_for_bundle(&self.project.root, spine_source, self.font_dirs.clone())?;
        let bundle = world.compile_bundle()?;
        let virtual_fs = export_bundle(&bundle)?;

        Ok(Some(virtual_fs))
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

        let content_dir = self
            .project
            .config
            .resolve_content_dir(&self.project.root)
            .unwrap_or_else(|| self.project.root.clone());

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

            // Execute copy patterns.
            resolver.copy_globs(&self.project.config.copy, &self.project.root, None)?;
            for block in &manifest_blocks {
                resolver.copy_globs(
                    &block.assets.copy,
                    &block.source_root,
                    block.assets.dest.as_deref(),
                )?;
            }
            for block in plugin_section.asset_blocks() {
                resolver.copy_globs(&block.copy, &self.project.root, block.dest.as_deref())?;
            }

            // Resolve spine options.
            let spine_cfg = plugin_section.spine.as_ref();
            let spine = SpineOptions {
                title: spine_cfg.and_then(|s| s.title.clone()),
                vertebrae: spine_cfg.map(|s| s.vertebrae.clone()).unwrap_or_default(),
                merge: spine_cfg.and_then(|s| s.merge).unwrap_or(false),
            };

            let ctx = PluginContext {
                project: &self.project,
                output_config: &self.output,
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
            let spine_files = spine.generate(&content_dir)?;

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
                self.font_dirs.clone(),
            )?;
            let bundle = world.compile_bundle()?;
            let virtual_fs = export_bundle(&bundle)?;

            // Flatten VirtualFs entries into plugin-facing SpineOutput list.
            // VirtualPath::get_with_slash() gives the path string (e.g. "/intro.html").
            // Strip the leading "/" to produce a relative filename.
            // Match each output back to its Vertebra to include harvested rheo-vars.
            let outputs: Vec<SpineOutput> = virtual_fs
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
                    SpineOutput {
                        output_path,
                        bytes,
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
        fn compile(&self, _ctx: PluginContext<'_>, _outputs: &[SpineOutput]) -> crate::Result<()> {
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
