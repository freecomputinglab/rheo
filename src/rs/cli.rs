use crate::CompilationResults;
use crate::compile::RheoCompileOptions;
use crate::plugins::{FormatPlugin, OpenHandle, PluginConfig, PluginContext, SpineOptions, plugins_for_names};
use crate::reticulate::spine::generate_spine;
use crate::Result;
use clap::{Parser, Subcommand};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

/// CLI format flags (what the user requested via command-line)
#[derive(Debug, Clone, Copy)]
struct FormatFlags {
    pdf: bool,
    html: bool,
    epub: bool,
}

impl FormatFlags {
    fn any_set(&self) -> bool {
        self.pdf || self.html || self.epub
    }
}

/// Pre-compiled setup context for compilation commands
struct CompilationContext {
    /// Loaded project configuration
    project: crate::project::ProjectConfig,
    /// Format names to compile (resolved from CLI flags and config)
    formats: Vec<String>,
    /// Output configuration with resolved build directory
    output_config: crate::output::OutputConfig,
}

/// Determine which formats to compile based on CLI flags and config defaults
fn determine_formats(flags: FormatFlags, config_defaults: &[String]) -> Result<Vec<String>> {
    // If any CLI flags are set, use those
    if flags.any_set() {
        let mut formats = Vec::new();
        if flags.pdf {
            formats.push("pdf".to_string());
        }
        if flags.html {
            formats.push("html".to_string());
        }
        if flags.epub {
            formats.push("epub".to_string());
        }
        return Ok(formats);
    }

    // Otherwise, use config defaults if not empty; fall back to all plugin names
    if !config_defaults.is_empty() {
        Ok(config_defaults.to_vec())
    } else {
        Ok(crate::plugins::all_plugins()
            .iter()
            .map(|p| p.name().to_string())
            .collect())
    }
}

#[derive(Parser, Debug)]
#[command(name = "rheo")]
#[command(about = "A tool for flowing Typst documents into publishable outputs", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Decrease output verbosity (errors only)
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Increase output verbosity (show debug information)
    #[arg(short, long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Compile Typst documents to PDF, HTML, and/or EPUB
    Compile {
        /// Path to project directory or single .typ file
        path: PathBuf,

        /// Path to custom rheo.toml config file
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,

        /// Build output directory (overrides rheo.toml if set)
        #[arg(long)]
        build_dir: Option<PathBuf>,

        /// Compile to PDF only
        #[arg(long)]
        pdf: bool,

        /// Compile to HTML only
        #[arg(long)]
        html: bool,

        /// Compile to EPUB only
        #[arg(long)]
        epub: bool,
    },

    /// Watch Typst documents and recompile on changes
    Watch {
        /// Path to project directory or single .typ file
        path: PathBuf,

        /// Path to custom rheo.toml config file
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,

        /// Build output directory (overrides rheo.toml if set)
        #[arg(long)]
        build_dir: Option<PathBuf>,

        /// Watch and compile to PDF only
        #[arg(long)]
        pdf: bool,

        /// Watch and compile to HTML only
        #[arg(long)]
        html: bool,

        /// Watch and compile to EPUB only
        #[arg(long)]
        epub: bool,

        /// Open output in appropriate viewer (HTML opens in browser with live reload)
        #[arg(long)]
        open: bool,
    },

    /// Clean build artifacts for a project
    Clean {
        /// Path to project directory or single .typ file (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Path to custom rheo.toml config file
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,

        /// Build output directory to clean (overrides rheo.toml if set)
        #[arg(long)]
        build_dir: Option<PathBuf>,
    },

    /// Initialize a new Rheo project
    Init {
        /// Path to the new project directory
        path: PathBuf,
    },
}

