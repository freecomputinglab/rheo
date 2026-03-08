mod dom;
mod html_head;
mod server;

use rheo_core::compile::RheoCompileOptions;
use rheo_core::config::PluginSection;
use rheo_core::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use rheo_core::world::RheoWorld;
use rheo_core::{FormatPlugin, OpenHandle, PluginContext, PluginInput};
use rheo_core::{Result, RheoError};
use std::path::Path;
use tracing::{debug, info, warn};
use typst_html::HtmlDocument;

/// Reload callback type - called by watch loop after successful compilation.
/// Defined here because it's only needed by the HTML plugin's development server.
pub type ReloadCallback = Box<dyn Fn() + Send + Sync>;

/// Server handle for HTML plugin's development server
pub struct HtmlServerHandle {
    pub runtime: tokio::runtime::Runtime,
    pub server_task: tokio::task::JoinHandle<()>,
    pub url: String,
    pub reload_callback: ReloadCallback,
}

/// Format-specific configuration parsed from the `[html]` section of rheo.toml.
struct HtmlConfig {
    stylesheets: Vec<String>,
    fonts: Vec<String>,
}

fn parse_html_config(section: &PluginSection) -> HtmlConfig {
    let stylesheets = section
        .extra
        .get("stylesheets")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| vec!["style.css".to_string()]);
    let fonts = section
        .extra
        .get("fonts")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    HtmlConfig { stylesheets, fonts }
}

pub struct HtmlPlugin;

impl FormatPlugin for HtmlPlugin {
    fn name(&self) -> &'static str {
        "html"
    }

    fn open(&self, output_dir: &Path, _format_name: &str) -> Result<OpenHandle> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| RheoError::io(e, "creating tokio runtime"))?;

        let (server_task, reload_tx, url) = runtime
            .block_on(async { server::start_server(output_dir.to_path_buf(), 3000).await })?;

        if let Err(e) = server::open_browser(&url) {
            warn!(error = %e, "failed to open browser, but server is running");
        }

        let reload_callback: ReloadCallback = Box::new(move || {
            let _ = reload_tx.send(());
        });

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
        if ctx.spine.merge {
            return Err(RheoError::project_config(
                "HTML does not support merged compilation",
            ));
        }
        // If the project didn't provide style.css, write the bundled default.
        if !ctx.inputs.contains_key("stylesheet") {
            let dest = ctx.output_config.dir_for_plugin("html").join("style.css");
            std::fs::write(&dest, rheo_core::DEFAULT_HTML_STYLESHEET)
                .map_err(|e| RheoError::io(e, "writing default style.css"))?;
            debug!(dest = %dest.display(), "wrote bundled default style.css");
        }

        let html_config = parse_html_config(&ctx.config);
        compile_html_new(ctx.options, &html_config.stylesheets, &html_config.fonts)
    }
}

pub fn compile_html_to_document(
    input: &Path,
    root: &Path,
    format_name: &str,
) -> Result<HtmlDocument> {
    let world = RheoWorld::new(root, input, Some(format_name))?;
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    unwrap_compilation_result(Some(&world), result, Some(html_filter))
}

pub fn compile_document_to_string(document: &HtmlDocument) -> Result<String> {
    typst_html::html(document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}

fn compile_html_impl(
    world: &RheoWorld,
    output: &Path,
    stylesheets: &[String],
    fonts: &[String],
) -> Result<()> {
    info!("compiling to HTML");
    let result = typst::compile::<HtmlDocument>(world);

    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    let document = unwrap_compilation_result(Some(world), result, Some(html_filter))?;

    debug!(output = %output.display(), "exporting to HTML");
    let html_string =
        typst_html::html(&document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))?;

    let stylesheet_refs: Vec<&str> = stylesheets.iter().map(|s| s.as_str()).collect();
    let font_refs: Vec<&str> = fonts.iter().map(|s| s.as_str()).collect();
    let html_string = html_head::inject_head_links(&html_string, &stylesheet_refs, &font_refs)?;

    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

/// Compile Typst document to HTML using an engine-provided World.
pub fn compile_html_new(
    options: RheoCompileOptions,
    stylesheets: &[String],
    fonts: &[String],
) -> Result<()> {
    compile_html_impl(options.world, &options.output, stylesheets, fonts)
}
