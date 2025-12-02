use crate::compile::RheoCompileOptions;
use crate::config::{EpubOptions, HtmlOptions};
use crate::formats::{epub, html, pdf};
use crate::{FilterPatterns, OutputFormat, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

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

    /// Initialize a new Typst project from a template
    Init {
        /// Name of the new project
        name: String,

        /// Template type (book, thesis, blog, cv)
        #[arg(long, default_value = "book")]
        template: String,
    },

    /// List available example projects
    ListExamples,
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
    let mut formats = Vec::new();

    for &format in requested_formats {
        match format {
            OutputFormat::Html => {
                // HTML is always compiled per-file
                formats.push(format);
            }
            OutputFormat::Pdf => {
                // PDF is only compiled per-file if merge config is absent
                if config.pdf.merge.is_none() {
                    formats.push(format);
                }
            }
            OutputFormat::Epub => {
                // EPUB is never compiled per-file (always merged)
            }
        }
    }

    formats
}

/// Perform compilation for a project with specified formats
///
/// This is the core compilation logic used by both `compile` and `watch` commands.
///
/// # Arguments
/// * `project` - Project configuration with source files and assets
/// * `output_config` - Output directory configuration
/// * `formats` - List of formats to compile to
///
/// # Returns
/// * `Ok(())` if at least one format fully succeeded
/// * `Err` if all formats failed
fn perform_compilation(
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
    let mut pdf_succeeded = 0;
    let mut pdf_failed = 0;
    let mut html_succeeded = 0;
    let mut html_failed = 0;
    let mut epub_succeeded = 0;
    let mut epub_failed = 0;

    // Use current working directory as root for Typst world
    // This allows absolute imports like /src/typst/rheo.typ to work
    let repo_root = std::env::current_dir()
        .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;

    // Use content_dir as compilation root if configured, otherwise use project root
    // This allows files in subdirectories to reference files in parent directories
    let compilation_root = project
        .config
        .resolve_content_dir(&project.root)
        .unwrap_or_else(|| project.root.clone());

    // Determine which formats should be compiled per-file
    let per_file_formats = get_per_file_formats(&project.config, formats);

    for typ_file in &project.typ_files {
        let filename = get_output_filename(typ_file)?;

        // Compile to PDF (per-file mode)
        if per_file_formats.contains(&OutputFormat::Pdf) {
            let output_path = output_config.pdf_dir.join(&filename).with_extension("pdf");
            let options = RheoCompileOptions::new(typ_file, &output_path, &compilation_root, &repo_root);
            match pdf::compile_pdf_new(options, None) {
                Ok(_) => pdf_succeeded += 1,
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e, "PDF compilation failed");
                    pdf_failed += 1;
                }
            }
        }

        // Compile to HTML
        if per_file_formats.contains(&OutputFormat::Html) {
            let output_path = output_config
                .html_dir
                .join(&filename)
                .with_extension("html");
            let options = RheoCompileOptions::new(typ_file, &output_path, &compilation_root, &repo_root);
            match html::compile_html_new(options, HtmlOptions::default()) {
                Ok(_) => html_succeeded += 1,
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e, "HTML compilation failed");
                    html_failed += 1;
                }
            }
        }
    }

    // Copy assets for HTML
    if formats.contains(&OutputFormat::Html) {
        info!("copying HTML assets");
        // TODO: make this configurable via rheo.toml
        let html_filter =
            FilterPatterns::from_patterns(&["!**/*.typ".to_string(), "!img/**".to_string()])?;
        let content_dir = project.config.content_dir.as_deref().map(Path::new);

        if let Err(e) = crate::assets::copy_html_assets(
            &project.root,
            &output_config.html_dir,
            &html_filter,
            content_dir,
        ) {
            error!(error = %e, "failed to copy HTML assets");
        }
    }

    // Generate merged PDF if configured
    if formats.contains(&OutputFormat::Pdf) && project.config.pdf.merge.is_some() {
        let pdf_filename = format!("{}.pdf", project.name);
        let pdf_path = output_config.pdf_dir.join(&pdf_filename);

        let options = RheoCompileOptions::new(PathBuf::new(), &pdf_path, &compilation_root, &repo_root);
        match pdf::compile_pdf_new(options, Some(&project.config.pdf)) {
            Ok(_) => {
                pdf_succeeded = 1;
                info!(output = %pdf_path.display(), "PDF merge complete");
            }
            Err(e) => {
                error!(error = %e, "PDF merge failed");
                pdf_failed = 1;
            }
        }
    }

    // Generate EPUB if requested
    if formats.contains(&OutputFormat::Epub) {
        let epub_filename = format!("{}.epub", project.name);
        let epub_path = output_config.epub_dir.join(&epub_filename);

        let options = RheoCompileOptions::new(PathBuf::new(), &epub_path, &compilation_root, &repo_root);
        let epub_options = EpubOptions::from(&project.config.epub);
        match epub::compile_epub_new(options, epub_options) {
            Ok(_) => {
                epub_succeeded += 1;
                info!(output = %epub_path.display(), "EPUB generation complete");
            }
            Err(e) => {
                error!(error = %e, "EPUB generation failed");
                epub_failed += 1;
            }
        }
    }

    // Report results with per-format summary
    let total_files = project.typ_files.len();

    // Log format-specific results
    if formats.contains(&OutputFormat::Pdf) {
        if pdf_failed > 0 {
            warn!(
                failed = pdf_failed,
                succeeded = pdf_succeeded,
                total = total_files,
                "PDF compilation"
            );
        } else {
            info!(
                succeeded = pdf_succeeded,
                total = total_files,
                "PDF compilation complete"
            );
        }
    }

    if formats.contains(&OutputFormat::Html) {
        if html_failed > 0 {
            warn!(
                failed = html_failed,
                succeeded = html_succeeded,
                total = total_files,
                "HTML compilation"
            );
        } else {
            info!(
                succeeded = html_succeeded,
                total = total_files,
                "HTML compilation complete"
            );
        }
    }

    if formats.contains(&OutputFormat::Epub) {
        if epub_failed > 0 {
            warn!(
                failed = epub_failed,
                succeeded = epub_succeeded,
                total = total_files,
                "EPUB compilation"
            );
        } else {
            info!(
                succeeded = epub_succeeded,
                total = total_files,
                "EPUB compilation complete"
            );
        }
    }

    // Graceful degradation: succeed if ANY format fully succeeded
    let pdf_fully_succeeded =
        formats.contains(&OutputFormat::Pdf) && pdf_failed == 0 && pdf_succeeded > 0;
    let html_fully_succeeded =
        formats.contains(&OutputFormat::Html) && html_failed == 0 && html_succeeded > 0;
    let epub_fully_succeeded =
        formats.contains(&OutputFormat::Epub) && epub_failed == 0 && epub_succeeded > 0;

    if pdf_fully_succeeded || html_fully_succeeded || epub_fully_succeeded {
        // At least one format succeeded completely
        if pdf_failed > 0 || html_failed > 0 || epub_failed > 0 {
            info!("compilation succeeded with warnings (some formats failed)");
        } else {
            info!("compilation succeeded");
        }
        Ok(())
    } else {
        // All requested formats had failures
        let total_failed = pdf_failed + html_failed + epub_failed;
        Err(crate::RheoError::project_config(format!(
            "all formats failed: {} file(s) could not be compiled",
            total_failed
        )))
    }
}

