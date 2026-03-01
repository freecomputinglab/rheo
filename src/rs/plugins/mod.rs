use crate::compile::RheoCompileOptions;
use crate::config::{RheoConfig, SpineConfig};
use crate::output::OutputConfig;
use crate::project::ProjectConfig;
use crate::{OutputFormat, Result};
use std::path::Path;

pub mod epub;
pub mod html;
pub mod pdf;

/// How a plugin handles multiple files.
pub enum CompilationDispatch {
    /// Compile each .typ file independently (HTML, PDF default)
    PerFile,
    /// Always merge all spine files into one output (EPUB, merged PDF)
    Merged,
}

/// Context passed to plugin.compile() for each compilation unit
pub struct PluginContext<'a> {
    pub project: &'a ProjectConfig,
    pub output_config: &'a OutputConfig,
    pub options: RheoCompileOptions<'a>,
}

pub trait FormatPlugin: Send + Sync {
    /// Plugin identifier and CLI flag (e.g. "html", "pdf", "epub")
    fn name(&self) -> &'static str;

    /// OutputFormat for RheoWorld link-transform selection
    fn output_format(&self) -> OutputFormat;

    /// Output file extension (usually same as name)
    fn extension(&self) -> &'static str;

    /// Whether this plugin supports the dev server / live preview
    fn supports_live_preview(&self) -> bool {
        false
    }

    /// Whether this plugin compiles per-file or merged, given current config
    fn compilation_dispatch(&self, config: &RheoConfig) -> CompilationDispatch;

    /// Spine config for file-set resolution (None = use all project .typ files)
    fn spine_config<'a>(&self, config: &'a RheoConfig) -> Option<&'a dyn SpineConfig>;

    /// Copy assets before compilation (e.g. style.css for HTML). Default: no-op.
    fn copy_assets(&self, _project: &ProjectConfig, _output_dir: &Path) -> Result<()> {
        Ok(())
    }

    /// Compile one file (PerFile dispatch) or merged output (Merged dispatch).
    /// The output path is already fully resolved via OutputConfig::dir_for_format.
    fn compile(&self, ctx: PluginContext<'_>) -> Result<()>;
}

pub fn all_plugins() -> Vec<Box<dyn FormatPlugin>> {
    vec![
        Box::new(pdf::PdfPlugin),
        Box::new(html::HtmlPlugin),
        Box::new(epub::EpubPlugin),
    ]
}

pub fn plugins_for_formats(formats: &[OutputFormat]) -> Vec<Box<dyn FormatPlugin>> {
    all_plugins()
        .into_iter()
        .filter(|p| formats.contains(&p.output_format()))
        .collect()
}
