mod dom;
mod html_head;
mod server;

use rheo_core::compile::RheoCompileOptions;
use rheo_core::config::HtmlOptions;
use rheo_core::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use rheo_core::world::RheoWorld;
use rheo_core::{FormatPlugin, OpenHandle, PluginContext, PluginInput, ReloadCallback};
use rheo_core::{Result, RheoError};
use std::path::Path;
use tracing::{debug, info, warn};
use typst_html::HtmlDocument;

/// Server handle for HTML plugin's development server
pub struct HtmlServerHandle {
    /// The tokio runtime running the server
    pub runtime: tokio::runtime::Runtime,
    /// The server task handle (for cleanup on drop)
    pub server_task: tokio::task::JoinHandle<()>,
    /// The URL the server is running on
    pub url: String,
    /// Callback to send reload events to connected clients
    pub reload_callback: ReloadCallback,
}

pub struct HtmlPlugin;

impl FormatPlugin for HtmlPlugin {
    fn name(&self) -> &'static str {
        "html"
    }

    fn open(&self, output_dir: &Path, _format_name: &str) -> Result<OpenHandle> {
        // Create tokio runtime for the async server
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| RheoError::io(e, "creating tokio runtime"))?;

        // Start the server (async call within runtime)
        let (server_task, reload_tx, url) = runtime
            .block_on(async { server::start_server(output_dir.to_path_buf(), 3000).await })?;

        // Open browser
        if let Err(e) = server::open_browser(&url) {
            warn!(error = %e, "failed to open browser, but server is running");
        }

        // Create reload callback
        let reload_callback: ReloadCallback = Box::new(move || {
            let _ = reload_tx.send(());
        });

        // Wrap the server handle in a box for the opaque OpenHandle::Server type
        let handle = HtmlServerHandle {
            runtime,
            server_task,
            url,
            reload_callback,
        };
        Ok(OpenHandle::Server(Box::new(handle)))
    }

    fn inputs(&self) -> &'static [PluginInput] {
        &[PluginInput {
            name: "stylesheet",
            path: "style.css",
            required: false,
        }]
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        if ctx.plugin_config.spine.merge {
            return Err(RheoError::project_config(
                "HTML does not support merged compilation",
            ));
        }
        // If the project didn't provide style.css, write the bundled default.
        if !ctx.inputs.contains_key("stylesheet") {
            const DEFAULT_CSS: &str = include_str!("../../../src/templates/init/style.css");
            let dest = ctx.output_config.dir_for_plugin("html").join("style.css");
            std::fs::write(&dest, DEFAULT_CSS)
                .map_err(|e| RheoError::io(e, "writing default style.css"))?;
            debug!(dest = %dest.display(), "wrote bundled default style.css");
        }
        let html_options = HtmlOptions {
            stylesheets: ctx.project.config.html.stylesheets.clone(),
            fonts: ctx.project.config.html.fonts.clone(),
        };
        compile_html_new(ctx.options, html_options)
    }
}

pub fn compile_html_to_document(
    input: &Path,
    root: &Path,
    format_name: &str,
) -> Result<HtmlDocument> {
    // Create the compilation world with specified format for link transformations
    let world = RheoWorld::new(root, input, Some(format_name))?;

    // Compile the document to HtmlDocument
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    // Filter out HTML development warning
    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    unwrap_compilation_result(Some(&world), result, Some(html_filter))
}

pub fn compile_document_to_string(document: &HtmlDocument) -> Result<String> {
    // Export to HTML string (no post-processing - that happens in the compilation pipeline)
    typst_html::html(document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}

// ============================================================================
// Single-file HTML compilation (implementation functions)
// ============================================================================

/// Implementation: Compile a Typst document to HTML.
///
/// Uses format-aware RheoWorld for automatic link transformation (.typ → .html).
/// Transformations happen on-demand during Typst compilation (including imports).
/// The engine provides the World, handling fresh vs incremental compilation.
///
/// Pipeline: Compile (with transformations) → Export → Inject Head → Write
fn compile_html_impl(
    world: &RheoWorld,
    output: &Path,
    html_options: &HtmlOptions,
) -> Result<()> {
    // Compile to HTML document (transformations happen in RheoWorld)
    info!("compiling to HTML");
    let result = typst::compile::<HtmlDocument>(world);

    // Filter out HTML development warning
    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    let document = unwrap_compilation_result(Some(world), result, Some(html_filter))?;

    // Export to HTML string
    debug!(output = %output.display(), "exporting to HTML");
    let html_string =
        typst_html::html(&document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))?;

    // Inject CSS and font links into <head>
    let stylesheets: Vec<&str> = html_options
        .stylesheets
        .iter()
        .map(|s| s.as_str())
        .collect();
    let fonts: Vec<&str> = html_options.fonts.iter().map(|s| s.as_str()).collect();
    let html_string = html_head::inject_head_links(&html_string, &stylesheets, &fonts)?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

// ============================================================================
// Unified public API
// ============================================================================

/// Compile Typst document to HTML.
///
/// The engine provides the World, handling fresh vs incremental compilation.
/// This is a single code path - the engine creates and manages the World lifecycle.
///
/// # Arguments
/// * `options` - Compilation options (input, output, root, world)
/// * `html_options` - HTML-specific options (stylesheets, fonts for head injection)
///
/// # Returns
/// * `Result<()>` - Success or compilation error
pub fn compile_html_new(options: RheoCompileOptions, html_options: HtmlOptions) -> Result<()> {
    compile_html_impl(options.world, &options.output, &html_options)
}