/// Perform compilation for a project with specified formats using an existing World (for watch mode).
///
/// This version reuses an existing RheoWorld instance, enabling incremental compilation
/// through Typst's comemo caching system. The World is updated for each file via set_main()
/// and reset() before compilation.
///
/// # Arguments
/// * `world` - Mutable reference to RheoWorld for reuse across compilations
/// * `project` - Project configuration with source files and assets
/// * `output_config` - Output directory configuration
/// * `formats` - List of formats to compile to
///
/// # Returns
/// * `Ok(())` if at least one format fully succeeded
/// * `Err` if all formats failed
fn perform_compilation_incremental(
    world: &mut crate::world::RheoWorld,
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
    let mut pdf_succeeded = 0;
    let mut pdf_failed = 0;
    let mut html_succeeded = 0;
    let mut html_failed = 0;

    // Determine which formats should be compiled per-file
    let per_file_formats = get_per_file_formats(&project.config, formats);

    for typ_file in &project.typ_files {
        let filename = get_output_filename(typ_file)?;

        // Update world for this file and reset cache
        world.set_main(typ_file)?;
        world.reset();

        // Compile to PDF (per-file mode)
        if per_file_formats.contains(&OutputFormat::Pdf) {
            let output_path = output_config.pdf_dir.join(&filename).with_extension("pdf");
            let options = RheoCompileOptions::incremental(
                typ_file,
                &output_path,
                &project.root,
                PathBuf::new(),
                world
            );
            match pdf::compile_pdf_new(options, None) {
                Ok(_) => pdf_succeeded += 1,
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e, "PDF compilation failed");
                    pdf_failed += 1;
                }
            }
        }

        // Compile to HTML
        if per_file_formats.contains(&OutputFormat::Html) {
            let output_path = output_config
                .html_dir
                .join(&filename)
                .with_extension("html");
            let options = RheoCompileOptions::incremental(
                typ_file,
                &output_path,
                &project.root,
                PathBuf::new(),
                world
            );
            match html::compile_html_new(options, HtmlOptions::default()) {
                Ok(_) => html_succeeded += 1,
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e, "HTML compilation failed");
                    html_failed += 1;
                }
            }
        }
    }

    // Copy assets for HTML
    if formats.contains(&OutputFormat::Html) {
        info!("copying HTML assets");
        // TODO: make this configurable via rheo.toml
        let html_filter =
            FilterPatterns::from_patterns(&["!**/*.typ".to_string(), "!img/**".to_string()])?;
        let content_dir = project.config.content_dir.as_deref().map(Path::new);

        if let Err(e) = crate::assets::copy_html_assets(
            &project.root,
            &output_config.html_dir,
            &html_filter,
            content_dir,
        ) {
            error!(error = %e, "failed to copy HTML assets");
        }
    }

    // Generate merged PDF if configured
    if formats.contains(&OutputFormat::Pdf) && project.config.pdf.merge.is_some() {
        let pdf_filename = format!("{}.pdf", project.name);
        let pdf_path = output_config.pdf_dir.join(&pdf_filename);

        // Get compilation root for PDF merge
        let compilation_root = project
            .config
            .resolve_content_dir(&project.root)
            .unwrap_or_else(|| project.root.clone());

        let options = RheoCompileOptions::incremental(
            PathBuf::new(),
            &pdf_path,
            &compilation_root,
            PathBuf::new(),
            world
        );
        match pdf::compile_pdf_new(options, Some(&project.config.pdf)) {
            Ok(_) => {
                pdf_succeeded = 1;
                info!(output = %pdf_path.display(), "PDF merge complete");
            }
            Err(e) => {
                error!(error = %e, "PDF merge failed");
                pdf_failed = 1;
            }
        }
    }

    let total_files = project.typ_files.len();

    // Log format-specific results
    if formats.contains(&OutputFormat::Pdf) {
        if pdf_failed > 0 {
            warn!(
                failed = pdf_failed,
                succeeded = pdf_succeeded,
                total = total_files,
                "PDF compilation"
            );
        } else {
            info!(
                succeeded = pdf_succeeded,
                total = total_files,
                "PDF compilation complete"
            );
        }
    }

    if formats.contains(&OutputFormat::Html) {
        if html_failed > 0 {
            warn!(
                failed = html_failed,
                succeeded = html_succeeded,
                total = total_files,
                "HTML compilation"
            );
        } else {
            info!(
                succeeded = html_succeeded,
                total = total_files,
                "HTML compilation complete"
            );
        }
    }

    // Graceful degradation: succeed if ANY format fully succeeded
    let pdf_fully_succeeded =
        formats.contains(&OutputFormat::Pdf) && pdf_failed == 0 && pdf_succeeded > 0;
    let html_fully_succeeded =
        formats.contains(&OutputFormat::Html) && html_failed == 0 && html_succeeded > 0;

    if pdf_fully_succeeded || html_fully_succeeded {
        // At least one format succeeded completely
        if pdf_failed > 0 || html_failed > 0 {
            info!("compilation succeeded with warnings (some formats failed)");
        } else {
            info!("compilation succeeded");
        }
        Ok(())
    } else {
        // All requested formats had failures
        let total_failed = pdf_failed + html_failed;
        Err(crate::RheoError::project_config(format!(
            "all formats failed: {} file(s) could not be compiled",
            total_failed
        )))
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
                // Detect project configuration first to get config defaults
                info!(path = %path.display(), "detecting project configuration");
                let project = crate::project::ProjectConfig::from_path(&path, config.as_deref())?;
                info!(name = %project.name, files = project.typ_files.len(), "detected project");

                // Determine which formats to compile using CLI flags or config defaults
                let flags = FormatFlags { pdf, html, epub };
                let formats = determine_formats(flags, &project.config.formats)?;

                // Resolve build_dir with priority: CLI > config > default
                let resolved_build_dir = if let Some(cli_path) = build_dir {
                    let cwd = std::env::current_dir()
                        .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;
                    info!(cli_build_dir = %cli_path.display(), "using build directory from CLI flag");
                    Some(resolve_path(&cwd, &cli_path))
                } else if let Some(config_path) = &project.config.build_dir {
                    let resolved = resolve_path(&project.root, Path::new(config_path));
                    info!(config_build_dir = %resolved.display(), "using build directory from rheo.toml");
                    Some(resolved)
                } else {
                    None
                };

                // Create output directories
                let output_config =
                    crate::output::OutputConfig::new(&project.root, resolved_build_dir);
                output_config.create_dirs()?;

                // Perform compilation
                perform_compilation(&project, &output_config, &formats)
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
                // Warn if EPUB requested
                if epub {
                    warn!("EPUB format is not yet supported and will be ignored");
                }

                // Detect project configuration first to get config defaults
                info!(path = %path.display(), "detecting project configuration");
                let project = crate::project::ProjectConfig::from_path(&path, config.as_deref())?;
                info!(name = %project.name, files = project.typ_files.len(), "detected project");

                // Determine which formats to compile using CLI flags or config defaults
                let flags = FormatFlags { pdf, html, epub };
                let formats = determine_formats(flags, &project.config.formats)?;

                // Log TODOs for --open with formats that aren't ready yet
                if open {
                    if formats.contains(&OutputFormat::Pdf) {
                        info!(
                            "TODO: PDF opening not yet implemented (need to decide on multi-file handling)"
                        );
                    }
                    if formats.contains(&OutputFormat::Epub) {
                        info!(
                            "TODO: EPUB opening not yet implemented (need bene viewer integration)"
                        );
                    }
                }

                // Resolve build_dir with priority: CLI > config > default
                let resolved_build_dir = if let Some(cli_path) = build_dir {
                    let cwd = std::env::current_dir()
                        .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;
                    info!(cli_build_dir = %cli_path.display(), "using build directory from CLI flag");
                    Some(resolve_path(&cwd, &cli_path))
                } else if let Some(config_path) = &project.config.build_dir {
                    let resolved = resolve_path(&project.root, Path::new(config_path));
                    info!(config_build_dir = %resolved.display(), "using build directory from rheo.toml");
                    Some(resolved)
                } else {
                    None
                };

                // Create output directories
                let output_config =
                    crate::output::OutputConfig::new(&project.root, resolved_build_dir);
                output_config.create_dirs()?;

                // Perform initial compilation
                info!("performing initial compilation");
                if let Err(e) = perform_compilation(&project, &output_config, &formats) {
                    warn!(error = %e, "initial compilation failed, continuing to watch");
                }

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

                // Set up file watcher with interior mutability for project and world updates
                use std::cell::RefCell;
                let project_cell = RefCell::new(project);

                // Create RheoWorld for incremental compilation (reused across file changes)
                let repo_root = std::env::current_dir()
                    .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;
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
                let remove_typ_links = !formats.contains(&OutputFormat::Html);
                let world = crate::world::RheoWorld::new(
                    &compilation_root,
                    initial_main,
                    &repo_root,
                    remove_typ_links,
                )?;
                drop(borrowed_project); // Release borrow before moving into RefCell

                let world_cell = RefCell::new(world);

                info!("starting file watcher");
                crate::watch::watch_project(&project_cell.borrow(), |event| {
                    let result = match event {
                        crate::watch::WatchEvent::FilesChanged => {
                            info!("files changed, recompiling");
                            perform_compilation_incremental(
                                &mut world_cell.borrow_mut(),
                                &project_cell.borrow(),
                                &output_config,
                                &formats,
                            )
                        }
                        crate::watch::WatchEvent::ConfigChanged => {
                            info!("config changed, reloading project");
                            // Reload project configuration
                            match crate::project::ProjectConfig::from_path(&path, config.as_deref())
                            {
                                Ok(new_project) => {
                                    *project_cell.borrow_mut() = new_project;
                                    let borrowed = project_cell.borrow();
                                    info!(name = %borrowed.name, files = borrowed.typ_files.len(), "reloaded project");

                                    // Recreate World with new configuration
                                    let new_compilation_root = borrowed
                                        .config
                                        .resolve_content_dir(&borrowed.root)
                                        .unwrap_or_else(|| borrowed.root.clone());
                                    let new_initial_main =
                                        borrowed.typ_files.first().ok_or_else(|| {
                                            crate::RheoError::project_config("no .typ files found")
                                        })?;

                                    // Use same remove_typ_links setting as initial World creation
                                    let remove_typ_links = !formats.contains(&OutputFormat::Html);
                                    match crate::world::RheoWorld::new(
                                        &new_compilation_root,
                                        new_initial_main,
                                        &repo_root,
                                        remove_typ_links,
                                    ) {
                                        Ok(new_world) => {
                                            *world_cell.borrow_mut() = new_world;
                                            perform_compilation_incremental(
                                                &mut world_cell.borrow_mut(),
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
                info!(path = %path.display(), "detecting project for cleanup");
                let project = crate::project::ProjectConfig::from_path(&path, config.as_deref())?;

                // Resolve build_dir with priority: CLI > config > default
                let resolved_build_dir = if let Some(cli_path) = build_dir {
                    let cwd = std::env::current_dir()
                        .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;
                    info!(cli_build_dir = %cli_path.display(), "using build directory from CLI flag");
                    Some(resolve_path(&cwd, &cli_path))
                } else if let Some(config_path) = &project.config.build_dir {
                    let resolved = resolve_path(&project.root, Path::new(config_path));
                    info!(config_build_dir = %resolved.display(), "using build directory from rheo.toml");
                    Some(resolved)
                } else {
                    None
                };

                let output_config =
                    crate::output::OutputConfig::new(&project.root, resolved_build_dir);
                info!(project = %project.name, "cleaning project build artifacts");
                output_config.clean()?;
                info!(project = %project.name, "cleaned project build artifacts");
                Ok(())
            }
            Commands::Init { name, template } => {
                info!(name, template, "initializing new project");
                // TODO: Implement init logic
                Ok(())
            }
            Commands::ListExamples => {
                info!("listing available example projects");
                // TODO: Implement list-examples logic
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
