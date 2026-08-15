pub mod feed;
mod server;

use serde::Deserialize;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

/// Output filename for the bundled default stylesheet when no user CSS resolves.
/// Distinct from `style.css` so it never clashes with a user's own stylesheet.
pub const DEFAULT_STYLESHEET_NAME: &str = "rheo-default.css";

use rheo_core::{
    AssetConfig, CastVertebra, EmbeddedDefault, FormatInitTemplate, FormatPlugin, OpenHandle,
    PluginContext, Result, RheoError, ServerHandle,
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
    pub vfs_arc: std::sync::Arc<tokio::sync::RwLock<Option<typst_bundle::VirtualFs>>>,
}

impl ServerHandle for HtmlServerHandle {
    fn url(&self) -> &str {
        &self.url
    }
    fn reload(&self) {
        (self.reload_callback)();
    }
    fn update_virtual_fs(&self, vfs: typst_bundle::VirtualFs) {
        let arc = self.vfs_arc.clone();
        self.runtime.spawn(async move {
            *arc.write().await = Some(vfs);
        });
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
            options_toml: None,
        }
    }

    fn open(&self, output_dir: &Path, _format_name: &str) -> Result<OpenHandle> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| RheoError::io(e, "creating tokio runtime"))?;

        let (server_task, reload_tx, url, vfs_arc) = runtime
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
            vfs_arc,
        };
        Ok(OpenHandle::Server(Box::new(handle)))
    }

    fn assets(&self) -> Vec<AssetConfig> {
        vec![
            AssetConfig {
                name: STYLESHEETS,
                default_path: "style.css",
                required: false,
                // No user/override stylesheet → ship the built-in default as a
                // real linked file (rheo-default.css) rather than an inline <style>.
                default_content: Some(EmbeddedDefault {
                    name: DEFAULT_STYLESHEET_NAME,
                    content: DEFAULT_STYLESHEET,
                }),
            },
            AssetConfig {
                name: SCRIPTS,
                default_path: "index.js",
                required: false,
                default_content: None,
            },
        ]
    }

    fn compile(&self, ctx: PluginContext<'_>, outputs: &[CastVertebra]) -> Result<()> {
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

        let js_paths: Vec<String> = js_assets
            .map(|v| v.iter().map(|a| a.built_relative_path.clone()).collect())
            .unwrap_or_default();

        let html_cfg = ctx.config.parse_extra::<HtmlConfig>()?;
        let feed_title = html_cfg.resolve_title(ctx.spine.title.as_deref(), &ctx.project.name);
        let feed_link = html_cfg
            .base_url()
            .map(|base| (format!("{base}/feed.xml"), feed_title.clone()));

        let needs_head_links = !css_paths.is_empty() || !js_paths.is_empty();
        // A bundle-root `.rheo/head.html` control asset (see
        // `ControlAssets`), if present, contributes to every page's `<head>`
        // rather than just one page's — read it once up front.
        let head_fragment = ctx.control.head_fragment.as_deref();

        for output in outputs {
            // A `<rheo-head>` wrapper needs hoisting even when neither
            // CSS/JS nor a feed link is configured, so we need the raw
            // string up front to peek for one before deciding whether a DOM
            // parse is required at all.
            let html_string = String::from_utf8(output.bytes.to_vec()).map_err(|e| {
                RheoError::invalid_data(format!("HTML output is not valid UTF-8: {}", e))
            })?;
            let has_rheo_head = html_string.contains("<rheo-head");

            let html_string = if needs_head_links
                || feed_link.is_some()
                || has_rheo_head
                || head_fragment.is_some()
            {
                let mut dom = output.html()?;
                if needs_head_links {
                    // Assets are written at the output root; make each ref
                    // depth-relative so nested pages resolve them.
                    let prefix = rheo_core::util::html::depth_prefix(&output.output_path);
                    let css_refs: Vec<String> =
                        css_paths.iter().map(|s| format!("{prefix}{s}")).collect();
                    let js_refs: Vec<String> =
                        js_paths.iter().map(|s| format!("{prefix}{s}")).collect();
                    let css: Vec<&str> = css_refs.iter().map(|s| s.as_str()).collect();
                    let js: Vec<&str> = js_refs.iter().map(|s| s.as_str()).collect();
                    dom.inject_head_links(&[], &css, &js)?;
                }
                if let Some((href, title)) = &feed_link {
                    dom.inject_feed_link(href, title)?;
                }
                dom.hoist_rheo_head()?;
                // Site-wide head content lands after this page's own
                // `<rheo-head>` contributions — see `append_head_fragment`'s
                // doc comment for the ordering rationale.
                if let Some(fragment) = head_fragment {
                    dom.append_head_fragment(fragment)?;
                }
                dom.serialize()?
            } else {
                html_string
            };

            let out_path = ctx.output_dir.join(&output.output_path);
            debug!(size = html_string.len(), "writing HTML file");
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    RheoError::io(e, format!("creating output directory {:?}", parent))
                })?;
            }
            std::fs::write(&out_path, &html_string)
                .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", out_path)))?;
            info!(output = %out_path.display(), "successfully compiled to HTML");
        }

        // Generate Atom feed if feed_base_url is configured.
        if let Some(base_url) = html_cfg.base_url() {
            feed::generate_feed(ctx, outputs, &base_url, &feed_title, &html_cfg)?;
        }

        Ok(())
    }
}

