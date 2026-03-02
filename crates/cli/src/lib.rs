use rheo_core::compile::RheoCompileOptions;
use rheo_core::manifest_version;
use rheo_core::output::OutputConfig;
use rheo_core::project::ProjectConfig;
use rheo_core::results::CompilationResults;
use rheo_core::world::RheoWorld;
use rheo_core::{FormatPlugin, PluginConfig, PluginContext, Result, RheoError, SpineOptions};
use clap::{Parser, Subcommand};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

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
    project: ProjectConfig,
    /// Format names to compile (resolved from CLI flags and config)
    formats: Vec<String>,
    /// Output configuration with resolved build directory
    output_config: OutputConfig,
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
        Ok(vec!["html".to_string(), "epub".to_string(), "pdf".to_string()])
    }
}

/// Get all plugins by name
fn plugins_for_names(names: &[String]) -> Vec<Box<dyn FormatPlugin>> {
    let mut plugins: Vec<Box<dyn FormatPlugin>> = Vec::new();

    for name in names {
        match name.as_str() {
            "html" => plugins.push(Box::new(rheo_html::HtmlPlugin)),
            "pdf" => plugins.push(Box::new(rheo_pdf::PdfPlugin)),
            "epub" => plugins.push(Box::new(rheo_epub::EpubPlugin)),
            _ => warn!("unknown format: {}", name),
        }
    }

    plugins
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
    project: &ProjectConfig,
    cli_build_dir: Option<PathBuf>,
) -> Result<Option<PathBuf>> {
    if let Some(cli_path) = cli_build_dir {
        // Priority 1: CLI argument (resolve relative to current directory)
        let cwd = std::env::current_dir()
            .map_err(|e| RheoError::io(e, "getting current directory"))?;
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
            RheoError::project_config(format!("invalid .typ filename: {:?}", typ_file))
        })
}

/// Returns the set of files to compile for a given plugin based on its spine config.
/// If no spine is configured, returns all project files.
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
            let spine_files = rheo_core::reticulate::spine::generate_spine(&content_dir, Some(spine), false)?;
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
    project: &ProjectConfig,
    output_config: &OutputConfig,
    plugins: &[Box<dyn FormatPlugin>],
    mut world: Option<&'a mut RheoWorld>,
    format_name: Option<&'a str>,
) -> Result<()> {
    // Check for .typ files
    if project.typ_files.is_empty() {
        return Err(RheoError::project_config(
            "no .typ files found in project",
        ));
    }

    // Track success/failure per format for graceful degradation
    let mut results = CompilationResults::new();

    for plugin in plugins {
        let plugin_output_dir = output_config.dir_for_plugin(plugin.name());
        std::fs::create_dir_all(&plugin_output_dir).map_err(|e| {
            RheoError::io(
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
            let mut temp_world = RheoWorld::new(
                &compilation_root,
                project.typ_files.first().ok_or_else(|| {
                    RheoError::project_config("no .typ files found")
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
                    let mut fresh_world = RheoWorld::new(
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
            Err(RheoError::project_config(
                "some formats failed to compile".to_string(),
            ))
        } else {
            // Total failure - all formats failed
            Err(RheoError::project_config(
                "all formats failed or no files were compiled".to_string(),
            ))
        }
    } else {
        // All requested formats succeeded
        info!("compilation complete");
        Ok(())
    }
}

/// Initialize a new rheo project by copying template files
///
/// # Arguments
/// * `target_dir` - Path where the new project should be created
///
/// # Returns
/// * `Ok(())` if initialization succeeded
/// * `Err(RheoError)` if initialization failed
fn init_project(target_dir: &Path) -> Result<()> {
    // Check if target directory already exists
    if target_dir.exists() {
        return Err(RheoError::project_config(&format!(
            "directory '{}' already exists",
            target_dir.display()
        )));
    }

    // Create target directory
    fs::create_dir_all(target_dir)
        .map_err(|e| RheoError::io(e, "creating target directory"))?;

    // Template directory is relative to the rheo-cli crate's source directory
    // Use CARGO_MANIFEST_DIR which is set at compile time
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let template_dir = manifest_dir.join("../../src/templates/init");

    // Copy all template files recursively
    copy_template_recursive(&template_dir, target_dir)?;

    // Replace {{VERSION}} placeholder in rheo.toml
    let toml_path = target_dir.join("rheo.toml");
    let toml_content = fs::read_to_string(&toml_path)
        .map_err(|e| RheoError::io(e, "reading rheo.toml template"))?;
    let toml_content = toml_content.replace("{{VERSION}}", manifest_version::CURRENT);
    fs::write(&toml_path, toml_content)
        .map_err(|e| RheoError::io(e, "writing rheo.toml"))?;

    info!(
        path = %target_dir.display(),
        "initialized rheo project"
    );
    Ok(())
}

/// Recursively copy template directory contents
///
/// # Arguments
/// * `src` - Source template directory
/// * `dst` - Destination directory
fn copy_template_recursive(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src)
        .map_err(|e| RheoError::io(e, "reading template directory"))?
    {
        let entry = entry.map_err(|e| RheoError::io(e, "reading directory entry"))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            fs::create_dir_all(&dst_path)
                .map_err(|e| RheoError::io(e, "creating directory"))?;
            copy_template_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)
                .map_err(|e| RheoError::io(e, "copying file"))?;
        }
    }
    Ok(())
}

impl Cli {
    pub fn parse() -> Self {
        Parser::parse()
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
            Commands::Clean {
                path,
                config,
                build_dir,
            } => {
                info!(path = %path.display(), "loading project");
                let project = ProjectConfig::from_path(&path, config.as_deref())?;

                // Resolve build directory
                let resolved_build_dir = resolve_build_dir(&project, build_dir)?;

                let output_config = OutputConfig::new(&project.root, resolved_build_dir);
                info!(project = %project.name, "cleaning build artifacts");
                output_config.clean()?;
                info!(project = %project.name, "build artifacts removed");
                Ok(())
            }
            Commands::Init { path } => {
                init_project(&path)
            }
            Commands::Watch { .. } => {
                // TODO: Implement watch mode (needs watch module in rheo-core or cli)
                Err(RheoError::project_config("watch mode not yet implemented in the new architecture"))
            }
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
        let project = ProjectConfig::from_path(path, config_path)?;
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
        let output_config = OutputConfig::new(&project.root, resolved_build_dir);

        Ok(CompilationContext {
            project,
            formats,
            output_config,
        })
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
