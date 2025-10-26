use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{debug, info};

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
                // Determine which formats to compile
                // If no flags specified, compile all formats (default behavior)
                let formats = if !pdf && !html && !epub {
                    vec![OutputFormat::Pdf, OutputFormat::Html, OutputFormat::Epub]
                } else {
                    let mut formats = Vec::new();
                    if pdf {
                        formats.push(OutputFormat::Pdf);
                    }
                    if html {
                        formats.push(OutputFormat::Html);
                    }
                    if epub {
                        formats.push(OutputFormat::Epub);
                    }
                    formats
                };

                info!(path = %path.display(), formats = ?formats, "compiling project");
                debug!("compilation orchestration will be implemented in rheo-12");
                // TODO: Actual compilation orchestration will be implemented in rheo-12
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
