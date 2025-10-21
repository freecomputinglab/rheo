use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "rheo")]
#[command(about = "A tool for flowing Typst documents into publishable outputs", long_about = None)]
#[command(version)]
pub struct Cli {
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

    pub fn run(self) -> Result<()> {
        match self.command {
            Commands::Compile { path, pdf, html, epub } => {
                println!("Compile command called with path: {:?}", path);
                println!("Flags - PDF: {}, HTML: {}, EPUB: {}", pdf, html, epub);
                // TODO: Implement compilation logic
                Ok(())
            }
            Commands::Clean { all } => {
                println!("Clean command called (all: {})", all);
                // TODO: Implement clean logic
                Ok(())
            }
            Commands::Init { name, template } => {
                println!("Init command called with name: {}, template: {}", name, template);
                // TODO: Implement init logic
                Ok(())
            }
            Commands::ListExamples => {
                println!("Listing examples...");
                // TODO: Implement list-examples logic
                Ok(())
            }
        }
    }
}
