use crate::compile::RheoCompileOptions;
use crate::output::OutputConfig;
use crate::project::ProjectConfig;
use crate::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Reload callback type - called by watch loop after successful compilation
pub type ReloadCallback = Box<dyn Fn() + Send + Sync>;

/// Handle returned by FormatPlugin::open() for managing the opened resource
pub enum OpenHandle {
    /// Server-based preview (HTML) - needs reload callback and stays alive during watch
    Server(ServerHandle),
    /// Direct file open (PDF/EPUB) - fire-and-forget, no reload needed
    Direct,
    /// No preview capability (disabled plugins)
    None,
}

/// Handle for server-based preview with reload capability
pub struct ServerHandle {
    /// The tokio runtime running the server
    pub runtime: tokio::runtime::Runtime,
    /// The server task handle (for cleanup on drop)
    pub server_task: tokio::task::JoinHandle<()>,
    /// The URL the server is running on
    pub url: String,
    /// Callback to send reload events to connected clients
    pub reload_callback: ReloadCallback,
}

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

    /// Open the output for this format in the appropriate viewer.
    ///
    /// Called by watch mode when --open flag is used.
    ///
    /// # Returns
    /// * `OpenHandle::Server` - For plugins that run a dev server (HTML)
    /// * `OpenHandle::Direct` - For plugins that open files directly (PDF/EPUB)
    /// * `OpenHandle::None` - For plugins that don't support opening
    ///
    /// # Context
    /// * `output_dir` - Path to the plugin's output directory (e.g., build/html)
    /// * `format_name` - The format name from CLI
    fn open(&self, output_dir: &Path, format_name: &str) -> Result<OpenHandle>;

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
