use crate::CompilationResults;
use crate::compile::RheoCompileOptions;
use crate::plugins::{FormatPlugin, PluginConfig, PluginContext, SpineOptions, plugins_for_formats};
use crate::reticulate::spine::generate_spine;
use crate::{OutputFormat, Result, open_all_files_in_folder};
use clap::{Parser, Subcommand};
use std::collections::HashSet;
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

/// World mode for perform_compilation
enum WorldMode<'a> {
    /// Fresh compilation (creates new World for each file)
    Fresh { root: PathBuf },
    /// Incremental compilation (reuses existing World)
    Incremental {
        world: &'a mut crate::world::RheoWorld,
    },
}

/// Pre-compiled setup context for compilation commands
struct CompilationContext {
    /// Loaded project configuration
    project: crate::project::ProjectConfig,
    /// Formats to compile (resolved from CLI flags and config)
    formats: Vec<OutputFormat>,
    /// Output configuration with resolved build directory
    output_config: crate::output::OutputConfig,
    /// Compilation root (content_dir or project root)
    compilation_root: PathBuf,
}

/// Determine which formats to compile based on CLI flags and config defaults
fn determine_formats(
    flags: FormatFlags,
    config_defaults: &[OutputFormat],
) -> Result<Vec<OutputFormat>> {
    // If any CLI flags are set, use those
    if flags.any_set() {
        let mut formats = Vec::new();
        if flags.pdf {
            formats.push(OutputFormat::Pdf);
        }
        if flags.html {
            formats.push(OutputFormat::Html);
        }
        if flags.epub {
            formats.push(OutputFormat::Epub);
        }
        return Ok(formats);
    }

    // Otherwise, use config defaults provided not empty
    if !config_defaults.is_empty() {
        Ok(config_defaults.to_vec())
    } else {
        Ok(OutputFormat::all_variants())
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

    match plugin.spine_config(&project.config) {
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
/// This is the unified compilation logic that supports both fresh and incremental compilation
/// based on the WorldMode parameter.
///
/// # Arguments
/// * `mode` - World mode (Fresh or Incremental)
/// * `project` - Project configuration with source files and assets
/// * `output_config` - Output directory configuration
/// * `plugins` - List of format plugins to compile with
///
/// # Returns
/// * `Ok(())` if at least one format fully succeeded
/// * `Err` if all formats failed
fn perform_compilation<'a>(
    mut mode: WorldMode<'a>,
    project: &crate::project::ProjectConfig,
    output_config: &crate::output::OutputConfig,
    plugins: &[Box<dyn FormatPlugin>],
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
        let plugin_output_dir = output_config.dir_for_format(plugin.output_format());
        plugin.copy_assets(project, plugin_output_dir)?;

        // Resolve standardized PluginConfig from format-specific spine config
        let spine_cfg = plugin.spine_config(&project.config);
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
                .with_extension(plugin.extension());

            let options = match &mode {
                WorldMode::Fresh { root: _ } => {
                    RheoCompileOptions::new(PathBuf::new(), &output_path, &compilation_root)
                }
                WorldMode::Incremental { .. } => {
                    if let WorldMode::Incremental { world } = &mut mode {
                        RheoCompileOptions::incremental(
                            PathBuf::new(),
                            &output_path,
                            &compilation_root,
                            world,
                        )
                    } else {
                        unreachable!()
                    }
                }
            };

            let ctx = PluginContext {
                project,
                output_config,
                options,
                plugin_config,
            };

            match plugin.compile(ctx) {
                Ok(_) => {
                    results.record_success(plugin.output_format());
                    info!(output = %output_path.display(), "{} generation complete", plugin.name());
                }
                Err(e) => {
                    error!(error = %e, "{} generation failed", plugin.name());
                    results.record_failure(plugin.output_format());
                }
            }
        } else {
            // Per-file mode: compile each .typ file independently
            let files = get_files_for_plugin(plugin.as_ref(), project)?;

            for typ_file in &files {
                // For incremental mode, prepare the World for compiling this specific file
                // 1. set_main() tells the World which file we're compiling
                // 2. reset() clears file caches while preserving fonts/packages
                if let WorldMode::Incremental { world } = &mut mode {
                    world.set_main(typ_file)?;
                    world.reset();
                }

                let filename = get_output_filename(typ_file)?;
                let output_path = plugin_output_dir
                    .join(&filename)
                    .with_extension(plugin.extension());

                let options = match &mode {
                    WorldMode::Fresh { root } => {
                        RheoCompileOptions::new(typ_file, &output_path, root)
                    }
                    WorldMode::Incremental { .. } => {
                        if let WorldMode::Incremental { world } = &mut mode {
                            RheoCompileOptions::incremental(
                                typ_file,
                                &output_path,
                                &project.root,
                                world,
                            )
                        } else {
                            unreachable!()
                        }
                    }
                };

                let ctx = PluginContext {
                    project,
                    output_config,
                    options,
                    plugin_config: plugin_config.clone(),
                };

                match plugin.compile(ctx) {
                    Ok(_) => results.record_success(plugin.output_format()),
                    Err(e) => {
                        error!(file = %typ_file.display(), error = %e, "{} compilation failed", plugin.name());
                        results.record_failure(plugin.output_format());
                    }
                }
            }
        }
    }

    // Report results with per-format summary
    let formats: Vec<OutputFormat> = plugins.iter().map(|p| p.output_format()).collect();
    results.log_summary(&formats);

    // Fail if any format had failures
    if results.has_failures() {
        if formats.iter().any(|fmt| results.get(*fmt).succeeded > 0) {
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

        // 4. Create output config and directories
        let output_config = crate::output::OutputConfig::new(&project.root, resolved_build_dir);
        output_config.create_dirs()?;

        // 5. Resolve compilation root from content_dir or project root
        let compilation_root = project
            .config
            .resolve_content_dir(&project.root)
            .unwrap_or_else(|| project.root.clone());

        Ok(CompilationContext {
            project,
            formats,
            output_config,
            compilation_root,
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
                let plugins = plugins_for_formats(&ctx.formats);

                // Create world mode (Fresh)
                let mode = WorldMode::Fresh {
                    root: ctx.compilation_root,
                };

                // Perform compilation
                perform_compilation(mode, &ctx.project, &ctx.output_config, &plugins)
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
                let plugins = plugins_for_formats(&ctx.formats);

                // Perform initial compilation (Fresh mode)
                info!("compiling project");
                let mode = WorldMode::Fresh {
                    root: ctx.compilation_root.clone(),
                };
                if let Err(e) =
                    perform_compilation(mode, &ctx.project, &ctx.output_config, &plugins)
                {
                    warn!(error = %e, "initial compilation failed, continuing to watch");
                }

                // Destructure context for use in watch loop
                let CompilationContext {
                    project,
                    formats: _,
                    output_config,
                    compilation_root: _,
                } = ctx;

                // Start web server if --open and any plugin supports live preview
                let server_info = if open && plugins.iter().any(|p| p.supports_live_preview()) {
                    // Need tokio runtime for async server
                    let runtime = tokio::runtime::Runtime::new()
                        .map_err(|e| crate::RheoError::io(e, "creating tokio runtime"))?;

                    let html_dir = output_config.html_dir.clone();
                    let (server_handle, reload_tx, server_url) = runtime
                        .block_on(async { crate::server::start_server(html_dir, 3000).await })?;

                    // Open browser
                    if let Err(e) = crate::server::open_browser(&server_url) {
                        warn!(error = %e, "failed to open browser, but server is running");
                    }

                    Some((runtime, server_handle, reload_tx))
                } else {
                    None
                };

                // Open output files for non-live-preview plugins
                if open {
                    for plugin in &plugins {
                        if !plugin.supports_live_preview() {
                            let dir =
                                output_config.dir_for_format(plugin.output_format()).clone();
                            open_all_files_in_folder(dir, plugin.extension())?;
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

                // For watch mode: find the first per-file plugin to determine World output format
                // for link transformation. If no per-file plugins, use None (no transformation).
                let output_format = plugins
                    .iter()
                    .find(|p| {
                        let spine_cfg = p.spine_config(&borrowed_project.config);
                        !spine_cfg.and_then(|s| s.merge()).unwrap_or(false)
                    })
                    .map(|p| p.output_format());

                let world = crate::world::RheoWorld::new(
                    &compilation_root,
                    initial_main,
                    output_format,
                )?;
                drop(borrowed_project); // Release borrow before moving into RefCell

                let world_cell = RefCell::new(world);

                // Canonicalize build directory for reliable path comparison in watcher
                // This prevents the watcher from triggering on its own output files
                let canonical_build_dir = output_config
                    .pdf_dir
                    .parent()
                    .expect("build dir has parent")
                    .canonicalize()
                    .map_err(|e| {
                        crate::RheoError::io(
                            e,
                            format!(
                                "canonicalizing build directory {:?}",
                                output_config.pdf_dir.parent()
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
                                let mode = WorldMode::Incremental {
                                    world: &mut world_cell.borrow_mut(),
                                };
                                perform_compilation(
                                    mode,
                                    &project_cell.borrow(),
                                    &output_config,
                                    &plugins,
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

                                        // Use same output_format logic as initial World creation
                                        let output_format = plugins
                                            .iter()
                                            .find(|p| {
                                                let spine_cfg =
                                                    p.spine_config(&borrowed.config);
                                                !spine_cfg
                                                    .and_then(|s| s.merge())
                                                    .unwrap_or(false)
                                            })
                                            .map(|p| p.output_format());

                                        match crate::world::RheoWorld::new(
                                            &new_compilation_root,
                                            new_initial_main,
                                            output_format,
                                        ) {
                                            Ok(new_world) => {
                                                *world_cell.borrow_mut() = new_world;
                                                let mode = WorldMode::Incremental {
                                                    world: &mut world_cell.borrow_mut(),
                                                };
                                                perform_compilation(
                                                    mode,
                                                    &borrowed,
                                                    &output_config,
                                                    &plugins,
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

                            if let Some((_, _, reload_tx)) = &server_info {
                                // Ignore errors if no clients are connected
                                let _ = reload_tx.send(());
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
        let config_defaults = vec![OutputFormat::Pdf];
        let flags = FormatFlags {
            pdf: true,
            html: false,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&OutputFormat::Pdf));
    }

    #[test]
    fn test_determine_formats_uses_config_defaults_when_no_flags() {
        let config_defaults = vec![OutputFormat::Html];
        let flags = FormatFlags {
            pdf: false,
            html: false,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&OutputFormat::Html));
    }

    #[test]
    fn test_determine_formats_falls_back_to_all_when_empty() {
        let config_defaults = vec![];
        let flags = FormatFlags {
            pdf: false,
            html: false,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 3);
        assert!(formats.contains(&OutputFormat::Pdf));
        assert!(formats.contains(&OutputFormat::Html));
        assert!(formats.contains(&OutputFormat::Epub));
    }

    #[test]
    fn test_determine_formats_multiple_cli_flags() {
        let config_defaults = vec![OutputFormat::Epub];
        let flags = FormatFlags {
            pdf: true,
            html: true,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 2);
        assert!(formats.contains(&OutputFormat::Pdf));
        assert!(formats.contains(&OutputFormat::Html));
    }

    #[test]
    fn test_determine_formats_all_three_formats() {
        let config_defaults = OutputFormat::all_variants();
        let flags = FormatFlags {
            pdf: false,
            html: false,
            epub: false,
        };

        let formats = determine_formats(flags, &config_defaults).unwrap();
        assert_eq!(formats.len(), 3);
        assert!(formats.contains(&OutputFormat::Pdf));
        assert!(formats.contains(&OutputFormat::Html));
        assert!(formats.contains(&OutputFormat::Epub));
    }
}
