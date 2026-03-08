use clap::{Arg, ArgAction, ArgMatches, Command};
use rheo_core::compile::RheoCompileOptions;
use rheo_core::config::PluginSection;
use rheo_core::manifest_version;
use rheo_core::output::OutputConfig;
use rheo_core::project::ProjectConfig;
use rheo_core::results::CompilationResults;
use rheo_core::world::RheoWorld;
use rheo_core::{FormatPlugin, PluginContext, Result, RheoError, SpineOptions};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

// Re-export logging functionality
pub use rheo_core::logging;

/// Initialize logging with specified verbosity
pub fn init_logging(verbose: bool, quiet: bool) -> Result<()> {
    let verbosity = if quiet {
        logging::Verbosity::Quiet
    } else if verbose {
        logging::Verbosity::Verbose
    } else {
        logging::Verbosity::Normal
    };
    logging::init(verbosity)
}

/// Returns all known format plugins. Adding a new plugin here is the only
/// change needed in `cli` to support a new output format.
fn all_plugins() -> Vec<Box<dyn FormatPlugin>> {
    vec![
        Box::new(rheo_html::HtmlPlugin),
        Box::new(rheo_pdf::PdfPlugin),
        Box::new(rheo_epub::EpubPlugin),
    ]
}

/// Build the top-level clap `Command`, adding per-plugin `--<name>` flags
/// dynamically to `compile` and `watch` subcommands.
fn build_cli() -> Command {
    let plugins = all_plugins();
    Command::new("rheo")
        .about("A tool for flowing Typst documents into publishable outputs")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue)
                .conflicts_with("verbose")
                .global(true)
                .help("Decrease output verbosity (errors only)"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .conflicts_with("quiet")
                .global(true)
                .help("Increase output verbosity (show debug information)"),
        )
        .subcommand(build_compile_command(&plugins))
        .subcommand(build_watch_command(&plugins))
        .subcommand(build_clean_command())
        .subcommand(build_init_command())
        .subcommand_required(true)
        .arg_required_else_help(true)
}

fn add_format_flags(mut cmd: Command, plugins: &[Box<dyn FormatPlugin>]) -> Command {
    for plugin in plugins {
        cmd = cmd.arg(
            Arg::new(plugin.name())
                .long(plugin.name())
                .action(ArgAction::SetTrue)
                .help(format!("Compile to {} only", plugin.name())),
        );
    }
    cmd
}

