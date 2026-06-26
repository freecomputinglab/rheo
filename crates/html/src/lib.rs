pub mod feed;
mod server;

use rheo_core::html_utils;
use serde::Deserialize;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

use rheo_core::{
    AssetConfig, FormatInitTemplate, FormatPlugin, OpenHandle, PluginContext, Result, RheoError,
    ServerHandle, SpineOutput,
};
use std::path::Path;
use tracing::{debug, info, warn};

/// Reload callback type - called by watch loop after successful compilation.
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

    fn format_init_template(&self) -> FormatInitTemplate {
        FormatInitTemplate {
            files: vec![
                ("style.css", include_str!("templates/style.css")),
                ("index.js", include_str!("templates/index.js")),
            ],
            options_toml: Some(include_str!("templates/init/rheo_section.toml")),
        }
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

    fn compile(&self, ctx: PluginContext<'_>, outputs: &[SpineOutput]) -> Result<()> {
        let css_assets = ctx.assets.get(&STYLESHEETS).filter(|v| !v.is_empty());
        let js_assets = ctx.assets.get(&SCRIPTS).filter(|v| !v.is_empty());

        let css_paths: Vec<String> = css_assets
            .map(|assets| {
                assets
                    .iter()
                    .inspect(|a| info!("Found CSS stylesheet: {}", a.resolved_path.display()))
                    .map(|a| a.built_relative_path.clone())
                    .collect()
            })
            .unwrap_or_default();
        let use_default_css = css_paths.is_empty();

        let js_paths: Vec<String> = js_assets
            .map(|v| v.iter().map(|a| a.built_relative_path.clone()).collect())
            .unwrap_or_default();

        let html_cfg = ctx.config.parse_extra::<HtmlConfig>()?;
        let feed_title = html_cfg.resolve_title(ctx.spine.title.as_deref(), &ctx.project.name);
        let feed_link = html_cfg
            .base_url()
            .map(|base| (format!("{base}/feed.xml"), feed_title));

        for output in outputs {
            let html_string = String::from_utf8(output.bytes.to_vec()).map_err(|e| {
                RheoError::invalid_data(format!("HTML output is not valid UTF-8: {}", e))
            })?;

            // Inject default stylesheet as inline styles when no user CSS is provided.
            let html_string = if use_default_css {
                info!("No stylesheet found, using default");
                html_utils::inject_inline_styles(&html_string, &[DEFAULT_STYLESHEET])?
            } else {
                html_string
            };

            let needs_head_links = !css_paths.is_empty() || !js_paths.is_empty();
            let css_refs: Vec<&str> = css_paths.iter().map(|s| s.as_str()).collect();
            let js_refs: Vec<&str> = js_paths.iter().map(|s| s.as_str()).collect();

            let html_string = if needs_head_links || feed_link.is_some() {
                let mut dom = html_utils::HtmlDom::parse(&html_string)?;
                if needs_head_links {
                    dom.inject_head_links(&[], &css_refs, &js_refs)?;
                }
                if let Some((href, title)) = &feed_link {
                    dom.inject_feed_link(href, title)?;
                }
                dom.serialize()?
            } else {
                html_string
            };

            let out_path = ctx.output_dir.join(&output.output_path);
            debug!(size = html_string.len(), "writing HTML file");
            std::fs::write(&out_path, &html_string)
                .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", out_path)))?;
            info!(output = %out_path.display(), "successfully compiled to HTML");
        }

        Ok(())
    }
}

/// Typed view of the `[html]` section's format-specific keys.
#[derive(Debug, Deserialize, Default)]
struct HtmlConfig {
    feed_base_url: Option<String>,
    #[allow(dead_code)]
    feed_author: Option<String>,
    feed_title: Option<String>,
}

impl HtmlConfig {
    fn base_url(&self) -> Option<String> {
        self.feed_base_url
            .as_deref()
            .map(|s| s.trim_end_matches('/').to_string())
    }

    fn resolve_title(&self, spine_title: Option<&str>, project_name: &str) -> String {
        self.feed_title
            .as_deref()
            .or(spine_title)
            .unwrap_or(project_name)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rheo_core::PluginSection;

    fn section_with(extra: toml::Table) -> PluginSection {
        PluginSection {
            extra,
            ..Default::default()
        }
    }

    fn html_config(extra: toml::Table) -> HtmlConfig {
        section_with(extra)
            .parse_extra::<HtmlConfig>()
            .expect("parse HtmlConfig")
    }

    #[test]
    fn test_feed_base_url_trims_trailing_slash() {
        let mut extra = toml::Table::new();
        extra.insert(
            "feed_base_url".to_string(),
            toml::Value::String("https://example.com/".to_string()),
        );
        assert_eq!(
            html_config(extra).base_url().as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn test_feed_base_url_absent() {
        assert_eq!(html_config(toml::Table::new()).base_url(), None);
    }

    #[test]
    fn test_feed_base_url_non_string_errors() {
        let mut extra = toml::Table::new();
        extra.insert("feed_base_url".to_string(), toml::Value::Integer(42));
        assert!(section_with(extra).parse_extra::<HtmlConfig>().is_err());
    }

    #[test]
    fn test_feed_author_present() {
        let mut extra = toml::Table::new();
        extra.insert(
            "feed_author".to_string(),
            toml::Value::String("Ada Lovelace".to_string()),
        );
        assert_eq!(
            html_config(extra).feed_author.as_deref(),
            Some("Ada Lovelace")
        );
    }

    #[test]
    fn test_feed_author_absent() {
        assert_eq!(html_config(toml::Table::new()).feed_author, None);
    }

    #[test]
    fn test_feed_author_non_string_errors() {
        let mut extra = toml::Table::new();
        extra.insert("feed_author".to_string(), toml::Value::Integer(42));
        assert!(section_with(extra).parse_extra::<HtmlConfig>().is_err());
    }

    #[test]
    fn test_feed_title_present() {
        let mut extra = toml::Table::new();
        extra.insert(
            "feed_title".to_string(),
            toml::Value::String("My Feed".to_string()),
        );
        assert_eq!(html_config(extra).feed_title.as_deref(), Some("My Feed"));
    }

    #[test]
    fn test_feed_title_absent() {
        assert_eq!(html_config(toml::Table::new()).feed_title, None);
    }

    #[test]
    fn test_resolve_title_feed_title_set() {
        let mut extra = toml::Table::new();
        extra.insert(
            "feed_title".to_string(),
            toml::Value::String("Feed Title".to_string()),
        );
        let cfg = html_config(extra);
        assert_eq!(
            cfg.resolve_title(Some("Spine Title"), "project"),
            "Feed Title"
        );
    }

    #[test]
    fn test_resolve_title_spine_fallback() {
        let cfg = html_config(toml::Table::new());
        assert_eq!(
            cfg.resolve_title(Some("Spine Title"), "project"),
            "Spine Title"
        );
    }

    #[test]
    fn test_resolve_title_project_fallback() {
        let cfg = html_config(toml::Table::new());
        assert_eq!(cfg.resolve_title(None, "my-project"), "my-project");
    }
}
