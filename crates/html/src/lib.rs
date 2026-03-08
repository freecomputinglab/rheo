mod dom;
mod html_head;
mod server;

use rheo_core::compile::RheoCompileOptions;
use rheo_core::config::PluginSection;
use rheo_core::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use rheo_core::world::RheoWorld;
use rheo_core::{FormatPlugin, OpenHandle, PluginContext, ServerHandle};
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

impl ServerHandle for HtmlServerHandle {
    fn url(&self) -> &str {
        &self.url
    }
    fn reload(&self) {
        (self.reload_callback)();
    }
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

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        if ctx.spine.merge {
            return Err(RheoError::project_config(
                "HTML does not support merged compilation",
            ));
        }

        let html_config = parse_html_config(&ctx.config);

        // Resolve and read each stylesheet, collecting raw CSS content for inlining.
        let mut css_contents: Vec<String> = Vec::new();
        for stylesheet_path in &html_config.stylesheets {
            let full_path = ctx.project.root.join(stylesheet_path);
            if full_path.exists() {
                match std::fs::read_to_string(&full_path) {
                    Ok(content) => css_contents.push(content),
                    Err(e) => {
                        warn!(path = %full_path.display(), error = %e, "failed to read stylesheet, skipping")
                    }
                }
            } else if stylesheet_path == "style.css" {
                // Default name with no file present: inline the bundled stylesheet.
                debug!("using bundled default style.css");
                css_contents.push(rheo_core::DEFAULT_HTML_STYLESHEET.to_string());
            } else {
                warn!(path = %full_path.display(), "stylesheet not found, skipping");
            }
        }

        compile_html_new(ctx.options, &css_contents, &html_config.fonts)
    }
}

pub use rheo_core::html_compile::{compile_document_to_string, compile_html_to_document};

fn compile_html_impl(
    world: &RheoWorld,
    output: &Path,
    css_contents: &[String],
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

    // Inject font links first (DOM-based), then inline styles (string-based).
    // Ordering matters: string-based injection must run last to avoid re-parsing
    // and HTML-escaping CSS content (e.g., `>` in selectors).
    let font_refs: Vec<&str> = fonts.iter().map(|s| s.as_str()).collect();
    let html_string = html_head::inject_head_links(&html_string, &[], &font_refs)?;

    let css_refs: Vec<&str> = css_contents.iter().map(|s| s.as_str()).collect();
    let html_string = html_head::inject_inline_styles(&html_string, &css_refs)?;

    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

/// Compile Typst document to HTML using an engine-provided World.
pub fn compile_html_new(
    options: RheoCompileOptions,
    css_contents: &[String],
    fonts: &[String],
) -> Result<()> {
    let world = options.world.expect("HTML plugin requires a world (never called in merged mode)");
    compile_html_impl(world, &options.output, css_contents, fonts)
}
