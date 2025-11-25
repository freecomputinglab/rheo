use crate::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

/// Output format for compilation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Pdf,
    Html,
    Epub,
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
        /// Path to the project directory
        path: PathBuf,

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
        /// Path to the project directory
        path: PathBuf,

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

    /// Clean build artifacts
    Clean {
        /// Clean all build artifacts (not just for a specific project)
        #[arg(long)]
        all: bool,
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

/// Determine which formats should be compiled for a given file
/// based on per-format exclusion patterns and requested formats
fn get_file_formats(
    file: &Path,
    project_root: &Path,
    config: &crate::RheoConfig,
    requested_formats: &[OutputFormat],
) -> Result<Vec<OutputFormat>> {
    // Make path relative to project root for matching
    let relative_path = file.strip_prefix(project_root)
        .map_err(|_| crate::RheoError::path(
            file,
            format!("file is not within project root {}", project_root.display())
        ))?;

    // Build exclusion sets for each format
    let html_exclusions = config.build_html_exclusion_set()?;
    let pdf_exclusions = config.build_pdf_exclusion_set()?;
    let epub_exclusions = config.build_epub_exclusion_set()?;

    let mut formats = Vec::new();

    for &format in requested_formats {
        let should_compile = match format {
            OutputFormat::Pdf => !pdf_exclusions.is_match(relative_path),
            OutputFormat::Html => !html_exclusions.is_match(relative_path),
            OutputFormat::Epub => !epub_exclusions.is_match(relative_path),
        };

        if should_compile {
            formats.push(format);
        }
    }

    Ok(formats)
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

    // Use current working directory as root for Typst world
    // This allows absolute imports like /src/typst/rheo.typ to work
    let repo_root = std::env::current_dir()
        .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;

    // Use content_dir as compilation root if configured, otherwise use project root
    // This allows files in subdirectories to reference files in parent directories
    let compilation_root = project.config.resolve_content_dir(&project.root)
        .unwrap_or_else(|| project.root.clone());

    for typ_file in &project.typ_files {
        let filename = get_output_filename(typ_file)?;

        // Determine which formats this file should be compiled to
        let file_formats = get_file_formats(
            typ_file,
            &project.root,
            &project.config,
            formats,
        )?;

        // Compile to PDF
        if file_formats.contains(&OutputFormat::Pdf) {
            let output_path =
                output_config.pdf_dir.join(&filename).with_extension("pdf");
            match crate::compile::compile_pdf(
                typ_file,
                &output_path,
                &compilation_root,
                &repo_root,
            ) {
                Ok(_) => pdf_succeeded += 1,
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e, "PDF compilation failed");
                    pdf_failed += 1;
                }
            }
        }

        // Compile to HTML
        if file_formats.contains(&OutputFormat::Html) {
            let output_path = output_config
                .html_dir
                .join(&filename)
                .with_extension("html");
            match crate::compile::compile_html(
                typ_file,
                &output_path,
                &compilation_root,
                &repo_root,
            ) {
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
        // Use new glob-based static file copying if patterns are configured
        let static_patterns = project.config.get_static_files_patterns();
        if !static_patterns.is_empty() {
            info!("copying static files using configured patterns");
            let content_dir = project.config.content_dir.as_deref().map(Path::new);
            if let Err(e) = crate::assets::copy_static_files(
                &project.root,
                &output_config.html_dir,
                static_patterns,
                content_dir,
            ) {
                warn!(error = %e, "failed to copy static files, continuing");
            }
        } else {
            // Fall back to legacy behavior for backward compatibility
            info!("copying assets for HTML output");
            if let Err(e) = crate::assets::copy_css(&project.root, &output_config.html_dir)
            {
                warn!(error = %e, "failed to copy CSS, continuing");
            }
            if let Err(e) =
                crate::assets::copy_images(&project.root, &output_config.html_dir)
            {
                warn!(error = %e, "failed to copy images, continuing");
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

    for typ_file in &project.typ_files {
        let filename = get_output_filename(typ_file)?;

        // Determine which formats this file should be compiled to
        let file_formats = get_file_formats(
            typ_file,
            &project.root,
            &project.config,
            formats,
        )?;

        // Update world for this file and reset cache
        world.set_main(typ_file)?;
        world.reset();

        // Compile to PDF
        if file_formats.contains(&OutputFormat::Pdf) {
            let output_path =
                output_config.pdf_dir.join(&filename).with_extension("pdf");
            match crate::compile::compile_pdf_incremental(world, &output_path) {
                Ok(_) => pdf_succeeded += 1,
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e, "PDF compilation failed");
                    pdf_failed += 1;
                }
            }
        }

        // Compile to HTML
        if file_formats.contains(&OutputFormat::Html) {
            let output_path = output_config
                .html_dir
                .join(&filename)
                .with_extension("html");
            match crate::compile::compile_html_incremental(world, typ_file, &output_path, &project.root) {
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
        // Use new glob-based static file copying if patterns are configured
        let static_patterns = project.config.get_static_files_patterns();
        if !static_patterns.is_empty() {
            info!("copying static files using configured patterns");
            let content_dir = project.config.content_dir.as_deref().map(Path::new);
            if let Err(e) = crate::assets::copy_static_files(
                &project.root,
                &output_config.html_dir,
                static_patterns,
                content_dir,
            ) {
                error!(error = %e, "failed to copy static files");
            } else {
                let count = static_patterns.len();
                info!(count, "copied static files");
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
                pdf,
                html,
                epub,
            } => {
                // Warn if EPUB requested
                if epub {
                    warn!("EPUB format is not yet supported and will be ignored");
                }

                // Determine which formats to compile
                // Default = PDF + HTML (EPUB not yet supported)
                let formats = if !pdf && !html {
                    vec![OutputFormat::Pdf, OutputFormat::Html]
                } else {
                    let mut formats = Vec::new();
                    if pdf {
                        formats.push(OutputFormat::Pdf);
                    }
                    if html {
                        formats.push(OutputFormat::Html);
                    }
                    formats
                };

                // Detect project configuration
                info!(path = %path.display(), "detecting project configuration");
                let project = crate::project::ProjectConfig::from_path(&path)?;
                info!(name = %project.name, files = project.typ_files.len(), "detected project");

                // Create output directories
                let output_config = crate::output::OutputConfig::new(&project.name);
                output_config.create_dirs()?;

                // Perform compilation
                perform_compilation(&project, &output_config, &formats)
            }
            Commands::Watch {
                path,
                pdf,
                html,
                epub,
                open,
            } => {
                // Warn if EPUB requested
                if epub {
                    warn!("EPUB format is not yet supported and will be ignored");
                }

                // Log TODOs for --open with formats that aren't ready yet
                if open {
                    if pdf || (!pdf && !html) {
                        info!("TODO: PDF opening not yet implemented (need to decide on multi-file handling)");
                    }
                    if epub {
                        info!("TODO: EPUB opening not yet implemented (need bene viewer integration)");
                    }
                }

                // Determine which formats to compile
                // --open alone compiles ALL formats (PDF + HTML)
                // --open with specific flags only compiles those formats
                // Without --open, default = PDF + HTML
                let formats = if !pdf && !html {
                    vec![OutputFormat::Pdf, OutputFormat::Html]
                } else {
                    let mut formats = Vec::new();
                    if pdf {
                        formats.push(OutputFormat::Pdf);
                    }
                    if html {
                        formats.push(OutputFormat::Html);
                    }
                    formats
                };

                // Detect project configuration
                info!(path = %path.display(), "detecting project configuration");
                let project = crate::project::ProjectConfig::from_path(&path)?;
                info!(name = %project.name, files = project.typ_files.len(), "detected project");

                // Create output directories
                let output_config = crate::output::OutputConfig::new(&project.name);
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
                    let (server_handle, reload_tx, server_url) = runtime.block_on(async {
                        crate::server::start_server(html_dir, 3000).await
                    })?;

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
                let compilation_root = borrowed_project.config.resolve_content_dir(&borrowed_project.root)
                    .unwrap_or_else(|| borrowed_project.root.clone());

                // Use first .typ file as initial main (will be updated for each compilation)
                let initial_main = borrowed_project.typ_files.first()
                    .ok_or_else(|| crate::RheoError::project_config("no .typ files found"))?;

                // For watch mode: if compiling HTML, keep .typ links for transformation
                // If compiling only PDF/EPUB, remove .typ links at source level
                let remove_typ_links = !formats.contains(&OutputFormat::Html);
                let world = crate::world::RheoWorld::new(&compilation_root, initial_main, &repo_root, remove_typ_links)?;
                drop(borrowed_project);  // Release borrow before moving into RefCell

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
                                &formats
                            )
                        }
                        crate::watch::WatchEvent::ConfigChanged => {
                            info!("config changed, reloading project");
                            // Reload project configuration
                            match crate::project::ProjectConfig::from_path(&path) {
                                Ok(new_project) => {
                                    *project_cell.borrow_mut() = new_project;
                                    let borrowed = project_cell.borrow();
                                    info!(name = %borrowed.name, files = borrowed.typ_files.len(), "reloaded project");

                                    // Recreate World with new configuration
                                    let new_compilation_root = borrowed.config.resolve_content_dir(&borrowed.root)
                                        .unwrap_or_else(|| borrowed.root.clone());
                                    let new_initial_main = borrowed.typ_files.first()
                                        .ok_or_else(|| crate::RheoError::project_config("no .typ files found"))?;

                                    // Use same remove_typ_links setting as initial World creation
                                    let remove_typ_links = !formats.contains(&OutputFormat::Html);
                                    match crate::world::RheoWorld::new(&new_compilation_root, new_initial_main, &repo_root, remove_typ_links) {
                                        Ok(new_world) => {
                                            *world_cell.borrow_mut() = new_world;
                                            perform_compilation_incremental(
                                                &mut world_cell.borrow_mut(),
                                                &borrowed,
                                                &output_config,
                                                &formats
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
            Commands::Clean { all } => {
                if all {
                    info!("cleaning all build artifacts");
                    crate::output::OutputConfig::clean_all()?;
                    info!("cleaned entire build/ directory");
                } else {
                    // Detect project from current directory
                    let current_dir = std::env::current_dir()
                        .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;

                    info!(path = %current_dir.display(), "detecting project for cleanup");
                    let project = crate::project::ProjectConfig::from_path(&current_dir)?;

                    let output_config = crate::output::OutputConfig::new(&project.name);
                    info!(project = %project.name, "cleaning project build artifacts");
                    output_config.clean_project()?;
                    info!(project = %project.name, "cleaned project build artifacts");
                }
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

    fn build_test_config(
        html_exclude: &[&str],
        pdf_exclude: &[&str],
        epub_exclude: &[&str],
    ) -> crate::RheoConfig {
        let mut config = crate::RheoConfig::default();
        config.html.exclude = html_exclude.iter().map(|s| s.to_string()).collect();
        config.pdf.exclude = pdf_exclude.iter().map(|s| s.to_string()).collect();
        config.epub.exclude = epub_exclude.iter().map(|s| s.to_string()).collect();
        config
    }

    #[test]
    fn test_no_exclusions_compiles_all_formats() {
        let config = build_test_config(&[], &[], &[]);
        let project_root = std::path::PathBuf::from("/tmp/project");
        let file = project_root.join("content/test.typ");
        let requested = vec![OutputFormat::Pdf, OutputFormat::Html];

        let formats = get_file_formats(&file, &project_root, &config, &requested).unwrap();

        assert_eq!(formats.len(), 2);
        assert!(formats.contains(&OutputFormat::Pdf));
        assert!(formats.contains(&OutputFormat::Html));
    }

    #[test]
    fn test_pdf_exclusion_excludes_pdf() {
        let config = build_test_config(&[], &["content/index.typ"], &[]);
        let project_root = std::path::PathBuf::from("/tmp/project");
        let file = project_root.join("content/index.typ");
        let requested = vec![OutputFormat::Pdf, OutputFormat::Html];

        let formats = get_file_formats(&file, &project_root, &config, &requested).unwrap();

        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&OutputFormat::Html));
        assert!(!formats.contains(&OutputFormat::Pdf));
    }

    #[test]
    fn test_html_exclusion_excludes_html() {
        let config = build_test_config(&["content/print/**/*.typ"], &[], &[]);
        let project_root = std::path::PathBuf::from("/tmp/project");
        let file = project_root.join("content/print/document.typ");
        let requested = vec![OutputFormat::Pdf, OutputFormat::Html];

        let formats = get_file_formats(&file, &project_root, &config, &requested).unwrap();

        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&OutputFormat::Pdf));
        assert!(!formats.contains(&OutputFormat::Html));
    }

    #[test]
    fn test_html_and_pdf_exclusions_leave_only_epub() {
        let config = build_test_config(&["content/ebook/**/*.typ"], &["content/ebook/**/*.typ"], &[]);
        let project_root = std::path::PathBuf::from("/tmp/project");
        let file = project_root.join("content/ebook/chapter1.typ");
        let requested = vec![OutputFormat::Pdf, OutputFormat::Html, OutputFormat::Epub];

        let formats = get_file_formats(&file, &project_root, &config, &requested).unwrap();

        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&OutputFormat::Epub));
        assert!(!formats.contains(&OutputFormat::Pdf));
        assert!(!formats.contains(&OutputFormat::Html));
    }

    #[test]
    fn test_file_not_matching_exclusions_gets_all_formats() {
        let config = build_test_config(&["content/index.typ"], &["content/print/**/*.typ"], &[]);
        let project_root = std::path::PathBuf::from("/tmp/project");
        let file = project_root.join("content/other/document.typ");
        let requested = vec![OutputFormat::Pdf, OutputFormat::Html];

        let formats = get_file_formats(&file, &project_root, &config, &requested).unwrap();

        assert_eq!(formats.len(), 2);
        assert!(formats.contains(&OutputFormat::Pdf));
        assert!(formats.contains(&OutputFormat::Html));
    }

    #[test]
    fn test_respects_requested_formats() {
        let config = build_test_config(&[], &[], &[]);
        let project_root = std::path::PathBuf::from("/tmp/project");
        let file = project_root.join("content/test.typ");
        let requested = vec![OutputFormat::Pdf]; // Only PDF requested

        let formats = get_file_formats(&file, &project_root, &config, &requested).unwrap();

        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&OutputFormat::Pdf));
        assert!(!formats.contains(&OutputFormat::Html));
    }

    #[test]
    fn test_html_excluded_file_with_html_requested() {
        let config = build_test_config(&["content/index.typ"], &[], &[]);
        let project_root = std::path::PathBuf::from("/tmp/project");
        let file = project_root.join("content/index.typ");
        let requested = vec![OutputFormat::Html]; // Only HTML requested, but file is html-excluded

        let formats = get_file_formats(&file, &project_root, &config, &requested).unwrap();

        // File is excluded from HTML, so it shouldn't compile to HTML even if requested
        assert_eq!(formats.len(), 0);
    }

    #[test]
    fn test_glob_pattern_matching() {
        let config = build_test_config(&[], &["content/web/**/*.typ"], &[]);
        let project_root = std::path::PathBuf::from("/tmp/project");

        // File matching the PDF exclusion pattern
        let file1 = project_root.join("content/web/blog/post.typ");
        let formats1 = get_file_formats(&file1, &project_root, &config, &[OutputFormat::Pdf, OutputFormat::Html]).unwrap();
        assert_eq!(formats1.len(), 1);
        assert!(formats1.contains(&OutputFormat::Html));

        // File not matching the exclusion pattern
        let file2 = project_root.join("content/print/document.typ");
        let formats2 = get_file_formats(&file2, &project_root, &config, &[OutputFormat::Pdf, OutputFormat::Html]).unwrap();
        assert_eq!(formats2.len(), 2);
        assert!(formats2.contains(&OutputFormat::Pdf));
        assert!(formats2.contains(&OutputFormat::Html));
    }
}