/// Resolve a path relative to a base directory
///
/// If path is absolute, returns it as-is.
/// If path is relative, resolves it relative to base.
fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// Resolve build directory with priority: CLI arg > config > default
///
/// # Arguments
/// * `project` - Project configuration (contains config and root)
/// * `cli_build_dir` - Optional CLI-provided build directory
///
/// # Returns
/// * `Some(PathBuf)` if build_dir is explicitly set via CLI or config
/// * `None` to use default (project_root/build)
fn resolve_build_dir(
    project: &crate::project::ProjectConfig,
    cli_build_dir: Option<PathBuf>,
) -> Result<Option<PathBuf>> {
    if let Some(cli_path) = cli_build_dir {
        // Priority 1: CLI argument (resolve relative to current directory)
        let cwd = std::env::current_dir()
            .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;
        debug!(dir = %cli_path.display(), "build directory");
        Ok(Some(resolve_path(&cwd, &cli_path)))
    } else if let Some(config_path) = &project.config.build_dir {
        // Priority 2: Config file (resolve relative to project root)
        let resolved = resolve_path(&project.root, Path::new(config_path));
        debug!(dir = %resolved.display(), "build directory");
        Ok(Some(resolved))
    } else {
        // Priority 3: Default (None signals OutputConfig::new to use default)
        Ok(None)
    }
}

/// Helper to extract output filename from .typ file path
fn get_output_filename(typ_file: &std::path::Path) -> Result<String> {
    typ_file
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            crate::RheoError::project_config(format!("invalid .typ filename: {:?}", typ_file))
        })
}

/// Returns the set of files to compile for a given plugin based on its spine config.
/// If no spine is configured, returns all project files.
fn get_files_for_plugin<'a>(
    plugin: &dyn FormatPlugin,
    project: &'a crate::project::ProjectConfig,
) -> Result<Vec<&'a PathBuf>> {
    let content_dir = project
        .config
        .resolve_content_dir(&project.root)
        .unwrap_or_else(|| project.root.clone());

    match project.config.spine_for_plugin(plugin.name()) {
        None => Ok(project.typ_files.iter().collect()),
        Some(spine) => {
            let spine_files = generate_spine(&content_dir, Some(spine), false)?;
            let spine_set: HashSet<_> = spine_files.iter().collect();
            Ok(project
                .typ_files
                .iter()
                .filter(|f| spine_set.contains(f))
                .collect())
        }
    }
}

