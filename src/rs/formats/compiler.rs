//! Format compiler trait interface for unified compilation across all output formats.
//!
//! This module defines the `FormatCompiler` trait that all output formats (PDF, HTML, EPUB)
//! implement to provide a consistent compilation interface.

use crate::compile::RheoCompileOptions;
use crate::{OutputFormat, Result};

/// Unified trait for format-specific compilation.
///
/// Each output format (PDF, HTML, EPUB) implements this trait to provide
/// a consistent compilation interface that supports:
/// - Fresh compilation (creates new World)
/// - Incremental compilation (reuses existing World)
/// - Format-specific configuration
/// - Per-file vs merged compilation modes
pub trait FormatCompiler {
    /// Format-specific configuration type (e.g., PdfConfig, HtmlOptions)
    type Config;

    /// Get the output format this compiler handles
    fn format(&self) -> OutputFormat;

    /// Get the file extension for this format (without dot, e.g., "pdf")
    fn extension(&self) -> &'static str {
        match self.format() {
            OutputFormat::Pdf => "pdf",
            OutputFormat::Html => "html",
            OutputFormat::Epub => "epub",
        }
    }

    /// Check if this format supports per-file compilation with the given config.
    ///
    /// Some formats (like EPUB) only support merged compilation,
    /// while others (PDF, HTML) can do both per-file and merged.
    fn supports_per_file(&self, config: &Self::Config) -> bool;

    /// Compile using the provided options.
    ///
    /// This is the main entry point that handles both fresh and incremental compilation
    /// based on whether `options.world` is present.
    ///
    /// # Arguments
    /// * `options` - Compilation options (includes optional World for incremental)
    /// * `config` - Format-specific configuration
    ///
    /// # Returns
    /// * `Result<()>` - Success or compilation error
    fn compile(&self, options: RheoCompileOptions, config: &Self::Config) -> Result<()>;
}

/// PDF format compiler
#[derive(Debug, Clone, Copy)]
pub struct PdfCompiler;

/// HTML format compiler
#[derive(Debug, Clone, Copy)]
pub struct HtmlCompiler;

/// EPUB format compiler
#[derive(Debug, Clone, Copy)]
pub struct EpubCompiler;

/// Dispatch enum for runtime format selection
///
/// This allows code to work with any format compiler at runtime
/// while maintaining type safety.
#[derive(Debug, Clone, Copy)]
pub enum AnyFormatCompiler {
    Pdf(PdfCompiler),
    Html(HtmlCompiler),
    Epub(EpubCompiler),
}

impl AnyFormatCompiler {
    /// Create a compiler instance for the given output format
    pub fn from_format(format: OutputFormat) -> Self {
        match format {
            OutputFormat::Pdf => AnyFormatCompiler::Pdf(PdfCompiler),
            OutputFormat::Html => AnyFormatCompiler::Html(HtmlCompiler),
            OutputFormat::Epub => AnyFormatCompiler::Epub(EpubCompiler),
        }
    }

    /// Get the output format for this compiler instance
    pub fn format(&self) -> OutputFormat {
        match self {
            AnyFormatCompiler::Pdf(_) => OutputFormat::Pdf,
            AnyFormatCompiler::Html(_) => OutputFormat::Html,
            AnyFormatCompiler::Epub(_) => OutputFormat::Epub,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_compiler_instance_creation() {
        let pdf_compiler = AnyFormatCompiler::from_format(OutputFormat::Pdf);
        assert_eq!(pdf_compiler.format(), OutputFormat::Pdf);

        let html_compiler = AnyFormatCompiler::from_format(OutputFormat::Html);
        assert_eq!(html_compiler.format(), OutputFormat::Html);

        let epub_compiler = AnyFormatCompiler::from_format(OutputFormat::Epub);
        assert_eq!(epub_compiler.format(), OutputFormat::Epub);
    }
}