/// One `[[html.feed_include]]` entry: opts a marrow-contributed page (no
/// source vertebra, so no `rheo-feed-title` var to read) back into the Atom
/// feed. `path` matches the plugin-output-relative output path via a glob;
/// `title` supplies the entry's title directly, since a contributed page's
/// title cannot be synthesised from its compiled HTML.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct FeedInclude {
    pub path: String,
    #[serde(default)]
    pub title: String,
}

/// Typed view of the `[html]` section's format-specific keys.
#[derive(Debug, Deserialize, Default)]
pub struct HtmlConfig {
    feed_base_url: Option<String>,
    feed_author: Option<String>,
    feed_title: Option<String>,
    #[serde(default)]
    feed_include: Vec<FeedInclude>,
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

    /// `ctx.control.head_fragment` (populated by `ControlAssets::extract` from
    /// a bundle-root `.rheo/head.html` control asset, before the plugin ever
    /// sees it) must land in the `<head>` of *every* compiled page, not just
    /// one — that's the whole point of a site-wide control asset, as opposed
    /// to a per-page `<rheo-head>` wrapper.
    #[test]
    fn test_compile_appends_control_head_fragment_to_every_page() {
        use rheo_core::config::project::{ProjectConfig, ProjectMode};
        use rheo_core::reticulate::{SpineLayout, SpineScan, VirtualSpine};
        use rheo_core::{ControlAssets, RheoConfig, TypstFormat};
        use std::collections::HashMap;
        use typst::foundations::Bytes;

        let dir = tempfile::tempdir().expect("tempdir");
        let output_dir = dir.path().to_path_buf();

        let make = |path: &str| CastVertebra {
            output_path: path.to_string(),
            bytes: Bytes::new(
                format!("<html><head><title>{path}</title></head><body><p>hi</p></body></html>")
                    .into_bytes(),
            ),
            format: TypstFormat::Html,
            title: path.to_string(),
            date: None,
            vars: HashMap::new(),
            contributed: false,
        };
        let outputs = vec![make("a.html"), make("sub/b.html")];

        let project = ProjectConfig {
            name: "test".to_string(),
            root: output_dir.clone(),
            config: RheoConfig::default(),
            typ_files: vec![],
            mode: ProjectMode::Directory,
            config_path: None,
        };
        let section = PluginSection::default();
        let assets = HashMap::new();
        let font_dirs: Vec<std::path::PathBuf> = vec![];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let virtual_spine =
            VirtualSpine::build(SpineScan::flat(&[], &output_dir), &output_dir, layout)
                .expect("build empty spine");
        let bundle_assets: Vec<(String, Bytes)> = vec![];
        let control = ControlAssets {
            head_fragment: Some(
                r#"<link rel="alternate" type="application/atom+xml" href="/feed.xml">"#
                    .to_string(),
            ),
        };
        let ctx = PluginContext {
            project: &project,
            output_dir: &output_dir,
            spine: &virtual_spine,
            config: &section,
            assets: &assets,
            font_dirs: &font_dirs,
            bundle_assets: &bundle_assets,
            control: &control,
        };

        HtmlPlugin.compile(ctx, &outputs).expect("compile");

        for path in ["a.html", "sub/b.html"] {
            let written = std::fs::read_to_string(output_dir.join(path)).expect("read output");
            assert!(
                written.contains("application/atom+xml"),
                "page {path} missing site-wide head fragment:\n{written}"
            );
            let head_end = written.find("</head>").expect("output has a <head>");
            let frag_pos = written.find("application/atom+xml").unwrap();
            assert!(
                frag_pos < head_end,
                "fragment must land inside <head> for {path}"
            );
        }
    }
}
