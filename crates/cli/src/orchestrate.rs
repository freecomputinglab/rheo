use crate::args::{determine_formats, plugins_for_formats};
use rheo_core::compile::RheoCompileOptions;
use rheo_core::config::PluginSection;
use rheo_core::output::OutputConfig;
use rheo_core::project::{ProjectConfig, ProjectMode};
use rheo_core::results::CompilationResults;
use rheo_core::reticulate::{SpineDocument, TracedSpine, generate_bundle_entry};
use rheo_core::world::RheoWorld;
use rheo_core::{FormatPlugin, PluginContext, Result, RheoError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

/// Pre-compiled setup context for compilation commands.
pub(crate) struct CompilationContext {
    pub project: ProjectConfig,
    pub plugins: Vec<Box<dyn FormatPlugin>>,
    pub output_config: OutputConfig,
}

/// Resolve a path relative to a base directory.
fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// Resolve build directory with priority: CLI arg > config > default.
pub(crate) fn resolve_build_dir(
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

/// Bundle compilation: generate bundle entry, inject into world, compile once.
/// Used by all plugins (HTML, PDF, EPUB) with the bundle API.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_with_bundle(
    plugin: &dyn FormatPlugin,
    output: &Path,
    project: &ProjectConfig,
    output_config: &OutputConfig,
    spine: &TracedSpine,
    plugin_section: &PluginSection,
    resolved_inputs: HashMap<&'static str, PathBuf>,
    results: &mut CompilationResults,
    compilation_root: &Path,
) -> Result<()> {
    let plugin_library = plugin.typst_library().map(|s| s.to_string());
    let mut bundle_world = RheoWorld::new(
        compilation_root,
        spine
            .documents
            .first()
            .map(|d| d.path.as_path())
            .unwrap_or(compilation_root),
        plugin_library,
    )?;

    let bundle_entry_source = generate_bundle_entry(
        spine,
        compilation_root,
        plugin.name(),
        plugin.typst_library().unwrap_or_default(),
    );
    bundle_world.inject_bundle_entry(bundle_entry_source);

    let options = RheoCompileOptions::new(output, compilation_root, &mut bundle_world);

    let ctx = PluginContext {
        project,
        output_config,
        options,
        spine: spine.clone(),
        config: plugin_section.clone(),
        inputs: resolved_inputs,
    };

    match plugin.compile(ctx) {
        Ok(_) => {
            results.record_success(plugin.name());
        }
        Err(e) => {
            error!(error = %e, "{} compilation failed", plugin.name());
            results.record_failure(plugin.name());
        }
    }
    Ok(())
}

pub(crate) fn perform_compilation(
    project: &ProjectConfig,
    output_config: &OutputConfig,
    plugins: &[Box<dyn FormatPlugin>],
) -> Result<()> {
    if project.typ_files.is_empty() {
        return Err(RheoError::project_config("no .typ files found in project"));
    }

    let mut results = CompilationResults::new();

    for plugin in plugins {
        let plugin_output_dir = output_config.dir_for_plugin(plugin.name());
        std::fs::create_dir_all(&plugin_output_dir).map_err(|e| {
            RheoError::io(
                e,
                format!("creating output directory for {}", plugin.name()),
            )
        })?;

        // Resolve declared inputs
        let mut resolved_inputs: HashMap<&'static str, PathBuf> = HashMap::new();
        for input in plugin.inputs() {
            let src = project.root.join(&input.path);
            if src.is_file() {
                let dest = plugin_output_dir.join(&input.path);
                std::fs::copy(&src, &dest).map_err(|e| {
                    RheoError::io(
                        e,
                        format!(
                            "copying plugin input '{}' from {} to {}",
                            input.name,
                            src.display(),
                            dest.display()
                        ),
                    )
                })?;
                resolved_inputs.insert(input.name, dest);
            } else if input.required {
                return Err(RheoError::project_config(format!(
                    "plugin '{}' requires input '{}' at '{}' but it was not found",
                    plugin.name(),
                    input.name,
                    &input.path
                )));
            }
        }

        // Execute copy patterns (global + per-plugin)
        let plugin_section_for_assets = project.config.plugin_section(plugin.name());
        for pattern in project
            .config
            .assets
            .iter()
            .chain(plugin_section_for_assets.assets.iter())
        {
            let abs_pattern = project.root.join(pattern).display().to_string();
            let entries = glob::glob(&abs_pattern).map_err(|e| {
                RheoError::project_config(format!("invalid copy pattern '{}': {}", pattern, e))
            })?;
            let mut matched = false;
            for entry in entries.filter_map(|e| e.ok()).filter(|p| p.is_file()) {
                matched = true;
                let rel = entry.strip_prefix(&project.root).unwrap_or(entry.as_path());
                let dest = plugin_output_dir.join(rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        RheoError::io(
                            e,
                            format!("creating directory for copy of {}", rel.display()),
                        )
                    })?;
                }
                std::fs::copy(&entry, &dest).map_err(|e| {
                    RheoError::io(
                        e,
                        format!("copying {} to {}", entry.display(), dest.display()),
                    )
                })?;
                debug!(src = %entry.display(), dest = %dest.display(), "copied file");
            }
            if !matched {
                debug!(pattern = %pattern, "copy pattern matched no files");
            }
        }

        // Compute compilation root once (content_dir from config or project root)
        let compilation_root = project
            .config
            .resolve_content_dir(&project.root)
            .unwrap_or_else(|| project.root.clone());

        // Resolve spine config and trace
        let spine = if project.mode == ProjectMode::SingleFile {
            TracedSpine {
                title: None,
                documents: vec![SpineDocument {
                    path: project.typ_files[0].clone(),
                    is_bundle_entry: false,
                }],
                assets: vec![],
                merge: false,
            }
        } else {
            let mut spine_cfg = project.config.spine_for_plugin(plugin.name());

            let default_spine;
            if spine_cfg.is_none() {
                use rheo_core::DocumentTitle;
                use rheo_core::config::Spine;
                default_spine = Spine {
                    title: Some(DocumentTitle::to_readable_name(&project.name)),
                    vertebrae: vec![],
                    merge: Some(plugin.default_merge()),
                };
                spine_cfg = Some(&default_spine);
            }

            let plugin_section_for_assets = project.config.plugin_section(plugin.name());
            let assets_config: Vec<String> = project
                .config
                .assets
                .iter()
                .chain(plugin_section_for_assets.assets.iter())
                .cloned()
                .collect();

            TracedSpine::trace(
                &project.root,
                &compilation_root,
                spine_cfg,
                &assets_config,
                plugin.default_merge(),
            )?
        };

        let plugin_section = project.config.plugin_section(plugin.name());

        let output = if spine.merge {
            plugin_output_dir
                .join(&project.name)
                .with_extension(plugin.output_extension())
        } else {
            plugin_output_dir.clone()
        };

        compile_with_bundle(
            plugin.as_ref(),
            &output,
            project,
            output_config,
            &spine,
            &plugin_section,
            resolved_inputs,
            &mut results,
            &compilation_root,
        )?;
    }

    let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
    results.log_summary(&names);

    if results.has_failures() {
        if names.iter().any(|name| results.get(name).succeeded > 0) {
            Err(RheoError::project_config(
                "some formats failed to compile".to_string(),
            ))
        } else {
            Err(RheoError::project_config(
                "all formats failed or no files were compiled".to_string(),
            ))
        }
    } else {
        info!("compilation complete");
        Ok(())
    }
}

