use crate::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

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
        .ok_or_else(|| crate::RheoError::project_config(
            format!("invalid .typ filename: {:?}", typ_file)
        ))
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
            Commands::Compile { path, pdf, html, epub } => {
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
                    return Err(crate::RheoError::project_config("no .typ files found in project"));
                }

                // 4. Compile each file
                let mut compiled_count = 0;
                let mut failed_count = 0;

                // Use current working directory as root for Typst world
                // This allows absolute imports like /src/typst/rheo.typ to work
                let repo_root = std::env::current_dir()
                    .map_err(|e| crate::RheoError::io(e, "getting current directory"))?;

                for typ_file in &project.typ_files {
                    let filename = get_output_filename(typ_file)?;

                    // Compile to PDF
                    if formats.contains(&OutputFormat::Pdf) {
                        let output_path = output_config.pdf_dir.join(&filename).with_extension("pdf");
                        match crate::compile::compile_pdf(typ_file, &output_path, &repo_root) {
                            Ok(_) => compiled_count += 1,
                            Err(e) => {
                                error!(file = %typ_file.display(), error = %e, "PDF compilation failed");
                                failed_count += 1;
                            }
                        }
                    }

                    // Compile to HTML
                    if formats.contains(&OutputFormat::Html) {
                        let output_path = output_config.html_dir.join(&filename).with_extension("html");
                        match crate::compile::compile_html(typ_file, &output_path, &repo_root) {
                            Ok(_) => compiled_count += 1,
                            Err(e) => {
                                error!(file = %typ_file.display(), error = %e, "HTML compilation failed");
                                failed_count += 1;
                            }
                        }
                    }
                }

                // 5. Copy assets for HTML
                if formats.contains(&OutputFormat::Html) {
                    info!("copying assets for HTML output");
                    if let Err(e) = crate::assets::copy_css(&project.root, &output_config.html_dir) {
                        warn!(error = %e, "failed to copy CSS, continuing");
                    }
                    if let Err(e) = crate::assets::copy_images(&project.root, &output_config.html_dir) {
                        warn!(error = %e, "failed to copy images, continuing");
                    }
                }

                // 6. Report results
                info!(compiled = compiled_count, failed = failed_count, "compilation complete");

                if failed_count > 0 {
                    return Err(crate::RheoError::project_config(
                        format!("{} file(s) failed to compile", failed_count)
                    ));
                }

                Ok(())
            }
            Commands::Clean { all } => {
                info!(all, "cleaning build artifacts");
                // TODO: Implement clean logic
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
