use crate::compile::RheoCompileOptions;
use crate::output::OutputConfig;
use crate::project::ProjectConfig;
use crate::Result;
use std::collections::HashMap;
use std::path::PathBuf;

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

/// Declares an additional non-Typst input file needed from the project directory.
pub struct PluginInput {
    /// Key used to retrieve this input from PluginContext::inputs
    pub name: &'static str,
    /// Path relative to the project root where the file is expected
    pub path: &'static str,
    /// If true, a missing file is a compile error; if false, it is absent from ctx.inputs
    pub required: bool,
}

/// Context passed to plugin.compile() for each compilation unit
pub struct PluginContext<'a> {
    pub project: &'a ProjectConfig,
    pub output_config: &'a OutputConfig,
    pub options: RheoCompileOptions<'a>,
    pub plugin_config: PluginConfig,
    /// Resolved additional input files declared by the plugin.
    /// Keyed by PluginInput::name. Only contains files that were found;
    /// missing optional inputs are absent. Values are source paths (project root).
    pub inputs: HashMap<&'static str, PathBuf>,
}

pub trait FormatPlugin: Send + Sync {
    /// Plugin identifier, CLI flag, and output subdirectory name (e.g. "html", "pdf", "epub")
    fn name(&self) -> &'static str;

    /// Whether this plugin supports the dev server / live preview
    fn supports_live_preview(&self) -> bool {
        false
    }

    /// Declare additional non-Typst input files this plugin needs.
    /// The engine finds each, copies it to the plugin output dir, and passes
    /// the source path in PluginContext::inputs before calling compile().
    fn inputs(&self) -> &'static [PluginInput] {
        &[]
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