/// Perform compilation for a project with specified plugins
///
/// The engine creates and provides a RheoWorld for each file being compiled.
/// This enables incremental compilation through Typst's comemo caching system.
///
/// # Arguments
/// * `project` - Project configuration with source files and assets
/// * `output_config` - Output directory configuration
/// * `plugins` - List of format plugins to compile with
/// * `world` - Optional existing World for incremental mode (None = fresh mode)
/// * `format_name` - Format name for link transformation (None for merged mode)
///
/// # Returns
/// * `Ok(())` if at least one format fully succeeded
/// * `Err` if all formats failed
fn perform_compilation<'a>(
    project: &crate::project::ProjectConfig,
    output_config: &crate::output::OutputConfig,
    plugins: &[Box<dyn FormatPlugin>],
    mut world: Option<&'a mut crate::world::RheoWorld>,
    format_name: Option<&'a str>,
) -> Result<()> {
    // Check for .typ files
    if project.typ_files.is_empty() {
        return Err(crate::RheoError::project_config(
            "no .typ files found in project",
        ));
    }

    // Track success/failure per format for graceful degradation
    let mut results = CompilationResults::new();

    for plugin in plugins {
        let plugin_output_dir = output_config.dir_for_plugin(plugin.name());
        std::fs::create_dir_all(&plugin_output_dir).map_err(|e| {
            crate::RheoError::io(
                e,
                format!("creating output directory for {}", plugin.name()),
            )
        })?;
        // Resolve declared inputs: find, copy, and collect source paths
        let mut resolved_inputs: HashMap<&'static str, PathBuf> = HashMap::new();
        for input in plugin.inputs() {
            let src = project.root.join(input.path);
            if src.is_file() {
                let dest = plugin_output_dir.join(input.path);
                std::fs::copy(&src, &dest).map_err(|e| {
                    crate::RheoError::io(
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
                return Err(crate::RheoError::project_config(format!(
                    "plugin '{}' requires input '{}' at '{}' but it was not found",
                    plugin.name(),
                    input.name,
                    input.path
                )));
            }
        }

        // Resolve standardized PluginConfig from format-specific spine config
        let spine_cfg = project.config.spine_for_plugin(plugin.name());
        let plugin_config = PluginConfig {
            spine: SpineOptions {
                title: spine_cfg.and_then(|s| s.title()).map(str::to_string),
                vertebrae: spine_cfg
                    .map(|s| s.vertebrae().to_vec())
                    .unwrap_or_default(),
                merge: spine_cfg.and_then(|s| s.merge()).unwrap_or(false),
            },
        };

        if plugin_config.spine.merge {
            // Merged mode: single output combining all spine files
            let compilation_root = project
                .config
                .resolve_content_dir(&project.root)
                .unwrap_or_else(|| project.root.clone());
            let output_path = plugin_output_dir
                .join(&project.name)
                .with_extension(plugin.name());

            // For merged mode, the plugin creates its own World (needs temp file first)
            // Create a temporary World just to satisfy the API - plugin will ignore it
            let mut temp_world = crate::world::RheoWorld::new(
                &compilation_root,
                project.typ_files.first().ok_or_else(|| {
                    crate::RheoError::project_config("no .typ files found")
                })?,
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
                plugin_config,
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
            // Per-file mode: compile each .typ file independently
            let files = get_files_for_plugin(plugin.as_ref(), project)?;

            if let Some(ref mut existing_world) = world {
                // Incremental mode: reuse existing World for each file
                for typ_file in &files {
                    // Prepare existing World for this file
                    existing_world.set_main(typ_file)?;
                    existing_world.reset();

                    let filename = get_output_filename(typ_file)?;
                    let output_path = plugin_output_dir
                        .join(&filename)
                        .with_extension(plugin.name());

                    let options = RheoCompileOptions::new(
                        typ_file,
                        &output_path,
                        &project.root,
                        existing_world,
                    );

                    let ctx = PluginContext {
                        project,
                        output_config,
                        options,
                        plugin_config: plugin_config.clone(),
                        inputs: resolved_inputs.clone(),
                    };

                    match plugin.compile(ctx) {
                        Ok(_) => results.record_success(plugin.name()),
                        Err(e) => {
                            error!(file = %typ_file.display(), error = %e, "{} compilation failed", plugin.name());
                            results.record_failure(plugin.name());
                        }
                    }
                }
            } else {
                // Fresh mode: create new World for each file
                for typ_file in &files {
                    let mut fresh_world = crate::world::RheoWorld::new(
                        &project.root,
                        typ_file,
                        format_name,
                    )?;

                    let filename = get_output_filename(typ_file)?;
                    let output_path = plugin_output_dir
                        .join(&filename)
                        .with_extension(plugin.name());

                    let options = RheoCompileOptions::new(
                        typ_file,
                        &output_path,
                        &project.root,
                        &mut fresh_world,
                    );

                    let ctx = PluginContext {
                        project,
                        output_config,
                        options,
                        plugin_config: plugin_config.clone(),
                        inputs: resolved_inputs.clone(),
                    };

                    match plugin.compile(ctx) {
                        Ok(_) => results.record_success(plugin.name()),
                        Err(e) => {
                            error!(file = %typ_file.display(), error = %e, "{} compilation failed", plugin.name());
                            results.record_failure(plugin.name());
                        }
                    }
                }
            }
        }
    }

    // Report results with per-format summary
    let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
    results.log_summary(&names);

    // Fail if any format had failures
    if results.has_failures() {
        if names.iter().any(|name| results.get(name).succeeded > 0) {
            // Partial success - some formats worked, some failed
            Err(crate::RheoError::project_config(
                "some formats failed to compile".to_string(),
            ))
        } else {
            // Total failure - all formats failed
            Err(crate::RheoError::project_config(
                "all formats failed or no files were compiled".to_string(),
            ))
        }
    } else {
        // All requested formats succeeded
        info!("compilation complete");
        Ok(())
    }
}

impl Cli {
    pub fn parse() -> Self {
        Parser::parse()
    }

    /// Get the verbosity level from CLI flags
    pub fn verbosity(&self) -> crate::logging::Verbosity {
        if self.quiet {
            crate::logging::Verbosity::Quiet
        } else if self.verbose {
            crate::logging::Verbosity::Verbose
        } else {
            crate::logging::Verbosity::Normal
        }
    }

    /// Load project and resolve all compilation settings
    ///
    /// This performs all the setup steps common to both compile and watch commands:
    /// - Loads project configuration
    /// - Resolves format flags
    /// - Resolves build directory
    /// - Creates output directories
    /// - Resolves compilation and repo roots
    ///
    /// # Arguments
    /// * `path` - Path to project directory or single .typ file
    /// * `config_path` - Optional custom rheo.toml path
    /// * `build_dir` - Optional custom build directory (overrides config)
    /// * `format_flags` - CLI format flags (pdf, html, epub)
    ///
    /// # Returns
    /// * `CompilationContext` with all resolved settings
    fn setup_compilation_context(
        path: &Path,
        config_path: Option<&Path>,
        build_dir: Option<PathBuf>,
        format_flags: FormatFlags,
    ) -> Result<CompilationContext> {
        // 1. Load project
        info!(path = %path.display(), "loading project");
        let project = crate::project::ProjectConfig::from_path(path, config_path)?;
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

        // 2. Determine formats from CLI flags and config
        let formats = determine_formats(format_flags, &project.config.formats)?;

        // 3. Resolve build directory from CLI arg or config
        let resolved_build_dir = resolve_build_dir(&project, build_dir)?;

        // 4. Create output config (directories are created per-plugin in perform_compilation)
        let output_config = crate::output::OutputConfig::new(&project.root, resolved_build_dir);

        Ok(CompilationContext {
            project,
            formats,
            output_config,
        })
    }

    /// Main entrypoint for the rheo CLI
    pub fn run(self) -> Result<()> {
        match self.command {
            Commands::Compile {
                path,
                config,
                build_dir,
                pdf,
                html,
                epub,
            } => {
                // Setup compilation context
                let flags = FormatFlags { pdf, html, epub };
                let ctx =
                    Self::setup_compilation_context(&path, config.as_deref(), build_dir, flags)?;

                // Build plugin list from resolved formats
                let plugins = plugins_for_names(&ctx.formats);

                // Determine format_name for link transformation
                // Find first per-file plugin to determine format name
                let format_name = plugins
                    .iter()
                    .find(|p| {
                        let spine_cfg = ctx.project.config.spine_for_plugin(p.name());
                        !spine_cfg.and_then(|s| s.merge()).unwrap_or(false)
                    })
                    .map(|p| p.name());

                // Perform compilation (fresh mode - no existing World)
                perform_compilation(
                    &ctx.project,
                    &ctx.output_config,
                    &plugins,
                    None, // Fresh mode: create new World for each file
                    format_name,
                )
            }
            Commands::Watch {
                path,
                config,
                build_dir,
                pdf,
                html,
                epub,
                open,
            } => {
                // Setup compilation context
                let flags = FormatFlags { pdf, html, epub };
                let ctx =
                    Self::setup_compilation_context(&path, config.as_deref(), build_dir, flags)?;

                // Build plugin list from resolved formats
                let plugins = plugins_for_names(&ctx.formats);

                // Determine format_name for link transformation
                let format_name = plugins
                    .iter()
                    .find(|p| {
                        let spine_cfg = ctx.project.config.spine_for_plugin(p.name());
                        !spine_cfg.and_then(|s| s.merge()).unwrap_or(false)
                    })
                    .map(|p| p.name());

                // Perform initial compilation (Fresh mode - no existing World)
                info!("compiling project");
                if let Err(e) = perform_compilation(
                    &ctx.project,
                    &ctx.output_config,
                    &plugins,
                    None, // Fresh mode: create new World for each file
                    format_name,
                ) {
                    warn!(error = %e, "initial compilation failed, continuing to watch");
                }

                // Destructure context for use in watch loop
                let CompilationContext {
                    project,
                    formats: _,
                    output_config,
                } = ctx;

                // Call open() on all plugins when --open flag is used
                // Store handles in a vector for the watch loop
                let mut open_handles: Vec<(String, OpenHandle)> = Vec::new();

                if open {
                    for plugin in &plugins {
                        let plugin_name = plugin.name().to_string();
                        let output_dir = output_config.dir_for_plugin(plugin.name());

                        match plugin.open(&output_dir, format_name.unwrap_or("unknown")) {
                            Ok(handle) => {
                                open_handles.push((plugin_name, handle));
                            }
                            Err(e) => {
                                warn!(plugin = %plugin_name, error = %e, "failed to open output");
                            }
                        }
                    }
                }

                // Set up file watcher with interior mutability for project and world updates
                use std::cell::RefCell;
                let project_cell = RefCell::new(project);

                // Create RheoWorld for incremental compilation (reused across file changes)
                let borrowed_project = project_cell.borrow();
                let compilation_root = borrowed_project
                    .config
                    .resolve_content_dir(&borrowed_project.root)
                    .unwrap_or_else(|| borrowed_project.root.clone());

                // Use first .typ file as initial main (will be updated for each compilation)
                let initial_main = borrowed_project
                    .typ_files
                    .first()
                    .ok_or_else(|| crate::RheoError::project_config("no .typ files found"))?;

                // For watch mode: find the first per-file plugin to determine the format name
                // for link transformation. If no per-file plugins, use None (no transformation).
                let format_name = plugins
                    .iter()
                    .find(|p| {
                        let spine_cfg =
                            borrowed_project.config.spine_for_plugin(p.name());
                        !spine_cfg.and_then(|s| s.merge()).unwrap_or(false)
                    })
                    .map(|p| p.name());

                let world = crate::world::RheoWorld::new(
                    &compilation_root,
                    initial_main,
                    format_name,
                )?;
                drop(borrowed_project); // Release borrow before moving into RefCell

                let world_cell = RefCell::new(world);

                // Canonicalize build directory for reliable path comparison in watcher
                // This prevents the watcher from triggering on its own output files
                let canonical_build_dir = output_config
                    .base
                    .canonicalize()
                    .or_else(|_| {
                        // If the base dir doesn't exist yet, create it and canonicalize
                        std::fs::create_dir_all(&output_config.base).ok();
                        output_config.base.canonicalize()
                    })
                    .map_err(|e| {
                        crate::RheoError::io(
                            e,
                            format!(
                                "canonicalizing build directory {:?}",
                                output_config.base
                            ),
                        )
                    })?;

                info!("watching for changes");
                crate::watch::watch_project(
                    &project_cell.borrow(),
                    &canonical_build_dir,
                    |event| {
                        let result = match event {
                            crate::watch::WatchEvent::FilesChanged => {
                                info!("change detected, recompiling");
                                perform_compilation(
                                    &project_cell.borrow(),
                                    &output_config,
                                    &plugins,
                                    Some(&mut world_cell.borrow_mut()), // Incremental mode: reuse World
                                    format_name,
                                )
                            }
                            crate::watch::WatchEvent::ConfigChanged => {
                                info!("configuration changed, reloading");
                                // Reload project configuration
                                match crate::project::ProjectConfig::from_path(
                                    &path,
                                    config.as_deref(),
                                ) {
                                    Ok(new_project) => {
                                        *project_cell.borrow_mut() = new_project;
                                        let borrowed = project_cell.borrow();
                                        let file_word = if borrowed.typ_files.len() == 1 {
                                            "file"
                                        } else {
                                            "files"
                                        };
                                        info!(name = %borrowed.name, files = borrowed.typ_files.len(), "reloaded ({} {})", borrowed.typ_files.len(), file_word);

                                        // Recreate World with new configuration
                                        let new_compilation_root = borrowed
                                            .config
                                            .resolve_content_dir(&borrowed.root)
                                            .unwrap_or_else(|| borrowed.root.clone());
                                        let new_initial_main =
                                            borrowed.typ_files.first().ok_or_else(|| {
                                                crate::RheoError::project_config(
                                                    "no .typ files found",
                                                )
                                            })?;

                                        // Use same format_name logic as initial World creation
                                        let new_format_name = plugins
                                            .iter()
                                            .find(|p| {
                                                let spine_cfg =
                                                    borrowed.config.spine_for_plugin(p.name());
                                                !spine_cfg
                                                    .and_then(|s| s.merge())
                                                    .unwrap_or(false)
                                            })
                                            .map(|p| p.name());

                                        match crate::world::RheoWorld::new(
                                            &new_compilation_root,
                                            new_initial_main,
                                            new_format_name,
                                        ) {
                                            Ok(new_world) => {
                                                *world_cell.borrow_mut() = new_world;
                                                perform_compilation(
                                                    &borrowed,
                                                    &output_config,
                                                    &plugins,
                                                    Some(&mut world_cell.borrow_mut()), // Incremental mode: reuse World
                                                    new_format_name,
                                                )
                                            }
                                            Err(e) => {
                                                error!(error = %e, "failed to recreate World after config change");
                                                Err(e)
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!(error = %e, "failed to reload project config");
                                        Err(e)
                                    }
                                }
                            }
                        };

                        // Send reload event if compilation succeeded and we have a server
                        if result.is_ok() {
                            // Evict old entries from the comemo cache to prevent unbounded memory growth
                            // during long watch sessions. This matches Typst CLI's behavior.
                            comemo::evict(10);

                            // Send reload events to all server-based handles
                            for (_plugin_name, handle) in &open_handles {
                                if let OpenHandle::Server(server_handle) = handle {
                                    (server_handle.reload_callback)();
                                }
                            }
                        }

                        result
                    },
                )?;

                // Server will be dropped and cleaned up automatically here

                Ok(())
            }
            Commands::Clean {
                path,
                config,
                build_dir,
            } => {
                info!(path = %path.display(), "loading project");
                let project = crate::project::ProjectConfig::from_path(&path, config.as_deref())?;

                // Resolve build directory
                let resolved_build_dir = resolve_build_dir(&project, build_dir)?;

                let output_config =
                    crate::output::OutputConfig::new(&project.root, resolved_build_dir);
                info!(project = %project.name, "cleaning build artifacts");
                output_config.clean()?;
                info!(project = %project.name, "build artifacts removed");
                Ok(())
            }
            Commands::Init { path } => crate::init::init_project(&path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_formats_cli_flags_override_config() {
        // CLI flags should override config defaults
        let config_defaults = vec!["pdf".to_string()];
        let flags = FormatFlags {
            pdf: true,
            html: false,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&"pdf".to_string()));
    }

    #[test]
    fn test_determine_formats_uses_config_defaults_when_no_flags() {
        let config_defaults = vec!["html".to_string()];
        let flags = FormatFlags {
            pdf: false,
            html: false,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&"html".to_string()));
    }

    #[test]
    fn test_determine_formats_falls_back_to_all_when_empty() {
        let config_defaults: Vec<String> = vec![];
        let flags = FormatFlags {
            pdf: false,
            html: false,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 3);
        assert!(formats.contains(&"pdf".to_string()));
        assert!(formats.contains(&"html".to_string()));
        assert!(formats.contains(&"epub".to_string()));
    }

    #[test]
    fn test_determine_formats_multiple_cli_flags() {
        let config_defaults = vec!["epub".to_string()];
        let flags = FormatFlags {
            pdf: true,
            html: true,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 2);
        assert!(formats.contains(&"pdf".to_string()));
        assert!(formats.contains(&"html".to_string()));
    }

    #[test]
    fn test_determine_formats_all_three_formats() {
        let config_defaults = vec!["html".to_string(), "epub".to_string(), "pdf".to_string()];
        let flags = FormatFlags {
            pdf: false,
            html: false,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 3);
        assert!(formats.contains(&"pdf".to_string()));
        assert!(formats.contains(&"html".to_string()));
        assert!(formats.contains(&"epub".to_string()));
    }
}
