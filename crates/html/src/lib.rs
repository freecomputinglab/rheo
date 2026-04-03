mod dom;
mod html_head;
mod server;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

use rheo_core::{
    FormatPlugin, OpenHandle, PluginAsset, PluginContext, Result, RheoError, ServerHandle,
};
use std::path::Path;
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

pub struct HtmlPlugin;

const PLUGIN_NAME: &str = "html";
const STYLESHEETS: &str = "css_stylesheet";

impl FormatPlugin for HtmlPlugin {
    fn name(&self) -> &'static str {
        &PLUGIN_NAME
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

    fn assets(&self) -> Vec<PluginAsset> {
        vec![PluginAsset {
            name: &STYLESHEETS,
            // TODO: make it possible to configure a custom path for any PluginAsset
            default_path: "style.css",
            required: false,
        }]
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        let html_string = ctx.compile_to_html_string()?;

        // If a custom asset is specified, we inject the link to the asset into each HTML file.
        // If not, we inline the default CSS.
        let css_path = ctx.assets.get(&STYLESHEETS);
        let html_string = if let Some(css_fname) = css_path {
            info!("Found stylesheet {}", &css_fname.display());
            html_head::inject_head_links(&html_string, &[&css_fname.display().to_string()], &[])?
        } else {
            info!("No stylesheet found, using default");
            html_head::inject_inline_styles(&html_string, &[&DEFAULT_STYLESHEET.to_string()])?
        };

        debug!(size = html_string.len(), "writing HTML file");
        let output = &ctx.options.output;
        std::fs::write(&output, &html_string)
            .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

        info!(output = %output.display(), "successfully compiled to HTML");
        Ok(())
    }
}
