use crate::config::PluginSection;
use crate::output::OutputConfig;
use crate::project::ProjectConfig;
use std::collections::HashMap;
use std::path::Path;

/// Handle returned by FormatPlugin::open() for managing the opened resource
pub enum OpenHandle {
    /// Server-based preview (HTML) - opaque handle containing runtime, task, URL, and reload callback
    Server(Box<dyn std::any::Any + Send + Sync>),
    /// Direct file open (PDF/EPUB) - fire-and-forget, no reload needed
    Direct,
    /// No preview capability
    None,
}

use crate::compile::RheoCompileOptions;

/// Standardized spine options resolved by rheo core before calling compile().
#[derive(Debug, Clone)]
pub struct SpineOptions {
    pub title: Option<String>,
    pub vertebrae: Vec<String>,
    /// true = merged output, false = per-file output
    pub merge: bool,
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

/// Context passed to plugin.compile() for each compilation unit.
pub struct PluginContext<'a> {
    pub project: &'a ProjectConfig,
    pub output_config: &'a OutputConfig,
    pub options: RheoCompileOptions<'a>,
    /// Resolved spine options (title, vertebrae, merge flag).
    pub spine: SpineOptions,
    /// Full parsed plugin section from rheo.toml (or default if not configured).
    /// Plugins read format-specific fields from `config.extra` (e.g. stylesheets,
    /// identifier, date).
    pub config: PluginSection,
    /// Resolved additional input files declared by the plugin.
    pub inputs: HashMap<&'static str, PathBuf>,
}

pub trait FormatPlugin: Send + Sync {
    /// Plugin identifier, CLI flag, and output subdirectory name (e.g. "html", "pdf", "epub").
    fn name(&self) -> &'static str;

    /// Whether this plugin merges files by default.
    /// Override to return `true` for formats that always produce a single merged output (e.g. EPUB).
    fn default_merge(&self) -> bool {
        false
    }

    /// Set plugin-specific smart defaults when no rheo.toml exists.
    ///
    /// Called by the CLI for each plugin after loading a project without a config file.
    /// The default implementation is a no-op.
    ///
    /// # Arguments
    /// * `section` - The plugin's section (entry from `plugin_sections`, or a fresh default)
    /// * `project_name` - Derived project/file name for title inference
    fn apply_defaults(&self, _section: &mut PluginSection, _project_name: &str) {}

    /// Open the output for this format in the appropriate viewer.
    fn open(&self, output_dir: &Path, format_name: &str) -> crate::Result<OpenHandle>;

    /// Declare additional non-Typst input files this plugin needs.
    fn inputs(&self) -> &'static [PluginInput] {
        &[]
    }

    /// Compile one file (merge=false) or merged output (merge=true).
    fn compile(&self, ctx: PluginContext<'_>) -> crate::Result<()>;
}
