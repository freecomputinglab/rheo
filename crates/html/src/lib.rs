pub mod feed;
mod server;

use crate::feed::{AtomEntry, AtomFeed};
use chrono::{DateTime, Utc};
use rheo_core::PluginSection;
use rheo_core::{compile_document_to_string, html_utils};

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
            },
            AssetConfig {
                name: SCRIPTS,
                default_path: "index.js",
                required: false,
            },
        ]
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

        // Inline styles are applied via string manipulation (see
        // `inject_inline_styles`), so they happen before any DOM parse.
        let html_string = html_utils::inject_inline_styles(&html_string, inline_styles)?;

        // Single-parse invariant: the remaining per-page mutations
        // (`inject_head_links` for stylesheets/scripts, then `inject_feed_link`
        // for Atom autodiscovery) share ONE `HtmlDom`, so each page is parsed and
        // serialized at most once. Head-links run before the feed link to match
        // the prior two-pass ordering (both insert after the last <meta>).
        let needs_head_links = !css_paths.is_empty() || !js_paths.is_empty();
        let feed_link = feed_base_url(ctx.config)
            .map(|base| (format!("{base}/feed.xml"), ctx.project.name.clone()));

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

        self.generate_feed(&ctx)?;
        Ok(())
    }
}

impl HtmlPlugin {
    /// Emit `build/html/feed.xml` once per build, one `<entry>` per vertebra
    /// that declares `rheo-feed-title`. Gated on `[html].feed_base_url` being set.
    ///
    /// `compile` runs once per file (HTML merge=false), so generation is gated
    /// to a single invocation: the merged case (`input == None`) or the first
    /// spine vertebra.
    fn generate_feed(&self, ctx: &PluginContext<'_>) -> Result<()> {
        let Some(base) = feed_base_url(ctx.config) else {
            debug!("no [html].feed_base_url set; skipping Atom feed");
            return Ok(());
        };

        let spine_paths = ctx.spine.generate(&ctx.spine_root())?;
        if !is_feed_generation_invocation(ctx.options.input.as_deref(), spine_paths.first()) {
            return Ok(());
        }

        let mut entries = Vec::new();
        for v in ctx.compile_spine_items_to_html(self)? {
            let Some(title) = v.vars.get("feed-title").and_then(|val| val.as_str()) else {
                continue;
            };

            let updated = feed_updated(&v)?;
            let html = compile_document_to_string(&v.document)?;
            let body = html_utils::extract_feed_content_html(&html)?;
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
            title: ctx.project.name.clone(),
            updated: Utc::now(),
            self_href: format!("{base}/feed.xml"),
            author: feed_author(ctx.config).unwrap_or_else(|| "Rheo".to_string()),
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

/// Whether this `compile` invocation is the one that should generate the feed:
/// the merged case (no input) or the first spine vertebra. Paths are compared
/// canonically, falling back to direct comparison if canonicalization fails.
fn is_feed_generation_invocation(
    input: Option<&Path>,
    first_vertebra: Option<&std::path::PathBuf>,
) -> bool {
    let Some(input) = input else {
        return true; // merged mode: single invocation
    };
    let Some(first) = first_vertebra else {
        return false;
    };
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(input) == canon(first)
}

/// The entry's `updated` timestamp: `rheo-feed-updated` (RFC 3339) if present,
/// else the source file's modification time.
fn feed_updated(v: &rheo_core::CompiledHtmlVertebra) -> Result<DateTime<Utc>> {
    if let Some(s) = v.vars.get("feed-updated").and_then(|val| val.as_str()) {
        return DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| {
                RheoError::invalid_data(format!(
                    "{}: rheo-feed-updated must be an RFC 3339 datetime",
                    v.path.display()
                ))
            });
    }

    let modified = std::fs::metadata(&v.path)
        .and_then(|m| m.modified())
        .map_err(|e| RheoError::io(e, format!("reading mtime of {:?}", v.path)))?;
    Ok(DateTime::<Utc>::from(modified))
}

/// Read the `feed_base_url` key from a plugin section's config (mirrors epub's
/// `parse_identifier`). Any trailing `/` is trimmed so callers can join paths
/// with a single `/`. Returns `None` when the key is absent or not a string.
///
/// NOTE: this is the interim reader; once `[{format}.options]` lands (rheo-uico)
/// `feed_base_url` becomes a validated option read via
/// `ctx.option_str("feed_base_url")`.
pub fn feed_base_url(section: &PluginSection) -> Option<String> {
    section
        .extra
        .get("feed_base_url")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_end_matches('/').to_string())
}

/// Read the `feed_author` key from a plugin section's config (mirrors
/// `feed_base_url`). Used as the Atom feed's `<author><name>`. Returns `None`
/// when the key is absent or not a string, in which case the caller defaults to
/// `"Rheo"`.
pub fn feed_author(section: &PluginSection) -> Option<String> {
    section
        .extra
        .get("feed_author")
        .and_then(|v| v.as_str())
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn section_with(extra: toml::Table) -> PluginSection {
        PluginSection {
            extra,
            ..Default::default()
        }
    }

    #[test]
    fn test_feed_base_url_trims_trailing_slash() {
        let mut extra = toml::Table::new();
        extra.insert(
            "feed_base_url".to_string(),
            toml::Value::String("https://example.com/".to_string()),
        );
        let section = section_with(extra);
        assert_eq!(
            feed_base_url(&section).as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn test_feed_base_url_absent() {
        let section = section_with(toml::Table::new());
        assert_eq!(feed_base_url(&section), None);
    }

    #[test]
    fn test_feed_base_url_non_string() {
        let mut extra = toml::Table::new();
        extra.insert("feed_base_url".to_string(), toml::Value::Integer(42));
        let section = section_with(extra);
        assert_eq!(feed_base_url(&section), None);
    }

    #[test]
    fn test_feed_author_present() {
        let mut extra = toml::Table::new();
        extra.insert(
            "feed_author".to_string(),
            toml::Value::String("Ada Lovelace".to_string()),
        );
        let section = section_with(extra);
        assert_eq!(feed_author(&section).as_deref(), Some("Ada Lovelace"));
    }

    #[test]
    fn test_feed_author_absent() {
        let section = section_with(toml::Table::new());
        assert_eq!(feed_author(&section), None);
    }

    #[test]
    fn test_feed_author_non_string() {
        let mut extra = toml::Table::new();
        extra.insert("feed_author".to_string(), toml::Value::Integer(42));
        let section = section_with(extra);
        assert_eq!(feed_author(&section), None);
    }

    #[test]
    fn test_feed_gate_merged_mode_generates() {
        // No input (merged mode) → always the generating invocation.
        assert!(is_feed_generation_invocation(None, None));
        assert!(is_feed_generation_invocation(
            None,
            Some(&PathBuf::from("a.typ"))
        ));
    }

    #[test]
    fn test_feed_gate_first_vertebra_only() {
        let first = PathBuf::from("chapters/a.typ");
        assert!(is_feed_generation_invocation(
            Some(Path::new("chapters/a.typ")),
            Some(&first)
        ));
        assert!(!is_feed_generation_invocation(
            Some(Path::new("chapters/b.typ")),
            Some(&first)
        ));
    }

    #[test]
    fn test_feed_gate_no_spine_no_generation() {
        assert!(!is_feed_generation_invocation(
            Some(Path::new("a.typ")),
            None
        ));
    }
}
