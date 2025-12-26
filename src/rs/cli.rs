use crate::CompilationResults;
use crate::compile::RheoCompileOptions;
use crate::config::{EpubOptions, HtmlOptions};
use crate::formats::{epub, html, pdf};
use crate::{OutputFormat, Result, open_all_files_in_folder};
use clap::{Parser, Subcommand};
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

/// Compilation mode for perform_compilation
enum CompilationMode<'a> {
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

/// Determine which formats should be compiled for a given file.
///
/// Logic:
/// - HTML: Always compile (one HTML per .typ file)
/// - PDF: Only if pdf.merge is NOT configured (merged PDF is handled separately)
/// - EPUB: Never (EPUB is always merged and handled separately)
fn get_per_file_formats(
    config: &crate::RheoConfig,
    requested_formats: &[OutputFormat],
) -> Vec<OutputFormat> {
    requested_formats
        .iter()
        .copied()
        .filter(|format| format.supports_per_file(config))
        .collect()
}

/// Perform compilation for a project with specified formats
///
/// This is the unified compilation logic that supports both fresh and incremental compilation
/// based on the CompilationMode parameter.
///
/// # Arguments
/// * `mode` - Compilation mode (Fresh or Incremental)
/// * `project` - Project configuration with source files and assets
/// * `output_config` - Output directory configuration
/// * `formats` - List of formats to compile to
///
/// # Returns
/// * `Ok(())` if at least one format fully succeeded
/// * `Err` if all formats failed
fn perform_compilation<'a>(
    mut mode: CompilationMode<'a>,
    project: &crate::project::ProjectConfig,
    output_config: &crate::output::OutputConfig,
    formats: &[OutputFormat],
) -> Result<()> {
    // Check for .typ files
    if project.typ_files.is_empty() {
        return Err(crate::RheoError::project_config(
            "no .typ files found in project",
        ));
    }

    // Track success/failure per format for graceful degradation
    let mut results = CompilationResults::new();

    // Determine which formats should be compiled per-file
    let per_file_formats = get_per_file_formats(&project.config, formats);

    // Copy HTML assets (style.css) if HTML compilation is requested
    if per_file_formats.contains(&OutputFormat::Html) {
        output_config.copy_html_assets(project.style_css.as_deref())?;
    }

    // Per-file compilation
    for typ_file in &project.typ_files {
        let filename = get_output_filename(typ_file)?;

        // For incremental mode, update world for this file
        if let CompilationMode::Incremental { world } = &mut mode {
            world.set_main(typ_file)?;
            world.reset();
        }

        // Compile to PDF (per-file mode)
        if per_file_formats.contains(&OutputFormat::Pdf) {
            let output_path = output_config.pdf_dir.join(&filename).with_extension("pdf");
            let options = match &mode {
                CompilationMode::Fresh { root } => {
                    RheoCompileOptions::new(typ_file, &output_path, root)
                }
                CompilationMode::Incremental { .. } => {
                    if let CompilationMode::Incremental { world } = &mut mode {
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
            match pdf::compile_pdf_new(options, None) {
                Ok(_) => results.record_success(OutputFormat::Pdf),
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e, "PDF compilation failed");
                    results.record_failure(OutputFormat::Pdf);
                }
            }
        }

        // Compile to HTML
        if per_file_formats.contains(&OutputFormat::Html) {
            let output_path = output_config
                .html_dir
                .join(&filename)
                .with_extension("html");
            let options = match &mode {
                CompilationMode::Fresh { root } => {
                    RheoCompileOptions::new(typ_file, &output_path, root)
                }
                CompilationMode::Incremental { .. } => {
                    if let CompilationMode::Incremental { world } = &mut mode {
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
            // Get HTML options from config
            let html_options = HtmlOptions {
                stylesheets: project.config.html.stylesheets.clone(),
                fonts: project.config.html.fonts.clone(),
            };
            match html::compile_html_new(options, html_options) {
                Ok(_) => results.record_success(OutputFormat::Html),
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e, "HTML compilation failed");
                    results.record_failure(OutputFormat::Html);
                }
            }
        }
    }

    // Generate merged PDF if configured
    if formats.contains(&OutputFormat::Pdf) && project.config.pdf.merge.is_some() {
        let pdf_filename = format!("{}.pdf", project.name);
        let pdf_path = output_config.pdf_dir.join(&pdf_filename);

        let compilation_root = project
            .config
            .resolve_content_dir(&project.root)
            .unwrap_or_else(|| project.root.clone());

        let options = match &mode {
            CompilationMode::Fresh { root: _ } => {
                RheoCompileOptions::new(PathBuf::new(), &pdf_path, &compilation_root)
            }
            CompilationMode::Incremental { .. } => {
                if let CompilationMode::Incremental { world } = &mut mode {
                    RheoCompileOptions::incremental(
                        PathBuf::new(),
                        &pdf_path,
                        &compilation_root,
                        world,
                    )
                } else {
                    unreachable!()
                }
            }
        };
        match pdf::compile_pdf_new(options, Some(&project.config.pdf)) {
            Ok(_) => {
                results.record_success(OutputFormat::Pdf);
                info!(output = %pdf_path.display(), "PDF merge complete");
            }
            Err(e) => {
                error!(error = %e, "PDF merge failed");
                results.record_failure(OutputFormat::Pdf);
            }
        }
    }

    // Generate EPUB if requested
    if formats.contains(&OutputFormat::Epub) {
        let epub_filename = format!("{}.epub", project.name);
        let epub_path = output_config.epub_dir.join(&epub_filename);

        let compilation_root = project
            .config
            .resolve_content_dir(&project.root)
            .unwrap_or_else(|| project.root.clone());

        let options = match &mode {
            CompilationMode::Fresh { root: _ } => {
                RheoCompileOptions::new(PathBuf::new(), &epub_path, &compilation_root)
            }
            CompilationMode::Incremental { .. } => {
                if let CompilationMode::Incremental { world } = &mut mode {
                    RheoCompileOptions::incremental(
                        PathBuf::new(),
                        &epub_path,
                        &compilation_root,
                        world,
                    )
                } else {
                    unreachable!()
                }
            }
        };
        let epub_options = EpubOptions::from(&project.config.epub);
        match epub::compile_epub_new(options, epub_options) {
            Ok(_) => {
                results.record_success(OutputFormat::Epub);
                info!(output = %epub_path.display(), "EPUB generation complete");
            }
            Err(e) => {
                error!(error = %e, "EPUB generation failed");
                results.record_failure(OutputFormat::Epub);
            }
        }
    }

    // Report results with per-format summary
    results.log_summary(formats);

    // Graceful degradation: succeed if ANY requested format fully succeeded
    let any_format_succeeded = formats.iter().any(|fmt| {
        let result = results.get(*fmt);
        result.succeeded > 0 && result.failed == 0
    });

    if any_format_succeeded {
        // At least one format succeeded completely
        if results.has_failures() {
            info!("compilation complete (some formats had errors)");
        } else {
            info!("compilation complete");
        }
        Ok(())
    } else {
        // All requested formats had failures or no compilations occurred
        Err(crate::RheoError::project_config(
            "all formats failed or no files were compiled".to_string(),
        ))
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

                // Create compilation mode (Fresh)
                let mode = CompilationMode::Fresh {
                    root: ctx.compilation_root,
                };

                // Perform compilation
                perform_compilation(mode, &ctx.project, &ctx.output_config, &ctx.formats)
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

                // Perform initial compilation (Fresh mode)
                info!("compiling project");
                let mode = CompilationMode::Fresh {
                    root: ctx.compilation_root.clone(),
                };
                if let Err(e) =
                    perform_compilation(mode, &ctx.project, &ctx.output_config, &ctx.formats)
                {
                    warn!(error = %e, "initial compilation failed, continuing to watch");
                }

                // Destructure context for use in watch loop
                let CompilationContext {
                    project,
                    formats,
                    output_config,
                    compilation_root: _,
                } = ctx;

                // Start web server if --open and HTML is in formats
                let server_info = if open && formats.contains(&OutputFormat::Html) {
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

                // Open PDF(s) if --open and PDF is in formats
                if open && formats.contains(&OutputFormat::Pdf) {
                    let pdf_dir = output_config.pdf_dir.clone();
                    open_all_files_in_folder(pdf_dir, OutputFormat::Pdf)?;
                }

                // Open EPUB if --open and EPUB is in formats
                if open && formats.contains(&OutputFormat::Epub) {
                    let epub_dir = output_config.epub_dir.clone();
                    open_all_files_in_folder(epub_dir, OutputFormat::Epub)?;
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

                // For watch mode: if compiling HTML, keep .typ links for transformation
                // If compiling only PDF/EPUB, remove .typ links at source level
                let output_format = if formats.contains(&OutputFormat::Html) {
                    Some(OutputFormat::Html)
                } else {
                    None
                };
                let world =
                    crate::world::RheoWorld::new(&compilation_root, initial_main, output_format)?;
                drop(borrowed_project); // Release borrow before moving into RefCell

                let world_cell = RefCell::new(world);

                info!("watching for changes");
                crate::watch::watch_project(&project_cell.borrow(), |event| {
                    let result = match event {
                        crate::watch::WatchEvent::FilesChanged => {
                            info!("change detected, recompiling");
                            let mode = CompilationMode::Incremental {
                                world: &mut world_cell.borrow_mut(),
                            };
                            perform_compilation(
                                mode,
                                &project_cell.borrow(),
                                &output_config,
                                &formats,
                            )
                        }
                        crate::watch::WatchEvent::ConfigChanged => {
                            info!("configuration changed, reloading");
                            // Reload project configuration
                            match crate::project::ProjectConfig::from_path(&path, config.as_deref())
                            {
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
                                            crate::RheoError::project_config("no .typ files found")
                                        })?;

                                    // Use same output_format setting as initial World creation
                                    let output_format = if formats.contains(&OutputFormat::Html) {
                                        Some(OutputFormat::Html)
                                    } else {
                                        None
                                    };
                                    match crate::world::RheoWorld::new(
                                        &new_compilation_root,
                                        new_initial_main,
                                        output_format,
                                    ) {
                                        Ok(new_world) => {
                                            *world_cell.borrow_mut() = new_world;
                                            let mode = CompilationMode::Incremental {
                                                world: &mut world_cell.borrow_mut(),
                                            };
                                            perform_compilation(
                                                mode,
                                                &borrowed,
                                                &output_config,
                                                &formats,
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
                })?;

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