fn build_compile_command(plugins: &[Box<dyn FormatPlugin>]) -> Command {
    let cmd = Command::new("compile")
        .about("Compile Typst documents to PDF, HTML, and/or EPUB")
        .arg(
            Arg::new("path")
                .required(true)
                .index(1)
                .help("Path to project directory or single .typ file"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .value_name("PATH")
                .help("Path to custom rheo.toml config file"),
        )
        .arg(
            Arg::new("build-dir")
                .long("build-dir")
                .help("Build output directory (overrides rheo.toml if set)"),
        );
    add_format_flags(cmd, plugins)
}

fn build_watch_command(plugins: &[Box<dyn FormatPlugin>]) -> Command {
    let cmd = Command::new("watch")
        .about("Watch Typst documents and recompile on changes")
        .arg(
            Arg::new("path")
                .required(true)
                .index(1)
                .help("Path to project directory or single .typ file"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .value_name("PATH")
                .help("Path to custom rheo.toml config file"),
        )
        .arg(
            Arg::new("build-dir")
                .long("build-dir")
                .help("Build output directory (overrides rheo.toml if set)"),
        )
        .arg(
            Arg::new("open")
                .long("open")
                .action(ArgAction::SetTrue)
                .help("Open output in appropriate viewer (HTML opens in browser with live reload)"),
        );
    add_format_flags(cmd, plugins)
}

fn build_clean_command() -> Command {
    Command::new("clean")
        .about("Clean build artifacts for a project")
        .arg(
            Arg::new("path")
                .index(1)
                .default_value(".")
                .help("Path to project directory or single .typ file"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .value_name("PATH")
                .help("Path to custom rheo.toml config file"),
        )
        .arg(
            Arg::new("build-dir")
                .long("build-dir")
                .help("Build output directory to clean (overrides rheo.toml if set)"),
        )
}

fn build_init_command() -> Command {
    Command::new("init")
        .about("Initialize a new Rheo project")
        .arg(
            Arg::new("path")
                .required(true)
                .index(1)
                .help("Path to the new project directory"),
        )
}

/// Extract enabled format names from arg matches (names of plugins whose flags are set).
fn enabled_formats_from_matches(
    matches: &ArgMatches,
    plugins: &[Box<dyn FormatPlugin>],
) -> Vec<String> {
    plugins
        .iter()
        .filter(|p| matches.get_flag(p.name()))
        .map(|p| p.name().to_string())
        .collect()
}

/// Determine which format names to compile based on CLI flags and config defaults.
///
/// Priority:
/// 1. CLI flags (any set → use only those)
/// 2. Config `formats` list (non-empty → use that)
/// 3. All plugins (fallback)
fn determine_formats(
    enabled_from_cli: Vec<String>,
    config_defaults: &[String],
    all: &[Box<dyn FormatPlugin>],
) -> Vec<String> {
    if !enabled_from_cli.is_empty() {
        return enabled_from_cli;
    }
    if !config_defaults.is_empty() {
        return config_defaults.to_vec();
    }
    all.iter().map(|p| p.name().to_string()).collect()
}

/// Filter `all_plugins()` to only those whose names appear in `formats`.
fn plugins_for_formats(
    formats: &[String],
    all: Vec<Box<dyn FormatPlugin>>,
) -> Vec<Box<dyn FormatPlugin>> {
    all.into_iter()
        .filter(|p| formats.iter().any(|f| f == p.name()))
        .collect()
}

/// Pre-compiled setup context for compilation commands.
struct CompilationContext {
    project: ProjectConfig,
    plugins: Vec<Box<dyn FormatPlugin>>,
    output_config: OutputConfig,
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
fn resolve_build_dir(
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

fn get_output_filename(typ_file: &std::path::Path) -> Result<String> {
    typ_file
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| RheoError::project_config(format!("invalid .typ filename: {:?}", typ_file)))
}

fn get_files_for_plugin<'a>(
    plugin: &dyn FormatPlugin,
    project: &'a ProjectConfig,
) -> Result<Vec<&'a PathBuf>> {
    match project.config.spine_for_plugin(plugin.name()) {
        None => Ok(project.typ_files.iter().collect()),
        Some(spine) => {
            let content_dir = project
                .config
                .resolve_content_dir(&project.root)
                .unwrap_or_else(|| project.root.clone());
            let spine_files =
                rheo_core::reticulate::spine::generate_spine(&content_dir, Some(spine), false)?;
            let spine_set: HashSet<_> = spine_files.iter().collect();
            Ok(project
                .typ_files
                .iter()
                .filter(|f| spine_set.contains(f))
                .collect())
        }
    }
}

/// Per-plugin invariants shared across all files in a single-plugin compilation pass.
struct PerFileCtx<'a> {
    plugin: &'a dyn FormatPlugin,
    plugin_output_dir: &'a Path,
    project: &'a ProjectConfig,
    output_config: &'a OutputConfig,
    spine: &'a SpineOptions,
    plugin_section: &'a PluginSection,
    resolved_inputs: &'a HashMap<&'static str, PathBuf>,
}

/// Compile one file with the given world, recording success/failure in `results`.
///
/// `get_output_filename` errors propagate; `plugin.compile()` errors are recorded
/// as failures rather than propagated (so other files in the batch still compile).
fn compile_one_file(
    world: &mut RheoWorld,
    typ_file: &Path,
    pfc: &PerFileCtx<'_>,
    results: &mut CompilationResults,
) -> Result<()> {
    let filename = get_output_filename(typ_file)?;
    let output_path = pfc
        .plugin_output_dir
        .join(&filename)
        .with_extension(pfc.plugin.name());
    let options = RheoCompileOptions::new(typ_file, &output_path, &pfc.project.root, world);
    let ctx = PluginContext {
        project: pfc.project,
        output_config: pfc.output_config,
        options,
        spine: pfc.spine.clone(),
        config: pfc.plugin_section.clone(),
        inputs: pfc.resolved_inputs.clone(),
    };
    match pfc.plugin.compile(ctx) {
        Ok(_) => results.record_success(pfc.plugin.name()),
        Err(e) => {
            error!(file = %typ_file.display(), error = %e, "{} compilation failed", pfc.plugin.name());
            results.record_failure(pfc.plugin.name());
        }
    }
    Ok(())
}

fn perform_compilation<'a>(
    project: &ProjectConfig,
    output_config: &OutputConfig,
    plugins: &[Box<dyn FormatPlugin>],
    mut world: Option<&'a mut RheoWorld>,
    format_name: Option<&'a str>,
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
            let src = project.root.join(input.path);
            if src.is_file() {
                let dest = plugin_output_dir.join(input.path);
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
                resolved_inputs.insert(input.name, src);
            } else if input.required {
                return Err(RheoError::project_config(format!(
                    "plugin '{}' requires input '{}' at '{}' but it was not found",
                    plugin.name(),
                    input.name,
                    input.path
                )));
            }
        }

        // Execute copy patterns (global + per-plugin)
        let plugin_section_for_copy = project.config.plugin_section(plugin.name());
        for pattern in project
            .config
            .copy
            .iter()
            .chain(plugin_section_for_copy.copy.iter())
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

        // Resolve spine options
        let spine_cfg = project.config.spine_for_plugin(plugin.name());
        let spine = SpineOptions {
            title: spine_cfg.and_then(|s| s.title.clone()),
            vertebrae: spine_cfg.map(|s| s.vertebrae.clone()).unwrap_or_default(),
            merge: spine_cfg
                .and_then(|s| s.merge)
                .unwrap_or(plugin.default_merge()),
        };

        // Get full plugin section
        let plugin_section = project.config.plugin_section(plugin.name());

        if spine.merge {
            let compilation_root = project
                .config
                .resolve_content_dir(&project.root)
                .unwrap_or_else(|| project.root.clone());
            let output_path = plugin_output_dir
                .join(&project.name)
                .with_extension(plugin.name());

            let mut temp_world = RheoWorld::new(
                &compilation_root,
                project
                    .typ_files
                    .first()
                    .ok_or_else(|| RheoError::project_config("no .typ files found"))?,
                format_name,
            )?;
            let options = RheoCompileOptions::new(
                PathBuf::new(),
                &output_path,
                &compilation_root,
                &mut temp_world,
            );

            let ctx = PluginContext {
                project,
                output_config,
                options,
                spine,
                config: plugin_section,
                inputs: resolved_inputs,
            };

            match plugin.compile(ctx) {
                Ok(_) => {
                    results.record_success(plugin.name());
                    info!(output = %output_path.display(), "{} generation complete", plugin.name());
                }
                Err(e) => {
                    error!(error = %e, "{} generation failed", plugin.name());
                    results.record_failure(plugin.name());
                }
            }
        } else {
            let files = get_files_for_plugin(plugin.as_ref(), project)?;
            let pfc = PerFileCtx {
                plugin: plugin.as_ref(),
                plugin_output_dir: &plugin_output_dir,
                project,
                output_config,
                spine: &spine,
                plugin_section: &plugin_section,
                resolved_inputs: &resolved_inputs,
            };

            if let Some(ref mut existing_world) = world {
                for typ_file in &files {
                    existing_world.set_main(typ_file)?;
                    existing_world.reset();
                    compile_one_file(existing_world, typ_file, &pfc, &mut results)?;
                }
            } else {
                for typ_file in &files {
                    let mut fresh_world = RheoWorld::new(&project.root, typ_file, format_name)?;
                    compile_one_file(&mut fresh_world, typ_file, &pfc, &mut results)?;
                }
            }
        }
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

