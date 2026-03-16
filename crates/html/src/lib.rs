mod dom;
mod html_head;
mod server;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

use rheo_core::{
    FormatPlugin, OpenHandle, PluginContext, PluginSection, Result, RheoCompileOptions, RheoError,
    ServerHandle, diagnostics::print_diagnostics,
};
use std::path::Path;
use tracing::{debug, info, warn};
use typst::diag::Warned;
use typst_pdf::PdfOptions;

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

    fn uses_bundle_api(&self) -> bool {
        true
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

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        if ctx.spine.merge {
            return Err(RheoError::project_config(
                "HTML does not support merged compilation",
            ));
        }

        compile_html_bundle(ctx.options, &ctx.config)
    }
}

/// Compile HTML using bundle API.
///
/// Uses typst::compile::<Bundle>() for multi-file bundle output and writes
/// each HTML file to the output directory with injected CSS and fonts.
fn compile_html_bundle(options: RheoCompileOptions, config: &PluginSection) -> Result<()> {
    let html_config = parse_html_config(config);

    info!("compiling HTML bundle");

    // Compile the bundle using the world (which has the synthetic bundle entry as main)
    let Warned { output, warnings } = typst::compile::<typst_bundle::Bundle>(options.world);

    // Print warnings (ignore errors from diagnostic printing)
    let _ = print_diagnostics(options.world, &[], &warnings);

    let bundle = output.map_err(|errors| {
        // Print errors to stderr with proper formatting
        let _ = print_diagnostics(options.world, &errors, &[]);
        // Return error for error handling
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::project_config(format!(
            "bundle compilation had errors: {}",
            error_messages.join(", ")
        ))
    })?;

    // Export the bundle to get HTML files
    let bundle_options = typst_bundle::BundleOptions {
        pixel_per_pt: 144.0,
        pdf: PdfOptions::default(),
    };

    let fs = typst_bundle::export(&bundle, &bundle_options)
        .map_err(|e| RheoError::project_config(format!("bundle export failed: {:?}", e)))?;

    debug!(file_count = fs.len(), "exported HTML bundle");

    // Write each HTML file to the output directory
    for (vpath, bytes) in &fs {
        let out_path = options.output.join(vpath.get_without_slash());
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RheoError::io(e, format!("creating output directory {}", parent.display()))
            })?;
        }

        // For HTML files, inject CSS and fonts
        if out_path.extension().is_some_and(|e| e == "html") {
            let html_string = String::from_utf8(bytes.to_vec()).map_err(|e| {
                RheoError::invalid_data(format!("HTML output is not valid UTF-8: {}", e))
            })?;

            // Load CSS contents if configured, falling back to default stylesheet
            let css_contents: Vec<String> = html_config
                .stylesheets
                .iter()
                .map(|path| {
                    let full_path = options.root.join(path);
                    match std::fs::read_to_string(&full_path) {
                        Ok(css) => css,
                        Err(_) => {
                            warn!(path = %path, "stylesheet not found, using default");
                            DEFAULT_STYLESHEET.to_string()
                        }
                    }
                })
                .collect();

            // Inject font links first (DOM-based), then inline styles (string-based)
            let font_refs: Vec<&str> = html_config.fonts.iter().map(|s| s.as_str()).collect();
            let html_string = html_head::inject_head_links(&html_string, &[], &font_refs)?;

            let css_refs: Vec<&str> = css_contents.iter().map(|s| s.as_str()).collect();
            let html_string = html_head::inject_inline_styles(&html_string, &css_refs)?;

            std::fs::write(&out_path, html_string).map_err(|e| {
                RheoError::io(e, format!("writing HTML file to {}", out_path.display()))
            })?;
        } else {
            // For non-HTML files (assets), write directly
            std::fs::write(&out_path, bytes)
                .map_err(|e| RheoError::io(e, format!("writing file to {}", out_path.display())))?;
        }

        debug!(output = %out_path.display(), "wrote bundle file");
    }

    info!(output = %options.output.display(), "successfully compiled HTML bundle");
    Ok(())
}
