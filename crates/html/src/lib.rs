pub mod feed;
mod server;

use crate::feed::{AtomEntry, AtomFeed};
use chrono::{DateTime, Utc};
use rheo_core::{compile_document_to_string, html_utils};
use serde::Deserialize;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

use rheo_core::{
    AssetConfig, CompiledHtmlVertebra, FormatInitTemplate, FormatPlugin, OpenHandle, PluginContext,
    Result, RheoError, RheoValue, ServerHandle,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

    fn format_init_template(&self) -> FormatInitTemplate {
        FormatInitTemplate {
            files: vec![
                // The stylesheet included with the template mirrors the default stylesheet, so that
                // users can build from it or start from scratch as they wish.
                ("style.css", include_str!("templates/style.css")),
                // A demonstrative JS file that just logs to console. Use JS files in your project
                // to add client-side interactivity to Rheo output.
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

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        self.render(&ctx)?;
        Ok(())
    }

    fn compile_vertebra(&self, ctx: PluginContext<'_>) -> Result<Option<CompiledHtmlVertebra>> {
        Ok(Some(self.render(&ctx)?))
    }

    /// Emit `build/html/feed.xml` once, after every page is compiled, from the
    /// documents produced during the per-file loop — no second compilation pass.
    fn finalize(&self, ctx: &PluginContext<'_>, compiled: &[CompiledHtmlVertebra]) -> Result<()> {
        self.generate_feed(ctx, compiled)
    }
}

impl HtmlPlugin {
    /// Compile one source file to its final HTML output (with CSS/JS and feed
    /// autodiscovery injection) and write it, returning the raw compiled document
    /// so the Atom feed can reuse it without recompiling.
    fn render(&self, ctx: &PluginContext<'_>) -> Result<CompiledHtmlVertebra> {
        let document = ctx.compile_to_html_document()?;
        let html_string = compile_document_to_string(&document)?;

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

        // Inline styles are applied via string manipulation (see
        // `inject_inline_styles`), so they happen before any DOM parse.
        let html_string = html_utils::inject_inline_styles(&html_string, inline_styles)?;

        // Single-parse invariant: the remaining per-page mutations
        // (`inject_head_links` for stylesheets/scripts, then `inject_feed_link`
        // for Atom autodiscovery) share ONE `HtmlDom`, so each page is parsed and
        // serialized at most once. Head-links run before the feed link to match
        // the prior two-pass ordering (both insert after the last <meta>).
        let needs_head_links = !css_paths.is_empty() || !js_paths.is_empty();
        let html_cfg = ctx.config.parse_extra::<HtmlConfig>()?;
        let feed_title = html_cfg.resolve_title(ctx.spine.title.as_deref(), &ctx.project.name);
        let feed_link = html_cfg
            .base_url()
            .map(|base| (format!("{base}/feed.xml"), feed_title));

        let html_string = if needs_head_links || feed_link.is_some() {
            let mut dom = html_utils::HtmlDom::parse(&html_string)?;
            if needs_head_links {
                dom.inject_head_links(&[], &css_paths, &js_paths)?;
            }
            if let Some((href, title)) = &feed_link {
                dom.inject_feed_link(href, title)?;
            }
            dom.serialize()?
        } else {
            html_string
        };

        debug!(size = html_string.len(), "writing HTML file");
        let output = &ctx.options.output;
        std::fs::write(output, &html_string)
            .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

        info!(output = %output.display(), "successfully compiled to HTML");

        Ok(CompiledHtmlVertebra {
            path: ctx
                .options
                .input()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            document,
            vars: Default::default(),
        })
    }

    /// Emit `build/html/feed.xml`: one `<entry>` per compiled vertebra that
    /// declares `rheo-feed-title`. Gated on `[html].feed_base_url` being set.
    ///
    /// `compiled` is every page produced by the per-file loop (spine order, vars
    /// populated), so no vertebra is recompiled here.
    fn generate_feed(
        &self,
        ctx: &PluginContext<'_>,
        compiled: &[CompiledHtmlVertebra],
    ) -> Result<()> {
        let cfg = ctx.config.parse_extra::<HtmlConfig>()?;
        let Some(base) = cfg.base_url() else {
            debug!("no [html].feed_base_url set; skipping Atom feed");
            return Ok(());
        };

        // Harvest each vertebra's `rheo-*` vars (parse-only, no recompile) keyed
        // by source path. Only done once a feed is actually configured.
        let vars_by_path: HashMap<PathBuf, HashMap<String, RheoValue>> =
            ctx.spine_vars()?.into_iter().collect();

        let mut entries = Vec::new();
        for v in compiled {
            let Some(vars) = vars_by_path.get(&v.path) else {
                continue;
            };
            let Some(title) = vars.get("feed-title").and_then(|val| val.as_str()) else {
                continue;
            };

            let updated = feed_updated(&v.path, vars)?;
            let html = compile_document_to_string(&v.document)?;
            let body = html_utils::HtmlDom::parse(&html)?.feed_content_inner_html()?;
            let stem = v
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let href = format!("{base}/{stem}.html");

            entries.push(AtomEntry {
                id: href.clone(),
                title: title.to_string(),
                updated,
                content_html: body,
                alternate_href: href,
            });
        }

        let feed = AtomFeed {
            id: format!("{base}/feed.xml"),
            title: cfg.resolve_title(ctx.spine.title.as_deref(), &ctx.project.name),
            updated: Utc::now(),
            self_href: format!("{base}/feed.xml"),
            author: cfg
                .feed_author
                .clone()
                .unwrap_or_else(|| "Rheo".to_string()),
            entries,
        };

        let feed_path = ctx
            .output_config
            .dir_for_plugin(PLUGIN_NAME)
            .join("feed.xml");
        std::fs::write(&feed_path, feed.serialize())
            .map_err(|e| RheoError::io(e, format!("writing Atom feed to {:?}", feed_path)))?;
        info!(output = %feed_path.display(), "generated Atom feed");
        Ok(())
    }
}

/// The entry's `updated` timestamp: `rheo-feed-updated` (RFC 3339) if present in
/// `vars`, else the source file's modification time.
fn feed_updated(path: &Path, vars: &HashMap<String, RheoValue>) -> Result<DateTime<Utc>> {
    if let Some(s) = vars.get("feed-updated").and_then(|val| val.as_str()) {
        return DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| {
                RheoError::invalid_data(format!(
                    "{}: rheo-feed-updated must be an RFC 3339 datetime",
                    path.display()
                ))
            });
    }

    let modified = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map_err(|e| RheoError::io(e, format!("reading mtime of {:?}", path)))?;
    Ok(DateTime::<Utc>::from(modified))
}

/// Typed view of the `[html]` section's format-specific keys.
#[derive(Debug, Deserialize, Default)]
struct HtmlConfig {
    /// Base URL for the Atom feed; when set, `feed.xml` is emitted and an
    /// autodiscovery `<link>` is injected into every page.
    feed_base_url: Option<String>,
    /// `atom:author` of the feed; defaults to `"Rheo"` when absent.
    feed_author: Option<String>,
    /// `<title>` of the Atom feed and the autodiscovery `<link>`.
    /// Falls back to the HTML spine title, then the project/directory name.
    feed_title: Option<String>,
}

impl HtmlConfig {
    /// The feed base URL with any trailing `/` trimmed, so callers can join
    /// paths with a single `/`. `None` when `feed_base_url` is unset.
    fn base_url(&self) -> Option<String> {
        self.feed_base_url
            .as_deref()
            .map(|s| s.trim_end_matches('/').to_string())
    }

    /// Resolve the feed title: `[html] feed_title` → spine title → project name.
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