fn init_project(target_dir: &Path) -> Result<()> {
    if target_dir.exists() {
        return Err(RheoError::project_config(format!(
            "directory '{}' already exists",
            target_dir.display()
        )));
    }

    fs::create_dir_all(target_dir).map_err(|e| RheoError::io(e, "creating target directory"))?;

    let toml_content =
        rheo_core::init_templates::RHEO_TOML.replace("{{VERSION}}", manifest_version::CURRENT);
    fs::write(target_dir.join("rheo.toml"), toml_content)
        .map_err(|e| RheoError::io(e, "writing rheo.toml"))?;

    fs::write(
        target_dir.join("style.css"),
        rheo_core::init_templates::STYLE_CSS,
    )
    .map_err(|e| RheoError::io(e, "writing style.css"))?;

    let content_dir = target_dir.join("content");
    fs::create_dir_all(&content_dir)
        .map_err(|e| RheoError::io(e, "creating content directory"))?;

    fs::write(
        content_dir.join("index.typ"),
        rheo_core::init_templates::CONTENT_INDEX_TYP,
    )
    .map_err(|e| RheoError::io(e, "writing index.typ"))?;
    fs::write(
        content_dir.join("about.typ"),
        rheo_core::init_templates::CONTENT_ABOUT_TYP,
    )
    .map_err(|e| RheoError::io(e, "writing about.typ"))?;
    fs::write(
        content_dir.join("references.bib"),
        rheo_core::init_templates::CONTENT_REFERENCES_BIB,
    )
    .map_err(|e| RheoError::io(e, "writing references.bib"))?;

    let img_dir = content_dir.join("img");
    fs::create_dir_all(&img_dir).map_err(|e| RheoError::io(e, "creating img directory"))?;
    fs::write(
        img_dir.join("header.svg"),
        rheo_core::init_templates::CONTENT_IMG_HEADER_SVG,
    )
    .map_err(|e| RheoError::io(e, "writing header.svg"))?;

    info!(path = %target_dir.display(), "initialized rheo project");
    Ok(())
}

