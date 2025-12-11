//! Shared post-processing utilities for HTML and EPUB output formats.
//!
//! This module provides common functionality for transforming compiled output:
//! - Link transformation (.typ → .html/.xhtml)
//! - DOM manipulation utilities (html5ever)
//! - HTML head injection (CSS/font links)

pub mod dom;
pub mod html_head;
pub mod links;

// Re-export commonly used functions
pub use links::transform_links;

use std::path::PathBuf;

/// Context for post-processing operations
#[derive(Debug, Clone)]
pub struct PostProcessContext {
    /// The file being processed
    pub input_path: PathBuf,
    /// Project root directory
    pub root_path: PathBuf,
    /// Target output format
    pub output_format: OutputFormat,
}

/// Target output format for link transformation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// HTML output (.html extension)
    Html,
    /// XHTML output for EPUB (.xhtml extension)
    Xhtml,
}

impl OutputFormat {
    /// Get the file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Html => ".html",
            OutputFormat::Xhtml => ".xhtml",
        }
    }
}

/// Asset references extracted from configuration
#[derive(Debug, Clone, Default)]
pub struct AssetRefs {
    /// Stylesheet paths (relative to build directory)
    pub stylesheets: Vec<String>,
    /// Font URLs to inject
    pub fonts: Vec<String>,
}
