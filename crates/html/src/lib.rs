mod dom;
mod html_head;
mod server;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

use rheo_core::{
    FormatPlugin, OpenHandle, PluginContext, PluginSection, Result, RheoError, ServerHandle,
    compile_document_to_string, compile_html_with_world,
};
use std::path::Path;
use std::{convert::From, path::PathBuf};
use tracing::{debug, info, warn};

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

impl From<&PluginSection> for HtmlConfig {
    fn from(section: &PluginSection) -> Self {
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
}

pub struct HtmlPlugin;

impl FormatPlugin for HtmlPlugin {
    fn name(&self) -> &'static str {
        "html"
    }

    fn init_templates(&self) -> Vec<(&'static str, &'static str)> {
        vec![("style.css", include_str!("templates/style.css"))]
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

    // TODO: because the case here is that compile is called for EVERY source file, we need a
    // `precompile` entrypoint that can do things like asset copying.

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        if ctx.spine.merge {
            return Err(RheoError::project_config(
                "HTML does not support merged compilation",
            ));
        }

        let html_config: HtmlConfig = (&ctx.config).into();
        let css_contents = collect_css(&html_config.stylesheets, &ctx.project.root);
        let world = ctx.options.world.ok_or_else(|| {
            RheoError::project_config(
                "HTML per-file compile requires a world; this is a rheo bug (internal invariant violation)",
            )
        })?;

        let document = compile_html_with_world(world)?;
        let output = ctx.options.output;

        debug!(output = %output.display(), "exporting to HTML");
        let html_string = compile_document_to_string(&document)?;

        // Inject font links first (DOM-based), then inline styles (string-based).
        // Ordering matters: string-based injection must run last to avoid re-parsing
        // and HTML-escaping CSS content (e.g., `>` in selectors).
        let font_refs: Vec<&str> = html_config.fonts.iter().map(|s| s.as_str()).collect();
        let html_string = html_head::inject_head_links(&html_string, &[], &font_refs)?;

        let css_refs: Vec<&str> = css_contents.iter().map(|s| s.as_str()).collect();
        let html_string = html_head::inject_inline_styles(&html_string, &css_refs)?;

        debug!(size = html_string.len(), "writing HTML file");
        std::fs::write(&output, &html_string)
            .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

        info!(output = %output.display(), "successfully compiled to HTML");
        Ok(())
    }
}

/// Resolve and read each stylesheet path, collecting raw CSS content for inlining.
fn collect_css(stylesheet_paths: &Vec<String>, project_root: &PathBuf) -> Vec<String> {
    let mut css_contents: Vec<String> = Vec::new();
    for stylesheet_path in stylesheet_paths {
        let full_path = project_root.join(stylesheet_path);
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
            css_contents.push(DEFAULT_STYLESHEET.to_string());
        }
    }
    css_contents
}