/// Setup: load project, apply smart defaults (if no config file), resolve plugins + build dir.
fn setup_compilation_context(
    path: &Path,
    config_path: Option<&Path>,
    build_dir: Option<PathBuf>,
    enabled_from_cli: Vec<String>,
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

    // Apply plugin smart defaults when no rheo.toml was loaded
    if project.config_path.is_none() {
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

    let plugins = plugins_for_formats(&formats, all);

    let resolved_build_dir = resolve_build_dir(&project, build_dir)?;
    let output_config = OutputConfig::new(&project.root, resolved_build_dir);

    Ok(CompilationContext {
        project,
        plugins,
        output_config,
    })
}

/// Main entry point using the builder-based dynamic CLI.
pub fn run() -> Result<()> {
    let cli = build_cli();
    let matches = cli.get_matches();

    let quiet = matches.get_flag("quiet");
    let verbose = matches.get_flag("verbose");
    init_logging(verbose, quiet)?;

    match matches.subcommand() {
        Some(("compile", sub)) => run_compile(sub),
        Some(("watch", _sub)) => Err(RheoError::project_config(
            "watch mode not yet implemented in the new architecture",
        )),
        Some(("clean", sub)) => run_clean(sub),
        Some(("init", sub)) => {
            let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
            init_project(&path)
        }
        _ => unreachable!("subcommand_required enforced by clap"),
    }
}

fn run_compile(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let config = sub.get_one::<String>("config").map(PathBuf::from);
    let build_dir = sub.get_one::<String>("build-dir").map(PathBuf::from);

    let all = all_plugins();
    let enabled = enabled_formats_from_matches(sub, &all);

    let ctx = setup_compilation_context(&path, config.as_deref(), build_dir, enabled)?;

    // Find first per-file plugin to use as the link-transformation format
    let format_name = ctx
        .plugins
        .iter()
        .find(|p| {
            let spine_cfg = ctx.project.config.spine_for_plugin(p.name());
            !spine_cfg.and_then(|s| s.merge).unwrap_or(false)
        })
        .map(|p| p.name());

    perform_compilation(
        &ctx.project,
        &ctx.output_config,
        &ctx.plugins,
        None,
        format_name,
    )
}

fn run_clean(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let config = sub.get_one::<String>("config").map(PathBuf::from);
    let build_dir = sub.get_one::<String>("build-dir").map(PathBuf::from);

    info!(path = %path.display(), "loading project");
    let project = ProjectConfig::from_path(&path, config.as_deref())?;
    let resolved_build_dir = resolve_build_dir(&project, build_dir)?;
    let output_config = OutputConfig::new(&project.root, resolved_build_dir);
    info!(project = %project.name, "cleaning build artifacts");
    output_config.clean()?;
    info!(project = %project.name, "build artifacts removed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_formats_cli_flags_override_config() {
        let all = all_plugins();
        let config_defaults = vec!["pdf".to_string()];
        let enabled = vec!["pdf".to_string()];

        let formats = determine_formats(enabled, &config_defaults, &all);
        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&"pdf".to_string()));
    }

    #[test]
    fn test_determine_formats_uses_config_defaults_when_no_flags() {
        let all = all_plugins();
        let config_defaults = vec!["html".to_string()];
        let enabled: Vec<String> = vec![];

        let formats = determine_formats(enabled, &config_defaults, &all);
        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&"html".to_string()));
    }

    #[test]
    fn test_determine_formats_falls_back_to_all_when_empty() {
        let all = all_plugins();
        let config_defaults: Vec<String> = vec![];
        let enabled: Vec<String> = vec![];

        let formats = determine_formats(enabled, &config_defaults, &all);
        // Should contain all plugin names
        assert_eq!(formats.len(), all_plugins().len());
        assert!(formats.contains(&"pdf".to_string()));
        assert!(formats.contains(&"html".to_string()));
        assert!(formats.contains(&"epub".to_string()));
    }

    #[test]
    fn test_determine_formats_multiple_cli_flags() {
        let all = all_plugins();
        let config_defaults = vec!["epub".to_string()];
        let enabled = vec!["pdf".to_string(), "html".to_string()];

        let formats = determine_formats(enabled, &config_defaults, &all);
        assert_eq!(formats.len(), 2);
        assert!(formats.contains(&"pdf".to_string()));
        assert!(formats.contains(&"html".to_string()));
    }

    #[test]
    fn test_all_plugins_contains_three_formats() {
        let plugins = all_plugins();
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"html"));
        assert!(names.contains(&"pdf"));
        assert!(names.contains(&"epub"));
        assert_eq!(names.len(), 3);
    }
}
