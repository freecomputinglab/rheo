mod server;

use rheo_core::html_utils;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

use rheo_core::plugins::{PackageAssets, ResolvedPackage};
use rheo_core::{
    AssetConfig, FormatPlugin, OpenHandle, PluginContext, Result, RheoError, ServerHandle,
    default_package_assets,
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
            },
            AssetConfig {
                name: SCRIPTS,
                default_path: "index.js",
                required: false,
            },
        ]
    }

    fn map_packages_to_assets(&self, packages: &[ResolvedPackage]) -> Vec<PackageAssets> {
        packages
            .iter()
            .map(|pkg| {
                let mut block = default_package_assets(pkg);
                block.assets.extra.insert(
                    "css_stylesheet".into(),
                    toml::Value::String("index.css".into()),
                );
                block
                    .assets
                    .extra
                    .insert("js_scripts".into(), toml::Value::String("index.js".into()));
                block
            })
            .collect()
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        let html_string = ctx.compile_to_html_string()?;

        let css_assets = ctx.assets.get(&STYLESHEETS).filter(|v| !v.is_empty());
        let js_assets = ctx.assets.get(&SCRIPTS).filter(|v| !v.is_empty());

        let (css_paths, inline_styles): (Vec<&str>, &[&str]) = match css_assets {
            Some(assets) => {
                for a in assets {
                    info!("Found CSS stylesheet: {}", a.resolved_path.display());
                }
                let paths = assets
                    .iter()
                    .map(|a| a.built_relative_path.as_str())
                    .collect();
                (paths, &[])
            }
            None => {
                info!("No stylesheet found, using default");
                (Vec::new(), &[DEFAULT_STYLESHEET])
            }
        };

        let js_paths: Vec<&str> = js_assets
            .map(|v| v.iter().map(|a| a.built_relative_path.as_str()).collect())
            .unwrap_or_default();

        let html_string = html_utils::inject_inline_styles(&html_string, inline_styles)?;
        let html_string = if css_paths.is_empty() && js_paths.is_empty() {
            html_string
        } else {
            html_utils::inject_head_links(&html_string, &[], &css_paths, &js_paths)?
        };

        debug!(size = html_string.len(), "writing HTML file");
        let output = &ctx.options.output;
        std::fs::write(output, &html_string)
            .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

        info!(output = %output.display(), "successfully compiled to HTML");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rheo_core::plugins::ResolvedPackage;

    #[test]
    fn test_html_plugin_map_packages_to_assets_override() {
        let dir = tempfile::tempdir().unwrap();
        let source_root = dir.path().join("mypkg");
        std::fs::create_dir_all(&source_root).unwrap();

        let resolved = vec![ResolvedPackage {
            name: "mypkg".into(),
            source_root: source_root.clone(),
            namespace: None,
            version: None,
        }];
        let result = HtmlPlugin.map_packages_to_assets(&resolved);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].assets.extra.get("css_stylesheet"),
            Some(&toml::Value::String("index.css".into()))
        );
        assert_eq!(
            result[0].assets.extra.get("js_scripts"),
            Some(&toml::Value::String("index.js".into()))
        );
        assert_eq!(result[0].assets.dest.as_deref(), Some("mypkg"));
        assert_eq!(result[0].assets.copy, vec!["**/*"]);
        assert_eq!(result[0].source_root, source_root);
    }
}
