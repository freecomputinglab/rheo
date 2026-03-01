use crate::compile::RheoCompileOptions;
use crate::config::{RheoConfig, SpineConfig};
use crate::output::OutputConfig;
use crate::project::ProjectConfig;
use crate::Result;
use std::path::Path;

pub mod epub;
pub mod html;
pub mod pdf;

/// Standardized spine options resolved by rheo core before calling compile().
#[derive(Debug, Clone)]
pub struct SpineOptions {
    pub title: Option<String>,
    pub vertebrae: Vec<String>,
    /// true = merged output, false = per-file output
    pub merge: bool,
}

/// Standardized plugin configuration passed to compile().
#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub spine: SpineOptions,
}

/// Context passed to plugin.compile() for each compilation unit
pub struct PluginContext<'a> {
    pub project: &'a ProjectConfig,
    pub output_config: &'a OutputConfig,
    pub options: RheoCompileOptions<'a>,
    pub plugin_config: PluginConfig,
}

pub trait FormatPlugin: Send + Sync {
    /// Plugin identifier, CLI flag, and output subdirectory name (e.g. "html", "pdf", "epub")
    fn name(&self) -> &'static str;

    /// Whether this plugin supports the dev server / live preview
    fn supports_live_preview(&self) -> bool {
        false
    }

    /// Spine config for file-set resolution (None = use all project .typ files)
    fn spine_config<'a>(&self, config: &'a RheoConfig) -> Option<&'a dyn SpineConfig>;

    /// Copy assets before compilation (e.g. style.css for HTML). Default: no-op.
    fn copy_assets(&self, _project: &ProjectConfig, _output_dir: &Path) -> Result<()> {
        Ok(())
    }

    /// Compile one file (merge=false) or merged output (merge=true).
    /// Inspect ctx.plugin_config.spine.merge to determine the mode.
    fn compile(&self, ctx: PluginContext<'_>) -> Result<()>;
}

pub fn all_plugins() -> Vec<Box<dyn FormatPlugin>> {
    vec![
        Box::new(pdf::PdfPlugin),
        Box::new(html::HtmlPlugin),
        Box::new(epub::EpubPlugin),
    ]
}

pub fn plugins_for_names(names: &[String]) -> Vec<Box<dyn FormatPlugin>> {
    all_plugins()
        .into_iter()
        .filter(|p| names.iter().any(|n| n == p.name()))
        .collect()
}
