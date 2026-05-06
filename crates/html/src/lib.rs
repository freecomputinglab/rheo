mod server;

use rheo_core::html_utils;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

use rheo_core::{
    AssetConfig, FormatPlugin, OpenHandle, PluginContext, Result, RheoError, ServerHandle,
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
const SCRIPTS: &str = "js_scripts";

impl FormatPlugin for HtmlPlugin {
    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn init_template_files(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            // The stylesheet included with the template mirrors the default stylesheet, so that
            // users can build from it or start from scratch as they wish.
            ("style.css", include_str!("templates/style.css")),
            // A demonstrative JS file that just logs to console. See the examples/ directory for
            // how to use Rheo with bundled JS.
            ("index.js", include_str!("templates/index.js")),
        ]
    }

    fn init_rheo_toml_section_template(&self) -> Option<&'static str> {
        Some(include_str!("templates/init/rheo_section.toml"))
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

    fn assets(&self) -> Vec<AssetConfig> {
        vec![
            AssetConfig {
                name: STYLESHEETS,
                default_path: "style.css",
                required: false,
                combine: None,
            },
            AssetConfig {
                name: SCRIPTS,
                default_path: "index.js",
                required: false,
                combine: None,
            },
        ]
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        let html_string = ctx.compile_to_html_string()?;

        let css_vec = ctx.assets.get(&STYLESHEETS);
        let html_string = if let Some(css_assets) = css_vec.filter(|v| !v.is_empty()) {
            for a in css_assets {
                info!("Found CSS stylesheet: {}", a.resolved_path.display());
            }
            let css_paths: Vec<&str> = css_assets
                .iter()
                .map(|a| a.built_relative_path.as_str())
                .collect();

            let js_paths: Vec<&str> = ctx
                .assets
                .get(&SCRIPTS)
                .map(|v| v.iter().map(|a| a.built_relative_path.as_str()).collect())
                .unwrap_or_default();

            html_utils::inject_head_links(&html_string, &[], &css_paths, &js_paths)?
        } else {
            info!("No stylesheet found, using default");
            html_utils::inject_inline_styles(&html_string, &[DEFAULT_STYLESHEET])?
        };

        debug!(size = html_string.len(), "writing HTML file");
        let output = &ctx.options.output;
        std::fs::write(output, &html_string)
            .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

        info!(output = %output.display(), "successfully compiled to HTML");
        Ok(())
    }
}
