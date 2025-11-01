use crate::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
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

                // 1. Detect project configuration
                info!(path = %path.display(), "detecting project configuration");
                let project = crate::project::ProjectConfig::from_path(&path)?;
                info!(name = %project.name, files = project.typ_files.len(), "detected project");

                // 2. Create output directories
                let output_config = crate::output::OutputConfig::new(&project.name);
                output_config.create_dirs()?;

                // 3. Check for .typ files
                if project.typ_files.is_empty() {
                    return Err(crate::RheoError::project_config(
                        "no .typ files found in project",
                    ));
                }

                // 4. Compile each file
                // Track success/failure per format for graceful degradation
                let mut pdf_succeeded = 0;
                let mut pdf_failed = 0;
                let mut html_succeeded = 0;
                let mut html_failed = 0;

                // Use current working directory as root for Typst world
                // This allows absolute imports like /src/typst/rheo.typ to work
                let repo_root = std::env::current_dir()
                    .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;

                for typ_file in &project.typ_files {
                    let filename = get_output_filename(typ_file)?;

                    // Get the document directory (parent of the typ file) as root
                    let file_root = typ_file.parent().ok_or_else(|| {
                        crate::RheoError::path(typ_file, "file has no parent directory")
                    })?;

                    // Compile to PDF
                    if formats.contains(&OutputFormat::Pdf) {
                        let output_path =
                            output_config.pdf_dir.join(&filename).with_extension("pdf");
                        match crate::compile::compile_pdf(
                            typ_file,
                            &output_path,
                            file_root,
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
                    if formats.contains(&OutputFormat::Html) {
                        let output_path = output_config
                            .html_dir
                            .join(&filename)
                            .with_extension("html");
                        match crate::compile::compile_html(
                            typ_file,
                            &output_path,
                            file_root,
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

                // 5. Copy assets for HTML
                if formats.contains(&OutputFormat::Html) {
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

                // 6. Report results with per-format summary
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