/// Setup: load project, apply smart defaults (if no config file), resolve plugins + build dir.
pub(crate) fn setup_compilation_context(
    path: &Path,
    config_path: Option<&Path>,
    build_dir: Option<PathBuf>,
    enabled_from_cli: Vec<String>,
    all_plugins: fn() -> Vec<Box<dyn FormatPlugin>>,
) -> Result<CompilationContext> {
    info!(path = %path.display(), "loading project");
    let mut project = ProjectConfig::from_path(path, config_path)?;
    let file_word = if project.typ_files.len() == 1 {
        "file"
    } else {
        "files"
    };
    info!(
        name = %project.name,
        files = project.typ_files.len(),
        "found {} Typst {}",
        project.typ_files.len(),
        file_word
    );

    let all = all_plugins();
    let formats = determine_formats(enabled_from_cli, &project.config.formats, &all);

    // Apply plugin smart defaults for all plugins
    {
        let plugins = plugins_for_formats(&formats, all_plugins());
        for plugin in &plugins {
            let section = project
                .config
                .plugin_sections
                .entry(plugin.name().to_string())
                .or_default();
            plugin.apply_defaults(section, &project.name);
        }
    }

    let plugins = plugins_for_formats(&formats, all_plugins());

    let resolved_build_dir = resolve_build_dir(&project, build_dir)?;
    let output_config = OutputConfig::new(&project.root, resolved_build_dir);

    Ok(CompilationContext {
        project,
        plugins,
        output_config,
    })
}
